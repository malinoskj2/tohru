#!/usr/bin/env python3
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "bin"))
from tohru_home_guard import plan
from tohru_status_ledger import WorkerLedger


class HomeGuard(unittest.TestCase):
    def test_known_slow_work_and_internal_subagents_are_denied_before_execution(self):
        for command in ("cargo test", "cargo --color never test", "nix build .", "npm test", "curl https://example.com", "sleep 30",
                        "orca orchestration worker-start --spec work", "orca orchestration check --wait"):
            self.assertEqual(plan("Bash", {"command":command}, 5000)["decision"], "deny", command)
        self.assertEqual(plan("Agent", {"prompt":"do work"}, 5000)["decision"], "deny")

    def test_background_handles_and_owner_questions_do_not_block_the_reply(self):
        self.assertEqual(plan("Bash", {"command":"cargo test", "run_in_background":True}, 0), {"decision":"pass", "background":True})
        self.assertEqual(plan("AskUserQuestion", {}, 0), {"decision":"pass"})
        self.assertEqual(plan("Bash", {"command":"orca orchestration worker-start --background --spec work"}, 5000)["decision"], "wrap")
        self.assertEqual(plan("Bash", {"command":"printf 'cargo test'"}, 5000)["decision"], "wrap")
        self.assertEqual(plan("Bash", {"command":"printf hello"}, 0)["decision"], "deny")

    def test_unexpected_foreground_overrun_is_killed_without_a_model_or_speech_limit(self):
        command = subprocess.list2cmdline([sys.executable, "-c", "import time; time.sleep(5)"])
        started = time.monotonic()
        result = subprocess.run([sys.executable, str(ROOT / "bin/tohru-home-guard"), "run", "--budget-ms", "100", "--command", command],
                                capture_output=True, text=True, timeout=3)
        self.assertEqual(result.returncode, 124)
        self.assertLess(time.monotonic() - started, 3)
        self.assertIn('"timed_out": true', result.stderr)

    def test_foreground_output_is_preserved_in_full(self):
        result = subprocess.run([sys.executable, str(ROOT / "bin/tohru-home-guard"), "run", "--budget-ms", "1000", "--command", "printf '%010000d' 0"],
                                capture_output=True, text=True, timeout=3)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(result.stdout), 10000)

    def test_receipt_ledger_preserves_source_scope_age_and_newer_receipts(self):
        with tempfile.TemporaryDirectory() as root:
            ledger = WorkerLedger(root)
            receipt = {"dispatch":{"id":"ctx_build", "status":"in_progress"}, "worker":{"state":"working"}}
            self.assertFalse(ledger.observe("home", "wrong", "voice", receipt, 100))
            self.assertTrue(ledger.observe("home", "ctx_build", "voice", receipt, 100))
            stale = {"dispatch":{"id":"ctx_build", "status":"completed"}}
            ledger.observe("home", "ctx_build", "older", stale, 99)
            rows = ledger.snapshot("home", 106)
            self.assertTrue(rows[0]["stale"])
            self.assertEqual(rows[0]["receipt"], receipt)
            self.assertEqual(rows[0]["observed_for_request"], "voice")
            self.assertEqual(ledger.snapshot("other", 101), [])
            self.assertEqual(os.stat(ledger.path).st_mode & 0o777, 0o600)
            ledger.close()


if __name__ == "__main__":
    unittest.main()
