# Motion reviewer brief

Give this file to a reviewer who did not make the animation: a fresh subagent with no memory of the
work, or another harness. Do not pass on the author's reasoning, earlier verdicts or what the owner
said about this clip; the reviewer gets the evidence and the intent only.

## Prompt

> You are reviewing a Tohru animation for faults. You did not make it, and your job is to find what
> is wrong, not to approve it. Assume it has faults until the evidence shows otherwise.
>
> Intent: INTENT (one or two sentences: what the character should do and where on the desktop).
> Evidence: EVIDENCE_DIR (from `tools/verify/motion.py`). Approved reference: shown on the right of
> `side-by-side.mp4`. Checklist: `verification/motion/rejections.md` in the Tohru checkout.
>
> 1. Read `report.md` and `checks.json`. For every strong flag, open `flagged/frame-NNNN.png`
>    (onion skin: red previous outline, white current, blue next) and decide whether the flag is a
>    visible fault, a harmless measurement or unclear.
> 2. Look at every page of `filmstrip-dark*.png` (and `filmstrip-light*` when present) in order,
>    then `onion*.png`, `difference*.png` and `outline-trail.png`. Note frames the checks missed.
> 3. Judge motion at real size. Extract frames from `desktop.mp4` and `side-by-side.mp4` at full
>    resolution with ffmpeg (for example
>    `ffmpeg -i desktop.mp4 -vf "select='between(n,START,END)'" -vsync 0 /tmp/review/%04d.png`)
>    and view consecutive frames. Stills of single frames hide snaps and slides.
> 4. Walk every checklist item. For each, answer fault, no fault or cannot tell from this evidence.
> 5. Report: a verdict (reject, needs the owner's eye, or no faults found), then each fault with its
>    checklist ID, frame range, what is visible, the image you saw it in, and severity (blocks,
>    noticeable, minor). List the checklist items you could not judge and why.
>
> Do not edit files, rerun the generator or propose fixes beyond one line per fault. Do not soften
> a fault because the clip is mostly good.

## Using the result

The author may not dismiss a reviewer fault. Fix it and review again, or show the owner both the
fault and the counter-evidence. A "no faults found" verdict is still not the owner's approval.
