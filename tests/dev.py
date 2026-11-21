#!/usr/bin/env python3
"""Exercise the contributor-only scene override and tohru dev against recording stand-ins."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
ROLES = [
    ('tohru-run', 'TOHRU_SCENE', 'bar.plm', 'tohru'),
    ('tohru-settings-run', 'TOHRU_SETTINGS_SCENE', 'settings/settings.plm', 'tohru-settings'),
    ('tohru-session-run', 'TOHRU_SESSION_SCENE', 'session/menu.plm', 'tohru-session'),
    ('tohru-launcher-run', 'TOHRU_LAUNCHER_SCENE', 'launcher/entry.plm', 'tohru-launcher'),
    ('tohru-notifications-run', 'TOHRU_NOTIFICATIONS_SCENE', 'notifications/notifications.plm', 'tohru-notifications'),
    ('tohru-polkit-run', 'TOHRU_POLKIT_SCENE', 'polkit/polkit.plm', 'tohru-polkit'),
]
OVERRIDES = ('TOHRU_SHELL_DIR', 'TOHRU_RUNTIME_OVERRIDE')


def wait_for(predicate, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


class Base(unittest.TestCase):
    def setUp(self):
        (ROOT / '.tmp').mkdir(exist_ok=True)
        directory = tempfile.TemporaryDirectory(prefix='tohru-dev-', dir=ROOT / '.tmp')
        self.addCleanup(directory.cleanup)
        self.path = Path(directory.name)
        self.fake = self.path / 'fake-bin'
        self.fake.mkdir()
        self.env = {k: v for k, v in os.environ.items() if k not in OVERRIDES}
        self.env['PATH'] = f'{self.fake}:{self.env["PATH"]}'

    def executable(self, path, body):
        path.write_text(body)
        path.chmod(0o755)
        return path


class Wrappers(Base):
    def setUp(self):
        super().setUp()
        self.record = self.path / 'calls'
        recorder = '#!/usr/bin/env python3\nimport json,sys\nwith open(%r, "a") as log: log.write(json.dumps([%r, *sys.argv[1:]]) + "\\n")\n'
        self.packaged = self.executable(self.path / 'packaged', recorder % (str(self.record), 'packaged'))
        self.override = self.executable(self.path / 'override', recorder % (str(self.record), 'override'))
        self.executable(self.fake / 'busctl', '#!/bin/sh\nexit 1\n')
        self.env.update(TOHRU_RUNTIME=str(self.packaged), XDG_RUNTIME_DIR=str(self.path))

    def run_role(self, script, **env):
        result = subprocess.run([str(ROOT / 'bin' / script)], env={**self.env, **env}, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.record.read_text().splitlines()]
        self.record.unlink()
        return calls

    def test_roles_are_unchanged_without_the_override(self):
        for script, variable, scene, name in ROLES:
            with self.subTest(script=script):
                tail = ['--no-hud', '--stall', '0', '--answer-as', name]
                self.assertEqual(self.run_role(script, **{variable: '/packaged/scene.plm'}), [['packaged', '--scene', '/packaged/scene.plm', *tail]])
                self.assertEqual(self.run_role(script), [['packaged', '--scene', str(ROOT / 'shell' / scene), *tail]])

    def test_shell_dir_and_runtime_override_win_over_packaged_values(self):
        for script, variable, scene, name in ROLES:
            with self.subTest(script=script):
                tail = ['--no-hud', '--stall', '0', '--answer-as', name]
                self.assertEqual(self.run_role(script, **{variable: '/packaged/scene.plm', 'TOHRU_SHELL_DIR': '/dev shell'}),
                                 [['packaged', '--scene', f'/dev shell/{scene}', *tail]])
                self.assertEqual(self.run_role(script, **{variable: '/packaged/scene.plm', 'TOHRU_RUNTIME_OVERRIDE': str(self.override)}),
                                 [['override', '--scene', '/packaged/scene.plm', *tail]])

    def test_locker_and_greeter_follow_the_shell_dir(self):
        for script, kind, main, variables in [
            ('tohru-lock', 'lock', 'lock.plm', ('TOHRU_LOCK_SCENE', 'TOHRU_LOCK_PLAIN_SCENE')),
            ('tohru-greeter', 'greeter', 'greeter.plm', ('TOHRU_GREETER_SCENE', 'TOHRU_GREETER_PLAIN_SCENE')),
        ]:
            with self.subTest(script=script):
                packaged = {variables[0]: '/packaged/styled.plm', variables[1]: '/packaged/plain.plm'}
                calls = self.run_role(script, **packaged)
                self.assertEqual([call[:3] for call in calls], [['packaged', '--check', '/packaged/styled.plm'], ['packaged', '--scene', '/packaged/styled.plm']])
                calls = self.run_role(script, **packaged, TOHRU_SHELL_DIR='/dev', TOHRU_RUNTIME_OVERRIDE=str(self.override))
                self.assertEqual([call[:3] for call in calls], [['override', '--check', f'/dev/{kind}/{main}'], ['override', '--scene', f'/dev/{kind}/{main}']])


class Dev(Base):
    def setUp(self):
        super().setUp()
        self.log = self.path / 'systemctl.log'
        self.executable(self.fake / 'systemctl', f'''#!/usr/bin/env bash
printf '%s\\n' "$*" >> {self.log}
if [[ $2 == list-units ]]; then
    printf 'tohru-bar.service loaded active running Tohru bar\\n'
    printf 'tohru-launcher.service loaded active running Tohru launcher\\n'
    printf 'other.service loaded active running Other\\n'
fi
[[ $2 != restart || -z ${{SLOW_RESTART:-}} ]] || sleep "$SLOW_RESTART"
''')
        self.shell = self.path / 'shell'
        (self.shell / 'launcher').mkdir(parents=True)
        (self.shell / 'bar.plm').write_text('')

    def invoke(self, *args, **env):
        return subprocess.run([str(ROOT / 'bin/tohru'), 'dev', *args], env={**self.env, **env}, text=True, capture_output=True, timeout=10)

    def start(self, *args, **env):
        process = subprocess.Popen([str(ROOT / 'bin/tohru'), 'dev', *args], env={**self.env, **env}, text=True, start_new_session=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(lambda: process.poll() is None and os.killpg(process.pid, signal.SIGKILL))
        return process

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_help_is_its_own_and_hidden_from_the_main_usage(self):
        result = self.invoke('--help')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(result.stdout.startswith('Usage: tohru dev [COMPONENT...]'))
        self.assertNotIn('dev', subprocess.run([str(ROOT / 'bin/tohru'), 'help'], text=True, capture_output=True).stdout)

    def test_argument_errors_exit_before_touching_systemd(self):
        missing = self.path / 'missing'
        for args, message in [
            (('wallpaper',), 'unknown component wallpaper'),
            (('--frobnicate',), 'unknown option --frobnicate'),
            (('--shell',), '--shell needs a value'),
            (('--runtime',), '--runtime needs a value'),
            (('--runtime=',), '--runtime needs a value'),
            (('--shell', str(missing)), 'is not a directory'),
            (('--shell', str(self.path)), 'has no bar.plm'),
            (('--shell', str(self.shell), '--runtime', str(missing)), 'is not an executable file'),
            (('--shell', str(self.shell), 'settings'), 'settings has no systemd unit'),
        ]:
            with self.subTest(args=args):
                result = self.invoke(*args)
                self.assertEqual(result.returncode, 2)
                self.assertIn(message, result.stderr)
        self.assertEqual(self.calls(), [])

    def test_outside_a_checkout_needs_a_shell_dir(self):
        outside = tempfile.TemporaryDirectory(prefix='tohru-dev-installed-')
        self.addCleanup(outside.cleanup)
        copy = Path(outside.name)
        shutil.copy(ROOT / 'bin/tohru', copy)
        shutil.copy(ROOT / 'bin/tohru-dev', copy)
        result = subprocess.run([str(copy / 'tohru'), 'dev'], cwd=copy, env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertIn('not in a Tohru checkout; pass --shell DIR', result.stderr)

    def test_session_returns_to_packaged_scenes_on_interrupt(self):
        runtime = self.executable(self.path / 'pleamar', '#!/bin/sh\n')
        process = self.start('--runtime', str(runtime))
        self.assertTrue(wait_for(lambda: any(c.startswith('--user restart') for c in self.calls())), self.calls())
        process.send_signal(signal.SIGINT)
        self.assertEqual(process.wait(timeout=10), 130)
        self.assertEqual(self.calls(), [
            "--user list-units --state=active --plain --no-legend tohru-*.service",
            f"--user set-environment TOHRU_SHELL_DIR={ROOT / 'shell'} TOHRU_RUNTIME_OVERRIDE={runtime}",
            "--user restart tohru-bar.service tohru-launcher.service",
            "--user unset-environment TOHRU_SHELL_DIR TOHRU_RUNTIME_OVERRIDE",
            "--user restart tohru-bar.service tohru-launcher.service",
        ])

    def test_second_interrupt_does_not_abandon_the_restore(self):
        process = self.start('--shell', str(self.shell), 'session', SLOW_RESTART='1')
        self.assertTrue(wait_for(lambda: '--user restart tohru-session.service' in self.calls()))
        os.killpg(process.pid, signal.SIGINT)
        self.assertTrue(wait_for(lambda: self.calls().count('--user restart tohru-session.service') == 2))
        os.killpg(process.pid, signal.SIGINT)
        os.killpg(process.pid, signal.SIGTERM)
        self.assertEqual(process.wait(timeout=10), 130)
        self.assertEqual(self.calls(), [
            f"--user set-environment TOHRU_SHELL_DIR={self.shell}",
            "--user unset-environment TOHRU_RUNTIME_OVERRIDE",
            "--user restart tohru-session.service",
            "--user unset-environment TOHRU_SHELL_DIR TOHRU_RUNTIME_OVERRIDE",
            "--user restart tohru-session.service",
        ])

    def test_nothing_running_is_an_error(self):
        (self.fake / 'systemctl').write_text(f'#!/bin/sh\nprintf "%s\\n" "$*" >> {self.log}\n')
        result = self.invoke('--shell', str(self.shell))
        self.assertEqual(result.returncode, 2)
        self.assertIn('no Tohru units are running', result.stderr)

    @unittest.skipUnless(shutil.which('inotifywait'), 'inotifywait is not installed')
    def test_watch_restarts_only_the_component_that_changed(self):
        process = self.start('--shell', str(self.shell), '--watch')
        self.assertTrue(wait_for(lambda: self.calls().count('--user restart tohru-bar.service tohru-launcher.service') == 1))
        time.sleep(0.5)
        (self.shell / 'launcher' / 'entry.plm').write_text('changed')
        self.assertTrue(wait_for(lambda: '--user restart tohru-launcher.service' in self.calls()), self.calls())
        (self.shell / 'shared.luau').write_text('changed')
        self.assertTrue(wait_for(lambda: self.calls().count('--user restart tohru-bar.service tohru-launcher.service') == 2), self.calls())
        process.send_signal(signal.SIGTERM)
        self.assertEqual(process.wait(timeout=10), 143)
        self.assertEqual(self.calls()[-1], '--user restart tohru-bar.service tohru-launcher.service')

    @unittest.skipUnless(shutil.which('inotifywait'), 'inotifywait is not installed')
    def test_nested_runs_roles_in_the_foreground_and_reaps_them(self):
        starts = self.path / 'starts.log'
        runtime = self.executable(self.path / 'pleamar', f'''#!/usr/bin/env python3
import json,os,sys,time
with open({str(starts)!r}, 'a') as log:
    log.write(json.dumps({{"pid": os.getpid(), "scene": sys.argv[2], "display": os.environ.get("WAYLAND_DISPLAY")}}) + "\\n")
time.sleep(60)
''')
        (self.shell / 'session').mkdir()
        entries = lambda: [json.loads(line) for line in starts.read_text().splitlines()] if starts.exists() else []
        process = self.start('--nested', '--watch', '--shell', str(self.shell), '--runtime', str(runtime), 'bar', 'launcher',
                             HYPRLAND_INSTANCE_SIGNATURE='test')
        self.assertTrue(wait_for(lambda: len(entries()) == 2))
        self.assertEqual(sorted(e['scene'] for e in entries()), [f'{self.shell}/bar.plm', f'{self.shell}/launcher/entry.plm'])
        self.assertEqual({e['display'] for e in entries()}, {'hyprland-1'})
        time.sleep(0.5)
        (self.shell / 'launcher' / 'entry.plm').write_text('changed')
        self.assertTrue(wait_for(lambda: len(entries()) == 3), entries())
        self.assertEqual(entries()[2]['scene'], f'{self.shell}/launcher/entry.plm')
        (self.shell / 'session' / 'menu.plm').write_text('unselected')
        time.sleep(1)
        self.assertEqual(len(entries()), 3)
        process.send_signal(signal.SIGINT)
        self.assertEqual(process.wait(timeout=10), 130)
        for entry in entries():
            with self.subTest(pid=entry['pid']):
                self.assertTrue(wait_for(lambda: not Path(f'/proc/{entry["pid"]}').exists(), 5))
        self.assertEqual(self.calls(), [])


if __name__ == '__main__':
    unittest.main()
