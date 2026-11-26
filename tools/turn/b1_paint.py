"""B1: paint the hull turn by projecting the sheet views straight onto the 3D proxy (no 2D synthesis).

python3 b1_paint.py PASSES SHEET_DIR PROJECTION OUT --views 0=front 45=three-quarter-right 90=profile-right
Each frame's 16-bit position and object-normal passes give every pixel's surface point. That point projects
orthographically into each sheet view; it takes paint from the views that saw it (its depth matches the key's front
surface), weighted by how squarely each view faced it. The weights depend only on the surface point, so the texture
is fixed to the mesh and turns rigidly with it, instead of cross-fading drawings by yaw.
"""
import argparse
import json
import math
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

from b2_stylize import edge_fill

ORTHO = 1.25


def read16(path):
    im = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
    return im[..., [2, 1, 0, 3]].astype(np.float32) / 65535.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("passes")
    ap.add_argument("sheet")
    ap.add_argument("projection")
    ap.add_argument("out")
    ap.add_argument("--views", nargs="+", required=True)
    ap.add_argument("--tol", type=float, default=0.02)
    ap.add_argument("--power", type=float, default=12.0)
    args = ap.parse_args()
    P, out = Path(args.passes), Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    pj = json.loads(Path(args.projection).read_text())
    unit, cy = pj["unit"], pj["cy"]
    views = {}
    for spec in args.views:
        yaw, name = spec.split("=", 1)
        yaw = float(yaw)
        sheet = np.asarray(Image.open(Path(args.sheet) / f"{name}.png").convert("RGBA"))
        key = read16(P / f"key-{yaw:g}-position.png")
        views[yaw] = {"paint": edge_fill(sheet, 40).astype(np.float32), "cx": pj["views"][name]["cx"],
                      "front": key, "size": key.shape[0]}
    order = sorted(views)
    yaws = json.loads((P / "yaws.json").read_text())
    for i, yaw in enumerate(yaws):
        name = f"{i:04d}.png"
        pos = read16(P / "position" / name)
        alpha = (pos[..., 3] * 255).round().astype(np.uint8)
        X, Y, Z = pos[..., 0] - 0.5, pos[..., 1] - 0.5, pos[..., 2] - 0.5
        nrm = read16(P / "onormal" / name)[..., :3] * 2 - 1
        acc = np.zeros(alpha.shape + (3,), np.float32)
        wsum = np.zeros(alpha.shape, np.float32)
        fallback = np.zeros(alpha.shape + (3,), np.float32)
        fbest = np.full(alpha.shape, np.inf, np.float32)
        for k in order:
            v = views[k]
            t = math.radians(k)
            sx = X * math.cos(t) - Y * math.sin(t)
            depth = X * math.sin(t) + Y * math.cos(t)
            u = (v["cx"] + sx * unit).astype(np.float32)
            vv = (cy - Z * unit).astype(np.float32)
            col = cv2.remap(v["paint"], u, vv, cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
            n = v["size"]
            ppu = n / ORTHO
            kx = (n / 2 + sx * ppu).astype(np.float32)
            ky = (n / 2 - Z * ppu).astype(np.float32)
            kf = cv2.remap(v["front"], kx, ky, cv2.INTER_NEAREST, borderMode=cv2.BORDER_CONSTANT)
            kdepth = (kf[..., 0] - 0.5) * math.sin(t) + (kf[..., 1] - 0.5) * math.cos(t)
            gap = np.where(kf[..., 3] > 0.5, depth - kdepth, np.inf)
            seen = gap < args.tol
            better = gap < fbest
            fallback[better] = col[better]
            fbest = np.minimum(fbest, gap)
            facing = np.clip(-(nrm[..., 0] * math.sin(t) + nrm[..., 1] * math.cos(t)), 0, 1)
            w = seen * facing ** args.power
            acc += col * w[..., None]
            wsum += w
        rgb = np.where(wsum[..., None] > 1e-3, acc / np.maximum(wsum, 1e-3)[..., None], fallback)
        Image.fromarray(np.dstack([np.clip(rgb, 0, 255).astype(np.uint8), alpha])).save(out / name)
    print(json.dumps({"frames": len(yaws), "views": order}))


if __name__ == "__main__":
    main()
