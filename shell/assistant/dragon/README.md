# Voice dragon sheets

The overlay in `../dragon.plm` draws these; `../puppet.luau` chooses what to show. They load only while voice is on.

The body sheet uses 300 px cells, twice the 150 px the dragon is drawn at. Each cell has the same canvas as the keys, so cells line up without offsets. Animation runs at 24 fps.

| File | Grid | Bytes | Decoded at 1× (2×) | Contents |
|---|---|---|---|---|
| `body.webp` | 8 × 26 | 3,720,154 | 18.7 MB (74.9 MB) | Every body frame, so changing frames never changes images. Cells 0–47: idle breath, 2 s. Cells 48–95: wing flutter with a tail flick; both loops start and end on cell 0. Cells 96–164: drawn transitions (first cell, frames): neutral→explain (96, 17), explain→cheer (113, 17), neutral→laugh (130, 17), neutral→thinking (147, 18), reversed for the way back. Cells 165–176: the 12 keys (neutral, happy, blink, neutral-talk, talk-explain, thinking, listening, laugh, pleased, surprised, scroll-talk, wing-cheer). Cells 177–181: neutral, talk-explain, happy, scroll-talk and wing-cheer with the mouth painted out. Cells 182–191: ten conversational talking keys (head-tilt, lean-in, right-gesture, shrug, wing-lift, count-one, soft-nod, recall-glance, open-palms, chest-paw). Cells 192–201: the same ten with the mouth painted out |
| `eyes.webp` | 4 × 1 | 4,876 | — | 100 × 52 px patches: half-blink, blink, glance left, glance up |
| `mouths.webp` | 8 × 1 | 3,700 | — | 44 × 40 px patches: rest, M/B/P, AA, EH, EE, OH, OO, F/V |
| `heart.png` | 1 | 2,221 | — | 48 px glass heart for the cheer |
| `glass.wgsl` | | | | Shimmer shader: a 7° Oklab hue wave every 7 s, and a highlight band that crosses in 1.8 s every 9 s |

Total: 3.7 MB on disk. In the nested Hyprland at scale 1, loading them added 32 MiB of VRAM and about 16 MB of RSS to the bar.

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

## Provenance

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

The generators, masks and packing scripts are not in the repository. They are in the dragon-poses worktree's `.tmp/dragon-anim/src/` (`pack.py` writes this folder).
