# B1 vs B2 head-turn shootout

One turn, profile right (yaw 90) to front (yaw 0), hold 6 + turn 14 + hold 6 frames at 24 fps, from model sheet B
(not yet locked by the owner). Both use one proxy: a visual hull carved from the sheet's front, 3/4 right, profile and
3/4 back silhouettes (`tools/turn/make_hull.py`; it reprojects onto those views at IoU 0.97 or better; the back view is
left out because its wings and horns disagree with the front). Run with `tools/turn/shootout.sh`.

- B1 (`b1_paint.py`): the 3D proxy rendered directly, its paint projected from the sheet views and fixed to the surface
  (each point takes the views that saw it, weighted by facing), so the texture turns rigidly with the mesh.
- B2 (`b2_stylize.py`): EbSynth maps the sheet keys at 0, 45 and 90 onto each frame, guided by the proxy's object
  position (weight 20), camera normals and a yaw guide; the two bracketing keys share one style image, so every patch
  comes from one drawing.

| | Gate (`gate.py --deep`) | Identity to sheet | Blind reviewer (2 runs) |
|---|---|---|---|
| B1 | fail, score 5.7: outline-pop f11-13, crest part-snap f15, tail hole blinks f12 | 0.79-0.87 | reject, reject: off-model painted marks, doubled tail, far-eye wink |
| B2 | fail, score 11.2: swap f11/f15, feature-slide, horn-count, outline-pop, tail hole f12 | 0.93-0.99 | reject, reject: cross-fade between keys, tail fades side to side, squint |

The gate and both reviewers agree that neither passes. B1 moves like a solid object but its paint is off-model: the
sheet views disagree with each other and with the hull in depth, so features drawn for one view land on surfaces another
view owns. B2 looks like the sheet frame by frame, but each frame is synthesised independently and the hand-off between
keys reads as a cross-fade. The outline pop and tail-hole blink are in both, so they come from the hull. Both reviewers
also called the timing stalled then rushed (six-frame hold, then the turn over about six frames of the ease).

## Round 2 (locked Sheet B, eased timing, real texture/rig + temporally-guided B2)

- **B1 (`sculpt_v14`, `tools/turn/build_dragon.py` + `tools/turn/contour.py`)**: sculpted Sheet-B-proportioned glossy pearlescent 3D rig (big head on squat pear body, swept S-wave flame crest, 3 axolotl cheek frills, scalloped bat wings visible from profile, 100% rigid 3D curled spiral tail, larger plum eyes + curved smile, crisp plum-lavender inner ink contour), hold 4 + eased turn 36 (`--ease-blend 0.60`) + hold 10 frames at 24 fps.
- **B2 (`b2_v5`, `tools/turn/b2_stylize.py`)**: EbSynth with exact 3D-advected temporal coherence guidance from the previous synthesized output frame (`--temporal-weight 14.0`, `--direction backward`, 4 aligned keys + appearance guide on `mv4_flrb` proxy, hold 4 + eased turn 28 + hold 7 frames at 24 fps). Eliminates all cross-fade ghosting, drawing swaps, feature slide, color drift and eye winks; remaining faults come from the underlying Hunyuan3D-2mv proxy silhouette. (A follow-up `b2_v6` run directly on the rigid `sculpt_v14` proxy matched Sheet B's front/3-4 look closely on frames 24–49, but lost the profile eye on frames 0–17 due to profile-key eye offset, failing CPU gate at score 82.43.)

| | Gate (`gate.py`) | Identity to sheet | Blind reviewer |
|---|---|---|---|
| B1 (`sculpt_v14`) | CPU (`turn`/`motion`/`flow`): **PASS, score 0.00** (0 findings). `--deep` (SAM 2.1 + OpenCLIP) pending GPU release (preceding `sculpt_v11`: `parts=0`, `identity=0`, 0.821–0.886) | 0.821–0.886 (`sculpt_v11`; `sculpt_v14` pending `--deep`) | Pending on `sculpt_v14` (`sculpt_v12` was rejected 1/1 for snowman neck, soft clay look, cat-ear profile crest, flat wings, vanishing tail — all addressed in `sculpt_v14`) |
| B2 (`b2_v5`) | `--deep`: **FAIL, score 6.86** (`motion=0`, `flow=0`, `identity=0`, `ghost=0`, `swap=0`, `feature-slide=0`, `wink=0`; only `outline-pop` 3.8 px f17–21, `horn-count` f7–12, `part-snap` f36 from `mv4_flrb` proxy) | 0.744–0.951 | Pending on `b2_v5` |

## Round 3 (B1-only production turns, B2 dropped)

Per decision, B2 was dropped and B1 was refined on `tools/turn/build_dragon.py` against Tohru's profile-head feedback and 2-seed blind reviews:

- **`sculpt_v14` blind (seeds 71, 72)**: 2/2 reject — clean rotation/slices, flagged abrupt linear ease ends (`M-timing`), far-eye bump past snout (`F1`), plain-disc tail with no spiral, flat profile head / snowman waist, and small front horn.
- **`sculpt_v16`**: added pure sine zero-jerk ease-in-out (`0.5*(1 - cos(pi*u))`), flush-embedded eyes, deeper rounded profile head, painted rigid tail spiral, and symmetric curved horns. **`gate.py --deep` (`90->0`): PASS, score 0.00** (`turn=0, motion=0, flow=0, parts=0, identity=0`, OpenCLIP **0.840–0.893**); reverse `0->90` CPU gate **PASS, score 0.00**. Blind (seeds 81, 82): 2/2 clean on motion/timing/eyes, rejected on likeness (mismatched limb colors, waist ridge, flat sideways horns, small tail, cat-ear profile crest).
- **`sculpt_v20`**: single smooth pear belly (no waist ridge), separate sculpted cheek frills, matching paws/feet, bigger curled tail, swept flame crest. `gate.py --deep`: FAIL 1.24 (`parts=0, flow=0, motion=0, identity=0` at 0.848–0.890; 1 `outline-pop` f26–27). Blind (seeds 91, 92): 2/2 clean on motion, flagged belly wider than head, right wing occluded by tail in front, and low horns.
- **`sculpt_v23` (final production renders, both directions `90->0` and `0->90`, 56f = 4+44+8 at 24 fps, 240×240 px packed cells + 166 px display)**: broad head wider than belly, wide-set large eyes + curved smile, matching symmetric paws/feet, 3 separate sculpted cheek frills, curved purple horns, BOTH arched bat wings visible symmetrically above the curled spiral tail in front and profile.
  - **`gate.py --deep` (`90->0`)**: **FAIL, score 2.22** (`flow=0`, **`parts=0`**, **`identity=0`** at OpenCLIP **0.855–0.901** — highest Sheet-B likeness). Remaining 2 findings: 1-frame `motion:transparent-holes` blink at f15 (19 px armpit gap under `wing_l`) and 1-frame `outline-pop` at f30 (`1.71 px` vs `1.40 px` limit).
  - **CPU `gate.py` (`0->90`)**: **FAIL, score 2.22** (exact symmetric mirror: f40 `transparent-holes`, f25 `outline-pop`).
  - **Blind review on `sculpt_v23`**: not yet run (stopped iterating per instruction to publish now).
