# NixOS VM integration tests (spike) proving Tohru's bar on headless Wayland
# compositors without a physical GPU (Pixman compositor + Mesa Lavapipe Vulkan).
#
# Run individually:
#   nix build --print-build-logs .#checks.x86_64-linux.compositor-sway
#   nix build --print-build-logs .#checks.x86_64-linux.compositor-weston
{ pkgs, tohru }:

let
  pleamar = "${tohru.runtime}/bin/pleamar";
  lvpIcd = "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json";

  pythonWithPillow = pkgs.python3.withPackages (ps: [ ps.pillow ]);

  commonMachine = {
    virtualisation = {
      memorySize = 2048;
      cores = 2;
      graphics = false;
    };

    users.users.alice = {
      isNormalUser = true;
      createHome = true;
    };

    fonts.packages = [ pkgs.dejavu_fonts ];
    fonts.fontconfig.enable = true;

    # Keep VM boot minimal and deterministic.
    documentation.enable = false;
    services.dbus.enable = true;
  };

  swayProbeScript = pkgs.writeText "tohru-sway-probe.py" ''
    import json
    import os
    from pathlib import Path
    import subprocess
    import sys
    import time
    from PIL import Image

    TOHRU = Path("${tohru}")
    PLEAMAR = "${pleamar}"
    LVP_ICD = "${lvpIcd}"
    OUT = Path("/tmp/tohru-sway-out")
    OUT.mkdir(parents=True, exist_ok=True)
    RT = Path("/tmp/tohru-sway-rt")
    RT.mkdir(mode=0o700, parents=True, exist_ok=True)
    HOME = OUT / "home"
    for sub in ("home", "config", "cache", "data", "state"):
        (OUT / sub).mkdir(parents=True, exist_ok=True)
    (RT / "ipc").mkdir(mode=0o700, parents=True, exist_ok=True)

    env = {
        "PATH": f"{TOHRU / 'bin'}:{os.environ['PATH']}",
        "HOME": str(HOME),
        "XDG_RUNTIME_DIR": str(RT),
        "XDG_CONFIG_HOME": str(OUT / "config"),
        "XDG_CACHE_HOME": str(OUT / "cache"),
        "XDG_DATA_HOME": str(OUT / "data"),
        "XDG_STATE_HOME": str(OUT / "state"),
        "PLEAMAR_SOCKETS": str(RT / "ipc"),
        "DBUS_SESSION_BUS_ADDRESS": f"unix:path={RT}/bus",
        "DBUS_SYSTEM_BUS_ADDRESS": f"unix:path={RT}/bus",
        "PULSE_SERVER": f"unix:{RT}/no-pulse",
        "WLR_BACKENDS": "headless",
        "WLR_HEADLESS_OUTPUTS": "1",
        "WLR_LIBINPUT_NO_DEVICES": "1",
        "WLR_RENDERER": "pixman",
        "VK_DRIVER_FILES": LVP_ICD,
        "VK_ICD_FILENAMES": LVP_ICD,
        "LIBGL_ALWAYS_SOFTWARE": "1",
        "GALLIUM_DRIVER": "llvmpipe",
        "WGPU_BACKEND": "vulkan",
        "PLEAMAR_BACKEND": "vulkan",
        "TOHRU_SHELL": "1",
    }

    # Disable rounded corner surfaces so Lavapipe compiles only the bar surface
    # and renders its first frame quickly; leave bar.preferred_output unset to
    # prove Sway 1.12's ext-workspace-v1 auto-maps the bar on HEADLESS-1.
    cfg_dir = OUT / "config" / "tohru"
    cfg_dir.mkdir(parents=True, exist_ok=True)
    (cfg_dir / "config.toml").write_text("[appearance]\ncorners = false\n")

    def wait_for(fn, label, timeout=30.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            val = fn()
            if val:
                return val
            time.sleep(0.1)
        raise AssertionError(f"Timed out waiting for: {label}")

    procs = []

    def stop_all():
        for p in reversed(procs):
            if p.poll() is None:
                try:
                    p.terminate()
                    p.wait(timeout=5)
                except Exception:
                    try:
                        p.kill()
                        p.wait(timeout=3)
                    except Exception:
                        pass

    try:
        bus_conf = RT / "bus.conf"
        bus_conf.write_text(
            f'<busconfig><type>session</type><listen>unix:path={RT}/bus</listen>'
            '<auth>EXTERNAL</auth><policy context="default">'
            '<allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/>'
            '</policy></busconfig>'
        )
        procs.append(subprocess.Popen(
            ["dbus-daemon", "--nofork", f"--config-file={bus_conf}"],
            env=env, stdout=open(OUT / "bus.log", "w"), stderr=subprocess.STDOUT,
        ))
        wait_for(lambda: (RT / "bus").is_socket(), "dbus socket")

        sway_conf = OUT / "sway.conf"
        sway_conf.write_text(
            "output HEADLESS-1 mode 1280x720@60Hz\n"
            "output HEADLESS-1 scale 1\n"
            "output HEADLESS-1 bg #406080 solid_color\n"
            "default_border none\n"
            "default_floating_border none\n"
            "seat seat0 hide_cursor 100\n"
        )
        procs.append(subprocess.Popen(
            ["sway", "-d", "-c", str(sway_conf)],
            env=env, stdout=open(OUT / "sway.log", "w"), stderr=subprocess.STDOUT,
        ))
        swaysock = wait_for(lambda: next(RT.glob("sway-ipc.*.sock"), None), "sway ipc socket")
        wl = wait_for(
            lambda: next((p for p in RT.glob("wayland-*") if not p.name.endswith(".lock")), None),
            "sway wayland socket",
        )
        env["SWAYSOCK"] = str(swaysock)
        env["WAYLAND_DISPLAY"] = wl.name

        # Confirm key protocols advertised by Sway 1.12.
        wl_info = subprocess.check_output(["wayland-info"], env=env, text=True, timeout=10)
        for iface in (
            "zwlr_layer_shell_v1",
            "ext_workspace_manager_v1",
            "zwlr_foreign_toplevel_manager_v1",
            "zwlr_screencopy_manager_v1",
            "wp_fractional_scale_manager_v1",
            "wp_viewporter",
        ):
            assert f"interface: '{iface}'" in wl_info, f"missing protocol {iface}"

        bar_log = OUT / "bar.log"
        bar = subprocess.Popen(
            [str(TOHRU / "bin/tohru-run")],
            env=env, stdout=open(bar_log, "w"), stderr=subprocess.STDOUT,
        )
        procs.append(bar)

        def say(cmd):
            return subprocess.run(
                [PLEAMAR, "--say", "tohru", cmd],
                env=env, capture_output=True, text=True, timeout=10,
            )

        def get_text(name):
            r = say(f"get {name}")
            if r.returncode != 0:
                return None
            out = r.stdout.strip()
            return None if out.startswith("?") else out

        # Wait until Lavapipe finishes compiling shaders and the bar renders its first frame.
        def main_panel():
            r = say("describe json")
            if r.returncode != 0 or not r.stdout.strip().startswith("["):
                return None
            parts = json.loads(r.stdout)
            return next((p for p in parts if p.get("name") == "main"), None)

        panel = wait_for(main_panel, "bar main layer surface mapped and responding", timeout=45.0)
        assert panel["size"] == [1280, 42], f"unexpected bar size: {panel['size']}"
        assert panel["scale"] == 1.0, f"unexpected bar scale: {panel['scale']}"
        node_names = {n["name"] for n in panel["nodes"]}
        for required in ("bar_clock_hit", "workspace_cell.1", "root_network", "sound_button", "bar_bell_hit"):
            assert required in node_names, f"missing zone {required} in {sorted(node_names)}"

        # 1. CLI status check (`tohru status --json`).
        status_raw = subprocess.check_output(
            [str(TOHRU / "bin/tohru"), "status", "--json"],
            env=env, text=True, timeout=10,
        )
        status = json.loads(status_raw)
        assert status["schema"] == 7, status
        assert get_text("bar_outputs") == "HEADLESS-1", get_text("bar_outputs")
        assert get_text("ws_available") == "true", get_text("ws_available")
        assert get_text("ws_error") == "", get_text("ws_error")

        # 2. Spawn a real XDG toplevel window: verify bar_title + 31 px exclusive zone.
        win_plm = OUT / "window.plm"
        win_plm.write_text(
            'scene TohruCiWindow {\n'
            '  surface { kind: window; size: 480, 200; title: "Tohru CI Window"; app_id: "tohru.ci.window" }\n'
            '  box { from: 0, 0; size: 480, 200; color: #2b3a4a }\n'
            '}\n'
        )
        win1 = subprocess.Popen(
            [PLEAMAR, "--scene", str(win_plm), "--no-hud", "--stall", "0"],
            env=env, stdout=open(OUT / "win1.log", "w"), stderr=subprocess.STDOUT,
        )
        procs.append(win1)
        wait_for(lambda: get_text("bar_title") == "Tohru CI Window", "bar_title update")

        def find_node(tree, title):
            if tree.get("name") == title:
                return tree
            for child in tree.get("nodes", []) + tree.get("floating_nodes", []):
                found = find_node(child, title)
                if found:
                    return found
            return None

        tree = json.loads(subprocess.check_output(["swaymsg", "-t", "get_tree", "-r"], env=env, text=True))
        win_node = find_node(tree, "Tohru CI Window")
        assert win_node is not None, "window missing from sway tree"
        rect = win_node["rect"]
        assert rect["y"] == 31, f"expected exclusive zone y=31, got {rect}"
        assert rect["height"] == 720 - 31, f"expected height 689 below bar, got {rect}"

        # 3. Screenshot on workspace 1 with the active window visible.
        png_path = OUT / "sway-bar.png"
        # Let one more frame settle before capture.
        time.sleep(0.4)
        subprocess.run(["grim", "-o", "HEADLESS-1", str(png_path)], env=env, check=True, timeout=10)
        with Image.open(png_path) as img:
            assert img.size == (1280, 720), f"unexpected screenshot dimensions: {img.size}"
            rgb = img.convert("RGB")
            px = rgb.load()
            bg = (0x40, 0x60, 0x80)
            # Left margin outside the 100 px bar inset should remain desktop background.
            margin_match = sum(
                1 for y in range(2, 28) for x in range(10, 85)
                if all(abs(px[x, y][c] - bg[c]) <= 2 for c in range(3))
            )
            assert margin_match > 1800, f"bar leaked into left margin ({margin_match})"
            # Bar pill region should differ from desktop background (translucent dark fill
            # plus hairline, workspace ribbon, title, icons and clock).
            bar_changed = sum(
                1 for y in range(2, 31) for x in range(110, 1170)
                if any(abs(px[x, y][c] - bg[c]) > 4 for c in range(3))
            )
            bar_bright = sum(
                1 for y in range(2, 31) for x in range(110, 1170)
                if max(px[x, y]) > 180
            )
            assert bar_changed > 20000, f"bar pill did not render ({bar_changed} changed px)"
            assert bar_bright > 250, f"bar text/icons/hairline missing ({bar_bright} bright px)"

        # 4. Workspace switching reaction (compositor -> bar via ext-workspace-v1).
        subprocess.run(["swaymsg", "workspace", "2"], env=env, check=True, timeout=10)
        win2_plm = OUT / "window2.plm"
        win2_plm.write_text(
            'scene TohruCiWindow2 {\n'
            '  surface { kind: window; size: 480, 200; title: "Workspace Two Window"; app_id: "tohru.ci.window2" }\n'
            '  box { from: 0, 0; size: 480, 200; color: #3a2b4a }\n'
            '}\n'
        )
        win2 = subprocess.Popen(
            [PLEAMAR, "--scene", str(win2_plm), "--no-hud", "--stall", "0"],
            env=env, stdout=open(OUT / "win2.log", "w"), stderr=subprocess.STDOUT,
        )
        procs.append(win2)
        wait_for(lambda: get_text("bar_title") == "Workspace Two Window", "workspace 2 window title")
        wait_for(lambda: get_text("ws_advertised.2") == "true", "workspace 2 advertised")

        # Note: emitting workspace_select currently hits a real permission bug in shell/bar.plm
        # (declares "workspaces" read-only, missing "workspaces.focus"). Record ws_error and
        # switch back via swaymsg until that fix lands.
        r = say("emit workspace_select 1")
        assert r.returncode == 0, r.stderr
        time.sleep(0.3)
        ws_select_error = get_text("ws_error") or ""

        subprocess.run(["swaymsg", "workspace", "1"], env=env, check=True, timeout=10)
        wait_for(lambda: get_text("bar_title") == "Tohru CI Window", "bar_title restored on workspace 1")

        # 5. Clean shutdown.
        win2.terminate()
        win1.terminate()
        win2.wait(timeout=5)
        win1.wait(timeout=5)
        assert say("quit").returncode == 0
        rc = bar.wait(timeout=15)
        assert rc == 0, f"bar exited with {rc}"

        summary = {
            "compositor": "sway-1.12",
            "renderer": "pixman + mesa-lavapipe",
            "bar_outputs": "HEADLESS-1",
            "bar_size": panel["size"],
            "exclusive_zone_y": rect["y"],
            "bar_changed_px": bar_changed,
            "bar_bright_px": bar_bright,
            "workspace_select_error": ws_select_error,
            "result": "passed",
        }
        (OUT / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps(summary, indent=2), flush=True)
    finally:
        stop_all()
  '';

  westonProbeScript = pkgs.writeText "tohru-weston-probe.py" ''
    import json
    import os
    from pathlib import Path
    import subprocess
    import time

    TOHRU = Path("${tohru}")
    LVP_ICD = "${lvpIcd}"
    OUT = Path("/tmp/tohru-weston-out")
    OUT.mkdir(parents=True, exist_ok=True)
    RT = Path("/tmp/tohru-weston-rt")
    RT.mkdir(mode=0o700, parents=True, exist_ok=True)
    for sub in ("home", "config", "cache", "data", "state"):
        (OUT / sub).mkdir(parents=True, exist_ok=True)
    (RT / "ipc").mkdir(mode=0o700, parents=True, exist_ok=True)

    env = {
        "PATH": f"{TOHRU / 'bin'}:{os.environ['PATH']}",
        "HOME": str(OUT / "home"),
        "XDG_RUNTIME_DIR": str(RT),
        "XDG_CONFIG_HOME": str(OUT / "config"),
        "XDG_CACHE_HOME": str(OUT / "cache"),
        "XDG_DATA_HOME": str(OUT / "data"),
        "XDG_STATE_HOME": str(OUT / "state"),
        "PLEAMAR_SOCKETS": str(RT / "ipc"),
        "DBUS_SESSION_BUS_ADDRESS": f"unix:path={RT}/bus",
        "DBUS_SYSTEM_BUS_ADDRESS": f"unix:path={RT}/bus",
        "PULSE_SERVER": f"unix:{RT}/no-pulse",
        "VK_DRIVER_FILES": LVP_ICD,
        "VK_ICD_FILENAMES": LVP_ICD,
        "LIBGL_ALWAYS_SOFTWARE": "1",
        "GALLIUM_DRIVER": "llvmpipe",
        "WGPU_BACKEND": "vulkan",
        "PLEAMAR_BACKEND": "vulkan",
        "TOHRU_SHELL": "1",
    }

    procs = []
    try:
        weston = subprocess.Popen(
            [
                "weston", "--backend=headless", "--renderer=pixman",
                "--width=1280", "--height=720", "--socket=wayland-weston", "--no-config",
            ],
            env=env, stdout=open(OUT / "weston.log", "w"), stderr=subprocess.STDOUT,
        )
        procs.append(weston)
        deadline = time.monotonic() + 15.0
        while time.monotonic() < deadline and not (RT / "wayland-weston").is_socket():
            time.sleep(0.05)
        assert (RT / "wayland-weston").is_socket(), "weston socket did not appear"
        env["WAYLAND_DISPLAY"] = "wayland-weston"

        wl_info = subprocess.check_output(["wayland-info"], env=env, text=True, timeout=10)
        assert "interface: 'zwlr_layer_shell_v1'" not in wl_info, "expected weston without layer-shell"

        result = subprocess.run(
            [str(TOHRU / "bin/tohru-run")],
            env=env, capture_output=True, text=True, timeout=15,
        )
        combined = result.stdout + "\n" + result.stderr
        # Today Pleamar panics (exit 101) via `.expect("the compositor has no layer-shell")`.
        # Once the clean-exit patch lands, tighten this to `result.returncode == 1` and
        # `"panicked" not in combined`.
        assert result.returncode != 0, "tohru-run unexpectedly succeeded without layer-shell"
        assert "no layer-shell" in combined, f"missing clear layer-shell diagnostic:\n{combined}"

        summary = {
            "compositor": "weston-16.0.0",
            "layer_shell_present": False,
            "exit_code": result.returncode,
            "panicked": "panicked" in combined,
            "result": "passed",
        }
        (OUT / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps(summary, indent=2), flush=True)
    finally:
        for p in reversed(procs):
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(timeout=5)
                except Exception:
                    p.kill()
  '';
in
{
  compositor-sway = pkgs.testers.runNixOSTest {
    name = "tohru-compositor-sway";
    nodes.machine = commonMachine // {
      environment.systemPackages = [
        tohru
        pkgs.sway
        pkgs.grim
        pkgs.wayland-utils
        pkgs.dbus
        pythonWithPillow
      ];
    };
    testScript = ''
      machine.wait_for_unit("multi-user.target")
      machine.succeed("su - alice -c '${pythonWithPillow}/bin/python3 ${swayProbeScript}'")
      machine.copy_from_machine("/tmp/tohru-sway-out/sway-bar.png", "")
      machine.copy_from_machine("/tmp/tohru-sway-out/summary.json", "")
    '';
  };

  compositor-weston = pkgs.testers.runNixOSTest {
    name = "tohru-compositor-weston";
    nodes.machine = commonMachine // {
      environment.systemPackages = [
        tohru
        pkgs.weston
        pkgs.wayland-utils
        pkgs.python3
      ];
    };
    testScript = ''
      machine.wait_for_unit("multi-user.target")
      machine.succeed("su - alice -c '${pkgs.python3}/bin/python3 ${westonProbeScript}'")
      machine.copy_from_machine("/tmp/tohru-weston-out/summary.json", "")
    '';
  };
}
