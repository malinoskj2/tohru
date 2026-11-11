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

    def test_palette_route_is_closed_before_ipc(self):
        modes=['mocha','macchiato','rose_pine','nord','gruvbox','tokyo_night','dracula','everforest']
        for index,mode in enumerate(modes):
            result=self.invoke('config','set','appearance.palette',mode)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout),['--say','tohru','emit setting_palette '+str(index)])
        for invalid in ['Mocha','rose-pine','nord ','0','../other','nord;quit','']:
            result=self.invoke('config','set','appearance.palette',invalid)
            self.assertEqual(result.returncode,2)
            self.assertEqual(result.stdout,'')

    def test_session_menu_routes_never_start_a_role(self):
        for action in ['open','close','toggle']:
            result=self.invoke('session',action)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout),['--say','tohru-session','emit menu_'+action])
        self.assertEqual(self.invoke('session','poweroff').returncode,2)
        result=self.invoke('routing-error')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(json.loads(result.stdout),['--say','tohru','get routing_error'])

    def test_launcher_routes_never_start_a_role(self):
        for action in ['toggle','open','close']:
            result=self.invoke('launcher',action)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout),['--say','tohru-launcher','emit launcher_'+action])
        for args in [('launcher',),('launcher','show'),('launcher','open','extra'),('launcher','open;quit'),('launcher','toggle\nemit quit')]:
            with self.subTest(args=args):
                result=self.invoke(*args)
                self.assertEqual(result.returncode,2)
                self.assertEqual(result.stdout,'')
                self.assertIn('Usage: tohru launcher toggle|open|close',result.stderr)
        self.assertEqual(self.invoke('launcher','toggle',NATIVE_EXIT='1').returncode,1)

    def test_native_input_app_and_output_commands_reject_ambiguous_or_invalid_ids(self):
        for arguments, event in [
            (('audio','input','volume','up'),'audio_input_up'),
            (('audio','input','volume','down'),'audio_input_down'),
            (('audio','input','mute','toggle'),'audio_input_mute'),
            (('audio','output','select','0'),'audio_output_select 0'),
            (('audio','output','select','4294967295'),'audio_output_select 4294967295'),
            (('audio','app','7','volume','up'),'audio_app_up 7'),
            (('audio','app','7','volume','down'),'audio_app_down 7'),
            (('audio','app','7','mute','toggle'),'audio_app_mute 7'),
        ]:
            result=self.invoke(*arguments)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout),['--say','tohru','emit '+event])
        result=self.invoke('audio','input','select','0')
        self.assertEqual(result.returncode,2)
        self.assertIn('unavailable',result.stderr)
        self.assertEqual(result.stdout,'')
        for value in ['-1','01','1.5','4294967296','1e2','99999999999999999999','0\nemit quit']:
            for arguments in [('audio','output','select',value),('audio','app',value,'mute','toggle')]:
                result=self.invoke(*arguments)
                self.assertEqual(result.returncode,2)
                self.assertEqual(result.stdout,'')


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
            (("settings", "close"), ["--say", "tohru-settings", "quit"]),
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
            (("config", "set", "bar.info_button", "true"), ["--say", "tohru", "emit setting_info_button 1"]),
            (("config", "set", "bar.about_button", "false"), ["--say", "tohru", "emit setting_about_button 0"]),
            (("config", "set", "appearance.corners", "false"), ["--say", "tohru", "emit setting_corners 0"]),
            (("config", "set", "appearance.corner_size", "1"), ["--say", "tohru", "emit setting_corner_size 1"]),
            (("config", "set", "appearance.corner_size", "100"), ["--say", "tohru", "emit setting_corner_size 100"]),
        ]:
            with self.subTest(args=args):
                result = self.invoke(*args)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), expected)

    def test_settings_open_raises_or_starts_its_own_process(self):
        result = self.invoke("settings", "open")
        self.assertEqual(result.returncode, 0, result.stderr)
        launched = self.path / "launched"
        starter = self.path / "settings run"
        starter.write_text(f"#!/usr/bin/env bash\nprintf '%s' \"$*\" > '{launched}'\n")
        starter.chmod(0o755)
        result = self.invoke("settings", "open", NATIVE_EXIT="1", TOHRU_SETTINGS_RUN=str(starter))
        self.assertEqual(result.returncode, 0, result.stderr)
        for _ in range(100):
            if launched.exists():
                break
            subprocess.run(["sleep", "0.05"])
        self.assertEqual(launched.read_text(), "")
        launched.unlink()
        self.invoke("settings", "open", TOHRU_SETTINGS_RUN=str(starter))
        subprocess.run(["sleep", "0.3"])
        self.assertFalse(launched.exists())

    def test_output_is_literal_and_cannot_inject_commands(self):
        marker = self.path / "injected"
        value = f"$(touch {marker}) ; emit quit"
        result = self.invoke("config", "set", "bar.preferred_output", value)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), ["--say", "tohru", f"text output_request {value}", "emit setting_output"])
        self.assertFalse(marker.exists())
        self.assertEqual(json.loads(self.invoke("config", "set", "bar.preferred_output", "").stdout), ["--say", "tohru", "text output_request ", "emit setting_output"])
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
            ("screenshot", "region"), ("wallpaper", "open"),
            ("config", "set", "wallpaper.enabled", "true"), ("config", "set", "capture.save", "false"),
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
