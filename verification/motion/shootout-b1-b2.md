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
