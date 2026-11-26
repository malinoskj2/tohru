#!/usr/bin/env python3
"""Run the gate over the calibration corpus and check every expectation: rejected clips fail, accepted pass."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--corpus", default=str(HERE / "gate-corpus.json"))
    ap.add_argument("--out", default=str(ROOT / ".tmp/verify/gate-calibration"))
    ap.add_argument("--deep", action="store_true")
    ap.add_argument("--media", action="store_true", help="also write each clip's review package")
    ap.add_argument("--only", nargs="*")
    args = ap.parse_args()
    corpus = json.loads(Path(args.corpus).read_text())
    root = ROOT / corpus["root"]
    out = Path(args.out)
    rows, wrong = [], []
    for clip in corpus["clips"]:
        if args.only and clip["label"] not in args.only:
            continue
        src = root / clip["dir"]
        if not src.exists():
            rows.append({**clip, "verdict": "missing"})
            continue
        cmd = [sys.executable, str(HERE / "gate.py"), str(src), f"--range={clip['range']}", "--label", clip["label"],
               "--out", str(out / clip["label"])]
        cmd += ["--deep"] if args.deep else []
        cmd += [] if args.media else ["--no-media"]
        subprocess.run(cmd, capture_output=True, text=True)
        try:
            g = json.loads((out / clip["label"] / "gate.json").read_text())
        except OSError:
            rows.append({**clip, "verdict": "error"})
            wrong.append(clip["label"])
            continue
        checks = sorted({f["check"] for f in g["findings"]})
        row = {**clip, "verdict": g["verdict"], "score": g["score"], "checks": checks}
        rows.append(row)
        if clip["expect"] in ("pass", "fail") and g["verdict"] != clip["expect"]:
            wrong.append(clip["label"])
        print(f"{clip['label']:18} expect {clip['expect']:4}  got {g['verdict']:4}  score {g['score']:6}  {' '.join(checks)}", flush=True)
    (out / "calibration.json").write_text(json.dumps({"rows": rows, "wrong": wrong, "deep": args.deep}, indent=1))
    print(json.dumps({"wrong": wrong}))
    raise SystemExit(1 if wrong else 0)


if __name__ == "__main__":
    main()
