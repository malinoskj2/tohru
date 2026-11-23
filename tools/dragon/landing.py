#!/usr/bin/env python3
"""Draw the entrance brake from registered body parts, projected bone chains and the actual endpoint pixels.
The JSON records each painted part's visible selection, joint chain and underlap in the original 240px cell.
Facing keys share the live puppet's artwork. No generated in-between or whole-sprite opacity transition is used.
"""

import argparse
import json
from pathlib import Path
from tempfile import TemporaryDirectory

import cv2
import numpy as np
from PIL import Image
from scipy.interpolate import PchipInterpolator
from scipy.ndimage import distance_transform_edt

cv2.setNumThreads(1)
ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / "shell/assistant/dragon"


# Each visible part has a source selection, projected bone chain, and an optional painted underlap.
# Polygon winding describes the painted material, not the movement itself.
def read_rig():
    raw = json.loads((ROOT / "tools/dragon/art/landing-rig.json").read_text())
    return {
        int(cell): {
            part: (
                [
                    [
                        (float(x), float(y))
                        for x, y in (p.split(",") for p in line.split())
                    ]
                    for line in value
                ]
                if part != "face"
                else value
            )
            for part, value in items.items()
        }
        for cell, items in raw.items()
    }


R = read_rig()


# Far paw in the thinking pose comes in front of her cheek.
def mask(poly):
    a = np.zeros((240, 240), np.uint8)
    if poly:
        cv2.fillPoly(a, [np.rint(poly).astype(np.int32)], 1)
    return a > 0


def chain(path, n=9):
    p = np.array(path, float)
    ds = np.r_[0, np.cumsum(np.linalg.norm(np.diff(p, axis=0), axis=1))]
    t = np.linspace(0, ds[-1], n)
    if len(p) > 2:
        return PchipInterpolator(ds, p)(t)
    return np.stack([np.interp(t, ds, p[:, k]) for k in range(2)], 1)


def unit(p):
    return p / (np.linalg.norm(p, axis=-1, keepdims=True) + 1e-9)


def normal(p):
    return np.stack([-p[..., 1], p[..., 0]], -1)


def field(b):
    return cv2.distanceTransform(
        b.astype("uint8"), cv2.DIST_L2, 5
    ) - cv2.distanceTransform((~b).astype("uint8"), cv2.DIST_L2, 5)


def sprite(k):
    l = k % 24 * 240
    t = k // 24 * 240
    return np.array(atlas.crop((l, t, l + 240, t + 240)))


# Underpaint the connected mass behind overlapping paws, horn bases and tail.
# Ellipses are inside the painted outer contour; just the covered joints extend.
CORE = {
    507: ((91, 102, 35, 34, -8), (128, 141, 36, 23, 0)),
    0: ((90, 78, 45, 42, -10), (104, 155, 47, 34, 67)),
    417: ((171, 103, 38, 33, -18), (141, 154, 49, 29, -32)),
    327: ((157, 69, 38, 35, -18), (135, 122, 45, 30, -48)),
    309: ((60, 114, 38, 32, 10), (115, 151, 45, 27, 9)),
    489: ((106, 78, 35, 29, -12), (115, 132, 36, 29, 65)),
}
TAIL = {
    507: [16, 15, 12, 9, 8, 5, 3, 1],
    0: [17, 20, 21, 20, 17, 14, 10, 7, 4, 1],
    417: [14, 16, 13, 10, 6, 3, 1],
    327: [17, 17, 14, 10, 7, 4, 1],
    309: [16, 15, 14, 12, 9, 6, 3, 1],
    489: [13, 13, 10, 8, 6, 4, 2, 1],
}


def oval(params):
    x, y, rx, ry, angle = params
    b = np.zeros((240, 240), np.uint8)
    cv2.ellipse(b, (int(x), int(y)), (int(rx), int(ry)), angle, 0, 360, 1, -1)
    return b > 0


def capsule(path, radii):
    c = chain(path, 60)
    p = np.array(path)
    s = np.r_[0, np.cumsum(np.linalg.norm(np.diff(p, axis=0), axis=1))]
    rr = np.interp(np.linspace(0, s[-1], len(c)), s, radii)
    b = np.zeros((240, 240), np.uint8)
    for pt, r in zip(c, rr):
        cv2.circle(b, tuple(np.rint(pt).astype(int)), round(r), 1, -1)
    return b > 0


def prep(k):
    im = sprite(k)
    reg = R[k]
    opacity = im[:, :, 3].astype(float) / 255
    keys = [
        "belly",
        "tail",
        "farwing",
        "wing",
        "head",
        "gold",
        "lilac",
        "farfoot",
        "foot",
        "farpaw",
        "paw",
    ]
    visible = opacity > 0.12
    raw = {}
    owned = np.full((240, 240), -1)
    for i, key in enumerate(keys):
        if key not in reg:
            continue
        raw[key] = mask(reg[key][0])
        if key == "tail":
            raw[key] &= capsule(reg[key][1], np.array(TAIL[k]) * 1.35 + 2)
        owned[visible & raw[key]] = i
    miss = visible & (owned < 0)
    near = distance_transform_edt(
        owned < 0, return_distances=False, return_indices=True
    )
    owned[miss] = owned[tuple(near[:, miss])]
    parts = {}
    colors = im[:, :, :3].copy()
    # Face marks remain individually registered as the muzzle and gaze swing toward the viewer.
    for i, (x, y, r) in enumerate(reg["face"]):
        key = "face" + str(i)
        yi, xi = np.indices((240, 240))
        rx, ry = (r * 1.5, r * 1.55) if i < 2 else (r * 1.8, r)
        box = (xi - x) ** 2 / rx**2 + (yi - y) ** 2 / ry**2 < 1
        inside = box & (opacity > 0.92)
        # Inpaint only the registered dark eye or smile. The feature carries
        # a signed material residual, so it cannot paint a second cheek.
        lum = colors[:, :, :3].astype("float32").mean(2)
        background = np.percentile(lum[inside], 82)
        ink = (inside & (lum < background - (13 if i == 2 else 20))).astype("uint8")
        ink = cv2.dilate(ink, np.ones((3, 3), np.uint8)) & inside.astype("uint8")
        plain = cv2.inpaint(colors, ink * 255, 3, cv2.INPAINT_TELEA)
        residual = colors.astype("float32") - plain.astype("float32")
        parts[key] = {
            "chain": chain([(x, y - ry), (x, y + ry)]),
            "width": rx,
            "dist": field(ink > 0),
            "rgb": residual,
        }
        colors = plain
    for i, key in enumerate(keys):
        if key not in reg:
            continue
        sel = owned == i
        if not sel.any():
            continue
        # Extend material only beneath adjoining parts. Visible exterior retains its own silhouette.
        under = mask(reg[key][2])
        full = sel | under
        if key in ("head", "belly"):
            full |= oval(CORE[k][key == "belly"])
        if key == "tail":
            full |= capsule(reg[key][1], TAIL[k])
        if key in ("paw", "foot", "farpaw", "farfoot"):
            full |= capsule(reg[key][1], [6, 7, 4] if "paw" in key else [8, 9, 5])
            # Remove selection triangles cut from a neighbor at the concave joint.
            full &= capsule(reg[key][1], [11, 14, 11] if "paw" in key else [13, 17, 14])
        if key == "belly":
            full = (
                cv2.morphologyEx(
                    full.astype("uint8"), cv2.MORPH_CLOSE, np.ones((13, 13), np.uint8)
                )
                > 0
            )
        full = (
            cv2.morphologyEx(
                full.astype("uint8"), cv2.MORPH_CLOSE, np.ones((3, 3), np.uint8)
            )
            > 0
        )
        full = (
            cv2.GaussianBlur(
                full.astype("float32"),
                (0, 0),
                1.6 if key not in ("head", "belly") else 2.5,
            )
            > 0.46
        )
        order = [
            "tail",
            "farwing",
            "wing",
            "farfoot",
            "belly",
            "foot",
            "head",
            "gold",
            "lilac",
            "farpaw",
            "paw",
        ]
        nearer = np.isin(
            owned, [keys.index(n) for n in order[order.index(key) + 1 :] if n in reg]
        )
        full = sel | (full & nearer & visible)
        inside = cv2.erode(sel.astype("uint8"), np.ones((5, 5), np.uint8)) > 0
        idx = distance_transform_edt(
            ~(inside if inside.any() else sel),
            return_indices=True,
            return_distances=False,
        )
        material = colors[idx[0], idx[1]].copy()
        coverage = inside.astype("float32")
        weight = cv2.GaussianBlur(coverage, (0, 0), 8)
        fill = cv2.GaussianBlur(
            colors.astype("float32") * coverage[:, :, None], (0, 0), 8
        ) / np.maximum(weight[:, :, None], 1e-5)
        material = np.where((weight > 0.01)[:, :, None], fill, material).astype("uint8")
        material[sel] = colors[sel]
        coords = chain(reg[key][1])
        length = np.linalg.norm(np.diff(coords, axis=0), axis=1).sum()
        parts[key] = {
            "chain": coords,
            "width": full.sum() / length,
            "dist": field(full),
            "rgb": material,
        }
        if key in ("head", "belly"):
            radii = sorted(CORE[k][key == "belly"][2:4])
            parts[key]["oval"] = (radii[1], radii[0]) if key == "head" else tuple(radii)
        if key == "tail":
            parts[key]["deep"] = k in (0, 507)
            path = np.array(reg[key][1])
            arc = np.r_[0, np.cumsum(np.linalg.norm(np.diff(path, axis=0), axis=1))]
            parts[key]["radii"] = np.interp(
                np.linspace(0, arc[-1], len(coords)), arc, TAIL[k]
            )
    wing = parts.pop("wing")
    other = parts.pop("farwing", None)
    side = -1 if wing["chain"][-1, 0] < wing["chain"][0, 0] else 1
    if other is None:
        # A contralateral wing is tucked behind the torso in the three-quarter keys.
        # Use the same painted membrane; at a frontal yaw both wings become visible.
        mirrored = wing["chain"].copy()
        mirrored[:, 0] = 239 - mirrored[:, 0]
        root = parts["belly"]["chain"][2]
        other = {
            "chain": root + (mirrored - mirrored[0]) * 0.12,
            "bind": mirrored,
            "width": wing["width"] * 0.32,
            "paint_width": wing["width"],
            "dist": np.ascontiguousarray(wing["dist"][:, ::-1]),
            "rgb": np.ascontiguousarray(wing["rgb"][:, ::-1]),
        }
    parts["leftwing" if side < 0 else "rightwing"] = wing
    parts["rightwing" if side < 0 else "leftwing"] = other
    return parts


def smooth(t):
    return t * t * (3 - 2 * t)


def mix(a, b, t):
    return a + (b - a) * t


def angles(c):
    d = np.diff(c, axis=0)
    return np.unwrap(np.arctan2(d[:, 1], d[:, 0]))


def interp_chain(a, b, t, key, root):
    da, db = angles(a), angles(b)
    delta = (db[0] - da[0] + np.pi) % (2 * np.pi) - np.pi
    if key in ("gold", "lilac") and abs(delta) > 1.9 and np.sin(da[0] + delta / 2) > 0:
        delta -= np.sign(delta) * 2 * np.pi
    delta = delta + (db - db[0]) - (da - da[0])
    tt = t
    if key == "tail":
        tt = t - 0.13 * np.sin(np.pi * t) * np.linspace(0, 1, len(da))
    ang = da + delta * tt
    length = mix(
        np.linalg.norm(np.diff(a, axis=0), axis=1),
        np.linalg.norm(np.diff(b, axis=0), axis=1),
        t,
    )
    if key in ("gold", "lilac") and abs(delta[0]) > 0.8:
        length *= 1 - 0.14 * np.sin(np.pi * t)
    if key == "tail":
        length *= 1 - 0.08 * np.sin(np.pi * t) * np.linspace(0, 1, len(length))
    v = np.stack([np.cos(ang) * length, np.sin(ang) * length], -1)
    return np.vstack([root, root + np.cumsum(v, axis=0)])


def attach(a, b, c, p, q, t):
    # Preserve the attach point in the parent's changing bone frame.
    def local(chain, p):
        u = unit(chain[-1] - chain[0])
        v = p - chain[0]
        return np.array([v @ u, v @ normal(u)])

    off = mix(local(a, p), local(b, q), t)
    axis = unit(c[-1] - c[0])
    return c[0] + off[0] * axis + off[1] * normal(axis)


parents = {
    "head": "belly",
    "tail": "belly",
    "leftwing": "belly",
    "rightwing": "belly",
    "foot": "belly",
    "farfoot": "belly",
    "paw": "belly",
    "farpaw": "belly",
    "gold": "head",
    "lilac": "head",
    "face0": "head",
    "face1": "head",
    "face2": "head",
}


def rect(m, w):
    # Inverse skin bind: stable bone-local tangent and transverse coordinates.
    pad = max(15, min(106, w * 1.1))
    lo = np.maximum(np.floor(m.min(axis=0) - pad), -24).astype(int)
    hi = np.minimum(np.ceil(m.max(axis=0) + pad), 264).astype(int)
    return lo, hi


def bind(c, src, r, xy):
    # Continuous interpolated tangent through the joints of the painted chain.
    ds = np.diff(c, axis=0)
    flat = xy.reshape(-1, 2)
    best = np.full(len(flat), np.inf, np.float32)
    ind = np.zeros(len(flat), np.int32)
    frac = np.zeros(len(flat))
    for i, d in enumerate(ds):
        t = np.clip(((flat - c[i]) * d).sum(axis=1) / (d @ d + 1e-12), 0, 1)
        delta = flat - c[i] - t[:, None] * d
        err = (delta * delta).sum(axis=1)
        keep = err < best
        best[keep] = err[keep]
        ind[keep] = i
        frac[keep] = t[keep]

    def tangents(curve):
        return unit(np.gradient(curve, axis=0))

    def basis(curve):
        ts = tangents(curve)
        return mix(curve[ind], curve[ind + 1], frac[:, None]), unit(
            mix(ts[ind], ts[ind + 1], frac[:, None])
        )

    j, u = basis(c)
    k, v = basis(src)
    delta = flat - j
    along = np.sum(delta * u, axis=1)
    across = np.sum(delta * normal(u), axis=1)
    return (
        (k + along[:, None] * v + r * across[:, None] * normal(v))
        .reshape(xy.shape)
        .astype(np.float32)
    )


def sample(a, coords):
    return cv2.remap(a, coords, None, cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)


def skeleton(pa, pb, s):
    sk = {}
    for key in [
        "belly",
        "head",
        "tail",
        "leftwing",
        "rightwing",
        "farfoot",
        "foot",
        "farpaw",
        "paw",
        "gold",
        "lilac",
        "face0",
        "face1",
        "face2",
    ]:
        if key not in pa or key not in pb:
            continue
        a, b = pa[key]["chain"], pb[key]["chain"]
        root = mix(a[0], b[0], s)
        if key in parents:
            p = parents[key]
            root = attach(pa[p]["chain"], pb[p]["chain"], sk[p], a[0], b[0], s)
        sk[key] = interp_chain(a, b, s, key, root)
        if key == "tail" and pb["tail"]["deep"]:
            # A longer resting coil rolls into depth while it swings. Shorten
            # only its projected reach, preserving the material radius. The
            # signed bends stay ordered and the hip always carries the root.
            turn = np.sin(np.pi * s) ** 2
            sk[key] = root + (sk[key] - root) * [1 - 0.16 * turn, 1 - 0.55 * turn]
        if (
            key in ("leftwing", "rightwing")
            and pa["face0"]["chain"][4, 0] > pa["head"]["chain"][4, 0]
        ):
            side = -1 if key == "leftwing" else 1
            u = np.linspace(0, 1, len(a))
            chest = sk["belly"][1]
            spread = (
                np.stack([side * (13 + 52 * u), -3 - 8 * np.sin(np.pi * u) + 3 * u], 1)
                + chest
            )
            sk[key] = mix(sk[key], spread, np.sin(np.pi * s) ** 2)
        if key in ("paw", "farpaw", "foot", "farfoot"):
            # Screen-side balancing reach. Both paws open away from the chest
            # before the final chin/chest tuck. Feet swing below the pelvis.
            side = -1 if key.startswith("far") else 1
            hand = "paw" in key
            pivot = sk["belly"][0 if hand else -1]
            offsets = np.linspace(0, 1, len(sk[key]))[:, None]
            middle = (
                pivot
                + np.array([side * (14 if hand else 8), 5 if hand else -1])
                + offsets * np.array([side * (9 if hand else 6), 12 if hand else 15])
            )
            weight = np.sin(np.pi * s) ** 2 * (0.92 if hand else 0.55)
            sk[key] = mix(sk[key], middle, weight)
        if (
            key in ("gold", "lilac")
            and pa["face0"]["chain"][4, 0] > pa["head"]["chain"][4, 0]
        ):
            side = 1 if key == "gold" else -1
            phase = np.sin(np.pi * s)
            angle = side * 0.49 * phase
            rot = np.array(
                [[np.cos(angle), -np.sin(angle)], [np.sin(angle), np.cos(angle)]]
            )
            sk[key] = root + (sk[key] - root) @ rot.T
            sk[key][:, 0] += side * 9 * phase
    return sk


def render(pa, pb, t, diag=None):
    out = np.zeros((288, 288, 4), np.float32)
    s = smooth(t)
    sk = skeleton(pa, pb, s)
    if diag is not None:
        diag.update({key: c.tolist() for key, c in sk.items()})
    order = [
        "tail",
        "leftwing",
        "rightwing",
        "farfoot",
        "belly",
        "foot",
        "gold-root",
        "lilac-root",
        "head",
        "gold",
        "lilac",
        "farpaw",
        "paw",
        "face0",
        "face1",
        "face2",
    ]
    for layer in order:
        key = layer.removesuffix("-root")
        stem = key != layer
        if key not in sk:
            continue
        a, b = pa[key], pb[key]
        mid = sk[key]
        w = mix(a["width"], b["width"], s)
        lo, hi = rect(mid, max(w * 1.6, 26))
        if np.any(hi <= lo):
            continue
        x, y = lo
        X, Y = hi
        yy, xx = np.mgrid[y:Y, x:X]
        xy = np.stack([xx, yy], -1)
        ca = bind(
            mid, a.get("bind", a["chain"]), a.get("paint_width", a["width"]) / w, xy
        )
        cb = bind(
            mid, b.get("bind", b["chain"]), b.get("paint_width", b["width"]) / w, xy
        )
        da = sample(a["dist"], ca) * w / a.get("paint_width", a["width"])
        db = sample(b["dist"], cb) * w / b.get("paint_width", b["width"])
        sdf = mix(da, db, s)
        wjoint = np.sin(np.pi * s) ** 0.7
        if key == "tail" or stem:
            # Bones carry a finite painted volume across occluded roots. Registered
            # radii follow the tail taper; a shoulder/hip cannot become a cut edge
            # when a neighboring piece uncovers it.
            spine = mid
            if key == "tail":
                rr = mix(a["radii"], b["radii"], s)
                spine = np.vstack([sk["belly"][-2], spine])
                rr = np.r_[rr[0] * 0.9, rr]
                knots = np.arange(len(spine))
                samples = np.linspace(0, knots[-1], 36)
                spine = PchipInterpolator(knots, spine)(samples)
                rr = PchipInterpolator(knots, rr)(samples)
            else:
                rr = np.interp(
                    np.linspace(0, 1, len(mid)),
                    [0, 0.28, 0.62, 0.84, 1],
                    [8.8, 9, 7.1, 4.3, 1.1],
                )
                spine = np.vstack([sk["head"][2] + [0, -6], spine])
                rr = np.r_[7.5, rr]
            volume = np.full(sdf.shape, -100.0, np.float32)
            if key == "tail":
                side_coordinate = np.zeros_like(volume)
            for j in range(len(spine) - 1):
                diff = spine[j + 1] - spine[j]
                u = np.clip(
                    ((xy - spine[j]) * diff).sum(axis=2) / (diff @ diff + 1e-9), 0, 1
                )
                dist = np.sqrt(
                    ((xy - spine[j] - u[:, :, None] * diff) ** 2).sum(axis=2)
                )
                radius = mix(rr[j], rr[j + 1], u)
                edge = radius - dist
                if key == "tail":
                    relative = xy - spine[j] - u[:, :, None] * diff
                    across = (relative @ normal(unit(diff))) / np.maximum(radius, 1)
                    side_coordinate = np.where(edge > volume, across, side_coordinate)
                volume = np.maximum(volume, edge)
            if stem:
                # Buried stem lives behind the skull. It bridges the horn to
                # the brow while the painted horn on top retains its own rim.
                sdf = volume - 12 * (1 - wjoint)
            elif key == "tail":
                sdf = mix(sdf, volume, smooth(min(1, s / 0.10, (1 - s) / 0.10)))
        if (
            key in ("belly", "head")
            and pa["face0"]["chain"][4, 0] > pa["head"]["chain"][4, 0]
        ):
            # At the frontal view the far-side cheek and shoulder cover the rear
            # scallops. Keep the skull and ribcage round as the appendages change depth.
            y0, x0 = np.mgrid[: Y - y, : X - x]
            center = mid[4] - lo
            rx, ry = mix(np.array(a["oval"]), np.array(b["oval"]), s)
            radial = np.sqrt(
                ((x0 - center[0]) / rx) ** 2 + ((y0 - center[1]) / ry) ** 2
            )
            ellipse = (1 - radial) * (31 if key == "head" else 28)
            sdf = mix(sdf, ellipse, 0.78 * np.sin(np.pi * s) ** 2)
        if key == "belly":
            axis = unit(mid[-1] - mid[0])
            off = xy - mid[4]
            transverse = off @ normal(axis)
            along = off @ axis
            rx, ry = mix(np.array(a["oval"]), np.array(b["oval"]), s)
            core = (1 - np.sqrt((transverse / rx) ** 2 + (along / ry) ** 2)) * rx
            sdf = np.maximum(sdf, core - 25 * (1 - wjoint))
        rgb = mix(
            sample(a["rgb"], ca).astype(float), sample(b["rgb"], cb).astype(float), s
        )
        if key == "tail":
            # A measured clean section of lavender tail supplies transverse
            # lighting where a previously buried segment turns into view.
            v = np.clip(side_coordinate, -1, 1)
            uv = np.stack(
                (np.full_like(v, 56.0) - v * 11, np.full_like(v, 174.0) - v * 3), axis=2
            ).astype(np.float32)
            rgb = mix(
                rgb,
                sample(limb_paint, uv),
                0.55 * smooth(min(1, s / 0.15, (1 - s) / 0.15)),
            )
        if key in ("paw", "farpaw", "foot", "farfoot"):
            # The far-side paws in the side views are partly covered. Raster
            # selections of those slivers cannot supply a complete exposed limb.
            # Swing an oval around its socket using the clean painted palm
            # or sole, then seat the matching endpoint drawing onto the joint.
            axis = unit(mid[-1] - mid[0])
            is_foot = "foot" in key
            along = np.clip(np.linalg.norm(mid[-1] - mid[0]) * 0.5 + 3, 9, 22)
            across = 10 if is_foot else 7.7
            off = xy - mid[4]
            u, v = off @ axis / along, off @ normal(axis) / across
            oval = (1 - np.sqrt(u * u + v * v)) * across
            use = smooth(min(1, s / 0.16, (1 - s) / 0.16))
            sdf = mix(sdf, oval, use)
            if is_foot:
                center = np.array([101, 157])
                basis = unit(np.array([-20.0, 16.0]))
            else:
                center = np.array([150, 108])
                basis = unit(np.array([28.0, -3.0]))
            uv = (
                center
                + u[:, :, None] * basis * (14 if is_foot else 11)
                + v[:, :, None] * normal(basis) * (7 if is_foot else 6)
            ).astype(np.float32)
            rgb = mix(rgb, sample(limb_paint, uv), use)
        alpha = np.clip(sdf * 0.7 + 0.5, 0, 1)[:, :, None]
        view = out[y + 24 : Y + 24, x + 24 : X + 24]
        if key.startswith("face"):
            view[:, :, :3] += rgb * view[:, :, 3:] / 255
            continue
        view[:, :, :3] = rgb * alpha + view[:, :, :3] * (1 - alpha)
        view[:, :, 3:] = alpha * 255 + view[:, :, 3:] * (1 - alpha)
    out[:, :, :3] *= 255 / np.maximum(1, out[:, :, 3:])
    rgba = np.clip(out, 0, 255).astype("uint8")
    _, cc, stats, _ = cv2.connectedComponentsWithStats(
        (rgba[:, :, 3] > 100).astype("uint8")
    )
    if len(stats) > 2:
        # The rasterizer can leave a one- or two-pixel fringe. A separate
        # painted feature indicates a bad attachment and must fail the pack.
        assert sorted(stats[1:, cv2.CC_STAT_AREA])[-2] <= 4, "detached landing part"
        body = cc == 1 + np.argmax(stats[1:, cv2.CC_STAT_AREA])
        allowed = cv2.dilate(body.astype("uint8"), np.ones((5, 5), np.uint8)).astype(
            bool
        )
        rgba[~allowed] = 0
    return rgba


FRAMES, TILE = 15, 98
TARGETS = {489: "ponder_hover", 507: "attentive_hover", 0: "neutral"}
SOURCES = {309: "left_glide", 417: "right_glide", 327: "rising_bank"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    if args.evidence:
        args.evidence.mkdir(parents=True, exist_ok=True)
    global atlas, limb_paint
    atlas = Image.open(ART / "body.webp").convert("RGBA")
    limb_paint = sprite(327)[:, :, :3].copy()
    rig = {cell: prep(cell) for cell in R}
    sheet = Image.new("RGBA", (FRAMES * TILE, len(SOURCES) * len(TARGETS) * TILE))
    lua = [
        "-- Generated by tools/dragon/landing.py. Source-space rectangles include the 24px padded gutter.",
        "return {",
    ]
    row = 0
    for source, facing in SOURCES.items():
        lua.append(f"    {facing} = {{")
        for target, posture in TARGETS.items():
            lua.append(f"        {posture} = {{row = {row},")
            joints = []
            for frame in range(FRAMES):
                sample = {}
                rgba = render(rig[source], rig[target], frame / (FRAMES - 1), sample)
                joints.append(sample)
                if frame in (0, FRAMES - 1):
                    rgba[:] = 0
                    rgba[24:264, 24:264] = sprite(source if frame == 0 else target)
                image = Image.fromarray(rgba)
                alpha = image.getchannel("A")
                x, y, r, b = alpha.getbbox()
                # Keep filtering away from the next tile. Undo the crop and its
                # non-square storage aspect on the native surface for each frame.
                assert min(x, y, 288 - r, 288 - b) > 1, (
                    source,
                    target,
                    frame,
                    "clipped",
                )
                x, y, r, b = x - 2, y - 2, r + 2, b + 2
                tile = image.crop((x, y, r, b)).resize(
                    (TILE, TILE), Image.Resampling.LANCZOS
                )
                sheet.paste(tile, (frame * TILE, row * TILE))
                lua.append(f"            {{{x - 24}, {y - 24}, {r - x}, {b - y}}},")
            lua.append("        },")
            if args.evidence:
                (args.evidence / f"{source}-{target}.json").write_text(
                    json.dumps(joints, separators=(",", ":"))
                )
            row += 1
        lua.append("    },")
    lua.append("}")
    scratch = ROOT / ".tmp"
    scratch.mkdir(exist_ok=True)
    with TemporaryDirectory(dir=scratch) as directory:
        staging = Path(directory)
        sheet.save(staging / "landing.webp", lossless=True, method=3)
        (staging / "landing-frames.luau").write_text("\n".join(lua) + "\n")
        (staging / "landing.webp").replace(ART / "landing.webp")
        (staging / "landing-frames.luau").replace(ART.parent / "landing-frames.luau")


if __name__ == "__main__":
    main()
