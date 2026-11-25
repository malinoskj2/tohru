#!/usr/bin/env python3
"""Exercise desktop driver evidence retention under a failed launch."""
import json
import shutil
from pathlib import Path
import importlib.util
import subprocess
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
DRIVER = ROOT / "tools/verify/desktop.py"
spec = importlib.util.spec_from_file_location("desktop", DRIVER)
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)


class VerificationDriver(unittest.TestCase):
    def drive(self, *args, env=None):
        result = subprocess.run([str(DRIVER), "run", *args], cwd=ROOT,
                                env=env, text=True, capture_output=True, timeout=30)
        evidence = Path(result.stdout.splitlines()[0])
        self.assertTrue(evidence.is_relative_to(ROOT / ".tmp"))
        summary = json.loads((evidence / "summary.json").read_text())
        cleanup = json.loads((evidence / "cleanup.json").read_text())
        self.assertTrue(cleanup["scratch_removed"])
        self.assertEqual(cleanup["failures"], [])
        self.assertTrue((evidence / "commands.jsonl").is_file())
        return result, evidence, summary

    def test_invalid_runtime_preserves_failure_after_cleanup(self):
        result, evidence, summary = self.drive("--feature", "calendar", "--runtime", shutil.which("false"))
        self.assertEqual(result.returncode, 1)
        self.assertEqual(summary["result"], "failed")
        self.assertIn("lacks --tohru-capabilities", (evidence / "failure.txt").read_text())
        self.assertIn("failure.txt", summary["artifacts_sha256"])
        self.assertTrue(all(value is not None for value in
                            json.loads((evidence / "cleanup.json").read_text())["processes"].values()))


class OwnedProcessCleanup(unittest.TestCase):
    def setUp(self):
        self.run = desktop.Run("calendar", None)
        self.addCleanup(shutil.rmtree, self.run.evidence, ignore_errors=True)

    def test_reaps_group_members_after_the_leader_exits(self):
        leader = self.run.start(["sh", "-c", "sleep 60 & exit 0"], "orphan")
        leader.wait(timeout=5)
        deadline = time.monotonic() + 5
        while not desktop.group_members(leader.pid) and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertTrue(desktop.group_members(leader.pid))
        self.assertEqual(self.run.cleanup(), [])
        self.assertEqual(desktop.group_members(leader.pid), [])
        self.assertEqual(json.loads((self.run.evidence / "cleanup.json").read_text())["group_leftovers"], {})

    def test_service_log_stops_growing_at_its_limit(self):
        limit = desktop.SERVICE_LOG_LIMIT
        desktop.SERVICE_LOG_LIMIT = 1000
        self.addCleanup(setattr, desktop, "SERVICE_LOG_LIMIT", limit)
        self.run.start(["head", "-c", "200000", "/dev/zero"], "chatty").wait(timeout=10)
        self.assertEqual(self.run.cleanup(), [])
        log = (self.run.evidence / "chatty.log").read_bytes()
        self.assertTrue(log.startswith(b"\0" * 1000))
        self.assertIn(b"[dropped 199000 bytes past 1000]", log)
        self.assertLess(len(log), 1100)

    def test_recorded_command_output_is_bounded(self):
        text = desktop.bounded("x" * (desktop.RECORDED_OUTPUT_LIMIT + 5))
        self.assertTrue(text.endswith("[truncated 5 characters]"))
        self.run.cleanup()


if __name__ == "__main__":
    unittest.main()
