#!/usr/bin/env python3
"""Each motion check catches its fault in a synthetic clip and stays quiet on smooth motion."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("motion", ROOT / "tools/verify/motion.py")
motion = importlib.util.module_from_spec(spec)
spec.loader.exec_module(motion)

SIZE = 160
COUNT = 30


def sprite(t, bump=False, flip=False, hole=False, eye=True, phase=None):
    """A soft-coloured body with a tail, an eye and a wing that beats unless frozen."""
    y, x = np.mgrid[:SIZE, :SIZE].astype(float)
    cx, cy = 50 + 2.0 * t, 80 + 6 * np.sin(t / 5)
    body = (x - cx) ** 2 / 26 ** 2 + (y - cy) ** 2 / 20 ** 2 < 1
    side = -1 if flip else 1
    tail = ((x - (cx + side * 30)) ** 2 + (y - cy - 6) ** 2) < 9 ** 2
    wing_phase = 0.6 * t if phase is None else phase
    wing = ((x - cx) ** 2 / (14 + 6 * np.sin(wing_phase)) ** 2
            + (y - (cy - 26 - 8 * np.sin(wing_phase))) ** 2 / (8 + 4 * np.cos(wing_phase)) ** 2) < 1
    shape = body | tail | wing
    if bump:
        shape |= ((x - cx) ** 2 + (y - (cy + 24)) ** 2) < 10 ** 2
    if hole:
        shape &= ~(((x - cx) ** 2 + (y - cy) ** 2) < 6 ** 2)
    frame = np.zeros((SIZE, SIZE, 4), np.uint8)
    frame[..., 0] = 200 + (x - cx) * 0.8
    frame[..., 1] = 150 + (y - cy)
    frame[..., 2] = 220
    eye_region = ((x - (cx + side * 12)) ** 2 + (y - (cy - 5)) ** 2) < 4 ** 2
    if eye:
        frame[eye_region & shape, :3] = (40, 20, 50)
    frame[..., 3] = shape * 255
    return frame


class Synthetic(unittest.TestCase):
    def setUp(self):
        self.work = Path(tempfile.mkdtemp(prefix="motion-test-"))
        self.addCleanup(shutil.rmtree, self.work)

    def clip(self, frames):
        folder = Path(tempfile.mkdtemp(dir=self.work))
        for i, frame in enumerate(frames):
            Image.fromarray(frame).save(folder / f"{i:04d}.png")
        return motion.Clip(str(folder))

    def checks(self, frames, **options):
        result = motion.analyse(self.clip(frames), **options)
        return {(f["check"], f["peak"]) for f in result["flags"] if f["strength"] >= 1}, result

    def assertFlag(self, found, check, frames):
        hits = {peak for name, peak in found if name == check}
        self.assertTrue(hits & set(frames), f"{check} not flagged near {frames}: {sorted(found)}")

    def test_smooth_motion_raises_nothing(self):
        found, _ = self.checks([sprite(t) for t in range(COUNT)])
        self.assertEqual(found, set())

    def test_one_frame_bulge(self):
        found, _ = self.checks([sprite(t, bump=t == 15) for t in range(COUNT)])
        shape_checks = {"outline-pulse", "silhouette-area", "part-area", "bbox-continuity"}
        self.assertIn(15, {peak for name, peak in found if name in shape_checks}, sorted(found))

    def test_blank_frame_and_direction_swap(self):
        frames = [sprite(t, flip=t > 15) if t != 15 else np.zeros_like(sprite(t)) for t in range(COUNT)]
        found, _ = self.checks(frames)
        self.assertFlag(found, "vanish", [15])
        self.assertFlag(found, "mirror-flip", [16])

    def test_lost_eye_is_a_part_spike(self):
        found, _ = self.checks([sprite(t, eye=t != 15) for t in range(COUNT)])
        self.assertFlag(found, "part-area", [15])

    def test_transient_hole(self):
        found, _ = self.checks([sprite(t, hole=t == 15) for t in range(COUNT)])
        self.assertFlag(found, "transparent-holes", [14, 15, 16])

    def test_static_image_sliding(self):
        found, result = self.checks([np.roll(sprite(0, phase=0), 2 * t, axis=1) for t in range(COUNT)])
        self.assertTrue(any(f["check"] == "rigid-motion" for f in result["flags"]), sorted(found))
        found, result = self.checks([sprite(t) for t in range(COUNT)])
        self.assertFalse(any(f["check"] == "rigid-motion" for f in result["flags"]))

    def test_loop_drift(self):
        frames = [sprite(t % 12) for t in range(12)] + [np.roll(sprite(0), 12, axis=1)]
        _, result = self.checks(frames, loop=True)
        self.assertTrue(any(f["check"] == "loop-drift" for f in result["flags"]))

    def test_body_lifting_off_an_anchored_edge(self):
        frames = []
        for t in range(COUNT):
            frame = np.zeros((SIZE, SIZE, 4), np.uint8)
            lift = 6 if 12 <= t < 16 else 0
            frame[SIZE - 50 - lift:SIZE - lift, 50:110] = (200, 180, 220, 255)
            frames.append(frame)
        found, _ = self.checks(frames, edge_anchor=["bottom"])
        self.assertFlag(found, "edge-contact", range(12, 16))


class Contract(unittest.TestCase):
    def test_motion_section_copies_match_and_links_resolve(self):
        copies = [ROOT / directory / "skills/verify/motion.md" for directory in (".agents", ".claude")]
        self.assertEqual(copies[0].read_bytes(), copies[1].read_bytes())
        content = copies[0].read_text()
        for link in __import__("re").findall(r"\]\(([^)]+)\)", content):
            self.assertTrue((copies[0].parent / link).is_file(), link)
        for check in ("silhouette-area", "part-area", "outline-pulse", "bbox-continuity", "mirror-flip", "vanish",
                      "rigid-motion", "transparent-holes", "edge-contact", "loop-drift"):
            self.assertIn(check, content)
            self.assertIn(f'"{check}"', (ROOT / "tools/verify/motion.py").read_text())
        skill = (ROOT / ".agents/skills/verify/SKILL.md").read_text()
        self.assertIn("(motion.md)", skill)


class EndToEnd(unittest.TestCase):
    def test_review_writes_evidence_and_side_by_side(self):
        work = Path(tempfile.mkdtemp(prefix="motion-e2e-"))
        self.addCleanup(shutil.rmtree, work)
        for name, bump in (("candidate", 15), ("approved", None)):
            (work / name).mkdir()
            for t in range(COUNT):
                Image.fromarray(sprite(t, bump=t == bump)).save(work / name / f"{t:04d}.png")
        out = work / "evidence"
        result = subprocess.run([str(ROOT / "tools/verify/motion.py"), str(work / "candidate"), "--approved",
                                 str(work / "approved"), "--display-size", "120", "--desktop-size", "640x360",
                                 "--out", str(out)], capture_output=True, text=True, timeout=180)
        self.assertEqual(result.returncode, 2, result.stderr)
        summary = json.loads((out / "summary.json").read_text())
        self.assertIn(15, summary["strong_frames"])
        self.assertEqual(summary["approved_flag_count"], 0)
        for name in ("filmstrip-dark.png", "filmstrip-light.png", "onion.png", "difference.png", "outline-trail.png",
                     "flagged/frame-0015.png", "side-by-side.mp4", "desktop.mp4", "report.md", "checks.json"):
            self.assertTrue((out / name).is_file(), name)
        for name, digest in summary["artifacts_sha256"].items():
            self.assertEqual(motion.sha256(out / name), digest)
        probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "stream=width,height",
                                                    "-of", "json", str(out / "side-by-side.mp4")]))
        self.assertEqual((probe["streams"][0]["width"], probe["streams"][0]["height"]), (640, 360))


if __name__ == "__main__":
    unittest.main()
