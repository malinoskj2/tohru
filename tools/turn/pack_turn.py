#!/usr/bin/env python3
"""Pack the verified B1 dragon head-turn sequences (90->0 and 0->90) into a production 240x240 RGBA WebP sheet.

Writes the sheet to shell/assistant/dragon/ and its manifest to tools/turn/art/.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
CELL = 240
COLS = 12
LOGICAL_CELL = 150
DISPLAY_CELL = 166
FPS = 24


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_frames(folder: Path):
    files = sorted(folder.glob("*.png"))
    if not files:
        raise SystemExit(f"No PNG frames found in {folder}")
    return [Image.open(p).convert("RGBA").resize((CELL, CELL), Image.Resampling.LANCZOS) for p in files]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--fwd", type=Path, required=True, help="90->0 (profile-right to front) RGBA frame directory")
    ap.add_argument("--rev", type=Path, required=True, help="0->90 (front to profile-right) RGBA frame directory")
    ap.add_argument("--out-dir", type=Path, default=ROOT / "shell/assistant/dragon")
    ap.add_argument("--manifest-dir", type=Path, default=ROOT / "tools/turn/art")
    ap.add_argument("--name", default="turn-sheet-b")
    ap.add_argument("--quality", type=int, default=90)
    args = ap.parse_args()

    fwd = load_frames(args.fwd)
    rev = load_frames(args.rev)
    frames = fwd + rev
    rows = (len(frames) + COLS - 1) // COLS

    atlas = Image.new("RGBA", (COLS * CELL, rows * CELL), (0, 0, 0, 0))
    for i, im in enumerate(frames):
        atlas.paste(im, ((i % COLS) * CELL, (i // COLS) * CELL))

    args.out_dir.mkdir(parents=True, exist_ok=True)
    sheet_path = args.out_dir / f"{args.name}.webp"
    atlas.save(sheet_path, quality=args.quality, method=6)

    manifest = {
        "name": args.name,
        "activated": True,
        "sheet": str(sheet_path.relative_to(ROOT)),
        "sheet_sha256": sha256(sheet_path),
        "bytes": sheet_path.stat().st_size,
        "width": atlas.width,
        "height": atlas.height,
        "cols": COLS,
        "rows": rows,
        "cell": CELL,
        "logical_cell": LOGICAL_CELL,
        "display_cell": DISPLAY_CELL,
        "fps": FPS,
        "frame_count": len(frames),
        "rgba_bytes": atlas.width * atlas.height * 4,
        "sequences": {
            "profile_right_to_front": {"start": 0, "count": len(fwd), "yaw0": 90, "yaw1": 0},
            "front_to_profile_right": {"start": len(fwd), "count": len(rev), "yaw0": 0, "yaw1": 90},
        },
        "generator_sha256": {
            "tools/turn/build_dragon.py": sha256(ROOT / "tools/turn/build_dragon.py"),
            "tools/turn/contour.py": sha256(ROOT / "tools/turn/contour.py"),
        },
    }
    args.manifest_dir.mkdir(parents=True, exist_ok=True)
    meta_path = args.manifest_dir / f"{args.name}.json"
    meta_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
