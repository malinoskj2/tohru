#!/usr/bin/env python3
"""CPU-only proof that the actual private runner overrides hostile transports."""
import importlib.util
import sys
sys.dont_write_bytecode = True
from pathlib import Path
import unittest
import tempfile

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
        app = "dev.tohru.WindowLifecycle"
        tree = {"nodes": [{"app_id": app, "id": 1}, {"app_id": "other"}],
                "floating_nodes": [{"nodes": [{"app_id": app, "id": 2}]}]}
        self.assertEqual({node["id"] for node in runner.windows_for_app(tree, app)}, {1, 2})
        self.assertEqual(runner.windows_for_app({"nodes": []}, app), [])

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
