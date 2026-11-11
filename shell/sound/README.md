# Sound

The bar's sound button, the sound popup and the audio commands behind `tohru audio`. They run in the bar process and use Pleamar's `audio` service, which speaks the PulseAudio protocol (PulseAudio or PipeWire's PulseAudio server).

## Files

| File | Contents |
|---|---|
| `declarations.plm` | The `audio.*` permissions, the output, input and application models (32, 32 and 64 entries), sound facts, `audio_status`, the events and the icons |
| `view.plm` | The sound button and the popup's keyboard rules |
| `panel.plm` | The popup's main part: output mute, volume slider, default output name and the More (⋯) button |
| `details.plm` | The More section: Outputs, Input and Apps tabs, four rows to a page |
| `controller.luau` | Watches `audio`, issues audio commands and publishes `audio_status`. Returns `{close}` |
| `speaker.svg`, `muted.svg`, `mic.svg`, `mic-off.svg` | Icons, also used by the volume OSD |

The button's volume icons are in `shell/service-assets/`.

## How it is wired

`shell/bar.plm` includes `sound/declarations.plm` and `sound/view.plm`, and declares the `sound_panel` popup, which includes `sound/panel.plm`. The popup is 300 px wide, centred under the sound button and kept 8 px from the output's edges; it grows when More is open or a request has failed.

`shell/bar.luau` starts the OSD first and passes its `observe` function to `require("sound/controller")`, so the OSD sees every audio report without a second watch. It also closes the other popups when the sound popup opens, and hides the OSD.

The controller makes a single `sys.watch("audio", ...)` call. Each report fills the models and the `audio.volume`, `audio.muted`, `audio.input` and `audio.input_muted` facts, then updates `audio_status` and the OSD. Commands never change what is displayed; only the next report does.

## Controls

| Where | Action |
|---|---|
| Sound button | Click toggles the popup; scrolling changes the volume by 5% per scroll step |
| Main part | Mute button and volume slider. While dragging, at most one volume write is sent every 50 ms, and the last value is sent on release or close |
| Outputs tab | Choose the default output |
| Input tab | Microphone level and mute for the default input. Other input devices are listed but cannot be chosen |
| Apps tab | Per-stream volume (−, +, 5% steps) and mute |

Keyboard, with the popup open: Tab switches between mute and slider (or, in More, between tabs), arrows change the level by 5%, Up and Down move between rows in More, Return or Space toggles mute or chooses the selected output, Page Up and Page Down change page, Escape closes.

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
- When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
