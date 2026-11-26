#!/usr/bin/env python3
"""Judge a character turn the way a viewer does and say in plain words what looks wrong."""
import argparse
import datetime
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import uuid

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[2]
LIGHT = (236, 233, 240)
DARK = (22, 20, 26)
ANGLES = np.arange(-165, -14, 5)

# Thresholds are in display pixels (what the owner sees) or relative units, so the verdict does not
# depend on the art's source resolution. Calibrated on the owner-rejected Rive dragon turns.
LIMITS = {
    "head-scale": 0.12,       # cranium height beyond the range of the turn's two end views
    "eye-scale": 0.12,        # eye height beyond the same range
    "outline-pop": 1.4,       # display px off the path between neighbouring drawings on ones; x frames apart / 2
    "ghost": 0.42,            # dissolve gain; natural flight frames stay under 0.3
    "feature-slide": 0.9,     # display px the eye leaves its own smooth path
    "eye-drop": 0.35,         # relative loss of eye area in one or two frames (wink or vanish)
    "uneven-motion": 2.4,     # step over the turn's usual step; two-frame velocity over its local median
    "shape-change": 0.10,     # aligned head-outline change per frame beyond its local trend
    "size-pop": 0.07,         # one-frame change in head size 3x the neighbouring steps between drawings
    "cut-edge": 1,            # straight horizontal alpha seams or frame-border clips on the head, per frame
    "horn-count": 1,          # change in the number of outline tips that lasts under 4 frames
}

WORDS = {
    "head-scale": "the head swells or shrinks while turning (volume pulse)",
    "eye-scale": "the eyes grow or shrink while turning (features change size, off-model)",
    "outline-pop": "part of the head outline pops out and back (silhouette wobble)",
    "ghost": "two drawings are cross-faded, so a faint second horn, frill or face shows through (blend/ghost lines)",
    "feature-slide": "the eye jumps off its path over the head (features sliding over the head)",
    "eye-drop": "an eye shrinks or disappears for a frame or two (wink)",
    "uneven-motion": "the turn jerks or stalls instead of moving evenly (uneven motion)",
    "shape-change": "the horns and frill change shape instead of turning (off-model drawing)",
    "horn-count": "horn or frill tips appear and vanish (off-model drawing)",
    "size-pop": "the head jumps bigger or smaller in a single frame (size pop)",
    "cut-edge": "a straight cut line or seam shows on the head, or the head is clipped (pasted-on look)",
}


def natural(path):
    return [int(p) if p.isdigit() else p for p in re.split(r"(\d+)", path.name)]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def load(source):
    source = Path(source).resolve()
    if source.is_dir():
        files = sorted((p for p in source.iterdir() if p.suffix.lower() == ".png"), key=natural)
        return [np.asarray(Image.open(p).convert("RGBA")) for p in files], {str(source): "dir"}
    probe = json.loads(subprocess.check_output(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
         "-of", "json", str(source)]))["streams"][0]
    w, h = probe["width"], probe["height"]
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(source), "-f", "rawvideo", "-pix_fmt", "rgba", "-"],
                         check=True, capture_output=True).stdout
    frames = np.frombuffer(raw, np.uint8).reshape(-1, h, w, 4)
    return list(frames), {str(source): sha256(source)}


class Frame:
    def __init__(self, rgba):
        self.rgba = rgba
        a = rgba[..., 3].astype(np.float32) / 255
        self.alpha = a
        self.mask = a > 0.5
        ys, xs = np.nonzero(self.mask)
        self.box = (xs.min(), ys.min(), xs.max() + 1, ys.max() + 1) if len(xs) else None
        lum = rgba[..., :3].astype(np.float32) @ np.array([0.299, 0.587, 0.114], np.float32) / 255
        self.lum = lum
        self.pre = rgba[..., :3].astype(np.float32) * a[..., None]
        self.flat = self.pre + np.array(LIGHT, np.float32) * (1 - a[..., None])

    def eyes(self):
        x0, y0, x1, y1 = self.box
        dark = (self.lum < 0.45) & (self.alpha > 0.9)
        dark[int(y0 + 0.6 * (y1 - y0)):] = False
        lab, n = ndimage.label(dark)
        found = []
        for i, sl in enumerate(ndimage.find_objects(lab)):
            blob = lab[sl] == i + 1
            area = int(blob.sum())
            h, w = blob.shape
            if area < 35 or h < 7 or w < 5 or not 0.6 <= h / w <= 3.2:
                continue
            yy, xx = np.nonzero(blob)
            found.append({"area": area, "x": float(xx.mean() + sl[1].start), "y": float(yy.mean() + sl[0].start),
                          "height": float(4 * yy.std()), "width": float(4 * xx.std())})
        return sorted(found, key=lambda e: -e["area"])[:3]


def radial(mask, cx, cy, rmax, amount=False):
    """Per ray, the farthest opaque pixel, or with amount the length of shape the ray crosses
    (continuous when a thin horn sweeps across it)."""
    out = []
    radii = np.arange(4, rmax)
    for deg in ANGLES:
        t = math.radians(deg)
        xs = np.clip((cx + radii * math.cos(t)).astype(int), 0, mask.shape[1] - 1)
        ys = np.clip((cy + radii * math.sin(t)).astype(int), 0, mask.shape[0] - 1)
        inside = mask[ys, xs]
        if amount:
            out.append(float(inside.sum()))
            continue
        hit = np.nonzero(inside)[0]
        out.append(float(radii[hit[-1]]) if len(hit) else 0.0)
    return np.array(out)


def sdf(mask):
    return ndimage.distance_transform_edt(~mask) - ndimage.distance_transform_edt(mask)


def outline_offset(masks, t, p, q, disk):
    """How far frame t's outline sits from the outline interpolated between p and q, in pixels:
    the 98th percentile over its boundary inside the head disk, and where the worst part is."""
    w = (t - p) / (q - p)
    mid = (1 - w) * sdf(masks[p]) + w * sdf(masks[q])
    edge = masks[t] & ~ndimage.binary_erosion(masks[t]) & disk
    if edge.sum() < 20:
        return 0.0, (0, 0)
    vals = np.abs(mid[edge])
    ys, xs = np.nonzero(edge)
    k = int(np.argmax(vals))
    return float(np.percentile(vals, 98)), (int(xs[k]), int(ys[k]))


def tips(profile):
    p = ndimage.gaussian_filter1d(profile, 1.0)
    count = 0
    for i in range(1, len(p) - 1):
        lo = min(p[max(0, i - 4):i].min(), p[i + 1:i + 5].min())
        if p[i] >= p[i - 1] and p[i] > p[i + 1] and p[i] - lo > 8:
            count += 1
    return count


def warp_to(src, dst):
    dis = cv2.DISOpticalFlow_create(cv2.DISOPTICAL_FLOW_PRESET_MEDIUM)
    gs = cv2.cvtColor(np.clip(src, 0, 255).astype(np.uint8), cv2.COLOR_RGB2GRAY)
    gd = cv2.cvtColor(np.clip(dst, 0, 255).astype(np.uint8), cv2.COLOR_RGB2GRAY)
    f = dis.calc(gd, gs, None)
    h, w = gd.shape
    gx, gy = np.meshgrid(np.arange(w, dtype=np.float32), np.arange(h, dtype=np.float32))
    return cv2.remap(src, gx + f[..., 0], gy + f[..., 1], cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)


def dissolve_gain(F, t, region):
    """How much better a 50/50 mix of motion-matched neighbours explains frame t than either alone.

    Real motion is explained by one warped neighbour; a cross-fade of two drawings that disagree in
    shape needs both, which is the faint second horn or face a viewer sees.
    """
    best, at = 0.0, 0
    c = F[t].flat
    for k in (1, 2, 3, 4):
        if t - k < 0 or t + k >= len(F):
            break
        wp, wn = warp_to(F[t - k].flat, c), warp_to(F[t + k].flat, c)
        sel = region & (np.abs(wp - wn).mean(axis=2) > 12)
        if sel.sum() < 100:
            continue
        rp = np.abs(c - wp).mean(axis=2)[sel].mean()
        rn = np.abs(c - wn).mean(axis=2)[sel].mean()
        rm = np.abs(c - 0.5 * (wp + wn)).mean(axis=2)[sel].mean()
        gain = (1 - rm / max(1e-6, min(rp, rn))) * min(1.0, 20 * sel.sum() / max(1, region.sum()))
        if gain > best:
            best, at = gain, k
    return best, at


def trend_residual(series, half=3, mask=None):
    """Distance of each value from a robust local line through its neighbours (itself excluded)."""
    s = np.asarray(series, float)
    out = np.zeros_like(s)
    for i in range(len(s)):
        idx = [j for j in range(i - half, i + half + 1) if 0 <= j < len(s) and j != i and np.isfinite(s[j])]
        if len(idx) < 3 or not np.isfinite(s[i]):
            out[i] = 0
            continue
        p = np.polyfit(idx, s[idx], 1 if len(idx) < 5 else 2)
        out[i] = s[i] - np.polyval(p, i)
    return out


def analyse(frames, first, last, display_scale):
    F = [Frame(f) for f in frames]
    n = len(F)
    eyes, anchor = [], []
    prev = None
    for t, fr in enumerate(F):
        cand = fr.eyes()
        eyes.append(cand)
        if not cand:
            anchor.append(None)
            continue
        if prev is None:
            pick = cand[0]
        else:
            pick = min(cand, key=lambda e: math.hypot(e["x"] - prev["x"], e["y"] - prev["y"]))
            if math.hypot(pick["x"] - prev["x"], pick["y"] - prev["y"]) > 45:
                pick = cand[0]
        anchor.append(pick)
        prev = pick
    ok = [a is not None for a in anchor]
    ax = np.array([a["x"] if a else np.nan for a in anchor])
    ay = np.array([a["y"] if a else np.nan for a in anchor])
    eh = np.array([a["height"] if a else np.nan for a in anchor])
    ea = np.array([a["area"] if a else np.nan for a in anchor])
    eye_count = np.array([len(e) for e in eyes])
    base_h = np.nanmedian(eh)
    rmax = int(max(60, 6.5 * base_h))
    # Smooth the anchor so the outline is measured from where the head is, not from eye noise.
    sx = ndimage.median_filter(np.nan_to_num(ax, nan=np.nanmedian(ax)), 3)
    sy = ndimage.median_filter(np.nan_to_num(ay, nan=np.nanmedian(ay)), 3)
    profiles = np.array([radial(F[t].mask, sx[t], sy[t], rmax) for t in range(n)])
    tipcount = np.array([tips(p) for p in profiles])
    amounts = np.array([radial(ndimage.gaussian_filter(F[t].alpha, 1.0) > 0.5, sx[t], sy[t], rmax, True) for t in range(n)])

    # Head disk for motion and ghost measures.
    yy, xx = np.mgrid[:F[0].mask.shape[0], :F[0].mask.shape[1]]
    step = np.zeros(n)
    ghost = np.zeros(n)
    for t in range(1, n):
        disk = (xx - sx[t]) ** 2 + (yy - sy[t]) ** 2 < rmax ** 2
        d = np.abs(F[t].pre - F[t - 1].pre).mean(axis=2)
        step[t] = d[disk].mean()
    for t in range(max(1, first), min(n - 1, last + 1)):
        x0, y0, x1, y1 = F[t].box
        region = np.zeros(F[t].mask.shape, bool)
        region[y0:int(y0 + 0.55 * (y1 - y0))] = True
        region &= F[t].alpha > 0.3
        if np.median(step[max(1, t - 2):t + 3]) > 6:
            ghost[t] = dissolve_gain(F, t, region)[0]

    # Cranium height: eye line to the top of the head, median over the columns between the eyes,
    # so a horn covering part of that band does not count.
    crown = np.full(n, np.nan)
    for t in range(n):
        es = sorted(eyes[t][:2], key=lambda e: e["x"])
        if not es:
            continue
        if len(es) == 2 and es[1]["x"] - es[0]["x"] > 12:
            x0, x1, ey = es[0]["x"], es[1]["x"], (es[0]["y"] + es[1]["y"]) / 2
        else:
            x0, x1, ey = es[0]["x"] - 12, es[0]["x"] + 12, es[0]["y"]
        # Height of the head's top edge above the eye line per column, then a grey opening wider
        # than a horn: horns and frill spikes are narrow peaks, the cranium is a wide dome.
        lo, hi = int(max(0, x0 - 8 * base_h)), int(min(F[t].mask.shape[1], x1 + 8 * base_h))
        cols = F[t].mask[:int(ey), lo:hi]
        heights = np.array([ey - np.argmax(c) if c.any() else 0.0 for c in cols.T])
        dome = ndimage.grey_opening(heights, size=max(3, int(3.2 * base_h)))
        band = dome[int(x0) - lo:int(x1) - lo + 1]
        if band.size:
            crown[t] = float(np.median(band))
    # Cut edges: painted outlines are curved, so a long dead-straight horizontal alpha edge near the
    # head is a paste seam or a chopped piece of an old head; touching the frame border is a clip.
    cut_len = int(max(32, 2.2 * base_h))
    cuts = np.zeros(n, int)
    for t in range(max(0, first), min(n, last + 1)):
        a8 = (F[t].alpha * 255).astype(np.uint8)
        edges = cv2.Canny(cv2.GaussianBlur(a8, (3, 3), 0), 40, 120)
        found = cv2.HoughLinesP(edges, 1, np.pi / 180, threshold=int(cut_len * 0.8), minLineLength=cut_len, maxLineGap=1)
        for x1, y1, x2, y2 in ([] if found is None else found[:, 0]):
            angle = abs(math.degrees(math.atan2(y2 - y1, x2 - x1))) % 180
            if min(angle, 180 - angle) <= 8 and math.hypot((x1 + x2) / 2 - sx[t], (y1 + y2) / 2 - sy[t]) < rmax:
                cuts[t] += 1
        disk = (xx - sx[t]) ** 2 + (yy - sy[t]) ** 2 < rmax ** 2
        border = np.zeros_like(disk)
        border[[0, -1], :] = border[:, [0, -1]] = True
        if (F[t].alpha[border & disk] > 0.5).sum() > 10:
            cuts[t] += 1
    # Head size: area of the head outline seen from the eye, as a linear size.
    head_size = np.sqrt(0.5 * (profiles ** 2).sum(axis=1) * math.radians(5))

    findings = []
    window = range(first, last + 1)

    def add(kind, frames_hit, value, detail):
        frames_hit = sorted(set(int(f) for f in frames_hit))
        findings.append({"check": kind, "frames": frames_hit, "value": round(float(value), 3),
                         "limit": LIMITS[kind], "severity": round(float(value) / LIMITS[kind], 2),
                         "what": WORDS[kind], "detail": detail})

    def beyond_ends(series, kind, limit, smooth, label):
        """Flag only growth or shrinkage past both ends of the turn: a real turn may change these
        measures between the two end views but never balloons past them."""
        a0 = np.nanmedian(series[max(0, first - 4):first + 1])
        a1 = np.nanmedian(series[last:min(n, last + 5)])
        lo, hi = min(a0, a1), max(a0, a1)
        filled = np.array(series, float)
        filled[~np.isfinite(filled)] = np.nanmedian(filled)
        sm = ndimage.median_filter(filled, smooth) if smooth > 1 else filled
        rel = np.where(sm > hi, sm / hi - 1, np.where(sm < lo, sm / lo - 1, 0.0))
        over = [t for t in window if abs(rel[t]) > limit]
        # A real turn can peak for a frame or a held drawing at the frontal view; a pulse lasts.
        if len(over) >= 3:
            peak = max(over, key=lambda t: abs(rel[t]))
            add(kind, over, abs(rel[peak]),
                f"{label} is {rel[peak]*100:+.0f}% beyond both ends of the turn at frame {peak} "
                f"({sm[peak]:.0f} px against {lo:.0f}-{hi:.0f}); {len(over)} frames between {min(over)} and {max(over)}")
        return rel

    rel = beyond_ends(crown, "head-scale", LIMITS["head-scale"], 3, "the cranium's height above the eyes")
    beyond_ends(eh, "eye-scale", LIMITS["eye-scale"], 3, "eye height")

    cut_frames = [t for t in window if cuts[t] >= LIMITS["cut-edge"]]
    if cut_frames:
        worst = max(cut_frames, key=lambda t: cuts[t])
        add("cut-edge", cut_frames, float(max(cuts[t] for t in cut_frames)) * min(3, len(cut_frames)) / 2,
            f"{len(cut_frames)} frames show a straight seam of {cut_len}+ px or a clipped head; {cuts[worst]} at frame {worst}")

    # Size pops: a sudden one-frame change in head size, where the frames around it barely change.
    pops = []
    for series, label in ((head_size, "head size"),):
        v = np.array(series, float)
        d = np.abs(np.diff(np.log(np.where(np.isfinite(v) & (v > 0), v, np.nan))))
        for t in window:
            if t < 1 or t >= n or not np.isfinite(d[t - 1]):
                continue
            # Held drawings step 0 between their frames, so compare with the steps between drawings.
            around = [d[k] for k in range(max(0, t - 7), min(len(d), t + 6)) if k != t - 1 and np.isfinite(d[k]) and d[k] > 0.004]
            if d[t - 1] > LIMITS["size-pop"] and d[t - 1] > 3 * max(np.median(around) if around else 0, 0.01):
                pops.append((float(d[t - 1]), t, label, v[t - 1], v[t]))
    if pops:
        peak = max(pops)
        add("size-pop", sorted({p[1] for p in pops}), peak[0],
            f"the {peak[2]} jumps {(np.exp(peak[0]) - 1) * 100:.0f}% between frames {peak[1] - 1} and {peak[1]} "
            f"({peak[3]:.0f} to {peak[4]:.0f} px) while the frames around it change little")

    # 2 outline pops per ray, display px. A drawing held on twos is one drawing: compare each new
    # drawing with the previous and next distinct ones, interpolated in time.
    held = np.zeros(n, bool)
    for t in range(1, n):
        held[t] = np.median(np.abs(amounts[t] - amounts[t - 1])) < 0.5 and np.percentile(np.abs(amounts[t] - amounts[t - 1]), 75) < 1.5
    distinct = [t for t in range(n) if not held[t]]
    worst = []
    soft = [ndimage.gaussian_filter(f.alpha, 0.8) > 0.5 for f in F]
    for i, t in enumerate(distinct):
        if t not in window or i == 0 or i == len(distinct) - 1:
            continue
        p, q = distinct[i - 1], distinct[i + 1]
        disk = (xx - sx[t]) ** 2 + (yy - sy[t]) ** 2 < rmax ** 2
        value, (px, py) = outline_offset(soft, t, p, q, disk)
        # A perfect turn held on twos or threes steps further between drawings, so the allowance
        # grows with the spacing of the neighbouring drawings (calibrated on a rendered 3D turn).
        allowed = LIMITS["outline-pop"] * (q - p) / 2
        worst.append((t, value * display_scale, int(math.degrees(math.atan2(py - sy[t], px - sx[t]))), allowed))
    pops = [w for w in worst if w[1] > w[3]]
    if pops:
        peak = max(pops, key=lambda w: w[1] / w[3])
        add("outline-pop", [p[0] for p in pops], peak[1] / peak[3] * LIMITS["outline-pop"],
            f"{len(pops)} frames; worst at frame {peak[0]}: the outline {where(peak[2])} of the eye sits {peak[1]:.1f} display px off the path between the drawings before and after it")

    # 3 ghosting.
    g = [t for t in window if ghost[t] > LIMITS["ghost"]]
    if g:
        peak = max(g, key=lambda t: ghost[t])
        add("ghost", g, ghost[peak], f"frames {', '.join(map(str, g))}; at {peak} a cross-fade of the neighbouring drawings explains the head {ghost[peak]*100:.0f}% better than movement does")

    # 4 feature slide: each new drawing's near eye against the path between the drawings before
    # and after it; held drawings are one drawing, and the allowance grows with their spacing.
    slide = np.zeros(n)
    s = []
    for i, t in enumerate(distinct):
        if t not in window or i == 0 or i == len(distinct) - 1:
            continue
        p, q = distinct[i - 1], distinct[i + 1]
        if not (ok[t] and ok[p] and ok[q]):
            continue
        w = (t - p) / (q - p)
        slide[t] = math.hypot(ax[t] - (ax[p] + w * (ax[q] - ax[p])), ay[t] - (ay[p] + w * (ay[q] - ay[p]))) * display_scale
        if slide[t] > LIMITS["feature-slide"] * (q - p) / 2:
            s.append((t, slide[t] / ((q - p) / 2)))
    if s:
        peak = max(s, key=lambda v: v[1])
        add("feature-slide", [v[0] for v in s], peak[1], f"at frame {peak[0]} the near eye is {slide[peak[0]]:.1f} display px off the path between the drawings before and after it")

    # 5 eye drop / wink.
    drops = []
    for t in window:
        if 0 < t < n - 1 and ok[t] and ok[t - 1] and ok[t + 1]:
            ref = 0.5 * (ea[t - 1] + ea[t + 1])
            if ref > 0 and (ref - ea[t]) / ref > LIMITS["eye-drop"]:
                drops.append((t, (ref - ea[t]) / ref))
        if 0 < t < n - 1 and eye_count[t] < min(eye_count[t - 1], eye_count[t + 1]):
            drops.append((t, 1.0))
    if drops:
        peak = max(drops, key=lambda d: d[1])
        add("eye-drop", [d[0] for d in drops], peak[1], f"an eye loses {peak[1]*100:.0f}% of its area or vanishes at frame {peak[0]} and comes back")

    # 6 uneven motion: one-frame pops against the turn's typical step, and stutter in the
    # two-frame velocity, which stays even for drawings held on twos.
    moving = [step[t] for t in window if step[t] > 4]
    typical = float(np.median(moving)) if moving else 0.0
    jerk = []
    if typical > 0:
        for t in window:
            if step[t] > LIMITS["uneven-motion"] * typical and step[t] > 12:
                jerk.append((t, step[t] / typical, "pop"))
            # A lone near-hold between moving frames reads as a hitch; regular holds on twos do not.
            if 1 < t < n - 2 and step[t] < typical / LIMITS["uneven-motion"] and min(step[t - 1], step[t + 1]) > 0.7 * typical \
                    and min(step[t - 2], step[t + 2]) > 0.7 * typical:
                jerk.append((t, typical / max(step[t], 1e-3), "stall"))
        # Six-frame velocity is even for drawings held on ones, twos or threes.
        pair = ndimage.uniform_filter1d(step, 6) * 6
        local = ndimage.median_filter(pair, 13)
        for t in window:
            if local[t] > 30 and not any(j[0] == t for j in jerk):
                ratio = pair[t] / local[t]
                if ratio > LIMITS["uneven-motion"] or ratio < 1 / LIMITS["uneven-motion"]:
                    jerk.append((t, max(ratio, 1 / max(ratio, 1e-3)), "stutter"))
    if jerk:
        peak = max(jerk, key=lambda j: j[1])
        pops = [j[0] for j in jerk if j[2] == "pop"]
        stalls = [j[0] for j in jerk if j[2] == "stall"]
        add("uneven-motion", [j[0] for j in jerk], peak[1],
            (f"one-frame jumps at {', '.join(map(str, pops))}; " if pops else "")
            + (f"hitches (one frame nearly stops) at {', '.join(map(str, stalls))}; " if stalls else "")
            + f"worst {peak[1]:.1f}x off the turn's usual step; "
            + f"{len(jerk)} uneven steps between {min(j[0] for j in jerk)} and {max(j[0] for j in jerk)}")

    # 7 aligned shape change beyond trend, per frame of elapsed time between distinct drawings.
    norm = amounts / np.maximum(1e-6, np.nanmean(amounts, axis=1, keepdims=True))
    change = np.zeros(n)
    for i in range(1, len(distinct)):
        t, p = distinct[i], distinct[i - 1]
        valid = (amounts[t] > 0) | (amounts[p] > 0)
        rate = np.abs(norm[t] - norm[p])[valid].mean() / (t - p) if valid.any() else 0
        change[p + 1:t + 1] = rate
    excess = change - ndimage.median_filter(change, 7)
    sc = [t for t in window if excess[t] > LIMITS["shape-change"]]
    if sc:
        peak = max(sc, key=lambda t: excess[t])
        add("shape-change", sc, excess[peak], f"the head outline's shape changes {excess[peak]*100:.0f}% more than usual between frames {peak-1} and {peak}")

    # 8 tips that come and go.
    flick = []
    for t in window:
        lo, hi = max(0, t - 2), min(n, t + 3)
        around = np.median(np.r_[tipcount[lo:t], tipcount[t + 1:hi]])
        if abs(tipcount[t] - around) >= LIMITS["horn-count"] and tipcount[lo] == tipcount[hi - 1]:
            flick.append(t)
    if flick:
        add("horn-count", flick, len(flick), f"outline tip count flickers at frames {', '.join(map(str, flick))} (counts {[int(tipcount[t]) for t in flick]})")

    metrics = {
        "eye_height": eh.tolist(), "eye_area": ea.tolist(), "eye_x": ax.tolist(), "eye_y": ay.tolist(),
        "eye_count": eye_count.tolist(), "head_scale_rel": [None if not np.isfinite(v) else v for v in rel.tolist()],
        "head_size": head_size.tolist(), "crown": crown.tolist(), "cut_edges": cuts.tolist(), "head_step": step.tolist(), "ghost": ghost.tolist(), "shape_change": change.tolist(),
        "tips": tipcount.tolist(), "slide_px": slide.tolist(),
        "outline_pop_px": {int(t): v for t, v, _, _ in worst},
    }
    return F, findings, metrics, (sx, sy, rmax), profiles


def where(deg):
    names = {(-180, -150): "left", (-150, -110): "upper left", (-110, -70): "above", (-70, -30): "upper right", (-30, 0): "right"}
    for (lo, hi), name in names.items():
        if lo <= deg < hi:
            return name
    return "around"


def score(findings):
    return round(sum(max(0.0, f["severity"] - 1) + 1 for f in findings), 2)


def composite(rgba, bg):
    out = np.empty(rgba.shape[:2] + (3,), np.float32)
    a = rgba[..., 3:4].astype(np.float32) / 255
    out[:] = np.array(bg, np.float32) * (1 - a) + rgba[..., :3].astype(np.float32) * a
    return out.astype(np.uint8)


def evidence(out, F, findings, metrics, head, first, last, display_size, fps, label):
    sx, sy, rmax = head
    n = len(F)
    hit = {t for f in findings for t in f["frames"]}
    # Head strip: every frame of the window, zoomed on the head, flagged frames outlined.
    cell = 200
    sel = list(range(first, last + 1))
    cols = 10
    sheet = Image.new("RGB", (cols * cell, ((len(sel) + cols - 1) // cols) * (cell + 16)), LIGHT)
    d = ImageDraw.Draw(sheet)
    bx0 = min(sx[t] - rmax for t in sel)
    bx1 = max(sx[t] + 0.8 * rmax for t in sel)
    by0 = min(sy[t] - rmax for t in sel)
    by1 = max(sy[t] + 0.7 * rmax for t in sel)
    cx, cy = (bx0 + bx1) / 2, (by0 + by1) / 2
    half = int(max(bx1 - bx0, by1 - by0) / 2 + 4)
    for i, t in enumerate(sel):
        im = Image.fromarray(composite(F[t].rgba, LIGHT)).crop((int(cx - half), int(cy - half), int(cx + half), int(cy + half))).resize((cell, cell), Image.LANCZOS)
        x, y = (i % cols) * cell, (i // cols) * (cell + 16)
        sheet.paste(im, (x, y + 16))
        d.text((x + 4, y + 2), f"{t}" + ("  FLAG" if t in hit else ""), fill=(200, 30, 30) if t in hit else (0, 0, 0))
        if t in hit:
            d.rectangle((x, y + 16, x + cell - 1, y + 16 + cell - 1), outline=(220, 40, 40), width=3)
    sheet.save(out / "head-strip.png")
    # Metric chart.
    w, h = 1200, 640
    chart = Image.new("RGB", (w, h), (255, 255, 255))
    d = ImageDraw.Draw(chart)
    series = [("cranium height beyond the ends", [v if v is not None else 0 for v in metrics["head_scale_rel"]], 0.3, (200, 60, 60)),
              ("head pixels changed per frame", metrics["head_step"], max(1e-6, max(metrics["head_step"][first:last + 1])), (60, 60, 200)),
              ("dissolve gain (limit 0.42)", metrics["ghost"], 1.0, (160, 60, 160)),
              ("outline shape change", metrics["shape_change"], max(1e-6, max(metrics["shape_change"])), (40, 140, 60))]
    rows = len(series)
    for k, (name, vals, top, col) in enumerate(series):
        y0 = 20 + k * (h - 40) // rows
        hh = (h - 40) // rows - 24
        d.text((10, y0), name, fill=col)
        base = y0 + 16 + hh // 2 if name.startswith("head size") else y0 + 16 + hh
        d.line((40, base, w - 10, base), fill=(200, 200, 200))
        pts = []
        for t in range(first, last + 1):
            x = 40 + (t - first) * (w - 60) / max(1, last - first)
            v = vals[t]
            yv = base - (v / top) * (hh / 2 if name.startswith("head size") else hh)
            pts.append((x, yv))
        d.line(pts, fill=col, width=2)
        for t in range(first, last + 1, 2):
            x = 40 + (t - first) * (w - 60) / max(1, last - first)
            d.text((x - 6, base + 2 if not name.startswith("head size") else y0 + 16 + hh), str(t), fill=(120, 120, 120))
    chart.save(out / "metrics.png")
    # Normal size and speed, and slowed head close-up, both on light.
    frames_dir = out / "frames-light"
    frames_dir.mkdir(exist_ok=True)
    for t in range(n):
        Image.fromarray(composite(F[t].rgba, LIGHT)).save(frames_dir / f"{t:04d}.png")
    size = display_size // 2 * 2
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", str(fps), "-i", str(frames_dir / "%04d.png"),
                    "-vf", f"scale={size}:{size}:flags=area,pad={size+64}:{size+64}:32:32:color=0xECE9F0",
                    "-c:v", "libx264", "-crf", "12", "-pix_fmt", "yuv420p", str(out / "normal-size.mp4")], check=True)
    # Pad first so a head near the canvas edge still gets a full square close-up.
    pad = int(2 * half)
    crop = (f"pad=iw+{2*pad}:ih+{2*pad}:{pad}:{pad}:color=0xECE9F0,"
            f"crop={2*int(half)}:{2*int(half)}:{int(cx-half)+pad}:{int(cy-half)+pad},scale=480:480:flags=lanczos")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-framerate", str(fps / 4), "-start_number", str(first),
                    "-i", str(frames_dir / "%04d.png"), "-frames:v", str(last - first + 1), "-vf", crop,
                    "-c:v", "libx264", "-crf", "14", "-pix_fmt", "yuv420p", "-r", "24", str(out / "head-quarter-speed.mp4")], check=True)
    for p in frames_dir.iterdir():
        p.unlink()
    frames_dir.rmdir()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source", help="RGBA frame directory or alpha video (ffv1/webm)")
    ap.add_argument("--range", default=None, help="turn window first-last (default: whole clip)")
    ap.add_argument("--label", default="turn")
    ap.add_argument("--display-size", type=int, default=166, help="art canvas long side on screen")
    ap.add_argument("--fps", type=float, default=24)
    ap.add_argument("--out", default=None)
    ap.add_argument("--no-media", action="store_true")
    args = ap.parse_args()
    frames, inputs = load(args.source)
    n = len(frames)
    first, last = (map(int, args.range.split("-"))) if args.range else (1, n - 2)
    display_scale = args.display_size / max(frames[0].shape[:2])
    F, findings, metrics, head, profiles = analyse(frames, first, last, display_scale)
    findings.sort(key=lambda f: -f["severity"])
    total = score(findings)
    verdict = "fail" if findings else "pass"
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = Path(args.out) if args.out else ROOT / ".tmp/verify/turn" / f"{args.label}-{stamp}-{uuid.uuid4().hex[:6]}"
    out.mkdir(parents=True, exist_ok=True)
    result = {"label": args.label, "source": str(Path(args.source).resolve()), "inputs": inputs, "frames": n,
              "window": [first, last], "display_size": args.display_size, "verdict": verdict, "score": total,
              "findings": findings, "limits": LIMITS}
    (out / "turn.json").write_text(json.dumps(result, indent=1))
    (out / "metrics.json").write_text(json.dumps(metrics))
    lines = [f"# Turn review: {args.label}", "",
             f"Source `{result['source']}`, {n} frames, turn window {first}–{last}, shown at {args.display_size} px.",
             f"Verdict **{verdict.upper()}**, fault score {total} (0 is clean; each finding adds 1 plus how far it exceeds its limit).", ""]
    if findings:
        lines += ["| Severity | What a viewer sees | Frames | Measured |", "| ---: | --- | --- | --- |"]
        for f in findings:
            fr = f["frames"]
            span = f"{fr[0]}–{fr[-1]}" if len(fr) > 1 else str(fr[0])
            lines.append(f"| {f['severity']} | {f['what']} | {span} | {f['detail']} |")
    else:
        lines.append("No check exceeded its limit. This is not approval: watch `normal-size.mp4` and `head-quarter-speed.mp4`.")
    lines += ["", "Evidence: `head-strip.png` (every turn frame, flagged ones outlined), `metrics.png`, "
              "`normal-size.mp4` (real size and speed), `head-quarter-speed.mp4`."]
    (out / "report.md").write_text("\n".join(lines) + "\n")
    if not args.no_media:
        evidence(out, F, findings, metrics, head, first, last, args.display_size, args.fps, args.label)
    print(json.dumps({"verdict": verdict, "score": total, "checks": sorted({f["check"] for f in findings}), "evidence": str(out)}))
    raise SystemExit(2 if findings else 0)


if __name__ == "__main__":
    main()
