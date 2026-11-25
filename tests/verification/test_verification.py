"""Exercise actual failure boundaries; fixtures here are not native admission proof."""
import json
import fcntl
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
(ROOT / '.tmp').mkdir(exist_ok=True)
sys.path.insert(0, str(ROOT / 'tools/verify'))
from evidence import REQUIRED, atomic, jsonl, publish, read, validate
from process import execute
import verify
import session_units


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=ROOT / '.tmp')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def run_child(self, program, timeout=3, limit=1024):
        return execute([sys.executable, '-c', program], self.root,
                       {'PATH': os.environ['PATH']}, self.root / 'child.log', timeout, limit)

    def test_overflow_is_bounded_while_child_alive_and_reaped(self):
        start = time.monotonic()
        receipt = self.run_child('import os,time; os.write(1,b"x"*3000000); time.sleep(30)')
        self.assertEqual(receipt['reason'], 'output_limit')
        self.assertEqual((self.root / 'child.log').stat().st_size, 1024)
        self.assertLess(time.monotonic() - start, 3)
        with self.assertRaises(ProcessLookupError):
            os.kill(receipt['pid'], 0)

    def test_timeout_after_stdout_closed(self):
        receipt = self.run_child('import os,time; os.close(1); os.close(2); time.sleep(30)', timeout=.2)
        self.assertEqual(receipt['reason'], 'timeout')
        self.assertTrue(receipt['reaped'])

    def test_descendant_holding_pipe_cannot_extend_deadline(self):
        program = ('import subprocess,time; '
                   'p=subprocess.Popen(["' + sys.executable + '","-c","import time; time.sleep(30)"]); '
                   'print(p.pid,flush=True)')
        receipt = self.run_child(program, timeout=.2)
        self.assertEqual(receipt['reason'], 'timeout')
        self.assertLess(receipt['duration_seconds'], 2)
        descendant = int((self.root / 'child.log').read_text())
        status = Path(f'/proc/{descendant}/stat')
        for _ in range(100):
            try:
                exited = status.read_text().split(') ')[1].startswith('Z')
            except (FileNotFoundError, ProcessLookupError):
                exited = True
            if exited:
                break
            time.sleep(.01)
        else:
            self.fail('owned descendant remained alive after timeout')

    def test_exit_failure_is_not_success_or_retry(self):
        receipt = self.run_child('print("one attempt"); raise SystemExit(7)')
        self.assertEqual(receipt['exit'], 7)
        self.assertEqual((self.root / 'child.log').read_text(), 'one attempt\n')


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=ROOT / '.tmp')
        self.addCleanup(self.directory.cleanup)
        self.run = Path(self.directory.name)
        state = {'schema_version': 1, 'run_id': self.run.name, 'scenario': 'native-authority',
                 'phase': 'cleaned', 'outcome': 'failed', 'action_start_ns': 10, 'action_end_ns': 20}
        atomic(self.run / 'state.json', state)
        for name in REQUIRED:
            atomic(self.run / name, {'run_id': self.run.name, 'scenario': 'native-authority', 'outcome': 'failed'})
        (self.run / 'actions.jsonl').unlink()
        jsonl(self.run / 'actions.jsonl', [{'sequence': 1, 'exit': 1}])

    def test_complete_collector_preserves_failed_scenario(self):
        collection = publish(self.run)
        self.assertEqual(validate(self.run)['outcome'], 'failed')
        self.assertTrue(read(collection / 'collection-complete.json')['complete'])

    def test_stale_and_cross_run_marker_rejected(self):
        for field, value in [('completed_ns', 1), ('run_id', 'foreign'), ('scenario', 'home-direct'),
                             ('collection_id', 'foreign'), ('cursor', 0), ('action_start_ns', 9)]:
            with self.subTest(field=field):
                collection = publish(self.run)
                marker = read(collection / 'collection-complete.json')
                marker[field] = value
                atomic(collection / 'collection-complete.json', marker)
                with self.assertRaises(ValueError):
                    validate(self.run, collection)

    def test_partial_and_tampered_artifacts_rejected(self):
        collection = publish(self.run)
        (collection / 'assertions.json').write_text('{}')
        with self.assertRaises(ValueError):
            validate(self.run, collection)
        collection = publish(self.run)
        (collection / 'collection-complete.json').unlink()
        with self.assertRaises(FileNotFoundError):
            validate(self.run, collection)

    def test_crash_between_marker_and_pointer_keeps_previous_complete(self):
        first = publish(self.run)
        original = verify.atomic

        def interrupted(path, value):
            if path.name == 'latest-completed.json':
                raise OSError('simulated publication interruption')
            original(path, value)

        with patch('evidence.atomic', interrupted):
            with self.assertRaises(OSError):
                publish(self.run)
        self.assertEqual(read(self.run / 'latest-completed.json')['collection_id'], first.name)
        self.assertEqual(validate(self.run)['outcome'], 'failed')
        self.assertEqual(len(list((self.run / 'collections').glob('*/collection-complete.json'))), 2)

    def test_cleanup_twice_and_interrupted_setup(self):
        state = read(self.run / 'state.json')
        state['phase'] = 'driven'
        atomic(self.run / 'resources.json', {'run_id': self.run.name, 'processes': []})
        atomic(self.run / 'state.json', state)
        self.assertEqual(verify.cleanup(self.run), 'failed')
        first = (self.run / 'after.json').read_bytes()
        self.assertEqual(verify.cleanup(self.run), 'failed')
        self.assertEqual((self.run / 'after.json').read_bytes(), first)
        state['phase'] = 'driving'
        atomic(self.run / 'state.json', state)
        self.assertEqual(verify.cleanup(self.run), 'cleanup_failed')

    def test_cleanup_before_drive_is_incomplete_and_repeatable(self):
        state = read(self.run / 'state.json')
        state['phase'] = 'launched'
        atomic(self.run / 'state.json', state)
        self.assertEqual(verify.cleanup(self.run), 'incomplete')
        self.assertEqual(verify.cleanup(self.run), 'incomplete')
        self.assertTrue(read(self.run / 'after.json')['drive_never_started'])

    def test_traversal_pointer_rejected(self):
        publish(self.run)
        atomic(self.run / 'latest-completed.json', {'collection_id': '../../../foreign'})
        with self.assertRaises(ValueError):
            validate(self.run)


class ContractTests(unittest.TestCase):
    def test_wrappers_discoverable_and_identical(self):
        wrappers = [ROOT / directory / 'skills/verify/SKILL.md' for directory in ('.agents', '.claude')]
        self.assertEqual(wrappers[0].read_bytes(), wrappers[1].read_bytes())
        for wrapper in wrappers:
            content = wrapper.read_text()
            self.assertIn('name: verify\n', content)
            for name in ('Launch', 'Doctor', 'Drive', 'Evidence', 'Cleanup', 'Helpers'):
                self.assertIn('## ' + name, content)
            for link in re.findall(r'\]\(([^)]+)\)', content):
                self.assertTrue((wrapper.parent / link).is_file(), link)

    def test_feature_and_source_references_exist(self):
        scenarios = read(ROOT / 'tools/verify/scenarios.json')
        self.assertEqual(len(scenarios), 10)
        for scenario, specification in scenarios.items():
            path = ROOT / specification['feature']
            self.assertTrue(path.is_file())
            content = path.read_text()
            for title in ('Sub-features', 'How to get to it (user POV)', 'Driving it with the shared runner', 'Gotchas'):
                self.assertIn('## ' + title, content)
            self.assertIn(scenario, content)
            for reference in re.findall(r'\b(?:tools|tests|integrations|bin)/[\w/.-]+\.(?:py|sh|md|mjs|json)', content):
                self.assertTrue((ROOT / reference).is_file(), reference)
        self.assertEqual([k for k, v in scenarios.items() if v['implemented']], ['native-authority', 'session-units'])

    def test_host_scenario_fails_closed_without_caller_or_effects(self):
        report = verify.doctor('job-conversation', 'host', None)
        self.assertEqual(report['outcome'], 'blocked')
        self.assertIn('not coordinating Home', report['authority'])

    def test_missing_native_source_blocks_before_launch(self):
        report = verify.doctor('native-authority', 'sandbox', ROOT / '.tmp/missing-source')
        self.assertEqual(report['outcome'], 'blocked')
        self.assertIsNone(report['source_identity'])


class PublicJourneyTests(unittest.TestCase):
    def test_real_cli_effects_and_negatives_survive_cleanup(self):
        result = subprocess.run([sys.executable, str(ROOT / 'tools/verify/verify.py'),
                                 'run', 'session-units', '--target', 'sandbox'],
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        receipt = json.loads(result.stdout)
        run = Path(receipt['evidence'])
        self.assertEqual(receipt['proof_level'], 'shipped source CLI with actual private file effects')
        assertions = read(run / 'assertions.json')['checks']
        self.assertEqual(len(assertions), 3)
        self.assertTrue(all(a['passed'] for a in assertions))
        self.assertEqual(len(session_units.snapshot(run / 'units')), 4)
        self.assertTrue(read(run / 'after.json')['generated_files_unchanged'])
        self.assertEqual(verify.cleanup(run), 'passed')
        self.assertEqual(validate(run)['outcome'], 'passed')
        self.assertFalse((run / 'rejected-units').exists())

    def test_inherited_live_selectors_cannot_redirect_the_journey(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.tmp') as directory:
            env = {**os.environ, 'HOME': directory, 'XDG_CONFIG_HOME': directory,
                   'XDG_RUNTIME_DIR': directory, 'PLEAMAR_SOCKETS': directory,
                   'TOHRU_RUNTIME': '/bin/false', 'BASH_ENV': directory + '/missing',
                   'TOHRU_MANAGED_CONFIG': directory + '/missing',
                   'DBUS_SESSION_BUS_ADDRESS': 'unix:path=' + directory + '/host'}
            result = subprocess.run([sys.executable, str(ROOT / 'tools/verify/verify.py'),
                                     'run', 'session-units', '--target', 'sandbox'],
                                    env=env, capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_content_assertion_rejects_wrong_exec_and_missing_files(self):
        files = {name: {'text': '[Unit]\n[Service]\n', 'sha256': 'irrelevant'}
                 for name in ['tohru-bar.service', 'tohru-launcher.service', 'tohru-lock.service', 'tohru-session.service']}
        self.assertFalse(session_units.generated_units_valid(files, ROOT / 'bin'))
        self.assertFalse(session_units.generated_units_valid({}, ROOT / 'bin'))

    def test_successful_exit_without_generated_files_cannot_pass(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.tmp') as directory:
            run = Path(directory)
            with patch('session_units.execute', return_value={'exit': 0, 'reason': None}):
                outcome, _, observations, assertions = session_units.drive(ROOT, run, 1)
            self.assertEqual(outcome, 'failed')
            self.assertEqual(observations[0]['files'], {})
            self.assertFalse(assertions[0]['passed'])

    def test_contending_command_does_not_write_or_recover_active_owner(self):
        parent = ROOT / '.tmp/verify'
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            run = Path(directory)
            atomic(run / 'state.json', {'run_id': run.name, 'phase': 'driving',
                                       'scenario': 'session-units', 'outcome': 'passed'})
            before = (run / 'state.json').read_bytes()
            with (run / 'lock').open('a') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                for operation in ('cleanup', 'drive', 'collect', 'validate'):
                    with self.subTest(operation=operation):
                        result = subprocess.run([sys.executable, str(ROOT / 'tools/verify/verify.py'),
                                                 operation, '--run', str(run)],
                                                capture_output=True, text=True, timeout=5)
                        self.assertEqual(result.returncode, 4, result.stdout + result.stderr)
                        self.assertEqual((run / 'state.json').read_bytes(), before)
                        self.assertEqual(set(p.name for p in run.iterdir()), {'state.json', 'lock'})


if __name__ == '__main__':
    (ROOT / '.tmp').mkdir(exist_ok=True)
    unittest.main()
