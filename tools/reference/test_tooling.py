"""Non-GPU boundary checks; synthetic processes are not benchmark evidence."""
import ast
import hashlib
import importlib.util
import json
import re
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('reference_benchmark', HERE / 'benchmark.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class ToolingChecks(unittest.TestCase):
    def test_software_renderer_cannot_pass_adapter_guard(self):
        with tempfile.TemporaryDirectory() as folder:
            log = Path(folder) / 'adapter.log'
            log.write_text('NVIDIA GeForce RTX 5090\nllvmpipe fallback\n')
            with self.assertRaises(ValueError):
                benchmark.hardware(log, 'NVIDIA GeForce RTX 5090')

    def test_unlisted_child_rejected_and_owned_app_stopped(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder)
            pid_file = out / 'pid'
            code = ('import os,subprocess,sys,time; '
                    'child=subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"]); '
                    'open(sys.argv[1],"w").write(str(os.getpid())); time.sleep(30)')
            side = {'command': [sys.executable, '-c', code, str(pid_file)],
                    'adapter': 'unused', 'compositor_pid': os.getpid(),
                    'compositor_log': str(out / 'unused'), 'helper_pids': {}}
            with patch.object(benchmark, 'hardware'):
                with self.assertRaisesRegex(ValueError, 'unaccounted descendants'):
                    benchmark.trial(side, 'synthetic', out, 0.25, 0.01)
            pid = int(pid_file.read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)

    def test_sampler_and_start_identity_for_current_process(self):
        before = benchmark.sampler.sample(os.getpid())
        after = benchmark.sampler.sample(os.getpid())
        self.assertGreaterEqual(after['ticks'], before['ticks'])
        self.assertGreater(after['rss_kib'], 0)
        self.assertGreater(benchmark.identity(os.getpid()), 0)

    def test_workspace_fixture_matches_original_git_blobs(self):
        fixture = HERE / 'fixtures/workspace'
        manifest = json.loads((fixture / 'provenance.json').read_text())
        self.assertEqual(manifest['source_commit'], '9119cfc23911e7c29bee66ee0ad459c303bbce5e')
        self.assertEqual(len(manifest['files']), 9)
        for entry in manifest['files']:
            with self.subTest(file=entry['file']):
                data = (fixture / entry['file']).read_bytes()
                self.assertEqual(len(data), entry['bytes'])
                self.assertEqual(hashlib.sha256(data).hexdigest(), entry['sha256'])
                original = subprocess.check_output(['git', 'show',
                    manifest['source_commit'] + ':' + entry['original_path']], cwd=HERE)
                self.assertEqual(data, original)
        track = (fixture / 'workspace_track.rs').read_text()
        events = track.split('const EVENTS:')[1].split('];')[0]
        parsed = [list(map(int, pair)) for pair in re.findall(r'\((\d+), (\d+)\)', events)]
        timeline = json.loads((HERE / 'workspace-replay.json').read_text())
        self.assertEqual(parsed, timeline['events_ms'])
        self.assertIn('1.0 / 240.0', (fixture / 'workspace_motion.rs').read_text())
        self.assertIn('replay_is_independent_of_render_cadence_and_finishes_flat', track)

    def test_standalone_sampler_matches_historical_function(self):
        original = subprocess.check_output(['git', 'show',
            '9119cfc23911e7c29bee66ee0ad459c303bbce5e:prototypes/benchmark_gpu.py'], cwd=HERE, text=True)
        current = (HERE / 'proc_sample.py').read_text()
        def function(source):
            node = next(n for n in ast.parse(source).body
                        if isinstance(n, ast.FunctionDef) and n.name == 'sample')
            return ast.get_source_segment(source, node)
        self.assertEqual(function(original), function(current))

    def test_invalid_durations_rejected_before_launch(self):
        result = subprocess.run([sys.executable, str(HERE / 'benchmark.py'),
            '--spec', '/does/not/exist', '--output', '/outside/worktree',
            '--gpu-slot', 'NON_GPU_VALIDATION', '--warmup', 'nan'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('acceptance sampling requires', result.stderr)


if __name__ == '__main__':
    unittest.main()
