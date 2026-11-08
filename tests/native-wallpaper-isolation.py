#!/usr/bin/env python3
"""Actual focused runner environment only; never start a display or service."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('wallpaper_runner', ROOT / 'tests/native-wallpaper.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class PrivateEnvironment(unittest.TestCase):
    def test_hostile_parent_endpoints_and_role_markers_are_replaced(self):
        (ROOT / '.tmp').mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT / '.tmp') as directory:
            root = Path(directory)
            run = root / 'run'; run.mkdir(mode=0o700)
            inherited = {**os.environ, 'HOME': '/host/home', 'DISPLAY': ':99', 'WAYLAND_DISPLAY': '/host/display',
                         'WAYLAND_SOCKET': '7', 'CUA_INJECT_SOCKET': '/host/input', 'PLEAMAR_SOCKETS': '/host/ipc',
                         'DBUS_SESSION_BUS_ADDRESS': 'unix:path=/host/bus', 'DBUS_SYSTEM_BUS_ADDRESS': 'unix:path=/host/system',
                         'PULSE_SERVER': 'tcp:host-audio', 'TOHRU_B_PRIVATE_BUS': '1', 'TOHRU_PRIVATE_BUS': '1',
                         'TOHRU_LOCKER': '1', 'TOHRU_MANAGED_CONFIG': '/host/config', 'LISTEN_FDNAMES': 'host'}
            env = runner.private_environment(inherited, run, root, root / 'runtime', root / 'scene')
            runner.base.assert_private_transport(env, run)
            self.assertEqual(env['HOME'], str(root / 'home'))
            self.assertEqual(env['XDG_CACHE_HOME'], str(root / 'cache'))
            self.assertEqual(env['XDG_DATA_DIRS'], str(root / 'data'))
            self.assertEqual(env['WLR_BACKENDS'], 'headless')
            for key in ('DISPLAY', 'WAYLAND_SOCKET', 'CUA_INJECT_SOCKET', 'TOHRU_B_PRIVATE_BUS', 'TOHRU_PRIVATE_BUS', 'TOHRU_LOCKER', 'TOHRU_MANAGED_CONFIG', 'LISTEN_FDNAMES'):
                self.assertNotIn(key, env)
            self.assertEqual(inherited['PLEAMAR_SOCKETS'], '/host/ipc')


if __name__ == '__main__':
    unittest.main()
