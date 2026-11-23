#!/usr/bin/env python3
"""No real GUI stop: state/ownership regressions for the independent reload supervisor."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import subprocess

spec = importlib.util.spec_from_file_location('supervisor', Path(__file__).with_name('restart-supervisor.py'))
supervisor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(supervisor)


class FakeHost:
    def __init__(self, attempts=('ready',)):
        self.old = True
        self.attempts = list(attempts)
        self.current = None
        self.actions = []
        self.pid = 200
        self.block_stop = False
        self.foreign = False

    def status(self):
        if self.foreign:
            return {'running': True, 'ready': True, 'pid': 999, 'runtime': 'manual'}
        if self.old:
            return {'running': True, 'ready': True, 'pid': 100, 'runtime': 'old'}
        if self.current == 'ready':
            return {'running': True, 'ready': True, 'pid': self.pid, 'runtime': 'new'}
        if self.current == 'manual':
            return {'running': True, 'ready': True, 'pid': 999, 'runtime': 'manual'}
        return {'running': False, 'ready': False}

    def daemon_pids(self):
        return [50]

    def alive(self, pid, birth):
        return self.old

    def stop(self, pid):
        self.actions.append(('stop', pid))
        if not self.block_stop:
            self.old = False

    def pause(self, seconds):
        self.actions.append(('delay', seconds))

    def launch(self, unit, log):
        self.actions.append(('launch', unit))
        self.current = self.attempts.pop(0)
        self.pid += 1

    def unit(self, unit):
        return {'pid': self.pid, 'active': self.current in ('ready', 'pending', 'manual'),
                'exit_status': '1' if self.current == 'failed' else '0'}


class SupervisorTests(unittest.TestCase):
    def setUp(self):
        self.plan = {'pid': 100, 'birth': 'identity', 'runtime': 'old', 'daemons': [50],
                     'delay': 2, 'unit': 'test', 'directory': '/unused'}
        self.events = []

    def run_plan(self, host, **kwargs):
        return supervisor.execute(self.plan, host, lambda stage, **fields: self.events.append((stage, fields)), **kwargs)

    def test_stale_identity_never_stops_anything(self):
        host = FakeHost()
        host.foreign = True
        self.assertFalse(self.run_plan(host))
        self.assertEqual(host.actions, [])

    def test_stop_then_delay_then_independently_owned_launch(self):
        host = FakeHost()
        self.assertTrue(self.run_plan(host))
        self.assertEqual(host.actions[:3], [('stop', 100), ('delay', 2), ('launch', 'test-gui-1')])
        self.assertEqual(self.events[-1][0], 'succeeded')
        self.assertEqual(self.events[-1][1]['pid'], 201)

    def test_launcher_exit_zero_without_gui_is_not_success(self):
        # The original bare CLI would exit zero/help without starting a GUI.
        host = FakeHost(('empty-cli', 'ready'))
        self.assertTrue(self.run_plan(host))
        self.assertIn(('launch_exited', {'attempt': 1, 'exit_status': '0'}), self.events)
        self.assertEqual([x for x in host.actions if x[0] == 'stop'], [('stop', 100)])
        self.assertEqual([x for x in host.actions if x[0] == 'launch'],
                         [('launch', 'test-gui-1'), ('launch', 'test-gui-2')])

    def test_manual_ready_process_is_not_credited_to_failed_launch(self):
        host = FakeHost(('manual',))
        self.assertFalse(self.run_plan(host))
        self.assertEqual(self.events[-1][0], 'failed')
        self.assertIn('not owned', self.events[-1][1]['reason'])

    def test_failed_launches_are_bounded_and_recorded(self):
        host = FakeHost(('failed', 'failed'))
        self.assertFalse(self.run_plan(host))
        self.assertEqual(len([x for x in host.actions if x[0] == 'launch']), 2)
        self.assertEqual(self.events[-1][0], 'failed')

    def test_slow_shutdown_is_not_forced(self):
        host = FakeHost()
        host.block_stop = True
        self.assertFalse(self.run_plan(host, stop_timeout=0))
        self.assertEqual(host.actions, [('stop', 100)])
        self.assertIn('no force kill', self.events[-1][1]['reason'])

    def test_no_second_launch_while_first_readiness_is_uncertain(self):
        host = FakeHost(('pending',))
        self.assertFalse(self.run_plan(host, ready_timeout=0))
        self.assertEqual(len([x for x in host.actions if x[0] == 'launch']), 1)
        self.assertIn('retained for inspection', self.events[-1][1]['reason'])

    def test_lost_launch_reply_never_duplicates_an_active_or_unknown_unit(self):
        for unknown in (False, True):
            host = FakeHost(('pending', 'ready'))
            launch = host.launch
            def uncertain(unit, log):
                launch(unit, log)
                raise subprocess.CalledProcessError(1, 'systemd-run')
            host.launch = uncertain
            if unknown:
                host.unit = lambda unit: {'pid': 0, 'active': False, 'unknown': True, 'exit_status': None}
            self.assertFalse(self.run_plan(host))
            self.assertEqual(len([x for x in host.actions if x[0] == 'launch']), 1)
            self.assertIn('original unit retained', self.events[-1][1]['reason'])

    def test_unit_inspection_timeout_is_unknown_and_activating_without_pid_is_pending(self):
        host = supervisor.Host({'systemctl': 'systemctl'})
        with patch.object(supervisor.subprocess, 'run', side_effect=subprocess.TimeoutExpired('systemctl', 5)):
            self.assertTrue(host.unit('test')['unknown'])
        result = subprocess.CompletedProcess([], 0, stdout='MainPID=0\nActiveState=activating\nSubState=start\nLoadState=loaded\n', stderr='')
        with patch.object(supervisor.subprocess, 'run', return_value=result):
            self.assertTrue(host.unit('test')['active'])

    def test_proven_failed_launch_reply_can_retry_without_second_shutdown(self):
        host = FakeHost(('failed', 'ready'))
        launch = host.launch
        def first_failed(unit, log):
            launch(unit, log)
            if unit.endswith('-1'):
                raise subprocess.CalledProcessError(1, 'systemd-run')
        host.launch = first_failed
        self.assertTrue(self.run_plan(host))
        self.assertEqual(len([x for x in host.actions if x[0] == 'stop']), 1)

    def test_gui_unit_unsets_unsafe_manager_environment_at_the_service_boundary(self):
        host = supervisor.Host({'systemd_run': 'systemd-run', 'gui': '/installed/gui',
              'profile': [], 'env': {'HOME': '/home/test'}})
        result = subprocess.CompletedProcess([], 0, stdout='', stderr='')
        with patch.object(supervisor.subprocess, 'run', return_value=result) as run:
            host.launch('test', Path('/tmp/owned-log'))
            args = run.call_args.args[0]
        unset = next(arg for arg in args if arg.startswith('--property=UnsetEnvironment='))
        self.assertIn('ELECTRON_RUN_AS_NODE', unset)
        self.assertIn('ORCA_AGENT_SESSION_ID', unset)
        self.assertIn('NODE_OPTIONS', unset)
        self.assertIn('LD_LIBRARY_PATH', unset)
        self.assertEqual(run.call_args.kwargs['timeout'], 10)

    def test_explicit_path_and_no_provider_or_node_handoff(self):
        env = supervisor.desktop_environment({'HOME': '/home/test', 'WAYLAND_DISPLAY': 'wayland-1',
              'PATH': '/missing', 'ELECTRON_RUN_AS_NODE': '1', 'ORCA_CLI_COMMAND': '/old/cli',
              'NODE_OPTIONS': '--old', 'LD_LIBRARY_PATH': '/old/libs'}, Path('/pinned/coreutils/bin'))
        self.assertEqual(env['WAYLAND_DISPLAY'], 'wayland-1')
        self.assertTrue(env['PATH'].startswith('/pinned/coreutils/bin:'))
        for key in ('ELECTRON_RUN_AS_NODE', 'ORCA_CLI_COMMAND', 'NODE_OPTIONS', 'LD_LIBRARY_PATH'):
            self.assertNotIn(key, env)


if __name__ == '__main__':
    unittest.main()
