#!/usr/bin/env python3
"""Exclusive GPU-slot integration check; private bus required (dbus-run-session)."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def wait_for(operation, label, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = operation()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError(f"Timed out: {label}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    args = parser.parse_args()
    evidence = ROOT / ".tmp" / f"gpu-foundation-{time.time_ns()}"
    evidence.mkdir()
    # Unix socket paths must remain below sockaddr_un's 108-byte limit.
    runtime_dir = ROOT / ".tmp" / f"r{os.getpid()}"
    runtime_dir.mkdir(mode=0o700)
    home = evidence / "home"
    home.mkdir()
    config = evidence / "config"
    (config / "tohru").mkdir(parents=True)
    user = config / "tohru/config.toml"
    user.write_text("# preserve me\n[future]\nvalue = 42\n[bar]\npreferred_output = 'DP-LEGACY'\n[appearance]\ncorners = false\n")
    managed = evidence / "managed.toml"
    managed.write_text("[bar]\nmooncoil = false\n")
    scene = evidence / "scene"
    shutil.copytree(ROOT / "shell", scene)
    sway_config = evidence / "sway.conf"
    sway_config.write_text("output HEADLESS-1 mode 1280x240@60Hz\noutput HEADLESS-1 scale 1\noutput HEADLESS-1 bg #30232b solid_color\nseat seat0 hide_cursor 100\n")
    env = {**os.environ,
           "HOME": str(home), "XDG_CONFIG_HOME": str(config),
           "XDG_CACHE_HOME": str(evidence / "cache"),
           "XDG_DATA_HOME": str(evidence / "data"),
           "XDG_STATE_HOME": str(evidence / "state"),
           "XDG_RUNTIME_DIR": str(runtime_dir),
           "WLR_BACKENDS": "headless", "WLR_HEADLESS_OUTPUTS": "1",
           "WLR_LIBINPUT_NO_DEVICES": "1", "WLR_RENDERER": "gles2",
           "WLR_RENDER_DRM_DEVICE": "/dev/dri/renderD128",
           "VK_DRIVER_FILES": "/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json",
           "WGPU_BACKEND": "vulkan", "PLEAMAR_BACKEND": "vulkan",
           "TOHRU_RUNTIME": str(args.runtime.resolve()),
           "TOHRU_MANAGED_CONFIG": str(managed), "TOHRU_SCENE": str(scene / "bar.plm")}
    for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE"):
        env.pop(key, None)
    # Refuse inherited live-session bus; launcher below must supply a new private bus.
    if not env.get("DBUS_SESSION_BUS_ADDRESS") or not env.get("TOHRU_PRIVATE_TEST_BUS"):
        raise RuntimeError("Run with TOHRU_PRIVATE_TEST_BUS=1 dbus-run-session -- python3 ...")
    events = []
    app = None
    app_log = None
    sway_log = (evidence / "sway.log").open("w")
    sway = subprocess.Popen(["sway", "--unsupported-gpu", "-d", "-c", str(sway_config)], env=env, stdout=sway_log, stderr=subprocess.STDOUT)
    try:
        socket = wait_for(lambda: next(runtime_dir.glob("sway-ipc.*.sock"), None), "private Sway socket")
        display = wait_for(lambda: next((p for p in runtime_dir.glob("wayland-*") if not p.name.endswith(".lock")), None), "private Wayland socket")
        env["SWAYSOCK"] = str(socket)
        env["WAYLAND_DISPLAY"] = display.name
        compositor_log = (evidence / "sway.log").read_text()
        assert "NVIDIA GeForce RTX 5090" in compositor_log, "compositor hardware adapter not verified"
        outputs = subprocess.check_output(["swaymsg", "-t", "get_outputs", "-r"], env=env, text=True)
        (evidence / "outputs.json").write_text(outputs)

        def cli(*arguments, check=True):
            result = subprocess.run([str(ROOT / "bin/tohru"), *arguments], env=env, capture_output=True, text=True, timeout=10)
            events.append({"args": arguments, "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
            if check:
                assert result.returncode == 0, events[-1]
            return result

        def snapshot():
            result = cli("status", "--json", check=False)
            try:
                return json.loads(result.stdout).get("settings")
            except json.JSONDecodeError:
                return None

        def expect(key, value, origin=None):
            def check():
                settings = snapshot()
                if settings and settings[key]["value"] == value and (origin is None or settings[key]["origin"] == origin):
                    return settings
                return None
            return wait_for(check, f"{key}={value!r}, origin={origin}")

        def launch(label):
            nonlocal app, app_log
            app_log = (evidence / f"app-{label}.log").open("w")
            app = subprocess.Popen([str(ROOT / "bin/tohru-run")], env=env, stdout=app_log, stderr=subprocess.STDOUT)
            expect("bar.mooncoil", False, "nix")
            assert "NVIDIA GeForce RTX 5090" in (evidence / f"app-{label}.log").read_text(), "app hardware adapter not verified"

        launch("first")
        expect("appearance.corners", False, "user")
        cli("config", "set", "bar.mooncoil", "true")
        settings = expect("bar.mooncoil", True, "runtime")
        assert settings["bar.mooncoil"]["managed"] and settings["bar.mooncoil"]["runtime_override"]
        assert managed.read_text() == "[bar]\nmooncoil = false\n"
        cli("config", "reload")
        expect("bar.mooncoil", True, "runtime")
        # Watcher must replace the actual scene logic VM, preserving Rust state.
        logic = scene / "bar.luau"
        previous_loads = (evidence / "app-first.log").read_text().count("bar.luau running")
        logic.write_text(logic.read_text() + "\n-- integration reload checkpoint\n")
        wait_for(lambda: (evidence / "app-first.log").read_text().count("bar.luau running") > previous_loads, "native logic reload")
        expect("bar.mooncoil", True, "runtime")
        cli("config", "set", "appearance.corners", "true")
        expect("appearance.corners", True, "user")
        assert "# preserve me" in user.read_text() and "value = 42" in user.read_text()
        assert cli("config", "set", "bar.preferred_output", "DP-TEST", check=False).returncode == 2
        expect("bar.preferred_output", "DP-LEGACY", "user")
        managed.write_text("[bar]\nmooncoil = 'invalid'\n")
        cli("config", "reload")
        wait_for(lambda: cli("settings-error").stdout.strip(), "invalid reload error")
        expect("bar.mooncoil", True, "runtime")
        managed.write_text("[bar]\nmooncoil = false\n")
        cli("config", "reload")
        wait_for(lambda: not cli("settings-error").stdout.strip(), "error cleared")
        screenshot = evidence / "foundation-bar.png"
        subprocess.run(["grim", "-o", "HEADLESS-1", str(screenshot)], env=env, check=True, timeout=10)
        subprocess.run([str(args.runtime.resolve()), "--say", "tohru", "quit"], env=env, check=True, timeout=10)
        app.wait(timeout=10)
        app_log.close()
        app = None
        launch("restart")
        settings = expect("appearance.corners", True, "user")
        expect("bar.preferred_output", "DP-LEGACY", "user")
        assert not settings["bar.mooncoil"]["runtime_override"]
        summary = {"result": "passed", "runtime": str(args.runtime.resolve()), "app_adapter": "NVIDIA GeForce RTX 5090 / Vulkan", "compositor_adapter": "NVIDIA GeForce RTX 5090 / GLES2", "screenshot": str(screenshot), "checks": ["native CLI", "managed override", "config reload", "actual logic VM reload", "unmanaged persistence", "unknown/comment preservation", "invalid reload atomicity", "process restart reset"], "performance_acceptance": False}
        (evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    finally:
        (evidence / "cli-events.json").write_text(json.dumps(events, indent=2) + "\n")
        for process in (app, sway):
            if process and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        if app_log:
            app_log.close()
        sway_log.close()
        print(evidence, flush=True)


if __name__ == "__main__":
    main()
