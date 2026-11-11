# Bar

The bar is the root of the `tohru` process. `shell/bar.plm` declares the bar surface and includes every popup, the volume OSD and the screen corners; `shell/bar.luau` starts their controllers and publishes settings.

## Files

| File | Contents |
|---|---|
| `../bar.plm` | Root scene: the 31 px reserved bar on `bar_outputs`, its glass and hairline, and every include |
| `../bar.luau` | Publishes settings to facts, handles `config_reload` and `setting_*`, starts the controllers, keeps one popup open at a time, forwards `session_toggle` to `tohru-session` |
| `../services.plm` | The network, Bluetooth, information and About buttons, the media label, and their placement |
| `../session-route.luau` | Sends `emit menu_toggle` to `tohru-session` and records failures in `routing_error` |
| `contents.plm` | Mooncoil mark, focused window title and icon, clock |
| `controller.luau` | Watches `window`, `apps`, `media` and `clock`, and fills the title, icon, media label, date and time |
| `state.plm` | Texts, facts, measures and images used by the contents, and the clock's position |
| `fill.wgsl` | The bar's fill shader |
| `mooncoil.svg` | The Mooncoil mark, also used by the About popup |
| `workspace.plm`, `workspace.luau`, `rail-state.plm`, `rail.luau`, `ribbon.wgsl` | The workspace switcher. `rail-state.plm` and `rail.luau` also drive the calendar's month row |
| `bell.plm`, `bell.luau` | The notification bell |
| `../panel-ink.plm` | The `ink_*` colours every popup draws with, and the switch between dark and light glass |
| `power.plm` | A battery percentage cell. No scene includes it |
| `entry.plm`, `entry.luau` | A standalone scene with only the bar contents |

## Layout

All positions are logical pixels, matching j2bar.

- The bar spans the output width minus 100 px on each side and reserves 31 px at the top.
- With `bar.mooncoil` on, the Mooncoil mark sits at the bar's left end; clicking it toggles the session menu. The workspace switcher follows it.
- The focused window's title and application icon are centred, at most 360 px wide, and shrink so they never overlap the left group or the button row.
- The button row is 24×22 px buttons at y=4, 2 px apart, ending 110 px from the right edge of the output. From the right: clock, network, sound, notification bell, Bluetooth, information (`bar.info_button`), About (`bar.about_button`). When the output is too narrow, About, information and Bluetooth drop out, in that order.
- While media plays, its title and artist (up to 160 px) appear left of the buttons, if there is room.
- The clock shows `Wed 11 Nov` and a 12-hour time in SF Pro Text with tabular numerals (`family: "SF Pro Text:tnum"`, from patch 0013), so the width does not change as digits change. Clicking it toggles the calendar.

The application icon comes from matching the focused window's class against the `apps` catalog by id, id suffix and `StartupWMClass`.

## Outputs

`workspace.luau` writes `bar_outputs`, which the bar surface and the OSD follow, and so the popups that open from the bar: the value of `bar.preferred_output`, or, when that is empty, every monitor the `workspaces` service reports. With a preferred output the switcher counts only that output's workspaces.

## Workspace switcher

The switcher shows workspaces 1 to 10 and can only switch to workspaces the compositor reports; it cannot create one. Its motion is Pleamar springs declared in `rail-state.plm`, which stop drawing once the wave has died out; `rail.luau` only sets where the bead should go and the random draws for each switch (spring, swell, crest reach, wave life). `ribbon.wgsl` draws it from Pleamar's eight shader values. Pleamar gathers Hyprland's workspace events for 50 ms, so the motion starts a few frames after j2bar's.

## Notification bell

The bell sits left of the volume button. It shows j2bar's 4 px pink unread mark while a notification has arrived unseen, and a dimmed bell-off icon in Do Not Disturb, which hides the mark. A left click marks notifications seen; a right click toggles Do Not Disturb. `tohru-notifications` owns both states and reports them with `notices_state` (see [../notifications/README.md](../notifications/README.md)); without that process the bell shows neither.

## Panel ink

Every popup says `bright: panel_bright` (patch 0020), the share of what is behind its glass that is bright. `../panel-ink.plm` turns `panel_light` on above 55% and off below 45%, as j2bar does, and eases the `ink_*` colours from Catppuccin Mocha to j2bar's light ink and Latte. The bar itself stays dark, as j2bar's does.

## Events

| Event | Sent by | Effect |
|---|---|---|
| `calendar_toggle` | Clock | Toggles the calendar |
| `session_toggle` | Mooncoil mark | Sends `emit menu_toggle` to `tohru-session` |
| `network_toggle`, `bluetooth_toggle`, `info_toggle`, `about_toggle`, `sound_toggle` | Button row | Toggle that popup |
| `bell_seen`, `bell_dnd` | Bell, left and right click | Send `emit notices_seen 0` and `emit notices_dnd 2` to `tohru-notifications` |
| `notices_state <n>` | `tohru-notifications` | Sets the bell: 1 unread, 2 Do Not Disturb, 3 both |
| `panel_close` | `tohru panel close` | Closes every popup |

## Tests

From the repository root, with Lua 5.4:

```sh
lua tests/bar/controllers.lua
lua tests/session-route.lua
pleamar --check shell/bar.plm
pleamar --check shell/bar/entry.plm
```

`tests/bar/native.py --runtime <pleamar>` runs the bar on a private Sway session; `--review-only`, `--icons-only`, `--launcher-only`, `--lifecycle-only` and `--services-only` select its other checks. `python3 tests/bar/panel-ink.py --runtime <pleamar>` checks the `bright:` property and the panels that use it; its hysteresis check needs a compositor. See `tests/WINDOW-RUNNER.md` for the helpers it needs.

## Known limitations

- On light panels the calendar's month rail still draws its dark-glass track, which barely shows.
- Pleamar draws text with swash rather than Pango, so letter shapes differ slightly from j2bar's.
