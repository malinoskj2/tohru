#!/usr/bin/env python3
"""Check the actual Bash frontend's argument boundary without a desktop session."""
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
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
        for panel in ['network', 'media', 'bluetooth', 'about', 'info']:
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
            (("panel", "open", "wallpaper"), ["--say", "tohru", "emit wall_open_request"]),
            (("panel", "toggle", "wallpaper"), ["--say", "tohru", "emit wall_toggle"]),
            (("panel", "close"), ["--say", "tohru", "emit panel_close"]),
            (("audio", "volume", "up"), ["--say", "tohru", "emit audio_step 1"]),
            (("audio", "volume", "down"), ["--say", "tohru", "emit audio_step -1"]),
            (("audio", "mute", "toggle"), ["--say", "tohru", "emit audio_mute"]),
            (("audio", "status", "--json"), ["--say", "tohru", "get audio_status"]),
            (("status", "--json"), ["--say", "tohru", "get settings_status"]),
            (("config", "reload"), ["--say", "tohru", "emit config_reload"]),
            (("notifications", "seen"), ["--say", "tohru-notifications", "emit notices_seen 0"]),
            (("notifications", "dnd", "off"), ["--say", "tohru-notifications", "emit notices_dnd 0"]),
            (("notifications", "dnd", "on"), ["--say", "tohru-notifications", "emit notices_dnd 1"]),
            (("notifications", "dnd", "toggle"), ["--say", "tohru-notifications", "emit notices_dnd 2"]),
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

    def test_settings_open_takes_the_bars_managed_file(self):
        managed = self.path / "managed.toml"
        managed.write_text("[bar]\n")
        runtime = self.path / "answering runtime"
        runtime.write_text(
            "#!/usr/bin/env bash\n"
            "[[ $2 == tohru-settings ]] && exit 1\n"
            f"[[ $3 == 'get managed_config' ]] && printf '%s\\n' '{managed}'\n"
        )
        runtime.chmod(0o755)
        seen = self.path / "seen"
        starter = self.path / "settings run"
        starter.write_text(f"#!/usr/bin/env bash\nprintf '%s' \"$TOHRU_MANAGED_CONFIG\" > '{seen}'\n")
        starter.chmod(0o755)
        for given, expected in [("", str(managed)), ("/declared.toml", "/declared.toml")]:
            with self.subTest(given=given):
                result = self.invoke("settings", "open", TOHRU_RUNTIME=str(runtime), TOHRU_SETTINGS_RUN=str(starter), TOHRU_MANAGED_CONFIG=given)
                self.assertEqual(result.returncode, 0, result.stderr)
                for _ in range(100):
                    if seen.exists():
                        break
                    subprocess.run(["sleep", "0.05"])
                self.assertEqual(seen.read_text(), expected)
                seen.unlink()

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
            ("notifications",), ("notifications", "seen", "extra"), ("notifications", "dnd"),
            ("notifications", "dnd", "maybe"), ("notifications", "dnd", "on", "extra"),
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


    def test_lock_style_route_is_closed_before_ipc(self):
        styles = ["still-glass", "ribbon-key", "flooded-monolith", "drifting-frost-veil", "unfolding-glass-capsule", "edge-glass-drawer", "horizon-meniscus", "refractive-typographic-lens", "corner-anchor", "bar-drop-curtain"]
        for index, style in enumerate(styles):
            result = self.invoke("config", "set", "lock.style", style)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), ["--say", "tohru", f"emit setting_lock_style {index}"])
        for invalid in ("Flooded-Monolith", "flooded_monolith", "plain", "0", "", "still-glass;quit", "still-glass\nemit quit"):
            with self.subTest(value=invalid):
                result = self.invoke("config", "set", "lock.style", invalid)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, "")

    def test_lock_runs_the_locker_and_takes_no_arguments(self):
        seen = self.path / "locked"
        locker = self.path / "locker"
        locker.write_text(f"#!/usr/bin/env bash\nprintf '%s' \"$#\" > '{seen}'\n")
        locker.chmod(0o755)
        self.assertEqual(self.invoke("lock", TOHRU_LOCK_RUN=str(locker)).returncode, 0)
        self.assertEqual(seen.read_text(), "0")
        seen.unlink()
        self.assertEqual(self.invoke("lock", "now", TOHRU_LOCK_RUN=str(locker)).returncode, 2)
        self.assertFalse(seen.exists())


class LockWrapper(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="tohru-lock-")
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name)
        tools = self.path / "bin"
        tools.mkdir()
        # No busctl: the test never touches logind's LockedHint.
        for tool in ("bash", "flock", "sleep", "dirname", "readlink", "python3", "env", "cat", "pgrep", "id"):
            found = shutil.which(tool)
            if found:
                (tools / tool).symlink_to(found)
        self.runs = self.path / "runs"
        self.runtime = self.path / "runtime"
        # Each launch logs its scene and takes the next exit status from EXITS; `--check` fails for CHECK_FAILS.
        self.runtime.write_text(
            "#!/usr/bin/env python3\nimport os,sys,signal\n"
            "a=sys.argv[1:]\n"
            "if a[0]=='--check': sys.exit(1 if a[1]==os.environ.get('CHECK_FAILS') else 0)\n"
            f"runs=open('{self.runs}','a+'); runs.seek(0); n=len(runs.readlines())\n"
            "runs.write(a[a.index('--scene')+1]+' '+os.environ.get('TOHRU_LOCKER','')+'\\n'); runs.close()\n"
            "code=os.environ['EXITS'].split(',')[n]\n"
            "if code=='segv': os.kill(os.getpid(), signal.SIGKILL)\n"
            "sys.exit(int(code))\n"
        )
        self.runtime.chmod(0o755)
        (self.path / "xdg").mkdir()
        self.env = {"PATH": str(tools), "HOME": str(self.path), "XDG_RUNTIME_DIR": str(self.path / "xdg"), "TOHRU_RUNTIME": str(self.runtime), "TOHRU_LOCK_SCENE": "styled.plm", "TOHRU_LOCK_PLAIN_SCENE": "plain.plm"}

    def lock(self, **env):
        result = subprocess.run([str(ROOT / "bin/tohru-lock")], env={**self.env, **env}, text=True, capture_output=True, timeout=30)
        return result, self.runs.read_text().split("\n")[:-1] if self.runs.exists() else []

    def test_unlocking_ends_the_locker(self):
        result, runs = self.lock(EXITS="0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(runs, ["styled.plm 1"])

    def test_a_style_that_does_not_load_locks_with_the_plain_one(self):
        result, runs = self.lock(EXITS="0", CHECK_FAILS="styled.plm")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(runs, ["plain.plm 1"])
        self.assertIn("using the plain style", result.stderr)

    def test_a_crash_or_a_quit_while_locked_relaunches_the_plain_style(self):
        for exits in ("segv,0", "75,0", "1,segv,0"):
            with self.subTest(exits=exits):
                self.runs.unlink(missing_ok=True)
                result, runs = self.lock(EXITS=exits)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(runs, ["styled.plm 1"] + ["plain.plm 1"] * (len(exits.split(",")) - 1))
                self.assertIn("starting the plain style", result.stderr)

    def test_a_refusal_from_the_compositor_is_not_retried(self):
        result, runs = self.lock(EXITS="3")
        self.assertEqual(result.returncode, 3)
        self.assertEqual(runs, ["styled.plm 1"])

    def test_one_locker_at_a_time(self):
        if not (self.path / "bin" / "flock").exists():
            self.skipTest("flock (util-linux) is not on PATH")
        with open(self.path / "xdg" / "tohru-lock.lock", "w") as held:
            fcntl.flock(held, fcntl.LOCK_EX)
            result, runs = self.lock(EXITS="0")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(runs, [])

    def test_running_tells_whether_a_locker_runs_without_starting_one(self):
        if not (self.path / "bin" / "pgrep").exists():
            self.skipTest("pgrep (procps) is not on PATH")
        running = lambda: subprocess.run([str(ROOT / "bin/tohru-lock"), "--running"], env=self.env, timeout=10).returncode
        self.assertEqual(running(), 1)
        locker = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)", "--scene", "x", "--answer-as", "tohru-lock"])
        self.addCleanup(locker.kill)
        self.assertEqual(running(), 0)
        locker.kill()
        locker.wait()
        self.assertEqual(running(), 1)
        self.assertFalse(self.runs.exists())

    def test_without_flock_it_locks_anyway(self):
        (self.path / "bin" / "flock").unlink(missing_ok=True)
        result, runs = self.lock(EXITS="0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(runs, ["styled.plm 1"])

if __name__ == "__main__":
    unittest.main()
