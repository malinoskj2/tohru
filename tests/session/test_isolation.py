#!/usr/bin/env python3
"""CPU regressions for the actual private native runner environment boundary."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import sys
sys.dont_write_bytecode = True

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
                         'TOHRU_PRIVATE_SESSION_TEST':'1', 'DBUS_SESSION_BUS_ADDRESS':'unix:path=/live/bus',
                         'DBUS_SYSTEM_BUS_ADDRESS':'unix:path=/live/system',
                         'PULSE_SERVER':'unix:/live/audio','PULSE_RUNTIME_PATH':'/live/audio-runtime',
                         'PULSE_COOKIE':'/live/secret-cookie', 'PATH':'/test/bin'}
            env = native.private_ipc_environment(inherited, run)
            self.assertEqual(env['PLEAMAR_SOCKETS'],str(run/'ipc'))
            self.assertEqual(env['PULSE_SERVER'],'unix:'+str(run/'pulse-native'))
            self.assertEqual(env['PULSE_COOKIE'],str(run/'pulse-cookie'))
            self.assertEqual(env['PULSE_RUNTIME_PATH'],str(run))
            self.assertEqual(env['DBUS_SESSION_BUS_ADDRESS'],'unix:path='+str(run/'bus'))
            self.assertEqual(env['DBUS_SYSTEM_BUS_ADDRESS'],env['DBUS_SESSION_BUS_ADDRESS'])
            self.assertNotIn('WAYLAND_SOCKET',env)
            self.assertNotIn('XDG_SESSION_ID',env)
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

    def test_evidence_write_failure_cannot_skip_any_owned_process(self):
        class Process:
            def __init__(self,fail=False): self.actions=[]; self.fail=fail
            def poll(self): return None
            def terminate(self):
                self.actions.append('terminate')
                if self.fail: raise OSError('terminate failure')
            def kill(self): self.actions.append('kill')
            def wait(self,timeout): self.actions.append('wait')
        class Log:
            def __init__(self): self.closed=False
            def close(self): self.closed=True
        with tempfile.TemporaryDirectory(dir=native.ROOT/'.tmp') as path:
            directory=Path(path);run=directory/'run';run.mkdir()
            processes=[Process(),Process(True),Process()];log=Log()
            with patch.object(Path,'write_text',side_effect=OSError('disk full')):
                errors=native.cleanup(processes,[log],run,directory,[])
            self.assertTrue(any('disk full' in error for error in errors))
            for process in processes:self.assertIn('wait',process.actions)
            self.assertIn('kill',processes[1].actions)
            self.assertTrue(log.closed)
            self.assertFalse(run.exists())

if __name__=='__main__': unittest.main()
