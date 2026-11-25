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
    def test_bar_wrapper_exposes_packaged_flight_helper(self):
        (ROOT / '.tmp').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='flight-launch-', dir=ROOT / '.tmp') as directory:
            recorder = Path(directory) / 'runtime'
            recorder.write_text('#!/usr/bin/env python3\nimport shutil\nprint(shutil.which("tohru-flight-scene"))\n')
            recorder.chmod(0o755)
            environment = dict(os.environ, TOHRU_RUNTIME=str(recorder))
            environment.pop('TOHRU_RUNTIME_OVERRIDE', None)
            result = subprocess.run([str(ROOT / 'bin/tohru-run')], env=environment, text=True, capture_output=True, check=True)
            self.assertEqual(result.stdout.strip(), str(ROOT / 'bin/tohru-flight-scene'))

    def test_wrappers_override_inherited_shell_role(self):
        (ROOT / '.tmp').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='role-launch-', dir=ROOT / '.tmp') as directory:
            recorder = Path(directory) / 'runtime'
            recorder.write_text('#!/usr/bin/env python3\nimport json,os,sys\nprint(json.dumps({"argv":sys.argv[1:],"shell":os.environ.get("TOHRU_SHELL")}))\n')
            recorder.chmod(0o755)
            environment = dict(os.environ, TOHRU_RUNTIME=str(recorder), TOHRU_SHELL='0', TOHRU_SCENE='/ordinary/scene')
            environment.pop('TOHRU_SETTINGS_SCENE', None)
            for script, name, shell, scene in [
                ('tohru-run', 'tohru', '1', '/ordinary/scene'),
                ('tohru-settings-run', 'tohru-settings', '1', str(ROOT / 'shell/settings/settings.plm')),
            ]:
                result = subprocess.run([str(ROOT / 'bin' / script)], env=environment, text=True, capture_output=True, check=True)
                value = json.loads(result.stdout)
                self.assertEqual(value, {'argv': ['--scene', scene, '--no-hud', '--stall', '0', '--answer-as', name], 'shell': shell})


if __name__ == '__main__':
    unittest.main()
