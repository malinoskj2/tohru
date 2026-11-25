#!/usr/bin/env python3
"""Exercise public staging and evidence retention under failed launch/isolation."""
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / ".agents/skills/verify/scripts/drive.py"


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

    def test_staging_ignores_inherited_live_selectors(self):
        (ROOT / ".tmp").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT / ".tmp") as directory:
            poison = Path(directory)
            env = {**os.environ, "HOME": directory, "XDG_CONFIG_HOME": directory,
                   "XDG_RUNTIME_DIR": directory, "PLEAMAR_SOCKETS": directory,
                   "TOHRU_RUNTIME": "/bin/false", "BASH_ENV": str(poison / "missing"),
                   "TOHRU_MANAGED_CONFIG": str(poison / "missing"),
                   "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + directory + "/host"}
            result, evidence, summary = self.drive("--feature", "session-units", env=env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(summary["result"], "passed")
            self.assertEqual(list(poison.iterdir()), [])
            self.assertEqual(len(list((evidence / "units").glob("*.service"))), 4)

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
