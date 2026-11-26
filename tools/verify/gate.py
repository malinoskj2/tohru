#!/usr/bin/env python3
"""One motion gate for character animation: every automatic check, then a package for a blind reviewer.

Stages, each adding findings in what a viewer would see:
  turn     tools/verify/turn.py: head and eye volume, outline pops, ghosts, slides, horn tips, rhythm
  motion   tools/verify/motion.py: whole-silhouette pops, flips, vanishing frames, rigid slides
  flow     dense optical flow: drawing swaps the flow cannot explain, scale pulses, colour drift
  parts    SAM 2.1 video masks for the crest and each eye: size past the end views, snaps, splits, drops
  identity OpenCLIP ViT-H-14 similarity of every frame to the model sheet's nearest view
A clip passes only when no stage finds anything. The review package (`review/`) holds what the blind
reviewer gets: the clip at real size and speed, every frame in order at real size, a slowed head
close-up and time slices, with no metrics and no label.
"""
import argparse
import datetime
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import uuid

import cv2
import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
LIGHT = (236, 233, 240)
APPROVED = Path("/home/jesse/projects/tohru/.tmp/integration-top-level/flight/evidence/landing-review-evidence/after/right.mp4")
APPROVED_SHA = "888cfa5b03e4"
CELL_165 = ROOT / "tools/dragon/art/legacy-body.webp", (1500, 6000, 1800, 6300)


def module(name):
    spec = importlib.util.spec_from_file_location(name, HERE / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


turn = module("turn")

# Relative units or display pixels, so verdicts do not depend on source resolution. Calibrated on the
# owner-rejected turns and the accepted flights, landings and rendered 3D turn (tools/verify/gate-corpus.json).
LIMITS = {
    "volume": 0.12,          # silhouette area beyond both end views, for 3+ frames
    "swap": 3.0,             # flow warp error over the clip's usual error, on a frame that also jumps
    "scale-pulse": 0.10,     # head scale from flow, integrated, beyond both end views
    "color-drift": 7.0,      # CIELAB distance of the subject's colour from the path between end views
    "color-flicker": 3.0,    # one-frame CIELAB jump against the neighbouring steps
    "part-scale": 0.30,      # crest or eye mask area beyond both end views, for 3+ frames
    "part-snap": 0.35,       # 1 - IoU of a part with its previous mask, beyond 3x its usual change
    "part-drop": 0.55,       # an eye's mask loses this share for 1-3 frames and comes back
    "identity": 0.06,        # similarity to the model sheet below both end views, for 2+ frames
    "wink": 1,               # an eye found on both sides of a 1-3 frame gap is missing inside it
}

MOTION_BLOCKING = {"vanish", "mirror-flip", "pose-snap", "rigid-motion", "transparent-holes", "edge-contact", "loop-drift"}

WORDS = {
    "volume": "the whole body swells or shrinks while turning (volume pulse)",
    "swap": "one drawing is swapped for another instead of moving into it (morph or pop)",
    "scale-pulse": "the head zooms bigger or smaller and back while turning (bobblehead swell)",
    "color-drift": "the colours shift away from both end views (palette drift)",
    "color-flicker": "the colours jump for a frame (flicker)",
    "part-scale": "a horn, crest or eye grows or shrinks past both end views",
    "part-snap": "a horn, crest or eye changes shape in one frame (redrawn, split or doubled)",
    "part-drop": "an eye shrinks or vanishes for a frame or two and comes back (wink)",
    "wink": "an eye vanishes for one to three frames and comes back (wink)",
    "identity": "partway through she looks less like the model sheet than at either end (different creature)",
}


def beyond_ends(series, first, last, limit, smooth=3, minimum=3):
    """Frames whose smoothed value lies beyond both end views by more than limit, and the peak."""
    s = np.array(series, float)
    good = np.isfinite(s)
    if good.sum() < 5:
        return [], 0.0, 0
    s[~good] = np.nanmedian(s)
    sm = ndimage.median_filter(s, smooth) if smooth > 1 else s
    n = len(s)
    a0 = np.median(sm[max(0, first - 3):first + 1])
    a1 = np.median(sm[last:min(n, last + 4)])
    lo, hi = min(a0, a1), max(a0, a1)
    if lo <= 0:
        return [], 0.0, 0
    rel = np.where(sm > hi, sm / hi - 1, np.where(sm < lo, sm / lo - 1, 0.0))
    over = [t for t in range(first, last + 1) if abs(rel[t]) > limit]
    if len(over) < minimum:
        return [], 0.0, 0
    peak = max(over, key=lambda t: abs(rel[t]))
    return over, float(rel[peak]), peak


class Gate:
    def __init__(self, frames, first, last, display_size, fps):
        self.frames = frames
        self.n = len(frames)
        self.first, self.last = first, last
        self.display_size = display_size
        self.fps = fps
        self.scale = display_size / max(frames[0].shape[:2])
        self.findings = []
        self.metrics = {}
        self.stages = {}
        self.advisory = []

    def add(self, stage, kind, frames_hit, value, detail, limit=None):
        limit = LIMITS[kind] if limit is None else limit
        frames_hit = sorted({int(f) for f in frames_hit})
        self.findings.append({"stage": stage, "check": kind, "frames": frames_hit, "value": round(float(value), 3),
                              "limit": limit, "severity": round(abs(float(value)) / limit, 2),
                              "what": WORDS[kind], "detail": detail})

    def window(self):
        return range(self.first, self.last + 1)

    # Stage: turn.py
    def run_turn(self):
        F, findings, metrics, head, _ = turn.analyse(self.frames, self.first, self.last, self.scale)
        for f in findings:
            f["stage"] = "turn"
        self.findings += findings
        self.F, self.head = F, head
        self.metrics["turn"] = metrics
        self.stages["turn"] = len(findings)

    # Stage: motion.py, run as its own tool so its evidence folder stays complete.
    def run_motion(self, source, out, approved):
        cmd = [sys.executable, str(HERE / "motion.py"), str(source), f"--range={self.first}-{self.last}",
               "--display-size", str(self.display_size), "--fps", str(self.fps), "--label", "gate", "--out", str(out)]
        if approved:
            cmd += ["--approved", str(approved)]
        subprocess.run(cmd, capture_output=True, text=True)
        try:
            checks = json.loads((out / "checks.json").read_text())
        except (OSError, ValueError):
            self.stages["motion"] = "error"
            return
        flags = checks if isinstance(checks, list) else checks.get("flags", checks.get("ranked", []))
        strong = [f for f in flags if f.get("strength", 0) >= 2 and f.get("beyond_approved") is not False]
        # Area and outline trends are noisy on hand-drawn flight (an accepted flight raises 3.1 where a
        # rejected turn raises 2.8); in a turn window the turn, flow and parts stages measure them calibrated.
        self.advisory = [f for f in strong if f.get("check") not in MOTION_BLOCKING]
        strong = [f for f in strong if f.get("check") in MOTION_BLOCKING]
        for f in strong:
            self.findings.append({"stage": "motion", "check": f"motion:{f.get('check')}", "frames": [int(f.get("peak", 0))],
                                  "value": round(float(f.get("strength", 0)), 3), "limit": 2, "severity": round(float(f.get("strength", 0)) / 2, 2),
                                  "what": f.get("what") or f.get("description") or f.get("check"), "detail": f.get("detail", "")})
        self.stages["motion"] = len(strong)

    # Stage: optical flow and colour.
    def run_flow(self):
        size = max(96, int(self.display_size * 2))
        k = size / max(self.frames[0].shape[:2])
        small = [cv2.resize(f, None, fx=k, fy=k, interpolation=cv2.INTER_AREA) for f in self.frames]
        alpha = [s[..., 3].astype(np.float32) / 255 for s in small]
        flat = [turn.composite(s, LIGHT) for s in small]
        gray = [cv2.cvtColor(f, cv2.COLOR_RGB2GRAY) for f in flat]
        lab = [cv2.cvtColor(f, cv2.COLOR_RGB2LAB).astype(np.float32) * [100 / 255, 1, 1] - [0, 128, 128] for f in flat]
        dis = cv2.DISOpticalFlow_create(cv2.DISOPTICAL_FLOW_PRESET_MEDIUM)
        h, w = gray[0].shape
        gx, gy = np.meshgrid(np.arange(w, dtype=np.float32), np.arange(h, dtype=np.float32))
        n = self.n
        warp_err = np.zeros(n)
        step = np.zeros(n)
        log_scale = np.zeros(n)
        area = np.array([a.sum() for a in alpha])
        sx, sy, rmax = self.head
        for t in range(1, n):
            fl = dis.calc(gray[t], gray[t - 1], None)
            warped = cv2.remap(flat[t - 1].astype(np.float32), gx + fl[..., 0], gy + fl[..., 1], cv2.INTER_LINEAR,
                               borderMode=cv2.BORDER_REPLICATE)
            fg = (alpha[t] > 0.5) | (alpha[t - 1] > 0.5)
            if fg.sum() < 20:
                continue
            warp_err[t] = np.abs(warped - flat[t]).mean(axis=2)[fg].mean()
            step[t] = np.abs(flat[t].astype(np.float32) - flat[t - 1]).mean(axis=2)[fg].mean()
            # Head scale change between frames: a similarity fit to the flow inside the head disk.
            disk = ((gx - sx[t] * k) ** 2 + (gy - sy[t] * k) ** 2 < (0.8 * rmax * k) ** 2) & (alpha[t] > 0.9)
            ys, xs = np.nonzero(disk)
            if len(xs) > 30:
                pick = np.linspace(0, len(xs) - 1, min(400, len(xs))).astype(int)
                dst = np.stack([xs[pick], ys[pick]], 1).astype(np.float32)
                src = dst + fl[ys[pick], xs[pick]]
                m, _ = cv2.estimateAffinePartial2D(src, dst, method=cv2.RANSAC, ransacReprojThreshold=1.5)
                if m is not None:
                    log_scale[t] = math.log(max(1e-3, math.hypot(m[0, 0], m[1, 0])))
        cum = np.cumsum(log_scale)
        win = list(self.window())
        # Volume: whole silhouette area.
        over, peak_rel, peak = beyond_ends(area, self.first, self.last, LIMITS["volume"])
        if over:
            self.add("flow", "volume", over, peak_rel, f"silhouette area {peak_rel*100:+.0f}% beyond both end views at frame {peak}; "
                     f"{len(over)} frames between {min(over)} and {max(over)}")
        # Scale pulse: the head's own size from motion, independent of outline and eye detection.
        over, peak_rel, peak = beyond_ends(np.exp(cum), self.first, self.last, LIMITS["scale-pulse"])
        if over:
            self.add("flow", "scale-pulse", over, peak_rel, f"the flow inside the head says it is {peak_rel*100:+.0f}% beyond both end views' size "
                     f"at frame {peak} ({len(over)} frames)")
        # Swaps: frames where flow cannot explain the change, on a frame that also changes a lot.
        moving = [warp_err[t] for t in win if t > 0 and step[t] > 0.5]
        usual = float(np.median(moving)) if moving else 0.0
        usual_step = float(np.median([step[t] for t in win if t > 0 and step[t] > 0.5] or [0]))
        swaps = []
        for t in win:
            if t < 1 or usual <= 0:
                continue
            around = [warp_err[j] for j in range(max(1, t - 4), min(n, t + 5)) if j != t and step[j] > 0.5]
            local = float(np.median(around)) if around else usual
            ratio = warp_err[t] / max(local, usual, 0.5)
            if ratio > LIMITS["swap"] and step[t] > 1.5 * usual_step and warp_err[t] > 2.0:
                swaps.append((t, ratio))
        if swaps:
            pk = max(swaps, key=lambda s: s[1])
            self.add("flow", "swap", [s[0] for s in swaps], pk[1], f"at frame {pk[0]} the flow leaves {warp_err[pk[0]]:.1f} levels unexplained, "
                     f"{pk[1]:.1f}x the neighbouring frames")
        # Colour: the subject's mean CIELAB against the straight path between the two end views.
        mean_lab = np.array([lab[t][alpha[t] > 0.9].mean(axis=0) if (alpha[t] > 0.9).sum() > 20 else [np.nan] * 3 for t in range(n)])
        a0 = np.nanmedian(mean_lab[max(0, self.first - 2):self.first + 1], axis=0)
        a1 = np.nanmedian(mean_lab[self.last:self.last + 3], axis=0)
        drift = np.zeros(n)
        for t in win:
            wgt = (t - self.first) / max(1, self.last - self.first)
            # Distance from the segment between end colours, not from its linear midpoint: the middle of a
            # turn may show more of one end's colours earlier than the other.
            d = mean_lab[t] - a0
            seg = a1 - a0
            u = float(np.clip(np.dot(d, seg) / max(1e-6, np.dot(seg, seg)), 0, 1)) if np.dot(seg, seg) > 1e-6 else wgt
            drift[t] = float(np.linalg.norm(mean_lab[t] - (a0 + u * seg)))
        over = [t for t in win if drift[t] > LIMITS["color-drift"]]
        if len(over) >= 2:
            pk = max(over, key=lambda t: drift[t])
            self.add("flow", "color-drift", over, drift[pk], f"mean colour {drift[pk]:.1f} ΔE off the path between the end views at frame {pk}")
        jumps = np.r_[0, np.linalg.norm(np.diff(mean_lab, axis=0), axis=1)]
        flick = []
        for t in win:
            if 1 <= t < n - 1:
                around = [jumps[j] for j in range(max(1, t - 4), min(n, t + 5)) if j not in (t, t + 1)]
                base = max(float(np.median(around)) if around else 0, 0.4)
                there_and_back = jumps[t] > LIMITS["color-flicker"] and jumps[t + 1] > LIMITS["color-flicker"] and \
                    np.linalg.norm(mean_lab[t + 1] - mean_lab[t - 1]) < 0.5 * min(jumps[t], jumps[t + 1])
                if there_and_back and jumps[t] > 3 * base:
                    flick.append((t, jumps[t]))
        if flick:
            pk = max(flick, key=lambda f: f[1])
            self.add("flow", "color-flicker", [f[0] for f in flick], pk[1], f"colour jumps {pk[1]:.1f} ΔE at frame {pk[0]} and back")
        # Winks: turn.py's eye-drop only sees a one-frame loss; owners also saw two- and three-frame winks.
        count = np.array(self.metrics["turn"]["eye_count"])
        winks = []
        for t in win:
            for span in (1, 2, 3):
                if t < 1 or t + span >= n or t + span > self.last + 1:
                    break
                outside = min(count[t - 1], count[t + span])
                if outside > 0 and count[t:t + span].max() < outside:
                    winks.append(t)
                    break
        if winks:
            self.add("flow", "wink", winks, len(winks), f"an eye is missing at frames {', '.join(map(str, winks))} with both eyes either side")
        self.metrics["flow"] = {"area": area.tolist(), "warp_error": warp_err.tolist(), "step": step.tolist(),
                                "head_scale": np.exp(cum).tolist(), "color_drift": drift.tolist(), "color_jump": jumps.tolist()}
        self.stages["flow"] = sum(1 for f in self.findings if f["stage"] == "flow")

    # Stage: SAM 2.1 part tracking.
    def run_parts(self, device):
        import torch
        from transformers import Sam2VideoModel, Sam2VideoProcessor
        path = Path(os.environ.get("MODEL_ROOT", "/mnt/ext-ssd/models")) / "by-name/sam2.1"
        model = Sam2VideoModel.from_pretrained(path).to(device, dtype=torch.bfloat16).eval()
        proc = Sam2VideoProcessor.from_pretrained(path)
        seq = list(range(self.first, self.last + 1))
        rgb = [turn.composite(self.frames[t], LIGHT) for t in seq]
        f0 = self.F[self.first]
        eyes = f0.eyes()[:2]
        if not eyes:
            self.stages["parts"] = "no eyes found on the first frame"
            return
        ex, ey = eyes[0]["x"], eyes[0]["y"]
        rmax = self.head[2]
        session = proc.init_video_session(video=[Image.fromarray(r) for r in rgb], inference_device=device, dtype=torch.bfloat16)
        names = {}

        def prompt(name, points):
            oid = len(names) + 1
            names[oid] = name
            proc.add_inputs_to_inference_session(session, frame_idx=0, obj_ids=oid, input_points=[[[list(map(float, p)) for p in points]]],
                                                 input_labels=[[[1] * len(points)]])
            with torch.inference_mode():
                model(inference_session=session, frame_idx=0)

        x_lo = int(max(0, ex - rmax))
        ys, xs = np.nonzero(f0.mask[:, x_lo:int(ex + rmax)])
        top_y = ys.min()
        top_x = xs[ys == top_y].mean() + x_lo
        prompt("crest", [(top_x, top_y + 0.12 * (ey - top_y))])
        for i, e in enumerate(eyes):
            prompt(f"eye-{i + 1}", [(e["x"], e["y"])])
        masks = {}
        with torch.inference_mode():
            for out in model.propagate_in_video_iterator(session):
                m = proc.post_process_masks([out.pred_masks], original_sizes=[list(rgb[0].shape[:2])], binarize=True)[0]
                masks[out.frame_idx] = m[:, 0].cpu().numpy()
        peak_gib = torch.cuda.max_memory_allocated() / 2 ** 30 if device == "cuda" else 0
        del model, session
        order = [names[o] for o in sorted(names)]
        series = {}
        for k, name in enumerate(order):
            area = np.full(self.n, np.nan)
            snap = np.zeros(self.n)
            comps = np.zeros(self.n, int)
            prev = None
            for i, t in enumerate(seq):
                m = masks[i][k]
                area[t] = m.sum()
                lab, c = ndimage.label(m)
                if c:
                    sizes = ndimage.sum(m, lab, range(1, c + 1))
                    comps[t] = int((sizes > 0.15 * sizes.max()).sum())
                if prev is not None and m.sum() > 10 and prev.sum() > 10:
                    # Align centroids so a moving part is not a change in shape.
                    cy0, cx0 = ndimage.center_of_mass(prev)
                    cy1, cx1 = ndimage.center_of_mass(m)
                    moved = ndimage.shift(prev.astype(np.float32), (cy1 - cy0, cx1 - cx0), order=0) > 0.5
                    snap[t] = 1 - (moved & m).sum() / max(1, (moved | m).sum())
                prev = m
            series[name] = {"area": area.tolist(), "snap": snap.tolist(), "components": comps.tolist()}
            disp = math.sqrt(np.nanmedian(area)) * self.scale
            if disp < 1.5:
                continue
            over, rel, pk = beyond_ends(area, self.first, self.last, LIMITS["part-scale"])
            if over:
                self.add("parts", "part-scale", over, rel, f"the {name} mask is {rel*100:+.0f}% beyond both end views' area at frame {pk}")
            snaps = []
            for t in seq[1:]:
                around = [snap[j] for j in range(max(self.first + 1, t - 4), min(self.last + 1, t + 5)) if j != t]
                usual = max(float(np.median(around)) if around else 0, 0.05)
                if snap[t] > LIMITS["part-snap"] and snap[t] > 3 * usual:
                    split = comps[t] != comps[t - 1]
                    snaps.append((t, snap[t], split))
            if snaps:
                pk = max(snaps, key=lambda s: s[1])
                self.add("parts", "part-snap", [s[0] for s in snaps], pk[1],
                         f"the {name} keeps only {(1-pk[1])*100:.0f}% of its shape from frame {pk[0]-1} to {pk[0]}"
                         + (f" and splits from {comps[pk[0]-1]} to {comps[pk[0]]} pieces" if pk[2] else ""))
            if name.startswith("eye"):
                drops = []
                for t in seq[1:-1]:
                    for span in (1, 2, 3):
                        if t + span > self.last:
                            break
                        ref = 0.5 * (area[t - 1] + area[t + span])
                        inner = area[t:t + span]
                        if ref > 0 and np.all(inner < (1 - LIMITS["part-drop"]) * ref):
                            drops.append((t, 1 - inner.min() / ref))
                            break
                if drops:
                    pk = max(drops, key=lambda d: d[1])
                    self.add("parts", "part-drop", [d[0] for d in drops], pk[1], f"the {name} mask loses {pk[1]*100:.0f}% at frame {pk[0]} and comes back")
        self.metrics["parts"] = {"order": order, "series": series, "peak_gib": round(peak_gib, 2)}
        self.stages["parts"] = sum(1 for f in self.findings if f["stage"] == "parts")

    # Stage: identity against the model sheet.
    def run_identity(self, sheet, device):
        import torch
        import open_clip
        path = Path(os.environ.get("MODEL_ROOT", "/mnt/ext-ssd/models")) / "by-name/openclip-vit-h-14"
        model, _, preprocess = open_clip.create_model_and_transforms("ViT-H-14", pretrained=str(path / "open_clip_model.safetensors"))
        model = model.to(device).eval()

        def crop(rgba):
            a = rgba[..., 3]
            ys, xs = np.nonzero(a > 128)
            if not len(xs):
                return Image.new("RGB", (224, 224), LIGHT)
            x0, x1, y0, y1 = xs.min(), xs.max(), ys.min(), ys.max()
            side = int(max(x1 - x0, y1 - y0) * 1.08) + 2
            cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
            im = Image.fromarray(turn.composite(rgba, LIGHT))
            canvas = Image.new("RGB", (side, side), LIGHT)
            canvas.paste(im.crop((cx - side // 2, cy - side // 2, cx + side - side // 2, cy + side - side // 2)), (0, 0))
            return canvas

        refs = [crop(np.asarray(r)) for r in sheet]

        @torch.inference_mode()
        def embed(images):
            out = []
            for i in range(0, len(images), 16):
                batch = torch.stack([preprocess(im) for im in images[i:i + 16]]).to(device)
                e = model.encode_image(batch).float()
                out.append(torch.nn.functional.normalize(e, dim=-1).cpu())
            return torch.cat(out).numpy()

        ref_e = embed(refs)
        frame_e = embed([crop(f) for f in self.frames])
        sim = (frame_e @ ref_e.T).max(axis=1)
        nearest = (frame_e @ ref_e.T).argmax(axis=1)
        # Frames against their own end views, as a second reference that needs no sheet.
        ends = frame_e[[self.first, self.last]]
        self_sim = (frame_e @ ends.T).max(axis=1)
        peak_gib = torch.cuda.max_memory_allocated() / 2 ** 30 if device == "cuda" else 0
        del model
        for series, label in ((sim, "the model sheet"),):
            lo = min(np.median(series[max(0, self.first - 2):self.first + 1]), np.median(series[self.last:self.last + 3]))
            sm = ndimage.median_filter(series, 3)
            over = [t for t in self.window() if lo - sm[t] > LIMITS["identity"]]
            if len(over) >= 2:
                pk = max(over, key=lambda t: lo - sm[t])
                self.add("identity", "identity", over, lo - sm[pk], f"similarity to {label} drops to {sm[pk]:.3f} at frame {pk}, "
                         f"{lo - sm[pk]:.3f} below the weaker end view ({len(over)} frames)")
        self.metrics["identity"] = {"sheet": sim.tolist(), "nearest_view": nearest.tolist(), "ends": self_sim.tolist(),
                                    "peak_gib": round(peak_gib, 2)}
        self.stages["identity"] = sum(1 for f in self.findings if f["stage"] == "identity")


def review_package(out, frames, first, last, display_size, fps, head):
    """What the blind reviewer sees: playback at real size and speed, every frame in order at real size,
    a slowed head close-up and time slices. No metrics, no label, no flags."""
    rev = out / "review"
    rev.mkdir(exist_ok=True)
    tmp = rev / "frames"
    tmp.mkdir(exist_ok=True)
    size = display_size // 2 * 2
    for t, f in enumerate(frames):
        Image.fromarray(turn.composite(f, LIGHT)).save(tmp / f"{t:04d}.png")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", str(fps), "-i", str(tmp / "%04d.png"),
                    "-vf", f"scale={size}:{size}:flags=area,pad={size + 32}:{size + 32}:16:16:color=0xECE9F0",
                    "-c:v", "libx264", "-crf", "10", "-pix_fmt", "yuv420p", str(rev / "normal-speed.mp4")], check=True)
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", str(fps), "-i", str(tmp / "%04d.png"),
                    "-vf", f"scale={size * 2}:{size * 2}:flags=area", "-c:v", "libx264", "-crf", "10", "-pix_fmt", "yuv420p",
                    str(rev / "double-size.mp4")], check=True)
    # Every frame of the clip in order at real size: a flipbook the reviewer pages left to right.
    small = [Image.fromarray(turn.composite(f, LIGHT)).resize((size, size), Image.LANCZOS) for f in frames]
    cols = 12
    rows = (len(small) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (size + 4), rows * (size + 16)), LIGHT)
    d = ImageDraw.Draw(sheet)
    for i, im in enumerate(small):
        x, y = i % cols * (size + 4), i // cols * (size + 16)
        sheet.paste(im, (x, y + 14))
        d.text((x + 2, y), str(i), fill=(90, 90, 90))
    sheet.save(rev / "every-frame-real-size.png")
    # Head close-up at quarter speed.
    sx, sy, rmax = head
    half = int(1.15 * rmax)
    cx, cy = int(np.median(sx[first:last + 1])), int(np.median(sy[first:last + 1]))
    pad = 2 * half
    crop = (f"pad=iw+{2 * pad}:ih+{2 * pad}:{pad}:{pad}:color=0xECE9F0,crop={2 * half}:{2 * half}:{cx - half + pad}:{cy - half + pad},"
            "scale=480:480:flags=lanczos")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", str(fps / 4), "-i", str(tmp / "%04d.png"), "-vf", crop,
                    "-c:v", "libx264", "-crf", "12", "-pix_fmt", "yuv420p", "-r", "24", str(rev / "head-quarter-speed.mp4")], check=True)
    # Time slices: one row and one column through the head stacked over time. A smooth turn draws smooth
    # curves; a snap is a kink, a pulse a bulge, a cross-fade a doubled line.
    flat = [turn.composite(f, LIGHT) for f in frames]
    row = int(np.median(sy))
    col = int(np.median(sx))
    rows_img = np.stack([f[row] for f in flat])
    cols_img = np.stack([f[:, col] for f in flat])
    k = 4
    Image.fromarray(np.repeat(rows_img, k, axis=0)).save(rev / "time-slice-eye-row.png")
    Image.fromarray(np.repeat(cols_img, k, axis=0)).save(rev / "time-slice-head-column.png")
    for p in tmp.iterdir():
        p.unlink()
    tmp.rmdir()
    return rev


def load_sheet(path):
    if path:
        p = Path(path)
        files = sorted(p.glob("*.png")) if p.is_dir() else [p]
        return [Image.open(f).convert("RGBA") for f in files]
    src, box = CELL_165
    return [Image.open(src).convert("RGBA").crop(box)]


def run(args):
    frames, inputs = turn.load(args.source)
    n = len(frames)
    first, last = map(int, args.range.split("-")) if args.range else (1, n - 2)
    gate = Gate(frames, first, last, args.display_size, args.fps)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = Path(args.out) if args.out else ROOT / ".tmp/verify/gate" / f"{args.label}-{stamp}-{uuid.uuid4().hex[:6]}"
    out.mkdir(parents=True, exist_ok=True)
    gate.run_turn()
    if not args.no_motion:
        approved = Path(args.approved) if args.approved else (APPROVED if APPROVED.exists() else None)
        gate.run_motion(args.source, out / "motion", approved)
    gate.run_flow()
    if args.deep:
        import torch
        device = "cuda" if torch.cuda.is_available() else "cpu"
        gate.run_parts(device)
        gate.run_identity(load_sheet(args.sheet), device)
    gate.findings.sort(key=lambda f: -f["severity"])
    verdict = "fail" if gate.findings else "pass"
    result = {"label": args.label, "source": str(Path(args.source).resolve()), "inputs": inputs, "frames": n,
              "window": [first, last], "display_size": args.display_size, "deep": args.deep, "verdict": verdict,
              "score": turn.score(gate.findings), "stages": gate.stages, "findings": gate.findings,
              "advisory": [{"check": f"motion:{f.get('check')}", "frame": f.get("peak"), "strength": f.get("strength")} for f in gate.advisory],
              "limits": {**turn.LIMITS, **LIMITS}}
    (out / "gate.json").write_text(json.dumps(result, indent=1))
    (out / "metrics.json").write_text(json.dumps(gate.metrics, default=float))
    lines = [f"# Gate: {args.label}", "", f"Source `{result['source']}`, {n} frames, window {first}–{last}, shown at {args.display_size} px.",
             f"Verdict **{verdict.upper()}**, fault score {result['score']}. Stages run: "
             + ", ".join(f"{k} ({v})" for k, v in gate.stages.items()) + ".", ""]
    if gate.findings:
        lines += ["| Severity | Stage | What a viewer sees | Frames | Measured |", "| ---: | --- | --- | --- | --- |"]
        for f in gate.findings:
            fr = f["frames"]
            span = f"{fr[0]}–{fr[-1]}" if len(fr) > 1 else str(fr[0])
            lines.append(f"| {f['severity']} | {f['stage']} | {f['what']} | {span} | {f['detail']} |")
    else:
        lines.append("No check exceeded its limit. This is necessary, not sufficient: the blind reviewer and the owner still watch it.")
    (out / "report.md").write_text("\n".join(lines) + "\n")
    if not args.no_media:
        review_package(out, frames, first, last, args.display_size, args.fps, gate.head)
    print(json.dumps({"verdict": verdict, "score": result["score"], "checks": sorted({f["check"] for f in gate.findings}),
                      "evidence": str(out)}))
    return 2 if gate.findings else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("source", help="RGBA frame directory or alpha video")
    ap.add_argument("--range", help="turn window first-last, a few frames past each end view (default: whole clip)")
    ap.add_argument("--label", default="clip")
    ap.add_argument("--display-size", type=int, default=166, help="art canvas long side on screen")
    ap.add_argument("--fps", type=float, default=24)
    ap.add_argument("--deep", action="store_true", help="also run SAM 2.1 part tracking and OpenCLIP identity (GPU; run under gpu-run)")
    ap.add_argument("--sheet", help="model sheet: a PNG or a folder of RGBA view PNGs (default: Cell 165)")
    ap.add_argument("--approved", help="approved clip for motion.py's calibration (default: the approved right landing)")
    ap.add_argument("--no-motion", action="store_true")
    ap.add_argument("--no-media", action="store_true")
    ap.add_argument("--out")
    raise SystemExit(run(ap.parse_args()))


if __name__ == "__main__":
    main()
