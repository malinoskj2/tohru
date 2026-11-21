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


if __name__ == '__main__':
    unittest.main()
