"""Exercise actual failure boundaries; fixtures here are not native admission proof."""
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/verify'))
from evidence import REQUIRED, atomic, jsonl, publish, read, validate
from process import execute
import verify


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
            if not status.exists() or status.read_text().split(') ')[1].startswith('Z'):
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
        self.assertEqual(len(scenarios), 9)
        for scenario, specification in scenarios.items():
            path = ROOT / specification['feature']
            self.assertTrue(path.is_file())
            content = path.read_text()
            for title in ('Sub-features', 'How to get to it (user POV)', 'Driving it with the shared runner', 'Gotchas'):
                self.assertIn('## ' + title, content)
            self.assertIn(scenario, content)
            for reference in re.findall(r'\b(?:tools|tests|integrations|bin)/[\w/.-]+\.(?:py|sh|md|mjs|json)', content):
                self.assertTrue((ROOT / reference).is_file(), reference)
        self.assertEqual([k for k, v in scenarios.items() if v['implemented']], ['native-authority'])

    def test_host_scenario_fails_closed_without_caller_or_effects(self):
        report = verify.doctor('job-conversation', 'host', None)
        self.assertEqual(report['outcome'], 'blocked')
        self.assertIn('not coordinating Home', report['authority'])

    def test_missing_native_source_blocks_before_launch(self):
        report = verify.doctor('native-authority', 'sandbox', ROOT / '.tmp/missing-source')
        self.assertEqual(report['outcome'], 'blocked')
        self.assertIsNone(report['source_identity'])


if __name__ == '__main__':
    (ROOT / '.tmp').mkdir(exist_ok=True)
    unittest.main()
