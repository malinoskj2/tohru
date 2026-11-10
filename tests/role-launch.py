#!/usr/bin/env python3
"""Execute actual launch wrappers against a recording process, never a runtime role."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RoleLaunch(unittest.TestCase):
    def test_hostile_inherited_role_flags_are_separated(self):
        (ROOT / '.tmp').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='role-launch-', dir=ROOT / '.tmp') as directory:
            recorder = Path(directory) / 'runtime'
            recorder.write_text('#!/usr/bin/env python3\nimport json,os,sys\nprint(json.dumps({"argv":sys.argv[1:],"shell":os.environ.get("TOHRU_SHELL"),"locker":os.environ.get("TOHRU_LOCKER")}))\n')
            recorder.chmod(0o755)
            environment = dict(os.environ, TOHRU_RUNTIME=str(recorder), TOHRU_SHELL='1', TOHRU_LOCKER='1', TOHRU_SCENE='/ordinary/scene')
            environment.pop('TOHRU_LOCK_SCENE', None)
            environment.pop('TOHRU_SETTINGS_SCENE', None)
            for script, name, shell, locker, scene in [
                ('tohru-run', 'tohru', '1', None, '/ordinary/scene'),
                ('tohru-locker-run', 'tohru-lock', None, '1', str(ROOT / 'shell/lock/lock.plm')),
                ('tohru-settings-run', 'tohru-settings', '1', None, str(ROOT / 'shell/settings/settings.plm')),
            ]:
                result = subprocess.run([str(ROOT / 'bin' / script)], env=environment, text=True, capture_output=True, check=True)
                value = json.loads(result.stdout)
                self.assertEqual(value, {'argv': ['--scene', scene, '--no-hud', '--stall', '0', '--answer-as', name], 'shell': shell, 'locker': locker})
            environment['TOHRU_LOCK_SCENE'] = '/private/fixture-lock.plm'
            result = subprocess.run([str(ROOT / 'bin/tohru-locker-run')], env=environment, text=True, capture_output=True, check=True)
            self.assertEqual(json.loads(result.stdout)['argv'][1], '/private/fixture-lock.plm')


if __name__ == '__main__':
    unittest.main()
