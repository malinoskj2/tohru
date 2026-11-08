#!/usr/bin/env python3
"""Exclusive GPU-slot integration check with runner-owned private transports."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import sys
import stat
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1]


def wait_for(operation, label, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = operation()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError(f"Timed out: {label}")


def isolated_transport(inherited, runtime_dir):
    """Every app/CLI child must use these endpoints, overriding hostile inheritance."""
    env = {**inherited,
           "XDG_RUNTIME_DIR": str(runtime_dir),
           "PLEAMAR_SOCKETS": str(runtime_dir / "pleamar-sockets"),
           "PULSE_SERVER": "unix:" + str(runtime_dir / "no-audio-socket"),
           "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(runtime_dir / "bus"),
           "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(runtime_dir / "bus")}
    for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "WAYLAND_SOCKET", "CUA_INJECT_SOCKET", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "TOHRU_PRIVATE_TEST_BUS", "TOHRU_B_PRIVATE_BUS", "XDG_SESSION_ID", "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE", "LISTEN_FDS", "LISTEN_PID", "PULSE_COOKIE", "PIPEWIRE_REMOTE"):
        env.pop(key, None)
    return env


def assert_private_transport(env, runtime_dir):
    runtime_dir = runtime_dir.resolve()
    assert Path(env["XDG_RUNTIME_DIR"]).resolve() == runtime_dir
    assert stat.S_IMODE(runtime_dir.stat().st_mode) == 0o700
    for key, prefix in (("PLEAMAR_SOCKETS", ""), ("PULSE_SERVER", "unix:"),
                        ("DBUS_SESSION_BUS_ADDRESS", "unix:path="), ("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=")):
        value = env[key]
        assert value.startswith(prefix)
        path = Path(value[len(prefix):])
        assert path.is_relative_to(runtime_dir) and path.resolve().is_relative_to(runtime_dir), key
        assert not path.is_symlink(), key
    for key in ("WAYLAND_SOCKET", "CUA_INJECT_SOCKET", "HYPRLAND_INSTANCE_SIGNATURE"):
        assert key not in env
    if "WAYLAND_DISPLAY" in env:
        assert not Path(env["WAYLAND_DISPLAY"]).is_absolute()
        assert (runtime_dir / env["WAYLAND_DISPLAY"]).resolve().is_relative_to(runtime_dir)
    if "SWAYSOCK" in env:
        assert Path(env["SWAYSOCK"]).resolve().is_relative_to(runtime_dir)


def private_bus_config(runtime_dir):
    return """<busconfig><type>session</type><listen>unix:path=""" + escape(str(runtime_dir / "bus")) + """</listen>
<auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/>
<allow receive_sender="*"/><allow own="*"/></policy></busconfig>"""


def finish_private_run(processes, handles, write_evidence):
    """Attempt every owned cleanup even if diagnostics/another owner fail."""
    errors = []
    try:
        write_evidence()
    except BaseException as error:
        errors.append(error)
    for process in processes:
        if process is None:
            continue
        try:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)
        except BaseException as error:
            errors.append(error)
            try:
                process.kill()
                process.wait(timeout=5)
            except BaseException as failure:
                errors.append(failure)
    for handle in handles:
        if handle is not None:
            try:
                handle.close()
            except BaseException as error:
                errors.append(error)
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--sound", action="store_true", help="also test root sound/Settings with owned private Pulse null sink")
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
           "PULSE_SERVER": "unix:" + str(runtime_dir / "no-audio-socket"),
           "TOHRU_RUNTIME": str(args.runtime.resolve()),
           "TOHRU_MANAGED_CONFIG": str(managed), "TOHRU_SCENE": str(scene / "bar.plm")}
    env = isolated_transport(env, runtime_dir)
    assert_private_transport(env, runtime_dir)
    events = []
    app = sway = bus = pulse = None
    app_log = None
    sway_log = (evidence / "sway.log").open("w")
    bus_log = (evidence / "bus.log").open("w")
    pulse_log = (evidence / "pulse.log").open("w")
    bus_config = runtime_dir / "bus.conf"
    bus_config.write_text(private_bus_config(runtime_dir))
    try:
        bus = subprocess.Popen(["dbus-daemon", "--nofork", "--config-file=" + str(bus_config)], env=env, stdout=bus_log, stderr=subprocess.STDOUT)
        wait_for(lambda: (runtime_dir / "bus").is_socket(), "owned private bus socket")
        if args.sound:
            pulse_socket = runtime_dir / "pulse"
            pulse_config = runtime_dir / "pulse.pa"
            pulse_config.write_text("load-module module-native-protocol-unix socket=" + str(pulse_socket) + " auth-anonymous=1\nload-module module-null-sink sink_name=tohru_private sink_properties=device.description=Tohru-Private\nset-default-sink tohru_private\n")
            env["PULSE_SERVER"] = "unix:" + str(pulse_socket)
            assert_private_transport(env, runtime_dir)
            pulse = subprocess.Popen(["pulseaudio", "--daemonize=no", "--use-pid-file=no", "--exit-idle-time=-1", "-n", "--file=" + str(pulse_config)], env=env, stdout=pulse_log, stderr=subprocess.STDOUT)
            wait_for(pulse_socket.is_socket, "owned private Pulse socket")
            subprocess.run(["pactl", "set-sink-volume", "tohru_private", "50%"], env=env, check=True, timeout=10)
        assert_private_transport(env, runtime_dir)
        sway = subprocess.Popen(["sway", "--unsupported-gpu", "-d", "-c", str(sway_config)], env=env, stdout=sway_log, stderr=subprocess.STDOUT)
        socket = wait_for(lambda: next(runtime_dir.glob("sway-ipc.*.sock"), None), "private Sway socket")
        display = wait_for(lambda: next((p for p in runtime_dir.glob("wayland-*") if not p.name.endswith(".lock")), None), "private Wayland socket")
        env["SWAYSOCK"] = str(socket)
        env["WAYLAND_DISPLAY"] = display.name
        compositor_log = (evidence / "sway.log").read_text()
        assert "NVIDIA GeForce RTX 5090" in compositor_log, "compositor hardware adapter not verified"
        outputs = subprocess.check_output(["swaymsg", "-t", "get_outputs", "-r"], env=env, text=True)
        (evidence / "outputs.json").write_text(outputs)

        def cli(*arguments, check=True):
            assert_private_transport(env, runtime_dir)
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
            assert_private_transport(env, runtime_dir)
            app_log = (evidence / f"app-{label}.log").open("w")
            app = subprocess.Popen([str(ROOT / "bin/tohru-run")], env=env, stdout=app_log, stderr=subprocess.STDOUT)
            expect("bar.mooncoil", False, "nix")
            assert "NVIDIA GeForce RTX 5090" in (evidence / f"app-{label}.log").read_text(), "app hardware adapter not verified"

        launch("first")
        if args.sound:
            def native(command):
                assert_private_transport(env, runtime_dir)
                result = subprocess.run([str(args.runtime.resolve()), "--say", "tohru", command], env=env, capture_output=True, text=True, timeout=10)
                events.append({"native": command, "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
                assert result.returncode == 0, events[-1]
                return result.stdout.strip()
            def audio_matches(volume, muted):
                value = json.loads(cli("audio", "status", "--json").stdout)
                return value if abs(value.get("volume", -10) - volume) < 0.015 and value.get("muted") == muted else None
            wait_for(lambda: audio_matches(0.5, False), "private native audio report")
            cli("audio", "volume", "up")
            wait_for(lambda: audio_matches(0.55, False), "5 percent native step")
            cli("audio", "mute", "toggle")
            wait_for(lambda: audio_matches(0.55, True), "native mute")
            cli("audio", "volume", "down")
            wait_for(lambda: audio_matches(0.5, True), "step preserves mute")
            cli("audio", "mute", "toggle")
            wait_for(lambda: audio_matches(0.5, False), "native unmute")
            cli("panel", "open", "sound")
            wait_for(lambda: native("get sound_open") == "true", "root sound popup open")
            native("emit audio_set 0.8")
            wait_for(lambda: audio_matches(0.8, False), "native slider write")
            cli("panel", "open", "calendar")
            wait_for(lambda: native("get cal_open") == "true" and native("get sound_open") == "false", "root opposite close")
            cli("panel", "open", "sound")
            wait_for(lambda: native("get cal_open") == "false" and native("get sound_open") == "true", "reverse root opposite close")
            # Refuse commands with an actual private server but no output.
            assert_private_transport(env, runtime_dir)
            subprocess.run(["pactl", "unload-module", "module-null-sink"], env=env, check=True, timeout=10)
            wait_for(lambda: json.loads(cli("audio", "status", "--json").stdout).get("output_present") is False, "private output removal")
            cli("audio", "mute", "toggle")
            wait_for(lambda: native("get audio_failed") == "true", "visible native refusal")
            time.sleep(0.2)
            subprocess.run(["grim", "-o", "HEADLESS-1", str(evidence / "sound-error.png")], env=env, check=True, timeout=10)
            subprocess.run(["pactl", "load-module", "module-null-sink", "sink_name=tohru_private", "sink_properties=device.description=Tohru-Private"], env=env, check=True, capture_output=True, timeout=10)
            wait_for(lambda: json.loads(cli("audio", "status", "--json").stdout).get("output_present") is True, "private output restored")
            cli("audio", "volume", "up")
            wait_for(lambda: native("get audio_failed") == "false", "native error clear")
            cli("settings", "open")
            wait_for(lambda: native("get settings_open") == "true" and native("get sound_open") == "false", "same-process Settings open")
            native("text settings_search mooncoil")
            wait_for(lambda: native("get settings_rows.count") == "1", "native Settings search")
            native("emit settings_row 0")
            expect("bar.mooncoil", True, "runtime")
            native("emit settings_category 2")
            wait_for(lambda: native("get settings_page") == "2", "native Settings navigation")
            native("emit settings_back")
            wait_for(lambda: native("get settings_page") == "1", "native Settings back")
            time.sleep(0.2)
            subprocess.run(["grim", "-o", "HEADLESS-1", str(evidence / "settings-window.png")], env=env, check=True, timeout=10)
            cli("settings", "close")
            wait_for(lambda: native("get settings_open") == "false", "Settings close")
            cli("settings", "open")
            wait_for(lambda: native("get settings_open") == "true", "Settings reopen")
            cli("settings", "close")
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
        summary = {"result": "passed", "runtime": str(args.runtime.resolve()), "app_adapter": "NVIDIA GeForce RTX 5090 / Vulkan", "compositor_adapter": "NVIDIA GeForce RTX 5090 / GLES2", "screenshot": str(screenshot), "checks": ["native CLI", "managed override", "config reload", "actual logic VM reload", "unmanaged persistence", "unknown/comment preservation", "invalid reload atomicity", "process restart reset"], "sound_settings_checks": args.sound, "performance_acceptance": False}
        (evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    finally:
        original = sys.exc_info()[1]
        errors = finish_private_run((app, sway, pulse, bus), (app_log, sway_log, pulse_log, bus_log),
            lambda: (evidence / "cli-events.json").write_text(json.dumps(events, indent=2) + "\n"))
        try:
            print(evidence, flush=True)
        except BaseException as failure:
            errors.append(failure)
        if errors:
            if original is not None:
                original.add_note("Cleanup/diagnostics failures: " + repr(errors))
            else:
                raise BaseExceptionGroup("Private run diagnostics/cleanup failed", errors)



if __name__ == "__main__":
    main()
