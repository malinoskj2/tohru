"""B2: carry the model sheet's painted views across a 3D proxy turn with temporally-guided EbSynth.

python3 b2_stylize.py PASSES SHEET_DIR OUT --views 0=front 85=profile-right --ebsynth BIN [options]

Each frame's 16-bit object-position and camera-normal passes come from render_turn.py on the 3D proxy.
For every frame t, we project frame t's 3D surface points back into the previous synthesized frame using
poses.json (exact 3D advection + depth disocclusion check) and feed that advected RGB as a temporal
coherence guide to EbSynth alongside object position, camera normals, optional appearance guide, and yaw.
This locks synthesized 2D brushstrokes onto the rotating 3D surface and removes cross-fade ghosting.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess

import cv2
import numpy as np
from PIL import Image, ImageOps

ORTHO = 1.25


def edge_fill(rgba, iterations=28, erode_px=0):
    """Extend colour past the silhouette so EbSynth patches at the edge do not pull in background or dark ink."""
    rgb = rgba[..., :3].astype(np.float32).copy()
    mask = rgba[..., 3] > 127
    if erode_px > 0:
        k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * erode_px + 1, 2 * erode_px + 1))
        mask = cv2.erode(mask.astype(np.uint8), k) > 0
    rgb[~mask] = 0.0
    for _ in range(iterations):
        blur = cv2.blur(rgb, (3, 3))
        wts = cv2.blur(mask.astype(np.float32), (3, 3))
        grow = (~mask) & (wts > 1e-4)
        if not grow.any():
            break
        rgb[grow] = blur[grow] / wts[grow, None]
        mask = mask | grow
    return np.clip(rgb, 0, 255).astype(np.uint8)


def ebsynth(binary, style, guides, out, work, uniformity=3500, patchsize=5, backend="cpu"):
    cmd = [binary, "-style", str(style)]
    for src, dst, weight in guides:
        if weight > 0:
            cmd += ["-guide", str(src), str(dst), "-weight", f"{weight:.4f}"]
    cmd += [
        "-output", str(out),
        "-uniformity", str(uniformity),
        "-patchsize", str(patchsize),
        "-pyramidlevels", "6",
        "-searchvoteiters", "12",
        "-patchmatchiters", "6",
        "-backend", backend,
    ]
    subprocess.run(cmd, check=True, capture_output=True, cwd=work)


def gray(v, shape):
    return np.full(shape + (3,), int(round(min(1.0, max(0.0, v)) * 255)), np.uint8)


def read16(path):
    im = cv2.imread(str(path), cv2.IMREAD_UNCHANGED)
    if im is None:
        raise FileNotFoundError(path)
    denom = 65535.0 if im.dtype == np.uint16 else 255.0
    return im[..., [2, 1, 0, 3]].astype(np.float32) / denom


def to8(path):
    rgba = read16(path)
    rgb = rgba[..., :3]
    a = rgba[..., 3]
    return (rgb * a[..., None] * 255).round().astype(np.uint8), (a * 255).round().astype(np.uint8)


def project_xyz(X, Y, Z, yaw_deg, dip_deg, size, ortho=ORTHO):
    """Exact Blender Euler XYZ rotation (dip around X, then yaw around Z) into orthographic screen coords."""
    rx = math.radians(dip_deg)
    rz = math.radians(yaw_deg)
    cx, sx = math.cos(rx), math.sin(rx)
    cz, sz = math.cos(rz), math.sin(rz)
    y1 = Y * cx - Z * sx
    z1 = Y * sx + Z * cx
    wx = X * cz - y1 * sz
    wy = X * sz + y1 * cz
    wz = z1
    ppu = size / ortho
    u = (size / 2.0 + wx * ppu).astype(np.float32)
    v = (size / 2.0 - wz * ppu).astype(np.float32)
    return u, v, wy


def advect_3d(prev_rgb, prev_pos16, curr_pos16, prev_yaw, prev_dip, ortho=ORTHO, tol=0.025):
    """Warp prev_rgb into the current frame using exact 3D object coordinates and depth visibility."""
    h, w = curr_pos16.shape[:2]
    X = curr_pos16[..., 0] - 0.5
    Y = curr_pos16[..., 1] - 0.5
    Z = curr_pos16[..., 2] - 0.5
    curr_fg = curr_pos16[..., 3] > 0.5

    u, v, depth = project_xyz(X, Y, Z, prev_yaw, prev_dip, w, ortho)
    warped = cv2.remap(prev_rgb.astype(np.float32), u, v, cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
    # Check visibility against prev_pos16 depth
    p_sample = cv2.remap(prev_pos16, u, v, cv2.INTER_NEAREST, borderMode=cv2.BORDER_CONSTANT)
    pX, pY, pZ = p_sample[..., 0] - 0.5, p_sample[..., 1] - 0.5, p_sample[..., 2] - 0.5
    _, _, p_depth = project_xyz(pX, pY, pZ, prev_yaw, prev_dip, w, ortho)
    visible = curr_fg & (p_sample[..., 3] > 0.5) & ((depth - p_depth) < tol)
    return np.clip(warped, 0, 255).astype(np.uint8), visible


def align_sheet_to_alpha(sheet_rgba, target_alpha):
    """Fit a Sheet B view onto the 3D key silhouette by bounding box alignment."""
    th, tw = target_alpha.shape
    sa = sheet_rgba[..., 3] > 127
    ys_s, xs_s = np.nonzero(sa)
    ys_t, xs_t = np.nonzero(target_alpha)
    sx0, sx1, sy0, sy1 = xs_s.min(), xs_s.max(), ys_s.min(), ys_s.max()
    tx0, tx1, ty0, ty1 = xs_t.min(), xs_t.max(), ys_t.min(), ys_t.max()
    kx = max(1, tx1 - tx0) / max(1, sx1 - sx0)
    ky = max(1, ty1 - ty0) / max(1, sy1 - sy0)
    M = np.float32([
        [kx, 0, (tx0 + tx1) / 2.0 - (sx0 + sx1) / 2.0 * kx],
        [0, ky, (ty0 + ty1) / 2.0 - (sy0 + sy1) / 2.0 * ky],
    ])
    return cv2.warpAffine(sheet_rgba, M, (tw, th), flags=cv2.INTER_AREA, borderValue=(0, 0, 0, 0))


def synthesize_pass(
    order_indices,
    keys,
    key_yaws,
    passes_dir,
    poses,
    app_dir,
    work,
    out_dir,
    args,
):
    """Synthesize a sequence of frames in order_indices with exact 3D temporal guidance from previous output."""
    out_dir.mkdir(parents=True, exist_ok=True)
    yaws = poses["yaw"]
    dips = poses["dip"]
    ortho = poses.get("ortho", ORTHO)
    span = max(key_yaws) - min(key_yaws) or 1.0

    prev_rgb = None
    prev_pos16 = None
    prev_yaw = None
    prev_dip = None

    for step_idx, i in enumerate(order_indices):
        name = f"{i:04d}.png"
        yaw, dip = yaws[i], dips[i]
        pos16 = read16(passes_dir / "position" / name)
        pos8, alpha = to8(passes_dir / "position" / name)
        nrm8, _ = to8(passes_dir / "normal" / name)
        shape = alpha.shape

        # Reuse exact previous frame on static hold frames (identical yaw & dip)
        if prev_rgb is not None and abs(yaw - prev_yaw) < 1e-5 and abs(dip - prev_dip) < 1e-5:
            Image.fromarray(np.dstack([prev_rgb, alpha])).save(out_dir / name)
            prev_pos16 = pos16
            print(f"[{out_dir.name}] {name} yaw={yaw:.2f} (hold copy)", flush=True)
            continue

        # Appearance guide (e.g., clean B1 toon render or smooth baked color) if provided
        if app_dir is not None:
            app_rgba = np.asarray(Image.open(app_dir / name).convert("RGBA"))
            app_t = edge_fill(app_rgba, 16, erode_px=3)
        else:
            app_t = None

        # Choose keys: either all keys or bracketing pair
        if args.all_keys:
            pair = list(key_yaws)
        else:
            lo = max([k for k in key_yaws if k <= yaw], default=key_yaws[0])
            hi = min([k for k in key_yaws if k >= yaw], default=key_yaws[-1])
            pair = sorted({lo, hi})

        s_sz = args.synth_size
        def rsz(im):
            return cv2.resize(im, (s_sz, s_sz), interpolation=cv2.INTER_AREA) if s_sz and im.shape[0] != s_sz else im

        pos8_s = rsz(pos8)
        nrm8_s = rsz(nrm8)
        shape_s = pos8_s.shape[:2]

        style = np.hstack([rsz(keys[k]["style"]) for k in pair])
        spos = np.hstack([rsz(keys[k]["pos"]) for k in pair])
        snrm = np.hstack([rsz(keys[k]["nrm"]) for k in pair])
        syaw = np.hstack([gray((k - min(key_yaws)) / span, shape_s) for k in pair])

        pad_zeros = [np.zeros_like(pos8_s)] * (len(pair) - 1)
        tpos = np.hstack([pos8_s] + pad_zeros)
        tnrm = np.hstack([nrm8_s] + pad_zeros)
        tyaw = np.hstack([gray((yaw - min(key_yaws)) / span, shape_s)] + pad_zeros)

        guides = []
        files = {}

        def save_tmp(label, im):
            p = work / f"{out_dir.name}-{name[:-4]}-{label}.png"
            Image.fromarray(im).save(p)
            files[label] = p
            return p

        p_style = save_tmp("style", style)
        guides.append((save_tmp("spos", spos), save_tmp("tpos", tpos), args.position_weight))
        guides.append((save_tmp("snrm", snrm), save_tmp("tnrm", tnrm), args.normal_weight))
        if args.yaw_weight > 0 and len(pair) > 1:
            guides.append((save_tmp("syaw", syaw), save_tmp("tyaw", tyaw), args.yaw_weight))

        if app_t is not None and args.appearance_weight > 0:
            sapp = np.hstack([rsz(keys[k]["app"]) for k in pair])
            tapp = np.hstack([rsz(app_t)] + pad_zeros)
            guides.append((save_tmp("sapp", sapp), save_tmp("tapp", tapp), args.appearance_weight))

        # Temporal guidance from previous synthesized output frame via exact 3D advection
        if prev_rgb is not None and args.temporal_weight > 0:
            adv_rgb, vis = advect_3d(prev_rgb, prev_pos16, pos16, prev_yaw, prev_dip, ortho=ortho, tol=0.025)
            if app_t is not None:
                adv_full = np.where(vis[..., None], adv_rgb, app_t)
            else:
                adv_full = edge_fill(np.dstack([adv_rgb, (vis.astype(np.uint8) * 255)]), 24)
            adv_full = (adv_full.astype(np.float32) * (alpha[..., None] / 255.0)).round().astype(np.uint8)
            ttemp = np.hstack([rsz(adv_full)] + pad_zeros)
            guides.append((p_style, save_tmp("ttemp", ttemp), args.temporal_weight))

        target = work / f"{out_dir.name}-{name[:-4]}-out.png"
        ebsynth(
            args.ebsynth,
            p_style,
            guides,
            target,
            work,
            uniformity=args.uniformity,
            patchsize=args.patchsize,
            backend=args.backend,
        )
        rgb = np.asarray(Image.open(target).convert("RGB"))[:, :shape_s[1]]
        if rgb.shape[:2] != shape:
            rgb = cv2.resize(rgb, (shape[1], shape[0]), interpolation=cv2.INTER_CUBIC)
        Image.fromarray(np.dstack([rgb, alpha])).save(out_dir / name)

        for p in files.values():
            p.unlink(missing_ok=True)
        target.unlink(missing_ok=True)

        prev_rgb = rgb
        prev_pos16 = pos16
        prev_yaw = yaw
        prev_dip = dip
        print(f"[{out_dir.name}] {name} yaw={yaw:.2f}", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("passes")
    ap.add_argument("sheet")
    ap.add_argument("out")
    ap.add_argument("--views", nargs="+", required=True, help="YAW=SHEET_VIEW_NAME or YAW=PATH")
    ap.add_argument("--projection", default=None, help="optional projection.json (for visual hull registration)")
    ap.add_argument("--appearance-dir", default=None, help="optional directory of aligned per-frame appearance renders + key-YAW-app.png")
    ap.add_argument("--ebsynth", required=True)
    ap.add_argument("--yaw-weight", type=float, default=2.0)
    ap.add_argument("--normal-weight", type=float, default=1.5)
    ap.add_argument("--position-weight", type=float, default=16.0)
    ap.add_argument("--appearance-weight", type=float, default=4.0)
    ap.add_argument("--temporal-weight", type=float, default=8.0)
    ap.add_argument("--uniformity", type=float, default=3500)
    ap.add_argument("--patchsize", type=int, default=5)
    ap.add_argument("--synth-size", type=int, default=320, help="working resolution for EbSynth (upscaled back to pass size)")
    ap.add_argument("--erode-style", type=int, default=5, help="strip dark 2D outline from style keys before edge_fill")
    ap.add_argument("--backend", default="cpu", choices=("cpu", "cuda"))
    ap.add_argument("--direction", default="backward", choices=("forward", "backward", "bidir"))
    ap.add_argument("--all-keys", action="store_true", help="include all keys in every frame's style atlas")
    args = ap.parse_args()

    P, out = Path(args.passes), Path(args.out)
    (out / "rgba").mkdir(parents=True, exist_ok=True)
    work = (out / "work").resolve()
    work.mkdir(parents=True, exist_ok=True)

    poses_path = P / "poses.json"
    if poses_path.exists():
        poses = json.loads(poses_path.read_text())
    else:
        yaws = json.loads((P / "yaws.json").read_text())
        poses = {"yaw": yaws, "dip": [0.0] * len(yaws), "ortho": ORTHO}

    pj = json.loads(Path(args.projection).read_text()) if args.projection else None
    app_dir = Path(args.appearance_dir) if args.appearance_dir else None

    keys = {}
    for spec in args.views:
        yaw_s, name = spec.split("=", 1)
        yaw = float(yaw_s)
        pos, alpha = to8(P / f"key-{yaw:g}-position.png")
        nrm, _ = to8(P / f"key-{yaw:g}-normal.png")
        n = alpha.shape[0]
        src_path = Path(name) if Path(name).exists() else Path(args.sheet) / f"{name}.png"
        sheet_rgba = np.asarray(Image.open(src_path).convert("RGBA"))
        if pj is not None and name in pj.get("views", {}):
            k = n / ORTHO / pj["unit"]
            cx = pj["views"][name]["cx"]
            M = np.float32([[k, 0, n / 2 - cx * k], [0, k, n / 2 - pj["cy"] * k]])
            reg = cv2.warpAffine(sheet_rgba, M, (n, n), flags=cv2.INTER_AREA, borderValue=(0, 0, 0, 0))
        elif sheet_rgba.shape[:2] == (n, n) and Path(name).exists():
            reg = sheet_rgba
        else:
            reg = align_sheet_to_alpha(sheet_rgba, alpha > 127)
        Image.fromarray(reg).save(work / f"key-{yaw:g}-registered.png")
        key_entry = {"style": edge_fill(reg, 36, erode_px=args.erode_style), "pos": pos, "nrm": nrm, "alpha": alpha}
        if app_dir is not None:
            key_app_path = app_dir / f"key-{yaw:g}.png"
            if key_app_path.exists():
                key_entry["app"] = edge_fill(np.asarray(Image.open(key_app_path).convert("RGBA")), 16, erode_px=3)
            else:
                key_entry["app"] = edge_fill(reg, 16, erode_px=args.erode_style)
        keys[yaw] = key_entry

    key_yaws = sorted(keys)
    n_frames = len(poses["yaw"])

    if args.direction == "forward":
        synthesize_pass(list(range(n_frames)), keys, key_yaws, P, poses, app_dir, work, out / "rgba", args)
    elif args.direction == "backward":
        synthesize_pass(list(range(n_frames - 1, -1, -1)), keys, key_yaws, P, poses, app_dir, work, out / "rgba", args)
    else:
        fwd_dir = out / "fwd"
        bwd_dir = out / "bwd"
        synthesize_pass(list(range(n_frames)), keys, key_yaws, P, poses, app_dir, work, fwd_dir, args)
        synthesize_pass(list(range(n_frames - 1, -1, -1)), keys, key_yaws, P, poses, app_dir, work, bwd_dir, args)
        # Smooth temporal ramp between forward and backward passes (using yaw progress)
        y0, y1 = poses["yaw"][0], poses["yaw"][-1]
        denom = (y1 - y0) or 1.0
        for i in range(n_frames):
            name = f"{i:04d}.png"
            f_im = np.asarray(Image.open(fwd_dir / name).convert("RGBA")).astype(np.float32)
            b_im = np.asarray(Image.open(bwd_dir / name).convert("RGBA")).astype(np.float32)
            u = float(np.clip((poses["yaw"][i] - y0) / denom, 0.0, 1.0))
            w = u * u * (3.0 - 2.0 * u)
            rgb = (1.0 - w) * f_im[..., :3] + w * b_im[..., :3]
            Image.fromarray(np.dstack([np.clip(rgb, 0, 255).astype(np.uint8), f_im[..., 3].astype(np.uint8)])).save(out / "rgba" / name)

    print(json.dumps({"frames": n_frames, "keys": key_yaws, "direction": args.direction}))


if __name__ == "__main__":
    main()
