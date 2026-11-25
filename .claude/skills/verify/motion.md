# Verify motion

Read this only for motion work: character animation, sprite or landing sheets, flights, AFK scenes,
puppet poses, transitions and their timing. Owners rejected Tohru animations that agents had
reported as fine, because a contact sheet or a few stills hid what moved badly. Judge every frame
in order and at desktop size, and have someone who did not make it look for faults.

## Capture

Review what the user sees. Get both when they exist:

- the ordered frames the generator produced (a PNG/WebP folder, or a sheet with `--grid` and `--row`);
- a native capture of the shell playing it on a desktop (`tools/dragon/capture.py`, the AFK harness),
  at the output's real size.

Every frame, not a sample. Keep the capture's exact source commit and runtime.

## Review

```sh
python3 tools/verify/motion.py FRAMES_OR_VIDEO --label NAME --approved APPROVED_CLIP \
  [--display-size PX] [--loop] [--parts parts.json] [--edge-anchor bottom]
```

- `--display-size` is the art canvas's long side on screen in pixels (the Rive turn's 480 px canvas
  shows at 166); captures are already at desktop size.
- `--approved` puts an approved clip beside the candidate at 1:1 desktop pixels and calibrates the
  checks: a flag only counts as strong if it beats the approved clip's strongest flag for that check.
- `--loop` adds the end-of-loop drift check. `--parts` names source-pixel rectangles (head, wing) to
  measure separately; an animation that stays in place also gets a 3 × 3 zone grid automatically.
- `--edge-anchor` names screen edges the character should rest on; captures check all four.

It needs ffmpeg, NumPy, Pillow and SciPy, runs on the CPU, and writes
`.tmp/verify/motion/NAME-STAMP-ID/` with every frame's metrics and hashes. Exit 2 means strong flags.

| Output | Use it to see |
| --- | --- |
| `report.md`, `checks.json` | Ranked flags with frames, where, and strength against the approved clip |
| `flagged/frame-NNNN.png` | Five frames around each strong flag, zoomed, onion-skinned |
| `filmstrip-dark*.png`, `filmstrip-light*.png` | Every frame in order, numbered, flagged frames outlined |
| `onion*.png` | Each frame with the previous (red) and next (blue) outlines |
| `difference*.png` | What changed since the previous frame; blue is silhouette change |
| `outline-trail.png` | All outlines over time; uneven spacing is a snap or stall |
| `desktop.mp4`, `side-by-side.mp4` | The candidate at real size, and beside the approved clip |
| `metrics.json` | Per-frame area, box, centroid, holes and part areas |

Checks: `silhouette-area` and `part-area` (area off its local trend, per colour part and zone),
`outline-pulse` (outline swells or dents for one or two frames and snaps back), `bbox-continuity`,
`centroid-jerk`, `pose-snap`, `mirror-flip`, `vanish` (blank frames), `rigid-motion` (the drawing
slides or zooms unchanged), `transparent-holes`, `edge-contact` and `loop-drift`. Frames cut by the
screen edge are skipped by the shape checks, so entrances need the reviewer's eyes.

## Approved references

| Clip | Use for |
| --- | --- |
| `/home/jesse/projects/tohru/.tmp/integration-top-level/flight/evidence/landing-review-evidence/after/right.mp4` (sha256 `888cfa5b…`, gallery #31) | Desktop captures of flight and AFK scenes |
| Same folder: `rise.mp4` (#27, `69c43ff7…`), `wrap.mp4` (#23, `4861dc7b…`), `sweep.mp4` (#35, `551e2557…`) | Other entrances |

The owner accepted these provisionally; their settle still reads as morphing. Check a reference's
hash before using it and say which one you compared against.

## Judge

1. Open every strong flag's strip. Decide fault, harmless or unclear, and say why.
2. Page through the filmstrips and onion sheets yourself; checks miss what they do not measure.
3. Walk all of [the owner's rejection checklist](../../../verification/motion/rejections.md).
4. Start a separate reviewer that did not make the animation with
   [the reviewer brief](../../../verification/motion/reviewer.md). Give it the evidence folder and the
   intent only. Its faults stand until fixed or shown to the owner.

## Report

Give the evidence folder, the approved clip used, each strong flag and what it was, the reviewer's
verdict and faults, the checklist items nobody could judge, and the media for the owner (show three
or more with the media gallery). Never call an animation good from automatic checks or your own
look alone; owner approval is separate from both.
