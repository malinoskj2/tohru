#!/usr/bin/env python3
"""Shuffle gate review packages into unlabelled clips and have a fresh reviewer, who made none of them, judge each.

The reviewer gets only `review/` (real-size playback, every frame at real size in order, a slowed head
close-up and time slices), a one-line brief of what each clip should show, the approved character art and the
owner's rejection checklist: no labels, no metrics, no verdicts. It runs as a
new headless Claude session with read-only tools, so nothing the author knows reaches it.
"""
import argparse
import datetime
import json
from pathlib import Path
import random
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]

PROMPT = """You are reviewing animations of a small pastel dragon mascot for faults. You did not make them, and your job is to
find what is wrong, not to approve them. Assume every clip has faults until the evidence shows otherwise. Some clips
here are known to be bad and some may be fine; the order is random and nothing about a clip's name means anything.

Every folder `clip-*` in this directory is one clip, shown at its real desktop size (about 166 px) at 24 fps. Its
`brief.txt` says what the clip is meant to show, such as a head turn or a flight loop; judge it against that brief,
and do not fault a clip for not doing something its brief does not ask for. Holding a drawing for two or three frames
(animating on twos or threes) is a deliberate style, not a fault by itself. If `reference-art.png` is here, it is
the approved look of the character: a clip that does not match it is off-model. In each clip folder:
- `normal-speed.mp4` and `double-size.mp4`: the clip at real speed. Watch motion by extracting every frame in order,
  e.g. `ffmpeg -v error -i clip-A/double-size.mp4 {scratch}/A-%03d.png`, then view runs of consecutive frames
  side by side or one after another. Snaps, pops, swells, winks and cross-fades exist only between consecutive frames.
- `every-frame-real-size.png`: every frame in order at real size, numbered, read left to right like a flipbook.
- `head-quarter-speed.mp4`: the head close-up at quarter speed (extract frames the same way).
- `time-slice-eye-row.png`, `time-slice-head-column.png`: one pixel row through the eyes and one column through the
  head, stacked over time from top to bottom. A smooth turn draws smooth curves; a kink is a snap, a bulge is a swell,
  a doubled line is a cross-fade.

The owner's checklist of faults they rejected before is `rejections.md`. In particular they rejected: the head or
skull bulging or swelling and shrinking back while turning; horns or the crest doubling, splitting, vanishing or being
redrawn in a single frame; an eye shrinking, winking or vanishing; colours shifting; two drawings cross-faded so a
ghost shows; one drawing swapped for another instead of rotating; jerky or stalling timing; becoming a different
creature partway.

For each clip: look at every frame in order, then decide. Do not edit or create files outside {scratch}.
End your reply with one JSON object on its own line, exactly this shape, covering every clip:
{"clips": {"clip-A": {"verdict": "reject" | "needs-owner" | "no-faults", "faults": [{"id": "B4", "frames": "12-18",
"what": "...", "severity": "blocks" | "noticeable" | "minor"}], "could_not_judge": ["..."]}}}
"reject" means at least one noticeable or blocking fault.
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("packages", nargs="+", help="LABEL=GATE_OUT_DIR, each holding review/")
    ap.add_argument("--intent", action="append", default=[], help="LABEL=what the clip is meant to show")
    ap.add_argument("--default-intent", default="a head or body turn")
    ap.add_argument("--reference", default=None, help="approved character art shown as reference-art.png")
    ap.add_argument("--out", default=None)
    ap.add_argument("--seed", type=int, default=None)
    ap.add_argument("--model", default="claude-opus-5-5")
    ap.add_argument("--dry-run", action="store_true", help="build the blind folder and prompt only")
    args = ap.parse_args()
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    out = Path(args.out) if args.out else ROOT / ".tmp/verify/blind" / stamp
    blind = out / "reviewer"
    blind.mkdir(parents=True, exist_ok=True)
    items = [p.split("=", 1) for p in args.packages]
    rng = random.Random(args.seed if args.seed is not None else stamp)
    rng.shuffle(items)
    intents = dict(i.split("=", 1) for i in args.intent)
    key = {}
    for i, (label, path) in enumerate(items):
        name = f"clip-{chr(65 + i)}"
        key[name] = label
        shutil.copytree(Path(path) / "review", blind / name)
        (blind / name / "brief.txt").write_text(intents.get(label, args.default_intent) + "\n")
    shutil.copy(ROOT / "verification/motion/rejections.md", blind / "rejections.md")
    if args.reference:
        shutil.copy(args.reference, blind / "reference-art.png")
    # A fresh scratch folder per run: frames left by an earlier run would read as extra frames of this one.
    scratch = Path("/tmp") / f"blind-review-{stamp}-{rng.randrange(1 << 30):x}"
    if scratch.exists():
        shutil.rmtree(scratch)
    scratch.mkdir(parents=True)
    prompt = PROMPT.replace("{scratch}", str(scratch))
    (blind / "PROMPT.md").write_text(prompt)
    (out / "key.json").write_text(json.dumps(key, indent=1))
    if args.dry_run:
        print(json.dumps({"blind": str(blind), "key": str(out / "key.json")}))
        return
    proc = subprocess.run(["claude", "-p", prompt, "--model", args.model, "--output-format", "text",
                           "--allowedTools", "Read,Glob,Bash(ffmpeg:*),Bash(ls:*),Bash(mkdir:*)",
                           "--disallowedTools", "Edit,Write,Agent,WebFetch,WebSearch"],
                          cwd=blind, capture_output=True, text=True, timeout=3600)
    shutil.rmtree(scratch, ignore_errors=True)
    (out / "review.md").write_text(proc.stdout + ("\n\nSTDERR:\n" + proc.stderr if proc.stderr.strip() else ""))
    found = None
    for line in reversed(proc.stdout.strip().splitlines()):
        line = line.strip().strip("`")
        if line.startswith("{") and '"clips"' in line:
            try:
                found = json.loads(line)
                break
            except ValueError:
                continue
    if found is None:
        m = re.search(r"\{\s*\"clips\".*\}", proc.stdout, re.S)
        found = json.loads(m.group(0)) if m else {"clips": {}}
    verdicts = {key[c]: v for c, v in found.get("clips", {}).items() if c in key}
    (out / "verdicts.json").write_text(json.dumps(verdicts, indent=1))
    print(json.dumps({label: v.get("verdict") for label, v in verdicts.items()}))


if __name__ == "__main__":
    main()
