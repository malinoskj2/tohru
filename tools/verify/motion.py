#!/usr/bin/env python3
"""Review an animation frame by frame and flag motion faults for a separate reviewer."""
import argparse
import datetime
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys
import uuid

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[2]
DARK = (22, 20, 26)
LIGHT = (236, 233, 240)
FLAG_COLOR = (255, 64, 64)


def natural(path):
    return [int(part) if part.isdigit() else part for part in re.split(r"(\d+)", path.name)]


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def font(size):
    return ImageFont.load_default(size=size)


class Clip:
    """Ordered frames with a subject mask per frame, at analysis resolution."""

    def __init__(self, spec, grid=None, row=None, frame_range=None, max_size=1000, fps=None,
                 background="auto", threshold=24):
        self.path = Path(spec).resolve()
        self.grid, self.row = grid, row
        self.video = self.path.is_file() and grid is None and self.path.suffix.lower() not in (".png", ".webp")
        if self.path.is_dir():
            files = sorted((p for p in self.path.iterdir() if p.suffix.lower() in (".png", ".webp")), key=natural)
            if not files:
                raise SystemExit(f"No PNG or WebP frames in {self.path}")
            frames = [np.asarray(Image.open(p).convert("RGBA")) for p in files]
            self.inputs = {str(p): sha256(p) for p in files}
            self.fps = fps or 24.0
        elif grid:
            cols, rows = grid
            sheet = np.asarray(Image.open(self.path).convert("RGBA"))
            height, width = sheet.shape[0] // rows, sheet.shape[1] // cols
            picked = [row] if row is not None else range(rows)
            frames = [sheet[r * height:(r + 1) * height, c * width:(c + 1) * width]
                      for r in picked for c in range(cols)]
            while frames and not frames[-1][..., 3].any():
                frames.pop()
            self.inputs = {str(self.path): sha256(self.path)}
            self.fps = fps or 24.0
        elif self.video:
            frames = self._decode(max_size, background, threshold)
            self.inputs = {str(self.path): sha256(self.path)}
            self.fps = fps or self.video_fps
        else:
            raise SystemExit(f"Unsupported source {self.path}: pass a frame directory, a video or --grid for a sheet")
        offset = 0
        if frame_range:
            first, last = frame_range
            offset = first
            frames = frames[first:last + 1]
        if len(frames) < 3:
            raise SystemExit("Need at least three frames")
        self.offset = offset
        self.frames = frames
        self.alpha = not self.video and any((f[..., 3] < 250).any() for f in frames[:: max(1, len(frames) // 8)])
        if not self.video and not self.alpha:
            raise SystemExit("Opaque still frames need a video source or transparent PNGs")
        self.source_size = (self.source_width, self.source_height) if self.video else frames[0].shape[1::-1]
        if not self.video:
            self.origin, self.crop_size = (0, 0), self.source_size
        self.scale = frames[0].shape[1] / self.crop_size[0]
        self.masks = self._masks(background, threshold)

    def _decode(self, max_size, background, threshold):
        probe = json.loads(subprocess.check_output(
            ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
             "stream=width,height,r_frame_rate", "-of", "json", str(self.path)]))["streams"][0]
        self.source_width, self.source_height = probe["width"], probe["height"]
        numerator, denominator = map(float, probe["r_frame_rate"].split("/"))
        self.video_fps = numerator / denominator
        # Find where the subject goes at low resolution, then analyse that region sharply:
        # a small character on a large desktop is otherwise only a few dozen pixels.
        rough = self._read(min(1.0, 480 / max(self.source_width, self.source_height)))
        factor = rough[0].shape[1] / self.source_width
        masks = self._subtract([f[..., :3] for f in rough], background, threshold, factor, (0, 0))
        boxes = [b for b in map(bbox, masks) if b]
        if boxes:
            margin = 0.04 * max(self.source_width, self.source_height)
            x0 = max(0, int(min(b[0] for b in boxes) / factor - margin)) // 2 * 2
            y0 = max(0, int(min(b[1] for b in boxes) / factor - margin)) // 2 * 2
            x1 = min(self.source_width, int(math.ceil(max(b[2] for b in boxes) / factor + margin)))
            y1 = min(self.source_height, int(math.ceil(max(b[3] for b in boxes) / factor + margin)))
        else:
            x0, y0, x1, y1 = 0, 0, self.source_width, self.source_height
        self.origin = (x0, y0)
        self.crop_size = ((x1 - x0) // 2 * 2, (y1 - y0) // 2 * 2)
        return self._read(min(1.0, max_size / max(self.crop_size)), crop=(x0, y0, *self.crop_size))

    def _read(self, factor, crop=None):
        width, height = crop[2:] if crop else (self.source_width, self.source_height)
        width, height = max(2, int(round(width * factor / 2)) * 2), max(2, int(round(height * factor / 2)) * 2)
        chain = (f"crop={crop[2]}:{crop[3]}:{crop[0]}:{crop[1]}," if crop else "") + f"scale={width}:{height}:flags=area"
        raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(self.path), "-vf", chain, "-f", "rawvideo",
                              "-pix_fmt", "rgba", "-"], check=True, capture_output=True).stdout
        count = len(raw) // (width * height * 4)
        return list(np.frombuffer(raw, np.uint8)[:count * width * height * 4].reshape(count, height, width, 4))

    def _masks(self, background, threshold):
        if self.alpha:
            return [f[..., 3] > 24 for f in self.frames]
        return self._subtract([f[..., :3] for f in self.frames], background, threshold, self.scale, self.origin)

    def _subtract(self, frames, background, threshold, factor, origin):
        stack = np.stack(frames)
        first, last = stack[0].astype(np.int16), stack[-1].astype(np.int16)
        if background == "auto":
            background = "first" if np.abs(first - last).max(axis=2).mean() < 2 else "median"
        if background == "first":
            base = first
        elif background == "median":
            base = np.median(stack[:: max(1, len(stack) // 60)], axis=0).astype(np.int16)
        else:
            image = Image.open(background).convert("RGB")
            span = (stack.shape[2] / factor, stack.shape[1] / factor)
            image = image.crop((origin[0], origin[1], origin[0] + span[0], origin[1] + span[1]))
            base = np.asarray(image.resize(stack.shape[2:0:-1], Image.Resampling.BOX)).astype(np.int16)
        self.background = background
        minimum = max(9, int(self.source_width * self.source_height * factor ** 2 * 0.0002))
        masks = []
        for frame in stack:
            mask = np.abs(frame.astype(np.int16) - base).max(axis=2) > threshold
            mask = ndimage.binary_opening(mask, iterations=1)
            labels, count = ndimage.label(mask)
            if count:
                sizes = ndimage.sum(mask, labels, range(1, count + 1))
                mask = np.isin(labels, 1 + np.flatnonzero(sizes >= minimum))
            masks.append(mask)
        return masks

    def rgb_on(self, index, color):
        frame = self.frames[index]
        if not self.alpha:
            return Image.fromarray(np.ascontiguousarray(frame[..., :3]))
        base = Image.new("RGBA", frame.shape[1::-1], color + (255,))
        return Image.alpha_composite(base, Image.fromarray(frame)).convert("RGB")

    def label(self, index):
        return self.offset + index


def bbox(mask):
    rows, cols = np.flatnonzero(mask.any(axis=1)), np.flatnonzero(mask.any(axis=0))
    if not len(rows):
        return None
    return int(cols[0]), int(rows[0]), int(cols[-1]) + 1, int(rows[-1]) + 1


def shifted_iou(a, b, shift):
    dy, dx = shift
    b = np.roll(np.roll(b, dy, axis=0), dx, axis=1)
    union = np.logical_or(a, b).sum()
    return float(np.logical_and(a, b).sum() / union) if union else 1.0


def mirrored(mask, centre_x):
    flipped = mask[:, ::-1]
    shift = int(round(2 * centre_x - (mask.shape[1] - 1)))
    return np.roll(flipped, shift, axis=1)


def normalized_shape(mask, box, size=64):
    crop = mask[box[1]:box[3], box[0]:box[2]]
    return np.asarray(Image.fromarray(crop.astype(np.uint8) * 255).resize((size, size), Image.Resampling.BILINEAR)) > 127


def iou(a, b):
    union = np.logical_or(a, b).sum()
    return float(np.logical_and(a, b).sum() / union) if union else 1.0


def holes(mask, minimum):
    labels, count = ndimage.label(~mask)
    if not count:
        return 0, 0
    border = set(np.unique(np.concatenate([labels[0], labels[-1], labels[:, 0], labels[:, -1]])))
    sizes = ndimage.sum(~mask, labels, range(1, count + 1))
    found = [size for label, size in enumerate(sizes, 1) if label not in border and size >= minimum]
    return len(found), int(sum(found))


def palette(clip, k=6):
    rng = np.random.default_rng(7)
    samples = []
    for frame, mask in zip(clip.frames[:: max(1, len(clip.frames) // 24)], clip.masks[:: max(1, len(clip.frames) // 24)]):
        pixels = frame[..., :3][ndimage.binary_erosion(mask, iterations=2)]
        if len(pixels):
            samples.append(pixels[rng.choice(len(pixels), min(len(pixels), 6000), replace=False)])
    data = np.concatenate(samples).astype(np.float32)
    centres = data[rng.choice(len(data), 1)]
    for _ in range(1, k):
        distance = ((data[:, None] - centres[None]) ** 2).sum(-1).min(1)
        weights = distance / distance.sum() if distance.sum() else None
        centres = np.vstack([centres, data[rng.choice(len(data), p=weights)]])
    for _ in range(20):
        nearest = ((data[:, None] - centres[None]) ** 2).sum(-1).argmin(1)
        centres = np.array([data[nearest == i].mean(0) if (nearest == i).any() else centres[i] for i in range(k)])
    return centres


def part_areas(frame, mask, centres, zones, regions, scale):
    areas = {}
    pixels = frame[..., :3][ndimage.binary_erosion(mask, iterations=2)].astype(np.float32)
    nearest = ((pixels[:, None] - centres[None]) ** 2).sum(-1).argmin(1) if len(pixels) else np.array([], int)
    for index, centre in enumerate(centres):
        areas["colour #%02x%02x%02x" % tuple(int(v) for v in centre)] = int((nearest == index).sum())
    for name, (x0, y0, x1, y1) in zones.items():
        areas[f"zone {name}"] = int(mask[y0:y1, x0:x1].sum())
    for name, (x0, y0, x1, y1) in regions.items():
        sx = lambda v: int(round(v * scale))
        areas[f"part {name}"] = int(mask[sx(y0):sx(y1), sx(x0):sx(x1)].sum())
    return areas


def off_trend(series, whole, half=4, predicted=None):
    """Distance of each value from a quadratic fitted to its neighbours (itself excluded).
    Steady growth, easing and turns stay near zero; a pop, dip or swell does not."""
    series = np.asarray(series, float)
    out = np.zeros(len(series))
    for i in range(len(series)):
        lo, hi = max(0, i - half), min(len(series), i + half + 1)
        index = [j for j in range(lo, hi) if j != i and whole[j]]
        if not whole[i] or sum(j < i for j in index) < 2 or sum(j > i for j in index) < 2:
            continue
        x = np.array(index, float) - i
        coefficients = np.polyfit(x, series[index], 2)
        out[i] = abs(series[i] - coefficients[-1])
        if predicted is not None:
            predicted[i] = coefficients[-1]
    return out


def rigid_step(clip, i, before=1):
    """Shift (dy, dx) that maps frame i-before onto frame i pixel for pixel, or None when the
    drawing itself changed. Revealed or occluded pixels are ignored, so a sprite sliding
    out from behind a window still counts as rigid."""
    a, b = clip.masks[i - before], clip.masks[i]
    if not a.any() or not b.any():
        return None
    box = bbox(a | b)
    pad = 4
    y0, x0 = max(0, box[1] - pad), max(0, box[0] - pad)
    y1, x1 = box[3] + pad, box[2] + pad
    def grey(index, mask):
        frame = clip.frames[index][y0:y1, x0:x1].astype(np.float32)
        value = frame[..., :3].mean(axis=2)
        if clip.alpha:
            value *= frame[..., 3] / 255
        return np.where(mask[y0:y1, x0:x1], value, 0)
    ga, gb = grey(i - before, a), grey(i, b)
    spectrum = np.fft.fft2(gb) * np.conj(np.fft.fft2(ga))
    peak = np.unravel_index(np.abs(np.fft.ifft2(spectrum / (np.abs(spectrum) + 1e-6))).argmax(), ga.shape)
    dy, dx = [p if p <= n // 2 else p - n for p, n in zip(peak, ga.shape)]
    shifted = np.roll(np.roll(ga, dy, axis=0), dx, axis=1)
    moved_mask = np.roll(np.roll(a[y0:y1, x0:x1], dy, axis=0), dx, axis=1)
    overlap = moved_mask & b[y0:y1, x0:x1]
    if overlap.sum() < 0.5 * min(a.sum(), b.sum()):
        return None
    inner = ndimage.binary_erosion(overlap, iterations=2)
    if not inner.any():
        return None
    residual = np.abs(gb - shifted)[inner]
    # Video compression adds noise that exact alpha frames do not have.
    mean_limit, outlier_limit = (3.0, 0.01) if clip.alpha else (5.0, 0.04)
    if residual.mean() >= mean_limit or (residual > 16).mean() >= outlier_limit:
        return None
    # Pixels revealed or hidden by a window form a band as thick as the shift; a limb that
    # moved on its own leaves a thicker difference.
    changed = moved_mask ^ b[y0:y1, x0:x1]
    thickness = 2 * ndimage.distance_transform_edt(changed).max() if changed.any() else 0
    return (int(dy), int(dx)) if thickness <= math.hypot(dy, dx) + 2.5 else None


def direction(dy, dx):
    names = ("right", "lower-right", "bottom", "lower-left", "left", "upper-left", "top", "upper-right")
    return names[int(((math.degrees(math.atan2(dy, dx)) + 22.5) % 360) // 45)]


def events(check, scores, threshold, clip, where=None, detail=None):
    limits = np.broadcast_to(np.asarray(threshold, float), (len(scores),))
    flagged = [i for i, value in enumerate(scores) if value > limits[i]]
    groups = []
    for index in flagged:
        if groups and index - groups[-1][-1] <= 2:
            groups[-1].append(index)
        else:
            groups.append([index])
    found = []
    for group in groups:
        peak = max(group, key=lambda i: scores[i] / max(limits[i], 1e-9))
        found.append({"check": check, "frames": [clip.label(group[0]), clip.label(group[-1])],
                      "peak": clip.label(peak), "value": round(float(scores[peak]), 3),
                      "threshold": round(float(limits[peak]), 3),
                      "where": where[peak] if where is not None else None, "detail": detail})
    return found


def spike_threshold(scores, floor, k, window=12):
    """k times the median activity around each frame, so steady fast motion (wingbeats)
    is judged against itself while a calm stretch stays strict; never below floor."""
    scores = np.asarray(scores, float)
    limits = np.empty(len(scores))
    for i in range(len(scores)):
        nearby = scores[max(0, i - window):i + window + 1]
        limits[i] = max(floor, k * float(np.median(nearby)))
    return limits


def analyse(clip, sensitivity=6.0, loop=False, regions=None, edge_anchor=None):
    regions = regions or {}
    count = len(clip.frames)
    scale, unit, k = clip.scale, 1 / clip.scale, sensitivity
    boxes = [bbox(m) for m in clip.masks]
    areas = np.array([m.sum() for m in clip.masks], float)
    if not areas.any():
        raise SystemExit("No subject found in any frame")
    median_area = float(np.median(areas[areas > 0]))
    radius = math.sqrt(median_area)
    # Frames where the subject is cut by the frame edge or barely present are entering or
    # leaving; shape checks skip them so an entrance is not mistaken for a pop.
    ox, oy = clip.origin
    frame_width, frame_height = clip.source_size
    edges = np.array([(ox + b[0] * unit, oy + b[1] * unit, ox + b[2] * unit, oy + b[3] * unit) if b else (np.nan,) * 4
                      for b in boxes], float)
    gaps = {"left": edges[:, 0], "top": edges[:, 1], "right": frame_width - edges[:, 2], "bottom": frame_height - edges[:, 3]}
    cut = np.array([bool(b) and min(g[i] for g in gaps.values()) <= 2 for i, b in enumerate(boxes)])
    present = areas > 0.25 * median_area
    # Frames cut by the screen edge are entering or leaving, and the two frames either side
    # are still settling; shape checks skip them. A lone blank frame is skipped by itself.
    settling = ndimage.binary_dilation(cut, iterations=2)
    run_start = ndimage.binary_dilation(~present, iterations=2) & ~ndimage.binary_dilation(present, iterations=3)
    whole = present & ~settling & ~ndimage.binary_dilation(run_start, iterations=2)
    window = max(6, int(round(clip.fps)))
    centres = palette(clip)
    centroids = np.array([ndimage.center_of_mass(m) if b else (np.nan, np.nan) for m, b in zip(clip.masks, boxes)])
    travel = np.nanmax(centroids, axis=0) - np.nanmin(centroids, axis=0)
    zones = {}
    if np.hypot(*travel) < 0.6 * radius:
        known = [b for b in boxes if b]
        x0, y0 = min(b[0] for b in known), min(b[1] for b in known)
        x1, y1 = max(b[2] for b in known), max(b[3] for b in known)
        xs, ys = np.linspace(x0, x1, 4).round().astype(int), np.linspace(y0, y1, 4).round().astype(int)
        for row, vertical in enumerate(("upper", "middle", "lower")):
            for col, horizontal in enumerate(("left", "centre", "right")):
                zones[f"{vertical}-{horizontal}"] = (xs[col], ys[row], xs[col + 1], ys[row + 1])
    rows, shapes = [], []
    for i, (frame, mask, box) in enumerate(zip(clip.frames, clip.masks, boxes)):
        hole_count, hole_area = holes(mask, max(4, int(median_area * 0.0005))) if box else (0, 0)
        rows.append({"frame": clip.label(i), "time": round(i / clip.fps, 4), "area": int(areas[i] * unit ** 2),
                     "whole": bool(whole[i]), "bbox": [round(v, 1) for v in edges[i]] if box else None,
                     "centroid": [round(ox + centroids[i][1] * unit, 2), round(oy + centroids[i][0] * unit, 2)] if box else None,
                     "holes": hole_count, "hole_area": int(hole_area * unit ** 2),
                     "parts": {n: int(v * unit ** 2) for n, v in part_areas(frame, mask, centres, zones, regions, scale).items()}})
        shapes.append(normalized_shape(mask, box) if box else None)
    flags = []

    expected = np.zeros(count)
    score = off_trend(areas / median_area, whole, predicted=expected)
    flags += events("silhouette-area", score, spike_threshold(score, 0.03, k, window), clip,
                    [f"{areas[i] * unit ** 2:.0f} px where its neighbours predict {expected[i] * median_area * unit ** 2:.0f}"
                     for i in range(count)],
                    detail="whole silhouette swells or shrinks off its trend (pop, bulge, part appearing)")

    for name in sorted(rows[0]["parts"]):
        series = np.array([r["parts"][name] for r in rows], float) / unit ** 2
        level = np.array([np.median(series[max(0, i - 4):i + 5]) for i in range(count)])
        usable = whole & (level > median_area * 0.004)
        if usable.sum() < 6:
            continue
        expected = np.zeros(count)
        score = off_trend(series, usable, predicted=expected) / np.maximum(level, median_area * 0.02)
        flags += events("part-area", score, spike_threshold(score, 0.08, k, window), clip,
                        [f"{name}: {series[i] * unit ** 2:.0f} px where its neighbours predict {expected[i] * unit ** 2:.0f}"
                         for i in range(count)],
                        "one part's area jumps off its trend (part vanishes, winks, bulges or changes colour)")

    pulse, pulse_where = np.zeros(count), [None] * count
    minimum = max(4, int(median_area * 0.001))
    for i in range(1, count - 1):
        before = clip.masks[i - 1]
        for span in (1, 2):
            after = i + span
            if after >= count or not whole[i - 1:after + 1].all():
                break
            changed = np.ones_like(before)
            for j in range(i, after):
                changed &= clip.masks[j] != before
            region = changed & (clip.masks[after] == before)
            labels, found = ndimage.label(region)
            if not found:
                continue
            depth = ndimage.distance_transform_edt(region)
            sizes = ndimage.sum(region, labels, range(1, found + 1))
            peaks = ndimage.maximum(depth, labels, range(1, found + 1))
            for label, (size, peak) in enumerate(zip(sizes, peaks), 1):
                thickness = 2 * peak * unit
                if size >= minimum and thickness > pulse[i]:
                    y, x = ndimage.center_of_mass(region, labels, label)
                    kind = "dents in" if before[int(y), int(x)] else "bulges out"
                    side = direction(y - centroids[i][0], x - centroids[i][1])
                    pulse[i] = thickness
                    pulse_where[i] = (f"{side} outline {kind} {thickness:.1f} px for {span} frame(s) and snaps back "
                                      f"near x={round(ox + x * unit)}, y={round(oy + y * unit)}")
    flags += events("outline-pulse", pulse, spike_threshold(pulse, 0.04 * radius * unit, k, window), clip, pulse_where,
                    "a stretch of outline swells or dents for one or two frames and snaps back")

    edge_trend = np.array([off_trend(edges[:, j], whole) for j in range(4)])
    box_score = edge_trend.max(axis=0)
    box_side = [("left", "top", "right", "bottom")[int(j)] for j in edge_trend.argmax(axis=0)]
    flags += events("bbox-continuity", box_score, spike_threshold(box_score, 0.04 * radius * unit, k, window), clip,
                    [f"{s} extent jumps" for s in box_side], "bounding box edge jumps off its path")
    centre_score = np.hypot(off_trend(centroids[:, 0], whole), off_trend(centroids[:, 1], whole)) * unit
    flags += events("centroid-jerk", centre_score, spike_threshold(centre_score, 0.03 * radius * unit, k, window), clip,
                    [f"body {s:.1f} px off its path" for s in centre_score], detail="body position leaves its path (teleport, stutter, wobble)")

    vanish = np.zeros(count)
    for i in range(1, count - 1):
        before = [j for j in range(max(0, i - 2), i) if areas[j] > 0.5 * median_area]
        after = [j for j in range(i + 1, min(count, i + 3)) if areas[j] > 0.5 * median_area]
        if areas[i] < 0.25 * median_area and before and after:
            vanish[i] = 1 - areas[i] / median_area
    flags += events("vanish", vanish, 0.25, clip, [f"subject at {areas[i] / median_area:.0%} of its size" for i in range(count)],
                    "the subject disappears for a frame or two (blank frame while switching art)")

    # Compare each frame with the last whole frame up to three back, so a blank frame
    # between two drawings does not hide the switch.
    change, mirror_gain = np.zeros(count), np.zeros(count)
    for i in range(1, count):
        previous = next((j for j in range(i - 1, max(-1, i - 4), -1) if whole[j]), None)
        if not whole[i] or previous is None:
            continue
        a, b = clip.masks[i], clip.masks[previous]
        shift = (int(round(centroids[i][0] - centroids[previous][0])), int(round(centroids[i][1] - centroids[previous][1])))
        direct = shifted_iou(a, b, shift)
        change[i] = 1 - direct
        if direct < 0.85:
            mirror_gain[i] = shifted_iou(a, mirrored(b, centroids[previous][1]), shift) - direct
    limits = spike_threshold(change, 0.12, k, window)
    snap = [c if c > limits[i] and c > 2 * max(change[i - 1], change[i + 1] if i + 1 < count else 0) else 0
            for i, c in enumerate(change)]
    flags += events("pose-snap", snap, limits, clip,
                    detail="silhouette changes shape in one frame far more than its neighbours (one-frame pose switch)")
    flags += events("mirror-flip", mirror_gain, 0.08, clip,
                    detail="the previous frame matches better mirrored: facing flips in one frame instead of turning")

    span = max(6, int(round(clip.fps / 2)))
    rigid = np.zeros(count)
    for start in range(0, count - span):
        end = start + span
        if not whole[start:end + 1].all():
            continue
        moved = sum(math.dist(centroids[i], centroids[i - 1]) / radius + abs(math.log(areas[i] / areas[i - 1])) / 2
                    for i in range(start + 1, end + 1))
        stillness = min(iou(shapes[start], shapes[i]) for i in range(start + 1, end + 1))
        if moved > 0.6 and stillness > 0.985:
            rigid[start:end + 1] = np.maximum(rigid[start:end + 1], moved)
    steps = [None] + [rigid_step(clip, i) for i in range(1, count)]
    sliding = np.array([step is not None and step != (0, 0) for step in steps])
    # Sub-pixel motion breaks exact matches now and then; tolerate one miss in three.
    sliding = ndimage.binary_closing(sliding, structure=np.ones(3)) | sliding
    labels, found = ndimage.label(sliding)
    for label in range(1, found + 1):
        index = np.flatnonzero(labels == label)
        travelled = sum(math.hypot(*steps[i]) for i in index if steps[i])
        # Art held on twos while the sprite travels matches step by step; a static drawing
        # also matches itself several frames later.
        gap = 4
        spans = [rigid_step(clip, i, gap) is not None for i in index[gap:]]
        if len(index) >= span and travelled > 0.3 * radius and spans and np.mean(spans) >= 0.6:
            rigid[index] = np.maximum(rigid[index], travelled / radius)
    flags += events("rigid-motion", rigid, 0.3, clip, [f"drawing unchanged while moving {r:.1f} body sizes" for r in rigid],
                    detail="subject travels or scales while its drawing stays the same (static image sliding, bobbing or zooming)")

    hole_area = np.array([r["hole_area"] for r in rows], float) / unit ** 2
    score = off_trend(hole_area / median_area, whole)
    flags += events("transparent-holes", score, spike_threshold(score, 0.004, k, window), clip,
                    detail="a see-through hole inside the silhouette opens or closes off its trend")
    counts = np.array([r["holes"] for r in rows])
    blink = np.zeros(count)
    for i in range(1, count - 1):
        if whole[i - 1:i + 3].all() and counts[i] != counts[i - 1] and counts[i + 1] == counts[i - 1]:
            blink[i] = abs(int(counts[i]) - int(counts[i - 1]))
    flags += events("transparent-holes", blink, 0.5, clip, detail="a hole appears or vanishes for a single frame")

    if clip.video or edge_anchor:
        reach = max(3, int(clip.fps))
        for side, gap in gaps.items():
            if edge_anchor and side not in edge_anchor:
                continue
            near = 0.08 * (frame_width if side in ("left", "right") else frame_height)
            score = np.zeros(count)
            for i in range(count):
                nearby = gap[max(0, i - reach):i + reach + 1]
                nearby_centres = centroids[max(0, i - reach):i + reach + 1]
                still = (not np.isnan(centroids[i]).any() and
                         np.nanmax(np.abs(nearby_centres - centroids[i]), initial=0) < 0.15 * radius)
                if 2 < gap[i] <= near and (nearby <= 1.0).any() and still:
                    score[i] = gap[i]
            flags += events("edge-contact", score, 2, clip, [f"{side} gap {g:.0f} px" for g in gap],
                            f"subject resting on the {side} screen edge lifts off it (body looks cut off)")

    seam = None
    if loop:
        seam_change = 1 - iou(clip.masks[0], clip.masks[-1])
        drift = float(math.dist(centroids[0], centroids[-1]) * unit) if boxes[0] and boxes[-1] else None
        typical = float(np.median(change[1:]))
        seam = {"last_to_first_shape_change": round(seam_change, 4), "typical_step_change": round(typical, 4),
                "centroid_drift_px": round(drift, 2) if drift is not None else None}
        limit = max(0.1, k * typical / 2)
        if seam_change > limit or (drift or 0) > 0.03 * radius * unit:
            flags.append({"check": "loop-drift", "frames": [clip.label(count - 1), clip.label(0)], "peak": clip.label(0),
                          "value": round(seam_change, 3), "threshold": round(limit, 3),
                          "where": f"centroid drifts {seam['centroid_drift_px']} px over the loop",
                          "detail": "the loop does not return to its first frame"})

    for flag in flags:
        flag["strength"] = round(flag["value"] / max(flag["threshold"], 1e-9), 2)
    flags.sort(key=lambda f: (f["peak"], f["check"]))
    for i, row in enumerate(rows):
        row["pulse_px"] = round(float(pulse[i]), 2)
        row["shape_change"] = round(float(change[i]), 4)
    return {"frames": rows, "flags": flags, "seam": seam, "median_area": median_area * unit ** 2,
            "radius_px": radius * unit, "zones": {n: [round(v * unit) for v in z] for n, z in zones.items()},
            "palette": ["#%02x%02x%02x" % tuple(int(v) for v in c) for c in centres]}


def subject_crop(clip, pad=0.08):
    boxes = [b for b in map(bbox, clip.masks) if b]
    x0, y0 = min(b[0] for b in boxes), min(b[1] for b in boxes)
    x1, y1 = max(b[2] for b in boxes), max(b[3] for b in boxes)
    margin = int(max(x1 - x0, y1 - y0) * pad)
    height, width = clip.masks[0].shape
    return max(0, x0 - margin), max(0, y0 - margin), min(width, x1 + margin), min(height, y1 + margin)


def frame_crops(clip):
    """One equal-sized crop per frame: the whole path for an animation in place, or a
    window following the subject when it travels, so each frame stays legible."""
    union = subject_crop(clip)
    boxes = [bbox(m) for m in clip.masks]
    known = [b for b in boxes if b]
    span_w = max(b[2] - b[0] for b in known)
    span_h = max(b[3] - b[1] for b in known)
    if union[2] - union[0] < 1.6 * span_w and union[3] - union[1] < 1.6 * span_h:
        return [union] * len(boxes)
    height, width = clip.masks[0].shape
    w, h = min(width, int(span_w * 1.3) + 8), min(height, int(span_h * 1.3) + 8)
    crops, last = [], known[0]
    for box in boxes:
        last = box or last
        cx, cy = (last[0] + last[2]) // 2, (last[1] + last[3]) // 2
        x0, y0 = min(max(0, cx - w // 2), width - w), min(max(0, cy - h // 2), height - h)
        crops.append((x0, y0, x0 + w, y0 + h))
    return crops


def fit(image, size):
    image = image.copy()
    image.thumbnail((size, size), Image.Resampling.LANCZOS)
    return image


def outline(mask):
    return mask & ~ndimage.binary_erosion(mask, iterations=1)


def onion(clip, i, crop):
    base = np.asarray(clip.rgb_on(i, DARK)).astype(np.float32) * 0.55
    out = base.copy()
    for j, colour in ((i - 1, (255, 70, 70)), (i + 1, (60, 200, 255)), (i, (255, 255, 255))):
        if 0 <= j < len(clip.masks):
            edge = ndimage.binary_dilation(outline(clip.masks[j]), iterations=1)
            out[edge] = colour
    x0, y0, x1, y1 = crop
    return Image.fromarray(out.astype(np.uint8)[y0:y1, x0:x1])


def difference(clip, i, crop):
    x0, y0, x1, y1 = crop
    if i == 0:
        return Image.new("RGB", (x1 - x0, y1 - y0), (0, 0, 0))
    a = np.asarray(clip.rgb_on(i, DARK)).astype(np.int16)
    b = np.asarray(clip.rgb_on(i - 1, DARK)).astype(np.int16)
    delta = np.abs(a - b).max(axis=2).astype(np.float32)
    shape = clip.masks[i] ^ clip.masks[i - 1]
    heat = np.clip(delta * 3, 0, 255)
    image = np.stack([heat, heat * 0.55, heat * 0.15], axis=2)
    image[shape] = (90, 170, 255)
    return Image.fromarray(image.astype(np.uint8)[y0:y1, x0:x1])


def sheet(cells, labels, flagged, path, columns=12, cell=150, background=DARK):
    pages = []
    per_page = columns * 10
    ink = (240, 240, 240) if sum(background) < 384 else (20, 20, 20)
    for page_index in range(0, len(cells), per_page):
        chunk = cells[page_index:page_index + per_page]
        rows = math.ceil(len(chunk) / columns)
        page = Image.new("RGB", (columns * cell, rows * (cell + 18)), background)
        draw = ImageDraw.Draw(page)
        for n, image in enumerate(chunk):
            index = page_index + n
            x, y = (n % columns) * cell, (n // columns) * (cell + 18)
            thumb = fit(image, cell - 6)
            page.paste(thumb, (x + (cell - thumb.width) // 2, y + 3 + (cell - 6 - thumb.height) // 2))
            colour = FLAG_COLOR if labels[index] in flagged else ink
            draw.text((x + 4, y + cell + 1), f"#{labels[index]} {flagged.get(labels[index], '')}", fill=colour, font=font(12))
            if labels[index] in flagged:
                draw.rectangle((x + 1, y + 1, x + cell - 2, y + cell - 2), outline=FLAG_COLOR, width=3)
        name = path.with_name(f"{path.stem}-{page_index // per_page + 1:02d}{path.suffix}") if len(cells) > per_page else path
        page.save(name)
        pages.append(name)
    return pages


def trail(clip, crop):
    x0, y0, x1, y1 = crop
    image = np.zeros((y1 - y0, x1 - x0, 3), np.float32)
    image[:] = DARK
    count = len(clip.masks)
    for i, mask in enumerate(clip.masks):
        t = i / max(1, count - 1)
        colour = np.array([255 * t, 120 + 100 * math.sin(math.pi * t), 255 * (1 - t)])
        image[outline(mask)[y0:y1, x0:x1]] = colour
    return Image.fromarray(image.astype(np.uint8))


def zoom_strip(clip, centre, path, height=360):
    picked = [i for i in range(centre - 2, centre + 3) if 0 <= i < len(clip.frames)]
    boxes = [b for b in (bbox(clip.masks[i]) for i in picked) if b]
    frame_height, frame_width = clip.masks[0].shape
    x0, y0 = min(b[0] for b in boxes), min(b[1] for b in boxes)
    x1, y1 = max(b[2] for b in boxes), max(b[3] for b in boxes)
    margin = max(6, int(0.1 * max(x1 - x0, y1 - y0)))
    crop = (max(0, x0 - margin), max(0, y0 - margin), min(frame_width, x1 + margin), min(frame_height, y1 + margin))
    scale = height / (crop[3] - crop[1])
    width = max(1, int((crop[2] - crop[0]) * scale))
    strip = Image.new("RGB", (width * len(picked), height + 22), DARK)
    draw = ImageDraw.Draw(strip)
    for n, i in enumerate(picked):
        resample = Image.Resampling.NEAREST if scale >= 1 else Image.Resampling.LANCZOS
        strip.paste(onion(clip, i, crop).resize((width, height), resample), (n * width, 22))
        draw.text((n * width + 6, 4), f"#{clip.label(i)}" + (" <- flagged" if i == centre else ""),
                  fill=FLAG_COLOR if i == centre else (230, 230, 230), font=font(14))
    strip.save(path)


class Desktop:
    """Frames at real desktop pixel size: RGBA art composited at its on-screen size, or the captured desktop."""

    def __init__(self, clip, display, size, anchor, backdrop, margin=24):
        self.clip = clip
        self.size = size
        if clip.video:
            self.display_scale = 1.0
            return
        canvas = clip.frames[0].shape[1::-1]
        self.display_scale = (display / max(canvas)) if display else 1.0
        self.art_size = (max(1, round(canvas[0] * self.display_scale)), max(1, round(canvas[1] * self.display_scale)))
        x = {"left": margin, "centre": (size[0] - self.art_size[0]) // 2}.get(anchor[1], size[0] - self.art_size[0] - margin)
        y = {"top": margin, "middle": (size[1] - self.art_size[1]) // 2}.get(anchor[0], size[1] - self.art_size[1] - margin)
        self.position = (x, y)
        self.backdrop = (Image.open(backdrop).convert("RGBA").resize(size) if backdrop
                         else Image.new("RGBA", size, (38, 34, 46, 255)))

    def frames(self, count):
        if self.clip.video:
            width, height = self.clip.source_size
            process = subprocess.Popen(["ffmpeg", "-v", "error", "-i", str(self.clip.path), "-f", "rawvideo",
                                        "-pix_fmt", "rgb24", "-"], stdout=subprocess.PIPE)
            first = self.clip.offset
            frames = []
            index = 0
            try:
                while len(frames) < len(self.clip.frames):
                    raw = process.stdout.read(width * height * 3)
                    if len(raw) < width * height * 3:
                        break
                    if index >= first:
                        frames.append(raw)
                    index += 1
            finally:
                process.stdout.close()
                process.kill()
                process.wait()
            for i in range(count):
                yield Image.frombytes("RGB", (width, height), frames[i % len(frames)])
            return
        for i in range(count):
            art = Image.fromarray(self.clip.frames[i % len(self.clip.frames)]).resize(self.art_size, Image.Resampling.LANCZOS)
            frame = self.backdrop.copy()
            frame.alpha_composite(art, self.position)
            yield frame.convert("RGB")

    def focus(self):
        x0, y0, x1, y1 = subject_crop(self.clip, pad=0)
        if self.clip.video:
            return (self.clip.origin[0] + (x0 + x1) / 2 / self.clip.scale,
                    self.clip.origin[1] + (y0 + y1) / 2 / self.clip.scale)
        return (self.position[0] + (x0 + x1) / 2 / self.clip.scale * self.display_scale,
                self.position[1] + (y0 + y1) / 2 / self.clip.scale * self.display_scale)


def half_crop(focus, size):
    width = size[0] // 2
    x = int(min(max(0, focus[0] - width / 2), size[0] - width))
    return x, 0, x + width, size[1]


def encode(frames, path, fps, size):
    process = subprocess.Popen(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
                                "-s", f"{size[0]}x{size[1]}", "-r", str(fps), "-i", "-", "-c:v", "libx264",
                                "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(path)],
                               stdin=subprocess.PIPE)
    for frame in frames:
        process.stdin.write(frame.tobytes())
    process.stdin.close()
    if process.wait():
        raise RuntimeError(f"ffmpeg failed writing {path}")


def side_by_side(candidate, approved, args, out):
    size = tuple(args.desktop_size)
    left = Desktop(candidate, args.display_size, size, args.anchor, args.desktop)
    right = Desktop(approved, args.approved_display_size or args.display_size, size, args.anchor, args.desktop)
    duration = max(len(candidate.frames) / candidate.fps, len(approved.frames) / approved.fps)
    count = min(int(round(duration * candidate.fps)), int(candidate.fps * 30))
    crops = half_crop(left.focus(), size), half_crop(right.focus(), size)
    stride = candidate.fps / approved.fps
    left_frames = left.frames(count)
    right_frames = list(right.frames(len(approved.frames)))

    def combined():
        for i, frame in enumerate(left_frames):
            canvas = Image.new("RGB", size)
            canvas.paste(frame.crop(crops[0]), (0, 0))
            canvas.paste(right_frames[int(i / stride) % len(right_frames)].crop(crops[1]), (size[0] // 2, 0))
            draw = ImageDraw.Draw(canvas)
            draw.line((size[0] // 2, 0, size[0] // 2, size[1]), fill=(255, 255, 255), width=2)
            draw.text((16, 12), f"candidate #{candidate.label(i % len(candidate.frames))}", fill=(255, 255, 255), font=font(22))
            draw.text((size[0] // 2 + 16, 12), "approved reference", fill=(255, 255, 255), font=font(22))
            if i == 0:
                canvas.save(out / "side-by-side-first.png")
            yield canvas

    encode(combined(), out / "side-by-side.mp4", candidate.fps, size)
    return {"desktop_size": size, "candidate_display_scale": left.display_scale,
            "approved_display_scale": right.display_scale, "frames": count, "fps": candidate.fps,
            "note": "Each half is a 1:1 pixel crop of a full desktop frame; nothing is zoomed."}


def desktop_clip(clip, args, out):
    size = tuple(args.desktop_size)
    desktop = Desktop(clip, args.display_size, size, args.anchor, args.desktop)
    frames = desktop.frames(len(clip.frames))
    first = next(frames)
    first.save(out / "desktop-first.png")
    encode((f for f in [first, *frames]), out / "desktop.mp4", clip.fps, first.size)


def run(args):
    label = args.label or Path(args.source).stem
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = Path(args.out) if args.out else ROOT / ".tmp/verify/motion" / f"{label}-{stamp}-{uuid.uuid4().hex[:6]}"
    out.mkdir(parents=True, exist_ok=False)
    print(out, flush=True)
    regions = json.loads(Path(args.parts).read_text()) if args.parts else {}
    clip = Clip(args.source, args.grid, args.row, args.range, args.max_size, args.fps, args.background)
    result = analyse(clip, args.sensitivity, args.loop, regions, args.edge_anchor)
    approved = approved_result = None
    ceilings = {}
    if args.approved:
        approved = Clip(args.approved, args.approved_grid, args.approved_row, args.approved_range, args.max_size,
                        args.approved_fps, args.background)
        approved_result = analyse(approved, args.sensitivity, args.loop, {}, args.edge_anchor)
    same_kind = approved is not None and approved.alpha == clip.alpha
    if same_kind:
        for flag in approved_result["flags"]:
            ceilings[flag["check"]] = max(ceilings.get(flag["check"], 0), flag["strength"])
    for flag in result["flags"]:
        flag["beyond_approved"] = flag["strength"] > ceilings.get(flag["check"], 0) if same_kind else None
    ranked = sorted(result["flags"], key=lambda f: -f["strength"])
    flagged = {}
    for flag in ranked[:args.max_flags]:
        flagged[flag["peak"]] = (flagged.get(flag["peak"], "") + " " + flag["check"].split("-")[0][:7]).strip()
    labels = [clip.label(i) for i in range(len(clip.frames))]
    crops = frame_crops(clip)
    count = len(clip.frames)
    sheet([clip.rgb_on(i, DARK).crop(crops[i]) for i in range(count)], labels, flagged, out / "filmstrip-dark.png")
    if clip.alpha:
        sheet([clip.rgb_on(i, LIGHT).crop(crops[i]) for i in range(count)], labels, flagged,
              out / "filmstrip-light.png", background=LIGHT)
    sheet([onion(clip, i, crops[i]) for i in range(count)], labels, flagged, out / "onion.png")
    sheet([difference(clip, i, crops[i]) for i in range(count)], labels, flagged, out / "difference.png")
    trail(clip, subject_crop(clip)).save(out / "outline-trail.png")
    (out / "flagged").mkdir()
    for peak in sorted(flagged):
        zoom_strip(clip, peak - clip.offset, out / "flagged" / f"frame-{peak:04d}.png")
    comparison = side_by_side(clip, approved, args, out) if approved else None
    desktop_clip(clip, args, out)
    (out / "metrics.json").write_text(json.dumps(result["frames"], indent=1) + "\n")
    checks = {"flags": ranked, "seam": result["seam"], "palette": result["palette"], "zones": result["zones"],
              "radius_px": round(result["radius_px"], 2), "sensitivity": args.sensitivity,
              "approved_ceilings": ceilings, "approved_flags": approved_result["flags"] if approved_result else None}
    (out / "checks.json").write_text(json.dumps(checks, indent=1) + "\n")
    strong = [f for f in ranked if f["strength"] >= 2 and f["beyond_approved"] is not False]
    summary = {"label": label, "source": str(clip.path), "inputs_sha256": clip.inputs, "frames": len(clip.frames),
               "first_frame": clip.offset, "fps": clip.fps, "alpha": clip.alpha,
               "background": getattr(clip, "background", "alpha"), "source_size": clip.source_size,
               "analysed_region": [*clip.origin, *clip.crop_size], "analysis_scale": clip.scale,
               "flag_count": len(ranked), "strong_flag_count": len(strong),
               "strong_frames": sorted({f["peak"] for f in strong}), "approved": args.approved,
               "approved_flag_count": len(approved_result["flags"]) if approved_result else None,
               "comparison": comparison, "argv": sys.argv,
               "head": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip(),
               "result": "flagged" if strong else ("weak flags" if ranked else "clean"),
               "limit": "Automatic checks find measurable discontinuities, not appeal; a separate reviewer and the owner decide."}
    write_report(out, summary, ranked, result["seam"], approved_result)
    summary["artifacts_sha256"] = {str(p.relative_to(out)): sha256(p) for p in sorted(out.rglob("*")) if p.is_file()}
    (out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(json.dumps({k: summary[k] for k in ("result", "flag_count", "strong_flag_count", "strong_frames",
                                              "approved_flag_count")} | {"evidence": str(out)}))
    return 2 if strong else 0


def write_report(out, summary, ranked, seam, approved_result):
    lines = [f"# Motion review: {summary['label']}", "",
             f"Source `{summary['source']}`, {summary['frames']} frames from #{summary['first_frame']} at {summary['fps']:g} fps. "
             f"Result **{summary['result']}**: {summary['strong_flag_count']} strong of {summary['flag_count']} automatic flags.", "",
             "Strength is value over threshold. Strong means at least 2 and, with a same-kind approved clip, above the "
             "strongest flag that clip raised for the same check.", ""]
    if approved_result is not None:
        lines += [f"Approved reference `{summary['approved']}` raised {len(approved_result['flags'])} flags.", ""]
    lines += ["| Strength | Beyond approved | Check | Frames | Peak | Where |", "| ---: | --- | --- | --- | --- | --- |"]
    for f in ranked:
        beyond = {True: "yes", False: "no", None: "n/a"}[f["beyond_approved"]]
        lines.append(f"| {f['strength']} | {beyond} | {f['check']} | {f['frames'][0]}–{f['frames'][1]} | {f['peak']} | {f['where'] or ''} |")
    if seam:
        lines += ["", f"Loop seam: {json.dumps(seam)}"]
    lines += ["", "For each peak open `flagged/frame-NNNN.png` (onion skin: red previous, white current, blue next), "
              "then the filmstrips, `difference*.png`, `outline-trail.png`, `desktop.mp4` and `side-by-side.mp4`.", ""]
    (out / "report.md").write_text("\n".join(lines))


def pair(text, cast=int, sep="x"):
    first, second = text.split(sep)
    return cast(first), cast(second)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", help="frame directory, video, or sheet image with --grid")
    parser.add_argument("--grid", type=pair, help="sheet columns x rows, e.g. 15x9")
    parser.add_argument("--row", type=int, help="sheet row to use (zero-based)")
    parser.add_argument("--range", type=lambda t: pair(t, sep="-"), help="first-last frame, inclusive")
    parser.add_argument("--fps", type=float)
    parser.add_argument("--label")
    parser.add_argument("--loop", action="store_true", help="also check the last frame returns to the first")
    parser.add_argument("--parts", help="JSON of named source-pixel rectangles {name: [x0, y0, x1, y1]}")
    parser.add_argument("--edge-anchor", nargs="*", choices=("left", "right", "top", "bottom"),
                        help="screen edges the subject should stay attached to (captures check all edges)")
    parser.add_argument("--background", default="auto", help="captures: auto, first, median or an image path")
    parser.add_argument("--sensitivity", type=float, default=6.0, help="spike multiple of the clip's median change")
    parser.add_argument("--max-size", type=int, default=1000, help="longest side analysed for video captures")
    parser.add_argument("--approved", help="approved clip to show side by side at real desktop size")
    parser.add_argument("--approved-grid", type=pair)
    parser.add_argument("--approved-row", type=int)
    parser.add_argument("--approved-range", type=lambda t: pair(t, sep="-"))
    parser.add_argument("--approved-fps", type=float)
    parser.add_argument("--display-size", type=int, help="on-screen size in px of the art canvas's long side")
    parser.add_argument("--approved-display-size", type=int)
    parser.add_argument("--desktop", help="desktop screenshot to composite art onto")
    parser.add_argument("--desktop-size", type=pair, default=(1920, 1080))
    parser.add_argument("--anchor", type=lambda t: t.split("-"), default=["bottom", "right"],
                        help="art placement on the desktop, e.g. bottom-right, middle-centre")
    parser.add_argument("--max-flags", type=int, default=24, help="strongest flags to mark and zoom")
    parser.add_argument("--out")
    sys.exit(run(parser.parse_args()))


if __name__ == "__main__":
    main()
