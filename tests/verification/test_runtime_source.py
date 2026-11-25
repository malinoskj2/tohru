#!/usr/bin/env python3
"""Provenance regressions using disposable checkout/build-input trees."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/verify/runtime-source.py"
spec = importlib.util.spec_from_file_location("runtime_source", SCRIPT)
provenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provenance)


class RuntimeSource(unittest.TestCase):
    def setUp(self):
        (ROOT / ".tmp").mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="source-regression-", dir=ROOT / ".tmp")
        self.addCleanup(self.temporary.cleanup)
        self.checkout = Path(self.temporary.name) / "checkout"
        self.store = Path(self.temporary.name) / "declared-input"
        for member in ("runtime/Cargo.toml", "runtime/Cargo.lock", "LICENSE",
                       "runtime/src/lib.rs", "runtime/src/README.md",
                       "runtime/tests/settings.rs", "runtime/tests/data/example.txt",
                       "runtime/pleamar/bindings/settings_luau.rs",
                       "runtime/pleamar/bindings/data/example.txt",
                       "runtime/pleamar/tests/role-install.rs"):
            path = self.checkout / member
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture: " + member)
        shutil.copytree(self.checkout, self.store)

    def test_deleted_checkout_files_cannot_pass(self):
        for member in ("runtime/src/lib.rs", "runtime/tests/settings.rs",
                       "runtime/tests/data/example.txt", "runtime/pleamar/bindings/settings_luau.rs",
                       "runtime/pleamar/bindings/data/example.txt", "runtime/Cargo.toml",
                       "runtime/pleamar/tests/role-install.rs"):
            with self.subTest(member=member):
                path = self.checkout / member
                content = path.read_bytes()
                path.unlink()
                try:
                    mismatches = [row["source"] for row in provenance.compare_foundation(self.checkout, self.store)
                                  if not row["match"]]
                    self.assertEqual(mismatches, [member])
                finally:
                    path.write_bytes(content)

    def test_checkout_only_file_cannot_pass(self):
        (self.store / "runtime/tests/data/example.txt").unlink()
        mismatches = [row for row in provenance.compare_foundation(self.checkout, self.store) if not row["match"]]
        self.assertEqual([row["source"] for row in mismatches], ["runtime/tests/data/example.txt"])
        self.assertEqual(mismatches[0]["difference"], "missing from declared inputs")

    def test_selectors_and_content_checks(self):
        (self.checkout / "runtime/src/README.md").unlink()
        self.assertTrue(all(row["match"] for row in provenance.compare_foundation(self.checkout, self.store)))
        (self.store / "runtime/src/lib.rs").write_text("different source")
        mismatches = [row["source"] for row in provenance.compare_foundation(self.checkout, self.store)
                      if not row["match"]]
        self.assertEqual(mismatches, ["runtime/src/lib.rs"])

    def test_unavailable_runtime_or_tool_has_actionable_json(self):
        for runtime in (self.checkout / "missing-runtime", SCRIPT):
            with self.subTest(runtime=runtime):
                result = subprocess.run([sys.executable, str(SCRIPT), "--runtime", str(runtime)],
                                        env={**os.environ, "PATH": ""}, text=True,
                                        capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 2, result.stderr)
                report = json.loads(result.stdout)
                self.assertEqual(report["result"], "unavailable")
                self.assertIn("Supply an existing Nix-built runtime", report["diagnosis"][-1])
                self.assertIn("missing-runtime" if runtime != SCRIPT else "nix-store", report["diagnosis"][0])

    def test_missing_declared_tree_is_unavailable(self):
        shutil.rmtree(self.store / "runtime/pleamar/bindings")
        with self.assertRaisesRegex(ValueError, "selector directory unavailable:.*declared-input/runtime/pleamar/bindings"):
            provenance.compare_foundation(self.checkout, self.store)


if __name__ == "__main__":
    unittest.main()
