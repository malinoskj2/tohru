#!/usr/bin/env python3
"""Exercise desktop driver evidence retention under a failed launch."""
import json
import shutil
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
DRIVER = ROOT / "tools/verify/desktop.py"


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


if __name__ == "__main__":
    unittest.main()
