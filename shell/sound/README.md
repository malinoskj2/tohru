# Sound

The bar's sound button, the sound popup and the audio commands behind `tohru audio`. They run in the bar process and use Pleamar's `audio` service, which speaks the PulseAudio protocol (PulseAudio or PipeWire's PulseAudio server).

## Files

| File | Contents |
|---|---|
| `declarations.plm` | The `audio.*` permissions, the output, input and application models (32, 32 and 64 entries), sound facts, `audio_status`, the events and the icons |
| `view.plm` | The sound button and the popup's keyboard rules |
| `panel.plm` | The popup's glass, the tank include, the master row (speaker slot, rail, value), the device name with its fold chevron, the default output's mark, the focus ring and the error card |
| `details.plm` | The rows that travel between the tank and the unfolded list: output rows, the microphone and the app streams, with their pagers |
| `tank.plm`, `tank.wgsl` | The water (`SoundTank`), drawn from `sound_water`, `sound_percent`, `sound_quiet`, `sound_w` and `sound_h` |
| `controller.luau` | Watches `audio`, issues audio commands, drives the master rail and keyboard focus, and publishes `audio_status`. Returns `{close}` |
| `speaker.svg`, `muted.svg`, `mic.svg`, `mic-off.svg` | Icons, also used by the volume OSD |
| `mic-filled.svg`, `mic-off-filled.svg` | The tank's microphone mini |

The button's volume icons are in `shell/service-assets/`.

## How it is wired

`shell/bar.plm` includes `sound/declarations.plm` and `sound/view.plm`, and declares the `sound_panel` popup, which includes `sound/panel.plm`. The popup surface is 304 px wide and sized for the unfolded row count, so it is not resized while the panel moves. Inside it the panel is centred under the sound button and kept 8 px from the output's edges: 260 × 190 folded (the tank), 280 px wide and as tall as its rows unfolded. `sound_fold` springs between the two (ω 14, damping ratio 0.78), and each row follows it with its own delay (microphone 30 ms, outputs 40/80/120 ms, apps 60/90/120/150 ms). A failed request adds a card below the panel. The popup always opens folded.

The master rail is the workspace switcher's ribbon: `bar/rail-state.plm` and `bar/rail.luau` run it as rail 2, in pixels rather than slots (x = 8 + 178 × volume). On unfold the bead lands at the left end and switches to the volume 180 ms later. A click switches it; while dragging, the bead chases the pointer through the rail's spring and each change of direction starts a fresh crest.

`shell/bar.luau` starts the OSD first and passes its `observe` function to `require("sound/controller")`, so the OSD sees every audio report without a second watch. It also closes the other popups when the sound popup opens, and hides the OSD.

The controller makes a single `sys.watch("audio", ...)` call. Each report fills the models and the `audio.volume`, `audio.muted`, `audio.input` and `audio.input_muted` facts, then updates `audio_status` and the OSD. Commands never change what is displayed; only the next report does.

## Controls

| Where | Action |
|---|---|
| Sound button | Click toggles the popup; scrolling changes the volume by 5% per scroll step |
| Anywhere on the popup | Scrolling changes the volume by 5% per step |
| Device name | Folds and unfolds the popup |
| Water (folded) | A vertical drag sets the volume to the pointer's height once it has moved 3 px |
| Number (folded) | A click toggles mute; a drag that starts on it moves the water |
| Speaker slot (unfolded) | Toggles mute |
| Master rail (unfolded) | Click or drag sets the volume |
| Output rows | Choose the default output. The default output heads the list with a glowing mark; up to 3 others are shown, with a pager when there are more |
| Microphone row | Icon toggles mute, the track sets the level. Shown when a default input is reported |
| App rows | Icon toggles mute, the track sets the level. 4 to a page, with a pager. Folded, the tank shows the microphone and the first 3 apps as minis |

While a slider is dragged, at most one write is sent every 50 ms per stream, and the last value is sent on release or close. A held slider's value, and the big number for the master, show the requested level, and keep showing it after release until a report agrees (at most 500 ms). Folded, the big number counts with the water as it moves (`sound_big`); a level set by hand there or on the rail is shown at once and kept until the water arrives. Muted streams are drawn at 38%.

| Key, with the popup open | Folded | Unfolded |
|---|---|---|
| Tab | Unfold | Fold |
| Left, Down / Right, Up | Volume −5% / +5% | — |
| Up / Down | — | Previous / next row: master, outputs (default first), microphone, apps |
| Left / Right | — | Level of the focused microphone or app −5% / +5%; on the master or an output row, the output volume |
| Return, Space | Toggle mute | Master, microphone or app: toggle mute; output: make it the default |
| Page Up / Page Down | — | Previous / next page of the focused list |
| M | Toggle output mute | Toggle output mute |
| Escape | Close | Close |

A focus ring is drawn on the focused row once the keyboard has been used; a click hides it.

## CLI

| Command | Event sent to `tohru` |
|---|---|
| `tohru panel toggle sound` / `open sound` | `sound_toggle` / `sound_open_request` |
| `tohru audio volume up` / `down` | `audio_step 1` / `audio_step -1` (5%) |
| `tohru audio mute toggle` | `audio_mute` |
| `tohru audio input volume up` / `down`, `tohru audio input mute toggle` | `audio_input_up`, `audio_input_down`, `audio_input_mute` |
| `tohru audio output select <id>` | `audio_output_select <id>` |
| `tohru audio app <id> volume up` / `down`, `tohru audio app <id> mute toggle` | `audio_app_up <id>`, `audio_app_down <id>`, `audio_app_mute <id>` |
| `tohru audio status --json` | `get audio_status` |

IDs must be unsigned 32-bit integers; the controller also rejects IDs that are not in the current report. `audio status --json` returns schema 2: whether a report has arrived (`observation`), output volume and mute, microphone volume and mute, the output, input and application lists with their IDs, and the last command error.

## Tests

```sh
lua tests/sound/reconciliation.lua
lua tests/sound/osd-integration.lua
cargo test --manifest-path tests/rust/Cargo.toml --test sound
```

Run them from the repository root; the Lua tests need Lua 5.4. `tests/native-settings.sh --runtime <pleamar> --sound` (see `tests/WINDOW-RUNNER.md`) drives the popup on a private Sway session with a PulseAudio null sink; it has not been re-run since the bar layout changed.

## Known limitations

- Input devices cannot be chosen: Pleamar's input and output device IDs can collide.
- There is no balance control or peak meter.
- An app without an icon leaves its icon slot empty.
- On light glass the ribbon draws the master rail's track in dark ink and its bead is darkened, so that it shows.
- When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
