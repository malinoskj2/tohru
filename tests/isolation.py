#!/usr/bin/env python3
"""CPU-only proof that the actual private runner overrides hostile transports."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_settings", ROOT / "tests/native-settings.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class Isolation(unittest.TestCase):
    def test_hostile_inherited_transport_is_replaced_for_all_children(self):
        private = ROOT / ".tmp" / "cpu-only-isolation" / "run"
        inherited = {"PLEAMAR_SOCKETS": "/host/session/ipc", "XDG_RUNTIME_DIR": "/host/run",
                     "PULSE_SERVER": "tcp:host-session", "WAYLAND_DISPLAY": "host-display",
                     "SWAYSOCK": "/host/sway", "DISPLAY": ":99",
                     "HYPRLAND_INSTANCE_SIGNATURE": "host-hyprland"}
        environment = runner.isolated_transport(inherited, private)
        self.assertEqual(environment["PLEAMAR_SOCKETS"], str(private / "pleamar-sockets"))
        self.assertTrue(Path(environment["PLEAMAR_SOCKETS"]).is_relative_to(private))
        self.assertEqual(environment["XDG_RUNTIME_DIR"], str(private))
        self.assertEqual(environment["PULSE_SERVER"], "unix:" + str(private / "no-audio-socket"))
        for key in ("WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE"):
            self.assertNotIn(key, environment)
        self.assertEqual(inherited["PLEAMAR_SOCKETS"], "/host/session/ipc")


if __name__ == "__main__":
    unittest.main()
