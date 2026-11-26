# Motion gate calibration

`tools/verify/gate.py --deep` (turn, motion, flow, SAM 2.1 parts, OpenCLIP identity against Cell 165) over
`tools/verify/gate-corpus.json`. Rerun with `gpu-run --vram 6GiB -- python3 tools/verify/gate-calibrate.py --deep`
(OpenCLIP needs `open_clip_torch`; the CPU stages need only the motion.py dependencies).

| Clip | Expected | Gate | Score | Checks that fired | Why it is in the corpus |
| --- | --- | --- | ---: | --- | --- |
| b5545db | fail | fail | 16.74 | eye-scale, feature-slide, ghost, head-scale, horn-count, outline-pop, scale-pulse, volume | owner-rejected: head bulges on the left as she turns (B4), wink (F1) |
| v17 | fail | fail | 16.94 | eye-scale, feature-slide, ghost, head-scale, horn-count, outline-pop, scale-pulse, volume | owner-rejected H3 trial: bobblehead swell, cheek bulge |
| onepiece | fail | fail | 14.79 | feature-slide, ghost, head-scale, outline-pop, part-snap, uneven-motion, volume | owner-rejected item 17 repair: paint seam on the left temple (B5) |
| outline-final | fail | fail | 15.82 | feature-slide, ghost, head-scale, horn-count, outline-pop, part-snap, uneven-motion, volume | owner-rejected item 17 repair: skull bulge remains (B4) |
| outline-crossing | fail | fail | 14.88 | feature-slide, ghost, head-scale, outline-pop, part-snap, uneven-motion, volume | owner-rejected item 17 repair: notched horn base |
| paws-a2f76c0 | fail | fail | 15.79 | feature-slide, ghost, head-scale, horn-count, outline-pop, part-snap, uneven-motion, volume | owner-rejected: forepaw ghost, head redraw pop |
| cadence | fail | fail | 14.14 | feature-slide, ghost, head-scale, horn-count, outline-pop, uneven-motion, volume | owner-rejected: forepaws shrink and regrow, head pop |
| frill-v2 | fail | fail | 32.18 | eye-scale, feature-slide, ghost, head-scale, horn-count, outline-pop, part-snap, shape-change, size-pop, swap, uneven-motion, volume | owner-rejected: frill and horn redraws, eye change |
| directed | fail | fail | 32.07 | eye-scale, feature-slide, ghost, head-scale, horn-count, outline-pop, part-snap, shape-change, size-pop, swap, uneven-motion, volume | owner-rejected: horn and frill redraws, eye shrink |
| eyefix-v4 | fail | fail | 28.37 | eye-scale, feature-slide, head-scale, motion:pose-snap, outline-pop, shape-change, size-pop, swap, uneven-motion, volume | owner-rejected: transplanted eye, cheek bulge, hold-then-jump |
| drawn | fail | fail | 22.01 | cut-edge, eye-drop, ghost, horn-count, motion:transparent-holes, outline-pop, part-snap, scale-pulse, shape-change, size-pop, volume | blind-rejected (blind-review clip C): single crest becomes two horns in one frame, different creature |
| vol-drawn | fail | fail | 19.78 | cut-edge, eye-scale, ghost, motion:transparent-holes, outline-pop, part-snap, scale-pulse, shape-change, size-pop, volume | blind-rejected (blind2 clip C): doubled horns from the drawn keys |
| gmfss | fail | fail | 20.32 | feature-slide, ghost, head-scale, horn-count, motion:transparent-holes, outline-pop, part-snap | blind-rejected (blind2 clip D): VFI morph between keys |
| flight-6-40 | pass | pass | 0 | none | owner-accepted flight stretch before the turn |
| flight-84-120 | pass | pass | 0 | none | owner-accepted flight stretch after the turn |
| 3d-ones | pass | pass | 0 | none | rendered 3D head turn on ones: rigid geometry, best motion of the blind set |
| 3d-on2 | pass | pass | 0 | none | same 3D turn held on twos |
| 3d-on3 | pass | pass | 0 | none | same 3D turn held on threes |
| vol-b55 | info | fail | 13.13 | feature-slide, ghost, head-scale, horn-count, outline-pop, part-snap | blind2 clip B, acceptable with minor faults; owner verdict unknown (tweened turns 'still look off') |

Rejected clips failed: 13/13. Accepted clips passed: 5/5.

Per fault the owner named:

- Skull bulge / bobblehead swell (b5545db, v17, outline-final): `head-scale`, `volume`, `scale-pulse` (flow), plus `part-snap` on the crest.
- Doubled or redrawn horns (drawn, vol-drawn, frill-v2, directed): `horn-count`, `shape-change`, SAM `part-snap` (the crest keeps under 10% of its shape between two frames), `cut-edge`.
- Eye shrink, wink or vanish (eyefix-v4, directed, frill-v2, drawn): `eye-scale`, `eye-drop`; the gate's own `wink` covers two- and three-frame winks that turn.py's one-frame check missed.

Calibration notes: part sizes are compared by height, which a turn about the vertical axis keeps, not by area, which foreshortens;
the crest is prompted on every tip at both end views so a correct two-horn turn keeps both horns in the track; held drawings are
excluded from the usual step so art on twos and threes is not flagged. motion.py's size-trend and outline-pulse flags are
advisory: uncalibrated they score an accepted flight (3.1) above a rejected turn (2.8). DINOv2 is unresolved in the model store,
so identity uses OpenCLIP only. Its mid-turn dip below the end views does not separate this corpus (rejected 0.031-0.056, an accepted
flight 0.035), because every drawn clip shares one base drawing; its absolute similarity to Cell 165 does separate off-model art
(the rendered 3D control scores 0.62 against 0.91 for the drawn clips). It is reported, and becomes a stronger check against the locked
5-view sheet's nearest view.
The approved desktop landings are captures without alpha, so only motion.py covers them.

## Blind reviewer

`tools/verify/blind.py` shuffles the gate's `review/` packages into unlabelled clips and has a fresh headless Claude (Opus,
read-only tools) judge them against `rejections.md`, a one-line brief per clip and the approved art. Two independent runs
(seeds 21 and 22) over eight packages:

| Clip | Expected | Run 21 | Run 22 | Faults it named |
|---|---|---|---|---|
| b5545db | fail | reject | reject | head swells about 1.9x in the front view (B4); horn vanishes or ghosts |
| cadence | fail | reject | reject | eyes shrink to dots (F1); crest cross-fade ghost; horn vanishes in two frames |
| drawn | fail | reject | reject | crest splits into two horns (B2); drawing swaps; head swells |
| eyefix-v4 | fail | reject | reject | profile snaps to front in one frame (T1); eyes shrink and one vanishes (F1) |
| 3d-ones (timing control) | pass | no-faults | no-faults | |
| vol-b55 (no owner verdict) | info | needs-owner | reject | horn shrinks and vanishes within two frames |
| flight-6-40 | pass | reject | reject | pop at the loop wrap (34 to 0); one wing drop |
| flight-84-120 | pass | reject | reject | wings barely flap; hitch at the loop wrap |

It rejects every owner-rejected clip in both runs, naming the owner's own faults, and passes the motion control. It rejects the
two accepted flight excerpts: their wrap is where the calibration excerpt was cut from the longer clip, not a loop seam the owner
saw, and a small flap is a taste call outside the checklist. So its rejections need the gate's numbers beside them, and an
excerpt should be given whole loops or a brief saying it does not loop. Earlier runs shared one `/tmp` scratch folder, so frames
left by one run read as extra frames of the next and produced invented snaps; each run now gets its own folder. Single runs
also vary (cadence was needs-owner in one contaminated run), so the reviewer runs at least twice and the stricter verdict
stands.
