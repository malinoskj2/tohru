"""Carve a visual-hull proxy from the model sheet's silhouettes, so every sheet view reprojects onto it exactly.

python3 make_hull.py SHEET_DIR OUT_DIR [SKIP_VIEW,...]
Views named in SKIP_VIEW are left out of the carve (a view drawn inconsistently with the others).
Coordinates are Blender's: X right, Y away from the camera (the dragon's front faces -Y), Z up. A sheet view drawn at
yaw t shows the dragon rotated by t about Z (render_turn.py's yaw), projected orthographically: screen x = X cos t -
Y sin t. Writes hull.glb (already in render units, rotation axis at the origin) and projection.json, which maps
object coordinates to each sheet view's pixels for painting.
"""
import json
import math
from pathlib import Path
import sys

import numpy as np
from PIL import Image
from scipy import ndimage
from skimage import measure
import trimesh

VIEWS = {"front": 0, "three-quarter-right": 45, "profile-right": 90, "three-quarter-back": 135, "back": 180}
UNIT = 600.0
N = 176


def main():
    sheet, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    sil = {k: np.asarray(Image.open(sheet / f"{k}.png").convert("RGBA"))[..., 3] > 127 for k in VIEWS}
    h, w = next(iter(sil.values())).shape
    rows = np.nonzero(np.any(np.stack(list(sil.values())), axis=(0, 2)))[0]
    cy = (rows.min() + rows.max()) / 2
    g = np.linspace(-0.6, 0.6, N)
    X, Y, Z = np.meshgrid(g, g, g, indexing="ij")
    v = np.clip(np.round(cy - Z * UNIT).astype(int), 0, h - 1)

    def proj(name, cx):
        t = math.radians(VIEWS[name])
        u = np.round(cx + (X * math.cos(t) - Y * math.sin(t)) * UNIT).astype(int)
        ok = (u >= 0) & (u < w)
        return sil[name][v, np.clip(u, 0, w - 1)] & ok

    skip = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else set()

    def carve(cxs):
        occ = np.ones(X.shape, bool)
        for k in VIEWS:
            if k not in skip:
                occ &= proj(k, cxs[k])
        return occ

    pitch = (g[1] - g[0]) * UNIT
    hs, ws = int(h / pitch) + 1, int(w / pitch) + 1
    small = {}
    for k in VIEWS:
        ys, xs = np.nonzero(sil[k])
        acc = np.zeros((hs, ws))
        np.add.at(acc, ((ys / pitch).astype(int), (xs / pitch).astype(int)), 1)
        small[k] = acc > pitch * pitch / 2

    def score(cxs):
        occ = carve(cxs)
        total = 0.0
        for k in VIEWS:
            t = math.radians(VIEWS[k])
            u = cxs[k] + (X * math.cos(t) - Y * math.sin(t)) * UNIT
            img = np.zeros((hs, ws), bool)
            img[(v[occ] / pitch).astype(int), np.clip(u[occ] / pitch, 0, ws - 1).astype(int)] = True
            total += (img & small[k]).sum() / (img | small[k]).sum()
        return total / len(VIEWS)

    # Start each axis at the silhouette's middle column, then refine by coordinate descent on reprojection IoU.
    cxs = {}
    for k in VIEWS:
        cols = np.nonzero(sil[k].any(0))[0]
        cxs[k] = (cols.min() + cols.max()) / 2
    best = score(cxs)
    for step in (16, 8, 4, 2):
        improved = True
        while improved:
            improved = False
            for k in list(VIEWS)[1:]:
                for d in (-step, step):
                    trial = dict(cxs, **{k: cxs[k] + d})
                    s = score(trial)
                    if s > best + 1e-4:
                        best, cxs, improved = s, trial, True
        print("step", step, round(best, 4), {k: round(c, 1) for k, c in cxs.items()}, flush=True)
    occ = carve(cxs)
    occ = ndimage.binary_opening(occ, iterations=1)
    lab, n = ndimage.label(occ)
    if n > 1:
        sizes = ndimage.sum(occ, lab, range(1, n + 1))
        occ = lab == (1 + int(np.argmax(sizes)))
    field = ndimage.gaussian_filter(occ.astype(np.float32), 1.0)
    verts, faces, _, _ = measure.marching_cubes(field, 0.5, spacing=(g[1] - g[0],) * 3)
    verts += g[0]
    mesh = trimesh.Trimesh(verts, faces[:, ::-1])
    trimesh.smoothing.filter_taubin(mesh, iterations=10)
    # glTF is Y-up with +Z towards the viewer: Blender (x, y, z) -> glTF (x, z, -y).
    gl = mesh.copy()
    gl.vertices = np.c_[mesh.vertices[:, 0], mesh.vertices[:, 2], -mesh.vertices[:, 1]]
    gl.export(out / "hull.glb")
    (out / "projection.json").write_text(json.dumps({"unit": UNIT, "cy": cy, "size": [w, h], "iou": best,
                                                     "views": {k: {"yaw": VIEWS[k], "cx": cxs[k]} for k in VIEWS}}, indent=1))
    print(json.dumps({"iou": round(best, 4), "faces": len(faces), "voxels": int(occ.sum())}))


if __name__ == "__main__":
    main()
