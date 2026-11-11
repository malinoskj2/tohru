# Volume OSD

A glass vessel at the right edge of the screen that shows the output or microphone level after it changes. It runs in the bar process, on the bar's outputs.

## Files

| File | Contents |
|---|---|
| `state.plm` | What is shown (`osd_open`, `osd_level`, `osd_label`, `osd_device`, `osd_input`, `osd_muted`), the last observed values, and the `osd_enter` and `osd_leave` events |
| `view.plm` | The `volume_osd` surface: 100×260 px, anchored right with a 20 px margin, on the overlay layer, on `bar_outputs`, no keyboard and no reserved space |
| `controller.luau` | Decides when to show and hide it. Returns `{observe, hide}` |

## How it is wired

`shell/bar.plm` includes `osd/state.plm` and `osd/view.plm`. `shell/bar.luau` creates the controller and passes its `observe` function to the sound controller, which calls it after every audio report. The OSD has no audio watch of its own and sends no commands.

- It shows when a report changes the default output's volume, mute or device, or the default microphone's. The first report, and reports that change nothing, do not show it. If both change at once, it shows the output.
- It shows the level as a fill and a percentage, "Mute" when muted, the speaker or microphone icon, and the device name below the vessel.
- It hides 1600 ms after the last change. Hovering over the vessel holds it open; leaving starts the 1600 ms again.
- Opening the sound popup hides it, and it does not show while the popup is open.
- If the device it is showing disappears, it hides.

The 44×200 px vessel is the surface's only zone, and it is active only while the OSD is shown. The surface itself exists for as long as the bar runs; when hidden it draws nothing.

## Tests

```sh
lua tests/sound/osd-integration.lua
cargo test --manifest-path tests/rust/Cargo.toml --test osd
```

Run them from the repository root; the Lua test needs Lua 5.4.

## Known limitations

The entrance is a Pleamar spring with a squash, not j2bar's Jelly and Slosh motion.
