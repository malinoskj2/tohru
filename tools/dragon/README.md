# Airborne dragon animation sources

This expands the shipped illustrated puppet. Twenty redrawn airborne keys change the pelvis, feet, tail, wings and facing. The original neutral remains the transition hub. The animator loads no art models at runtime.

`art/hover-source.png` is the original cloud output; `art/prompts.md` preserves its exact prompt and character reference. `art/poses.json` owns the names, measured eye/mouth coordinates, semantic groups and body descriptions. `art/legacy-body.webp` preserves the previous 202-frame sheet. `art/inbetweens/row*.png` retain four cloud-drawn animation sheets. `art/inbetweens/registration.json` selects three safe intermediate drawings for each sequence, including appropriate existing keys for gradual facing changes. The duplicate-head drawing at sheet 2, cell 5 is excluded. `art/transitions/*.webp` retain the selected 17-frame lossless strips independently of model availability. `art/model-provenance.json` records the offline RIFE interpolation checkpoint and runner. `art/packing.json` records dimensions, bytes, source RGBA costs and logical display budgets. `art/travel-registration.json` retains the moving mouth anchors.

Previous conversational-key generators were located through `/home/jesse/projects/tohru/.tmp/asset-archive.md` and the merged-children manifest (`dragon-talk-poses/.tmp/pack-talk-poses.py`). They are not needed to regenerate this expansion. The legacy sheet is an input snapshot; its original commit and checksum are in `art/legacy-provenance.json`.

## Repack without models

Use the Python dependencies in `requirements.txt`, then run:

```sh
python3 tools/dragon/pack.py --preview .tmp/full-body/keys
python3 tests/assistant/dragon-art.py
lua tests/assistant/puppet.lua
lua tests/assistant/full-body.lua
lua tests/assistant/voice-controller.lua
```

Packing writes the runtime body/face sheets, generated Luau metadata and the packing report. It calls no models. Body cells are 240 px, displayed at 150 logical px. The 24 × 24 grid stays within a 16384 px texture at display scales through 4×. The older 300 px frames are resampled once. All body states share one preloaded texture to avoid image-switch blanks. New packer writes stage under `.tmp/full-body/packing` and replace the completed runtime files.

`travel-mouths.py` also builds a small native mouth patch for every moving frame, including the reverse path. It finds eyes inside the peach head so dark tail/wing edges cannot become face anchors.

The native capture set includes normal speech settling as well as interruption and reduced-motion recovery.

Face cells have transparent gutters; only changed eyes/mouth pixels have opacity. Each view uses its own eyes and inpainted skin. Front-view eye patches are never used on the new turned faces. Eight mouth shapes and three blink states are precomposed offline per view. The closed, unblinking face shows the untouched body drawing. Facial patches follow the same parent transform as the body.

## Regenerate the interpolation

The cloud art and registration are retained, so this stage does not regenerate characters. Read `$MODEL_ROOT/catalog.json`, `readiness.json` and `README.md` before selecting the local model. Shared `rife` and the `frame-interpolation/rife/rife49.pth` alias must be ready; the script refuses missing weights rather than downloading. Respect `/etc/resource-scheduler/mode`; use `resource-run` on Slurm. Otherwise:

```sh
gpu-run --vram 6GiB --name 'Tohru drawn hover interpolation' -- \
  frame-interp-python tools/dragon/interpolate.py
python3 tools/dragon/pack.py --preview .tmp/full-body/keys
```

The selected five drawings per sequence are face-registered, with translation clamped to safe margins. RIFE 4.9 inserts three frames per interval. Premultiplied RGB and alpha are interpolated separately; alpha is clamped above premultiplied colour. Original endpoints are locked exactly. The model is cached inside this offline process, then released when the process exits; the reservation lasts through cleanup. The recorded peak PyTorch allocation at 300 × 300 was 83,891,200 bytes; this is not the whole CUDA process footprint. The 6 GiB reservation includes a conservative safety margin.

Initial optical-flow, ToonCrafter and mesh attempts were rejected for ghosting, anatomy drift or poor matting. They are development evidence in `.tmp/full-body/rejected-methods` and `.tmp/full-body/toon*`, not runtime sources. The selected workflow uses curated drawn intermediates with non-generative interpolation.

## Native verification and media

```sh
sandbox-display up --hyprland
python3 tools/dragon/review.py --runtime /path/to/pleamar
# In a second terminal, for a comparison of the original branch behavior:
python3 tools/dragon/review.py --baseline --runtime /path/to/pleamar
# With both harnesses running:
python3 tools/dragon/capture.py --runtime /path/to/pleamar
python3 tools/dragon/evidence.py
```

The baseline reads the pinned original commit, not the current HEAD. The harnesses use the real scene and puppet with synthetic voice events. They start no microphone, voice service or models. Native captures use CPU H.264 encoding. `evidence.py` produces one inspection clip per sequence, a contact sheet, actual-size stills and a gallery batch description. Development media live under the worktree's `.tmp/`.

To drive the current native puppet manually:

```sh
WAYLAND_DISPLAY=hyprland-1 pleamar --say dragon-body-review 'emit review_listen'
WAYLAND_DISPLAY=hyprland-1 pleamar --say dragon-body-review 'emit review_think'
WAYLAND_DISPLAY=hyprland-1 pleamar --say dragon-body-review 'emit review_speak'
WAYLAND_DISPLAY=hyprland-1 pleamar --say dragon-body-review \
  'text ai_draft That is wonderful!' 'emit review_sentence'
```

Other harness events: `review_pause`, `review_finish`, `review_interrupt`, `review_calm`, `review_reduced 1`, `review_key 0` through `review_key 19`, and `review_live`. Static key inspection freezes the controller; `review_live` resumes it. These events exist only in the harness.
