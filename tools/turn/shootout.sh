#!/bin/sh
# usage: tools/turn/shootout.sh SHEET_DIR WORK_DIR   (SHEET_DIR holds front.png ... back.png RGBA views)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
sheet=$1 work=$2
ebsynth=${EBSYNTH:-ebsynth}
views="0=front 45=three-quarter-right 90=profile-right"
mkdir -p "$work/pass"
python3 "$here/make_hull.py" "$sheet" "$work/hull" back
for m in position normal onormal alpha; do
  blender -b -P "$here/render_turn.py" -- "$work/hull/hull.glb" "$work/pass/$m" $m --keep-origin --depth16 --size 480 >/dev/null
done
for y in 0 45 90; do for m in position normal; do
  blender -b -P "$here/render_turn.py" -- "$work/hull/hull.glb" "$work/pass/key-$y-$m.png" $m --keep-origin --depth16 --at $y --size 480 >/dev/null
done; done
(cd "$here" && python3 b1_paint.py "$work/pass" "$sheet" "$work/hull/projection.json" "$work/b1" --views $views)
(cd "$here" && python3 b2_stylize.py "$work/pass" "$sheet" "$work/hull/projection.json" "$work/b2" --views $views --ebsynth "$ebsynth")
