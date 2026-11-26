"""B2: carry the model sheet's painted views across the 3D proxy turn with EbSynth.

python3 b2_stylize.py PASSES SHEET_DIR PROJECTION OUT --views 0=front 45=three-quarter-right 90=profile-right --ebsynth BIN
PASSES holds the proxy turn from render_turn.py (position/, normal/, yaws.json, 16-bit) and key-YAW-{position,normal}
stills. The proxy was carved from the sheet, so each view registers onto its key render exactly. EbSynth then maps
the keys onto every frame with the proxy's object-space position and camera normals as guides; the proxy's alpha is
the final silhouette, so volume and horn count come from the 3D model and paint from the sheet.
"""
import argparse
import json
from pathlib import Path
import subprocess

import cv2
import numpy as np
from PIL import Image

ORTHO = 1.25


def edge_fill(rgba, iterations=24):
    """Extend colour past the silhouette so EbSynth patches at the edge do not pull in background."""
    rgb = rgba[..., :3].copy()
    mask = rgba[..., 3] > 127
    rgb[~mask] = 0
    for _ in range(iterations):
        blur = cv2.blur(rgb.astype(np.float32), (3, 3))
        wts = cv2.blur(mask.astype(np.float32), (3, 3))
        grow = (~mask) & (wts > 0)
        rgb[grow] = (blur[grow] / wts[grow, None]).astype(np.uint8)
        mask = mask | grow
    return rgb


def ebsynth(binary, style, guides, out, work, uniformity=3500):
    cmd = [binary, "-style", style]
    for src, dst, weight in guides:
        cmd += ["-guide", src, dst, "-weight", str(weight)]
    cmd += ["-output", out, "-uniformity", str(uniformity), "-patchsize", "5", "-pyramidlevels", "6", "-searchvoteiters", "12",
            "-patchmatchiters", "6", "-backend", "cpu"]
    subprocess.run(cmd, check=True, capture_output=True, cwd=work)


def gray(v, shape):
    return np.full(shape + (3,), int(round(min(1.0, max(0.0, v)) * 255)), np.uint8)


def to8(path):
    im = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
    rgb = im[..., [2, 1, 0]].astype(np.float32) / (65535.0 if im.dtype == np.uint16 else 255.0)
    a = im[..., 3].astype(np.float32) / (65535.0 if im.dtype == np.uint16 else 255.0)
    return (rgb * a[..., None] * 255).round().astype(np.uint8), (a * 255).round().astype(np.uint8)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("passes")
    ap.add_argument("sheet")
    ap.add_argument("projection")
    ap.add_argument("out")
    ap.add_argument("--views", nargs="+", required=True, help="YAW=SHEET_VIEW_NAME")
    ap.add_argument("--ebsynth", required=True)
    ap.add_argument("--yaw-weight", type=float, default=6.0)
    ap.add_argument("--normal-weight", type=float, default=0.7)
    ap.add_argument("--position-weight", type=float, default=20.0)
    ap.add_argument("--uniformity", type=float, default=3500)
    ap.add_argument("--only", type=int, nargs="*", help="frame indices to synthesise")
    args = ap.parse_args()
    P, out = Path(args.passes), Path(args.out)
    (out / "rgba").mkdir(parents=True, exist_ok=True)
    work = (out / "work").resolve()
    work.mkdir(exist_ok=True)
    pj = json.loads(Path(args.projection).read_text())
    yaws = json.loads((P / "yaws.json").read_text())
    keys = {}
    for spec in args.views:
        yaw, name = spec.split("=", 1)
        yaw = float(yaw)
        pos, alpha = to8(P / f"key-{yaw:g}-position.png")
        nrm, _ = to8(P / f"key-{yaw:g}-normal.png")
        n = alpha.shape[0]
        # The proxy was carved from this view, so the sheet maps onto the key render by the orthographic scale alone.
        k = n / ORTHO / pj["unit"]
        cx = pj["views"][name]["cx"]
        M = np.float32([[k, 0, n / 2 - cx * k], [0, k, n / 2 - pj["cy"] * k]])
        sheet = np.asarray(Image.open(Path(args.sheet) / f"{name}.png").convert("RGBA"))
        reg = cv2.warpAffine(sheet, M, (n, n), flags=cv2.INTER_AREA, borderValue=(0, 0, 0, 0))
        Image.fromarray(reg).save(work / f"key-{yaw:g}-registered.png")
        keys[yaw] = {"style": edge_fill(reg), "pos": pos, "nrm": nrm, "alpha": alpha}
    order = sorted(keys)
    span = max(order) - min(order) or 1.0
    for i, yaw in enumerate(yaws):
        if args.only and i not in args.only:
            continue
        name = f"{i:04d}.png"
        lo = max([k for k in order if k <= yaw], default=order[0])
        hi = min([k for k in order if k >= yaw], default=order[-1])
        pair = sorted({lo, hi})
        pos, alpha = to8(P / "position" / name)
        nrm, _ = to8(P / "normal" / name)
        shape = alpha.shape
        # Both bracketing keys sit side by side in one style image, so every patch comes from one drawing or the
        # other (no cross-fade); a flat yaw guide steers each frame toward the nearer key.
        style = np.hstack([keys[k]["style"] for k in pair])
        spos = np.hstack([keys[k]["pos"] for k in pair])
        snrm = np.hstack([keys[k]["nrm"] for k in pair])
        syaw = np.hstack([gray((k - min(order)) / span, shape) for k in pair])
        tpos = np.hstack([pos] + [np.zeros_like(pos)] * (len(pair) - 1))
        tnrm = np.hstack([nrm] + [np.zeros_like(nrm)] * (len(pair) - 1))
        tyaw = np.hstack([gray((yaw - min(order)) / span, shape)] + [np.zeros_like(pos)] * (len(pair) - 1))
        files = {}
        for label, im in (("style", style), ("spos", spos), ("snrm", snrm), ("syaw", syaw), ("tpos", tpos),
                          ("tnrm", tnrm), ("tyaw", tyaw)):
            files[label] = work / f"{name[:-4]}-{label}.png"
            Image.fromarray(im).save(files[label])
        target = work / f"{name[:-4]}-out.png"
        ebsynth(args.ebsynth, str(files["style"]),
                [(str(files["spos"]), str(files["tpos"]), args.position_weight), (str(files["snrm"]), str(files["tnrm"]), args.normal_weight),
                 (str(files["syaw"]), str(files["tyaw"]), args.yaw_weight)], str(target), work, args.uniformity)
        rgb = np.asarray(Image.open(target).convert("RGB"))[:, :shape[1]]
        Image.fromarray(np.dstack([rgb, alpha])).save(out / "rgba" / name)
        for f in files.values():
            f.unlink()
        print(name, yaw, flush=True)
    print(json.dumps({"frames": len(yaws), "keys": order}))


if __name__ == "__main__":
    main()
