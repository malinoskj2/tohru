#!/usr/bin/env python3
"""CPU-only proof that the actual private runner overrides hostile transports."""
import importlib.util
import sys
sys.dont_write_bytecode = True
from pathlib import Path
import unittest
import tempfile
import subprocess

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_settings", ROOT / "tests/native-settings.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class Isolation(unittest.TestCase):
    def test_hostile_inherited_transport_is_replaced_for_all_children(self):
        private = ROOT / ".tmp" / "cpu-only-isolation" / "run"
        inherited = {"PLEAMAR_SOCKETS": "/host/session/ipc", "XDG_RUNTIME_DIR": "/host/run",
                     "PULSE_SERVER": "tcp:host-session", "PULSE_RUNTIME_PATH": "/host/pulse", "PULSE_STATE_PATH": "/host/state", "PULSE_CLIENTCONFIG": "/host/client.conf", "WAYLAND_DISPLAY": "host-display",
                     "SWAYSOCK": "/host/sway", "DISPLAY": ":99",
                     "HYPRLAND_INSTANCE_SIGNATURE": "host-hyprland", "WAYLAND_SOCKET": "9",
                     "CUA_INJECT_SOCKET": "/host/input", "TOHRU_PRIVATE_TEST_BUS": "1",
                     "TOHRU_B_PRIVATE_BUS": "1", "DBUS_SESSION_BUS_ADDRESS": "unix:path=/host/bus"}
        environment = runner.isolated_transport(inherited, private)
        self.assertEqual(environment["PLEAMAR_SOCKETS"], str(private / "ipc"))
        self.assertTrue(Path(environment["PLEAMAR_SOCKETS"]).is_relative_to(private))
        self.assertEqual(environment["XDG_RUNTIME_DIR"], str(private))
        self.assertEqual(environment["PULSE_SERVER"], "unix:" + str(private / "no-audio-socket"))
        self.assertEqual(environment["PULSE_RUNTIME_PATH"], str(private / "pulse-runtime"))
        self.assertEqual(environment["PULSE_STATE_PATH"], str(private / "pulse-state"))
        self.assertNotIn("PULSE_CLIENTCONFIG", environment)
        for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "WAYLAND_SOCKET", "CUA_INJECT_SOCKET", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "TOHRU_PRIVATE_TEST_BUS", "TOHRU_B_PRIVATE_BUS"):
            self.assertNotIn(key, environment)
        self.assertEqual(inherited["PLEAMAR_SOCKETS"], "/host/session/ipc")


    def test_clean_checkout_bootstrap_creates_evidence_parent_before_roles(self):
        with tempfile.TemporaryDirectory() as path:
            root = Path(path)
            self.assertFalse((root / ".tmp").exists())
            evidence = runner.create_evidence(root)
            self.assertTrue(evidence.is_dir())
            self.assertEqual(evidence.parent, root / ".tmp")

    def test_endpoint_containment_and_symlinks_rejected(self):
        with tempfile.TemporaryDirectory() as path:
            private = Path(path)
            private.chmod(0o700)
            env = runner.isolated_transport({}, private)
            runner.assert_private_transport(env, private)
            bad = {**env, "DBUS_SESSION_BUS_ADDRESS": "unix:path=/host/bus"}
            with self.assertRaises(AssertionError):
                runner.assert_private_transport(bad, private)
            (private / "ipc").symlink_to("/host/session")
            with self.assertRaises(AssertionError):
                runner.assert_private_transport(env, private)

    def test_actual_view_counter_detects_duplicate_roles_in_floating_nodes(self):
        app = "dev.tohru.Settings"
        tree = {"nodes": [{"app_id": app, "id": 1}, {"app_id": "other"}],
                "floating_nodes": [{"nodes": [{"app_id": app, "id": 2}]}]}
        self.assertEqual({node["id"] for node in runner.windows_for_app(tree, app)}, {1, 2})
        self.assertEqual(runner.windows_for_app({"nodes": []}, app), [])

    def test_search_focus_cannot_pass_when_keyboard_focus_is_broken(self):
        def native(command):
            self.assertTrue(command.startswith("get ") or command == "focus runner_focus_canary", "IPC search mutation bypasses focus evidence")
            return "" if command == "get settings_search" else "1"
        with self.assertRaisesRegex(AssertionError, "focused native search"):
            runner.focused_search_submit(native, lambda *keys: None, "mooncoil", timeout=0.001)

    def test_search_focus_types_before_submit_with_no_ipc_mutation(self):
        state = {"focus": "search", "query": "", "submitted": False}
        def keyboard(*keys):
            if keys == ("-M", "ctrl", "-k", "f", "-m", "ctrl", "mooncoil"): state["focus"] = "search"; state["query"] = keys[-1]
            elif keys == ("-k", "Return"): state["submitted"] = True
        def native(command):
            if command == "focus runner_focus_canary": state["focus"] = "canary"; return ""
            self.assertTrue(command.startswith("get "))
            return state["query"] if command == "get settings_search" else "canary" if command == "get runner_focus_canary" else "1"
        runner.focused_search_submit(native, keyboard, "mooncoil")
        self.assertTrue(state["submitted"])

    def test_native_first_field_autofocus_cannot_hide_broken_ctrl_f(self):
        state = {"focus": "search", "query": "", "canary": "canary"}
        def native(command):
            if command == "focus runner_focus_canary": state["focus"] = "canary"; return ""
            if command == "get settings_search": return state["query"]
            if command == "get runner_focus_canary": return state["canary"]
            self.assertEqual(command, "get settings_rows.count")
            return "1"
        def keyboard(*keys):
            # Deliberately ignore Ctrl+F; native first-field focus started on search.
            if keys[-1] == "mooncoil":
                if state["focus"] == "search": state["query"] = keys[-1]
                else: state["canary"] += keys[-1]
        with self.assertRaisesRegex(AssertionError, "focused native search"):
            runner.focused_search_submit(native, keyboard, "mooncoil", timeout=0.001)

    def test_intentional_quit_rejects_crash_status(self):
        class Process:
            def __init__(self, status): self.status = status
            def wait(self, timeout): return self.status
        runner.clean_exit(Process(0), "test")
        for status in (-11, 1):
            evidence = []
            with self.assertRaisesRegex(AssertionError, "exited"):
                runner.clean_exit(Process(status), "test", evidence)
            self.assertEqual(evidence, [{"process_exit": "test", "status": status}])
        class Timeout:
            def wait(self, timeout): raise subprocess.TimeoutExpired("native", timeout)
        with self.assertRaises(subprocess.TimeoutExpired): runner.clean_exit(Timeout(), "test")

    def test_focus_probe_only_changes_cloned_settings_view(self):
        with tempfile.TemporaryDirectory() as path:
            scene = Path(path)
            (scene / "settings").mkdir()
            view = scene / "settings/window.plm"
            view.write_text("part SettingsWindow {\n}\n")
            runner.install_focus_probe(scene)
            self.assertIn("input runner_focus_canary", view.read_text())
            self.assertIn("runner_focus_probe_enabled", view.read_text())

    def test_diagnostics_and_one_owner_failure_cannot_skip_other_cleanup(self):
        events = []
        class Process:
            def __init__(self, name, fail=False): self.name, self.fail = name, fail
            def poll(self): return None
            def terminate(self):
                events.append(self.name + " terminate")
                if self.fail: raise OSError("terminate failed")
            def kill(self): events.append(self.name + " kill")
            def wait(self, timeout): events.append(self.name + " reap")
        class Handle:
            def close(self): events.append("handle closed")
        def failed_write(): raise OSError("disk full")
        errors = runner.finish_private_run(
            [Process("app", True), Process("pointer"), Process("sway"), Process("pulse"), Process("bus")], [Handle()], failed_write)
        self.assertEqual(len(errors), 2)
        for owner in ("app", "pointer", "sway", "pulse", "bus"):
            self.assertIn(owner + " reap", events)
        self.assertIn("app kill", events)
        self.assertIn("handle closed", events)


if __name__ == "__main__":
    unittest.main()
