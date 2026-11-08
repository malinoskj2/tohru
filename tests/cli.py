#!/usr/bin/env python3
"""Check the actual Bash frontend's argument boundary without a desktop session."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class Cli(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="tohru-cli-")
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name)
        self.runtime = self.path / "native runtime"
        self.runtime.write_text("#!/usr/bin/env python3\nimport json,os,sys\nprint(json.dumps(sys.argv[1:]))\nsys.exit(int(os.environ.get('NATIVE_EXIT', '0')))\n")
        self.runtime.chmod(0o755)
        self.env = {**os.environ, "TOHRU_RUNTIME": str(self.runtime)}

    def invoke(self, *args, **env):
        return subprocess.run([str(ROOT / "bin/tohru"), *args], env={**self.env, **env}, text=True, capture_output=True)

    def test_service_panel_routes_are_fixed_native_events(self):
        for panel in ['network', 'bluetooth', 'about', 'info']:
            for action in ['open', 'toggle']:
                with self.subTest(panel=panel, action=action):
                    result = self.invoke('panel', action, panel)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(json.loads(result.stdout), ['--say', 'tohru', 'emit '+panel+'_'+action])
        for panel in ['network;quit', 'bluetooth\nemit quit', 'unknown']:
            self.assertEqual(self.invoke('panel', 'open', panel).returncode, 2)

    def test_native_arguments(self):
        for args, expected in [
            (("settings", "open"), ["--say", "tohru", "emit settings_open_request"]),
            (("settings", "close"), ["--say", "tohru", "emit settings_close"]),
            (("panel", "open", "sound"), ["--say", "tohru", "emit sound_open_request"]),
            (("panel", "toggle", "sound"), ["--say", "tohru", "emit sound_toggle"]),
            (("panel", "open", "calendar"), ["--say", "tohru", "emit calendar_open"]),
            (("panel", "toggle", "calendar"), ["--say", "tohru", "emit calendar_toggle"]),
            (("panel", "close"), ["--say", "tohru", "emit panel_close"]),
            (("audio", "volume", "up"), ["--say", "tohru", "emit audio_step 1"]),
            (("audio", "volume", "down"), ["--say", "tohru", "emit audio_step -1"]),
            (("audio", "mute", "toggle"), ["--say", "tohru", "emit audio_mute"]),
            (("audio", "status", "--json"), ["--say", "tohru", "get audio_status"]),
            (("status", "--json"), ["--say", "tohru", "get settings_status"]),
            (("config", "reload"), ["--say", "tohru", "emit config_reload"]),
            (("config", "set", "bar.mooncoil", "true"), ["--say", "tohru", "emit setting_mooncoil 1"]),
            (("config", "set", "appearance.corners", "false"), ["--say", "tohru", "emit setting_corners 0"]),
            (("config", "set", "appearance.corner_size", "1"), ["--say", "tohru", "emit setting_corner_size 1"]),
            (("config", "set", "appearance.corner_size", "100"), ["--say", "tohru", "emit setting_corner_size 100"]),
        ]:
            with self.subTest(args=args):
                result = self.invoke(*args)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), expected)

    def test_deprecated_output_is_rejected_without_execution(self):
        marker = self.path / "injected"
        value = f"$(touch {marker}) ; emit quit"
        result = self.invoke("config", "set", "bar.preferred_output", value)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertIn("deprecated", result.stderr)
        self.assertFalse(marker.exists())
        self.assertEqual(self.invoke("config", "set", "bar.preferred_output", "DP-1\nemit quit").returncode, 2)

    def test_grammar_and_dropped_features_fail(self):
        for args in [
            ("settings",), ("settings", "toggle"), ("settings", "open", "extra"),
            ("audio",), ("audio", "volume", "up", "extra"),
            ("audio", "volume", "louder"), ("audio", "mute", "on"),
            ("audio", "status"), ("audio", "status", "--json", "extra"),
            ("status",), ("status", "--json", "extra"), ("config",),
            ("config", "reload", "extra"), ("config", "set", "bar.mooncoil", "yes"),
            ("config", "set", "unknown.key", "true"), ("assistant",),
            ("gaming",), ("dnd",), ("workspace", "select", "1"),
            ("settings-error", "extra"), ("help", "extra"),
        ]:
            with self.subTest(args=args):
                self.assertEqual(self.invoke(*args).returncode, 2)

    def test_native_failure_is_preserved(self):
        result = self.invoke("status", "--json", NATIVE_EXIT="1")
        self.assertEqual(result.returncode, 1)

    def test_corner_size_rejection_never_calls_native(self):
        for value in ("0", "101", "-1", "32.5", "01", "1e2", "100\nemit quit", "9999999999999999999999999"):
            with self.subTest(value=value):
                result = self.invoke("config", "set", "appearance.corner_size", value)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
