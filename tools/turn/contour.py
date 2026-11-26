"""Add a clean, non-bloating soft-lilac pastel outline inside the rendered alpha silhouette (matches Sheet B)."""
import sys
from pathlib import Path
import cv2
import numpy as np
from PIL import Image

INK = np.array([156.0, 126.0, 184.0], dtype=np.float32)  # soft lilac matching Sheet B

def add_contour(rgba, width=2.4, strength=0.72):
    a = rgba[..., 3].astype(np.float32) / 255.0
    mask = (a > 0.5).astype(np.uint8)
    dist = cv2.distanceTransform(mask, cv2.DIST_L2, 5)
    # Soft inner contour band within `width` pixels of silhouette edge (never inflates silhouette!)
    band = np.clip(1.0 - (dist - 0.5) / max(0.5, width), 0.0, 1.0) * (a > 0.05)
    w = (band * strength)[..., None]
    rgb = rgba[..., :3].astype(np.float32)
    out_rgb = rgb * (1.0 - w) + INK * w
    return np.dstack([np.clip(out_rgb, 0, 255).astype(np.uint8), rgba[..., 3]])

if __name__ == "__main__":
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    width = float(sys.argv[3]) if len(sys.argv) > 3 else 2.4
    strength = float(sys.argv[4]) if len(sys.argv) > 4 else 0.72
    dst.mkdir(parents=True, exist_ok=True)
    for p in sorted(src.glob("*.png")):
        im = np.asarray(Image.open(p).convert("RGBA"))
        Image.fromarray(add_contour(im, width=width, strength=strength)).save(dst / p.name)
