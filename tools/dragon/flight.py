#!/usr/bin/env python3
"""Build closed wingbeats from the registered glide and ascent keys in body.webp."""

import math
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

ART = Path(__file__).resolve().parents[2] / "shell/assistant/dragon"
CELL, FRAMES = 240, 12
# Body atlas cell, wing root, wing tip and tail tip (source pixels).
KEYS = [(309, 102, 152, 107, 210), (417, 138, 83, 106, 25), (327, 119, 64, 93, 45)]


def smooth(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def wingbeat(key, root, tip, wing_y, tail_x, phase):
    y, x = np.indices((CELL, CELL), dtype=np.float32)
    span = (x - root) / (tip - root)
    reach = smooth(span) * (1 - smooth((span - 1) / 0.42))
    stroke = math.sin(phase)
    # A monotone vertical deformation about the wing root. The contour moves
    # freely at the tip. The chest, face, feet and distant tail stay registered.
    center = wing_y + (1 - np.clip(span, 0, 1)) * 15
    shift = 22 * reach * stroke
    moved = center + shift
    dy = np.where(
        y < moved,
        (y - (center - 55)) / (55 + shift),
        ((center + 40) - y) / (40 - shift),
    )
    source_y = y - shift * np.clip(dy, 0, 1)
    tail = np.exp(-(((x - tail_x) / 24) ** 2)) * smooth((y - 65) / 42)
    source_y += 4 * tail * math.sin(phase - 0.45) * math.sin(phase)
    return cv2.remap(
        key,
        x,
        source_y.astype(np.float32),
        cv2.INTER_CUBIC,
        borderMode=cv2.BORDER_CONSTANT,
    )


def main():
    cv2.setNumThreads(1)
    source = Image.open(ART / "body.webp").convert("RGBA")
    sheet = Image.new("RGBA", (CELL * FRAMES, CELL * len(KEYS)))
    for row, (cell, root, tip, y, tail) in enumerate(KEYS):
        left, top = cell % 24 * CELL, cell // 24 * CELL
        key = source.crop((left, top, left + CELL, top + CELL))
        # Interpolate premultiplied colors to keep dark fringes out of translucent glass.
        premult = np.asarray(key.convert("RGBa"))
        for frame in range(FRAMES):
            pixels = wingbeat(premult, root, tip, y, tail, 2 * math.pi * frame / FRAMES)
            sprite = (
                Image.frombytes("RGBa", (CELL, CELL), pixels.tobytes()).convert("RGBA")
                if frame
                else key
            )
            sheet.paste(sprite, (CELL * frame, CELL * row))
    sheet.save(ART / "flight.webp", lossless=True, method=3)


if __name__ == "__main__":
    main()
