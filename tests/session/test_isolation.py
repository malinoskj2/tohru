#!/usr/bin/env python3
"""CPU regressions for the actual private native runner environment boundary."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('session_native', HERE / 'native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)

class Isolation(unittest.TestCase):
    def test_hostile_inherited_socket_and_session_selectors_are_overridden(self):
        with tempfile.TemporaryDirectory(dir=native.ROOT / '.tmp') as path:
            run = Path(path)
            (run/'ipc').mkdir(mode=0o700)
            inherited = {'PLEAMAR_SOCKETS':'/live/host/sockets', 'WAYLAND_SOCKET':'9',
                         'SWAYSOCK':'/live/sway', 'CUA_INJECT_SOCKET':'/live/cursor',
                         'XDG_SESSION_ID':'live', 'DBUS_STARTER_ADDRESS':'live',
                         'PATH':'/test/bin'}
            env = native.private_ipc_environment(inherited, run)
            self.assertEqual(env, {'PLEAMAR_SOCKETS':str(run/'ipc'),'PATH':'/test/bin'})
            native.check_ipc_directory(env, run)
            self.assertEqual(inherited['PLEAMAR_SOCKETS'], '/live/host/sockets')

    def test_symlink_endpoint_and_permissive_socket_directory_are_rejected(self):
        with tempfile.TemporaryDirectory(dir=native.ROOT / '.tmp') as path:
            run=Path(path);(run/'ipc').mkdir(mode=0o700)
            env=native.private_ipc_environment({},run)
            (run/'ipc/escape.sock').symlink_to('/run/user/1000/pleamar/live.sock')
            with self.assertRaises(AssertionError): native.check_ipc_directory(env,run)
            (run/'ipc/escape.sock').unlink();(run/'ipc').chmod(0o755)
            with self.assertRaises(AssertionError): native.private_ipc_environment({},run)

if __name__=='__main__': unittest.main()
