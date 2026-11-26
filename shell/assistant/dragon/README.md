# Voice dragon sheets

The illustrated voice dragon now has **20 new full-body airborne keys**, each with a registered 17-frame outward transition and its reverse. Her new positions include loose suspended legs, nested knees, crossed feet, asymmetric curls, sideways reclines, belly-facing hover, forward/back leans and left/right three-quarter or near-profile banks. They are redrawn body alignments, not mirrored copies or whole-sprite rotations. All remain airborne at the bottom right; no new pose assumes a floor or perch.

`../puppet.luau` selects the frames and owns timing. `../dragon.plm` renders them. `../full-body.luau` is generated from the retained sources in `tools/dragon/`. The 150 px logical cell and 210 × 230 overlay stay the same, including the click-to-open zone. All drawings and face patches fit that cell; the separate shadow adjusts gently for the more extended floating poses.

## Runtime sheets and memory

| File | Grid / source size | Bytes | RGBA budget at 1× / 2× logical scale |
|---|---|---:|---:|
| `body.webp` | 24 × 24 cells, 5760 × 5760 | 7,365,870 | 49.44 / 197.75 MiB |
| `faces.webp` | 8 × 60, 800 × 4080 | 551,644 | 3.11 / 12.45 MiB |
| `travel.webp` | 16 × 170, 768 × 6120 | 1,540,598 | 4.48 / 17.93 MiB |
| `eyes.webp` | 4 × 1, 400 × 52 | 4,876 | 0.020 / 0.079 MiB |
| `mouths.webp` | 8 × 1, 352 × 40 | 3,700 | 0.013 / 0.054 MiB |
| `heart.png` | 48 × 48 | 2,221 | 0.002 / 0.009 MiB |
| `turn-sheet-b.webp` | 12 × 10 cells, 2880 × 2400 | 859,710 | 12.61 / 50.45 MiB, only while bound (`../sheet-turn.luau`) |

Total runtime art: **9,468,909 bytes (9.03 MiB)**, up from 3.56 MiB. Direct decoding of the source files to RGBA8 requires 157.09 MiB, up from 71.55 MiB. Budgets computed from the scene's declared image sizes are **57.07 MiB at 1×** and **228.28 MiB at 2×**, versus 17.89 / 71.55 MiB previously. These pixel budgets exclude decoder temporaries, mipmaps, fonts, driver allocations and voice models; they are not measured VRAM deltas. `tools/dragon/art/packing.json` records the exact dimensions and costs. The square atlas fits a 16384 px texture through 4× scale.

The fixed sheets preload with voice enablement, including a three-second hidden warm-up. Every state uses the same body texture and face textures, so pose changes do not load images. Turning voice off clears every source and the warm-up; generation ownership prevents a stale service callback ending a newer preload. Rendering runs only while visible. Each tick updates cached facts and cell indices; no subprocess or model runs per frame. The interpolator is an offline development tool.

Legacy body indices 0–201 keep their meaning: idle 0–47, flutter 48–95, moves 96–164, keys/mouthless conversational keys 165–201. New reversible moves and keys occupy 202–561; 562–575 are padding. Cells are packed at 240 px and displayed at 150 logical px. Nominal animation cadence is 24 fps (42 ms timer).

## Behavior

Listening enters `attentive_hover` with feet swept back and a downward tail coil. Thinking enters `ponder_hover`, with a cheek paw, loosely tucked knees and hanging tail. Both can change to other quiet body positions during longer waits. Neutral gets an initial quiet interval of about 10 seconds, then holds new positions for roughly 12–25 seconds. Listening/thinking variations hold roughly 9–18 seconds. The last three idle selections are excluded, and occasional larger drifts return through the registered neutral hub. Conversational gestures wait for a voice dip and a minimum hold. A normal speech end settles directly into a quiet airborne rest; interruption preserves listening/thinking instead of implying success. Positive emotion can select the airborne `bright_hover`; punctuation alone does not celebrate an error.

`assistant.reduced_motion = true` is a persisted Boolean, also exposed as **Reduce dragon motion** under Settings → Appearance. It suppresses posture travel, bobbing, squash, shimmer and hearts while retaining mouth animation and blinking. It applies live without restarting the voice service.

Every resting view uses its own eyes and skin for eight mouth shapes and three blink states. Changed features are feathered; the closed unblinking rest leaves the illustration untouched. New moving frames also have their own small, registered mouth patches, including reversed transitions, so speech continues through body turns. Blinks resume on the destination view; front-facing eye patches are never forced onto a turned face. The older facial anchors below still apply to legacy keys.

## Delivered full-body keys

| Key | Lower-body / perspective difference |
|---|---|
| `loose_hover` | Loose suspended legs and uncoiled S tail |
| `nested_hover` | Knees nested high, lower body enclosed by tail |
| `reclined_hover` | Reclined pelvis and two legs extended sideways |
| `right_bank` | Right three-quarter bank, knees behind, tail to left |
| `high_knee` | One knee raised, other leg dangling, offset tail |
| `left_glide` | Near horizontal left float, legs trailing, open tail |
| `rising_bank` | Ascending right bank, feet swept back |
| `belly_hover` | Belly faces viewer, broad balancing wings, feet behind |
| `long_hover` | Long vertical torso, feet trailing, long low tail |
| `crossed_feet` | Crossed floating feet, loose side coil |
| `hip_twist` | Pelvis faces right beneath left-turning shoulders |
| `right_glide` | Near profile right, near wing forward, tail stretched left |
| `forward_bank` | Head and chest forward, hips and tail higher behind |
| `lean_back` | Chest reclines while hips and feet advance |
| `side_float` | Sideways recline, both legs forward, tail counterbalances |
| `ponder_hover` | Paw at cheek, tucked asymmetrical legs, hanging tail |
| `attentive_hover` | Forward listening bank, feet drawn back, downward coil |
| `asymmetric_curl` | Right curl, one knee tucked, one foot suspended |
| `bright_hover` | Airborne stretch, raised wings and unequal legs |
| `open_hover` | Right three-quarter conversation, open paw, tail tucked left |

## Anchors

These are in logical pixels within the 150 px cell.

- **Eye patch:** drawn at (28, 33), size 50 × 26. On the idle loop it follows the breath: dx = 0.39 sin(2π(i − 4)/48), dy = −1.47 sin(2πi/48) for idle cell i. Blinks also play on the talking poses that keep neutral's dot eyes, with a per-pose offset:
  - neutral, right-gesture, shrug, wing-lift, count-one, open-palms, chest-paw (0, 0)
  - lean-in (−10.1, 5.7)
  - soft-nod (0.25, 7.8)
  - head-tilt, recall-glance and the earlier expressive keys omit the eye patch because they rotate or reshape the eyes.
- **Mouth patch:** drawn at (43.5, 47), size 22 × 20, plus a per-pose offset:
  - neutral, right-gesture, shrug, wing-lift, count-one, open-palms, chest-paw (0, 0)
  - talk-explain (0.59, 3.16)
  - happy (−0.23, 2.11)
  - scroll-talk (−0.23, 1.76)
  - wing-cheer (−0.12, 3.87)
  - head-tilt (−0.3, −2.5)
  - lean-in (−10.35, 6.1)
  - soft-nod (0.5, 7.9)
  - recall-glance (−1.2, 0.9)

  The jaw drops by 0.8 × the speech level.
- **Hearts:** both start when the cheer move reaches frame 9 and live 1.4 s.
  - First heart: centre (65.2, 65.8), scale 0.65.
  - Second heart: starts 0.2 s later, at centre (55.3, 75.2), scale 0.45.
  - Motion: back-out pop over 0.28 s, a 16 px rise, a gentle sway, and a fade from 0.9 s.

## Legacy provenance

- **Keys:** `codex-image` edits of the Tohru mascot, kept on its canvas and registered to neutral on the lower body. This covers the 12 poses, plus these halfway keys: mid-explain, mid-laugh, mid-cheer, the 7 visemes, glance-left and glance-up.
- **Conversational talking poses:** the ten calm talking keys (`head-tilt`, `lean-in`, `right-gesture`, `shrug`, `wing-lift`, `count-one`, `soft-nod`, `recall-glance`, `open-palms`, `chest-paw`) were generated from the official sheets (`neutral` cell and `shell/brand/mascot.png`) with `gemini-3.1-flash-image`, matted to transparent RGBA and downsampled to 300 × 300 px with the lower body and tail registered to neutral.
  - `recall-glance` (pose 08) was assembled from the official `neutral` cell, the `eyes.webp` glance-left patch at its anchor `(56, 66)`, a soft-masked −5.5° neck-pivot head tilt and `count-one`'s lowered right paw.
  - Their mouthless cells paint out the closed smile with Telea inpainting, matching the earlier talking keys.
- **Idle and flutter:** a procedural puppet of the neutral key. Wings, tail and head rotate on soft masks around their pivots, with a 1.4 % breath.
- **Explain, cheer and laugh moves:** a layered rig. Limb layers are cut from the keys and turned on their pivots with an overshoot ease through the halfway key; the body is swapped mid-move.
  - The laugh uses a variant key whose left wing comes from mid-laugh, so the paw is the only thing that moves.
- **Thinking move:** ToonCrafter, interpolating through mid-thinking. 16 frames are kept, with a half blend to each key at the ends and the colour locked to the keys.
- **Eye and mouth patches:** cut from the glance, blink and viseme keys against neutral, with feathered masks.
- **Heart:** drawn procedurally in the palette's peach, pink and lavender.

The prior generators were archived during worktree cleanup; see the archive index at `/home/jesse/projects/tohru/.tmp/asset-archive.md` and its external manifests. The new workflow, source drawings, registrations and packing tools are retained in `tools/dragon/`; the old body sheet is retained there as a reproducible input.

## New art provenance and reproduction

The key sheet and four drawn intermediate sheets came from built-in `image_gen.imagegen`, referenced to the shipped neutral and the exact character sheet. The original PNGs and prompts are retained under `tools/dragon/art/`. Intermediate selections are curated in `art/inbetweens/registration.json`; a duplicated-head cell is explicitly excluded. The selected drawings use face registration and RIFE 4.9 interpolation with separately interpolated premultiplied RGB and alpha. Original endpoints are locked exactly. Shared local weights were used under GPU reservations; none are copied into the project. Rejected optical-flow, ToonCrafter and mesh attempts remain only as development evidence in `.tmp/`.

See `tools/dragon/README.md` for repacking without models, interpolation with the shared model aliases, native sandbox verification, baseline comparison and gallery reproduction. The native recordings use synthetic voice events and no live microphone. This remains a discrete illustrated puppet with modest, drawn facing angles; larger turns can show brief interpolation softness. It is independent of the 3D and voice-latency projects.
