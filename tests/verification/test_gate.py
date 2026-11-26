#!/usr/bin/env python3
"""The gate passes a clean synthetic head turn and catches each fault the owner rejected in real turns."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("gate", ROOT / "tools/verify/gate.py")
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

SIZE = 240
COUNT = 40
FIRST, LAST = 4, 35


def head(t, swell=None, swap=None, tint=None, wink=None):
    """A round head with a crest and two eyes turning from profile towards the viewer, with optional faults."""
    y, x = np.mgrid[:SIZE, :SIZE].astype(float)
    u = np.clip((t - FIRST) / (LAST - FIRST), 0, 1)
    cx, cy, r = 120.0, 130.0, 50.0
    if swell and swell[0] <= t <= swell[1]:
        r *= 1.0 + 0.3 * np.sin(np.pi * (t - swell[0]) / (swell[1] - swell[0]))
    body = (x - cx) ** 2 + (y - cy) ** 2 < r ** 2
    crest_x = cx + 20 * (1 - u)
    crest = ((x - crest_x) / 10) ** 2 + ((y - (cy - r - 8)) / 18) ** 2 < 1
    if swap is not None and t == swap:
        crest = ((x - crest_x + 25) / 9) ** 2 + ((y - (cy - r - 8)) / 16) ** 2 < 1
        crest |= ((x - crest_x - 25) / 9) ** 2 + ((y - (cy - r - 8)) / 16) ** 2 < 1
    shape = body | crest
    frame = np.zeros((SIZE, SIZE, 4), np.uint8)
    frame[..., 0] = np.clip(235 - (y - cy) * 0.6, 0, 255)
    frame[..., 1] = np.clip(195 + (x - cx) * 0.3, 0, 255)
    frame[..., 2] = 200
    if tint and tint[0] <= t <= tint[1]:
        frame[..., 0] = 150
        frame[..., 2] = 250
    angle = (1 - u) * 1.1
    for side in (-1, 1):
        ex = cx + r * 0.55 * np.sin(angle + side * 0.45)
        rad = 6.0 * (0.3 if wink and wink[0] <= t <= wink[1] and side == 1 else 1.0)
        eye = ((x - ex) / (rad * 0.8)) ** 2 + ((y - (cy - 8)) / rad) ** 2 < 1
        frame[eye & shape, :3] = (50, 25, 45)
    frame[..., 3] = shape * 255
    return frame


class Gate(unittest.TestCase):
    def run_gate(self, frames):
        g = gate.Gate(frames, FIRST, LAST, 166, 24)
        g.run_turn()
        g.run_flow()
        return g

    def checks(self, frames):
        return {f["check"] for f in self.run_gate(frames).findings}

    def test_clean_turn_passes(self):
        self.assertEqual(self.checks([head(t) for t in range(COUNT)]), set())

    def test_skull_swell_is_caught_by_volume_and_scale(self):
        found = self.checks([head(t, swell=(12, 26)) for t in range(COUNT)])
        self.assertIn("volume", found)
        self.assertIn("head-scale", found)

    def test_one_frame_horn_redraw_is_caught(self):
        found = self.checks([head(t, swap=20) for t in range(COUNT)])
        self.assertTrue(found & {"swap", "horn-count", "outline-pop", "shape-change"}, found)

    def test_colour_shift_is_caught(self):
        found = self.checks([head(t, tint=(15, 24)) for t in range(COUNT)])
        self.assertIn("color-drift", found)

    def test_wink_is_caught(self):
        found = self.checks([head(t, wink=(18, 19)) for t in range(COUNT)])
        self.assertIn("wink", found)

    def test_beyond_ends_ignores_changes_between_the_end_views(self):
        series = np.linspace(100, 140, 30)
        self.assertEqual(gate.beyond_ends(series, 2, 27, 0.1)[0], [])
        bulge = series.copy()
        bulge[10:16] *= 1.4
        self.assertEqual(gate.beyond_ends(bulge, 2, 27, 0.1)[0], list(range(10, 16)))

    def test_cli_writes_verdict_and_blind_package(self):
        work = Path(tempfile.mkdtemp(prefix="gate-test-"))
        self.addCleanup(shutil.rmtree, work)
        frames = work / "frames"
        frames.mkdir()
        for t in range(COUNT):
            Image.fromarray(head(t)).save(frames / f"{t:04d}.png")
        out = work / "out"
        proc = subprocess.run([sys.executable, str(ROOT / "tools/verify/gate.py"), str(frames), f"--range={FIRST}-{LAST}",
                               "--no-motion", "--out", str(out)], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        result = json.loads((out / "gate.json").read_text())
        self.assertEqual(result["verdict"], "pass")
        for name in ("normal-speed.mp4", "double-size.mp4", "head-quarter-speed.mp4", "every-frame-real-size.png",
                     "time-slice-eye-row.png", "time-slice-head-column.png"):
            self.assertTrue((out / "review" / name).stat().st_size > 0, name)
        # The blind package carries no verdict, metrics or label.
        self.assertFalse(any(p.suffix in (".json", ".md") for p in (out / "review").iterdir()))


@unittest.skipUnless(os.environ.get("GATE_CORPUS"), "set GATE_CORPUS=1 to rerun the calibration corpus (minutes)")
class Calibration(unittest.TestCase):
    def test_corpus_expectations_hold(self):
        proc = subprocess.run([sys.executable, str(ROOT / "tools/verify/gate-calibrate.py")], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stdout[-3000:])


if __name__ == "__main__":
    unittest.main()
