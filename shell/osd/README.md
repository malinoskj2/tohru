# Volume OSD

A glass tanklet (`62 × 206 px`, `15 px` corners) at the right edge of the screen that shows the output or microphone level after it changes, sharing the design language of the sound popup (`shell/sound/`). It runs in the bar process, on the bar's outputs.

## Files

| File | Contents |
|---|---|
| `state.plm` | What is shown (`osd_open`, `osd_level`, `osd_label`, `osd_device`, `osd_input`, `osd_muted`, `osd_clock`), the last observed values, and the `osd_enter` and `osd_leave` events |
| `view.plm` | The `volume_osd` surface: `100 × 260 px`, anchored right with a `20 px` margin, on the overlay layer, on `bar_outputs`, no keyboard and no reserved space. Draws the `62 × 206 px` lens-glass vessel, inset hairline, `tank.wgsl` water layer, glowing `4 × 4 px` microphone mark, and device caption |
| `tank.wgsl` | Clear-to-frosted water shader matching `shell/sound/tank.wgsl`: organic drifting satin frost, subsurface meniscus shadow, slow-breathing five-stop workspace-ribbon light, luminous meniscus with inertia tilt and slosh, and 12-tap Vogel disc blur + optical refraction over the static thin numeral and foot glyph |
| `controller.luau` | Decides when to show and hide it. Returns `{observe, hide}` |

## How it is wired

`shell/bar.plm` includes `osd/state.plm` and `osd/view.plm`. `shell/bar.luau` creates the controller and passes its `observe` function to the sound controller, which calls it after every audio report. The OSD has no audio watch of its own and sends no commands.

- It shows when a report changes the default output's volume, mute or device, or the default microphone's. The first report, and reports that change nothing, do not show it. If both change at once, it shows the output.
- It shows the level as clear-to-frosted water behind a static `30 px` Ultralight numeral (`SF Pro Text`), `"Mute"` with drained quiet glass and `38%` ink when muted, the speaker or microphone icon at the foot, a `4 × 4 px` glowing mark at top-right when showing an active microphone, and the device name below the glass.
- Entering and exiting use a critically-damped lateral glide (`24 px`, `spring(196, 24.0)`) that imparts a natural inertia tilt and slosh to the meniscus; reduced motion holds the surface calm. When hidden, the shader group is hidden so its clock stops completely.
- It hides 1600 ms after the last change. Hovering over the `62 × 206 px` vessel holds it open; leaving starts the 1600 ms again.
- Opening the sound popup hides it, and it does not show while the popup is open.
- If the device it is showing disappears, it hides.

The `62 × 206 px` vessel is the surface's only zone, and it is active only while the OSD is shown. The surface itself exists for as long as the bar runs; when hidden it draws nothing.

## OSD family grammar

Other desktop OSDs belong to the same `62 px`-wide, `15 px`-corner glass family anchored `20 px` from the right edge:
- **Continuous meters (`62 × 206 px`):** output volume, microphone, and display brightness (clear-to-frosted water with warm solar light shafts and a sun glyph).
- **Discrete state companions (`62 × 84 px`):** keyboard layout (two-letter code, globe glyph, horizontal satin-frost settling wash), Caps Lock, and Do Not Disturb (satin-frosted active fill with the `4 × 4 px` glowing mark vs. sheer quiet glass when off).

## Tests

```sh
lua tests/sound/osd-integration.lua
cargo test --manifest-path tests/rust/Cargo.toml --test osd
```

Run them from the repository root; the Lua test needs Lua 5.4.

## Known limitations

- `bright:` backdrop brightness tracking (`runtime/pleamar/patches/0020-backdrop-bright-share.patch`) currently targets `popup` elements rather than overlay `surface` elements, so `volume_osd` currently renders with dark glass (`pk = 0`), while `tank.wgsl` already accepts `pk` for future light-glass adaptation.
