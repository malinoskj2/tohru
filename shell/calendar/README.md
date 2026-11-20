# Calendar

A month calendar that opens from the bar's clock, laid out as a wide split ("Horizon Scrubber"). It runs in the bar process; the popup hangs under the right end of the bar.

- **Left column:** the shown month and year, today's day as a large thin numeral that never moves, and under it today's weekday and ISO week (`Friday · W47`). The line below names the hovered day (`Wed 25 · +5d` in today's month, `Thu 11 Jun · W24` elsewhere), a clicked day, `Scrubbing June` during a scrub, or `Today` / `Viewing June`. Away from today's month a `Today · Nov 20` chip replaces the weekday line and returns to today; the numeral dims.
- **Right side:** ‹ › month buttons, the weekday initials, and the grid. Days of the adjacent months are dimmed, weekends a little, today has a small dot, and the hovered day is outlined.
- **Horizon:** under the grid, the year's months on the workspace switcher's ribbon (rail 1 in `../bar/rail-state.plm`), with the initials J…D under it and the bead on the shown month. The bead takes the ribbon's colour by position and its wave and stretch on every month change. Pressing a month goes there; dragging moves the bead with the pointer and changes the month live, and the release switches the bead back into its month. A month change slides the grid in from that side; during a scrub the grid is dimmed until the release.

Scrolling over the grid or the ‹ › buttons moves by a month (a notch upwards goes back; touchpad fractions add up to whole notches). The glass, inks and hairline follow `../panel-ink.plm`, light or dark from what is behind the popup.

## Files

| File | Contents |
|---|---|
| `state.plm` | Facts, texts, the `cal_days` model (up to 42 days) and the calendar events |
| `popup.plm` | The 446 px popup at `screen.width - 546, 37`: left column, month buttons and grid |
| `rail.plm` | The horizon: the ribbon, the month initials and the scrub zone |
| `controller.luau` | Date arithmetic, navigation, hover and scrub. Returns `{open, close, toggle, pitch}` |
| `entry.plm`, `entry.luau` | A standalone scene with a Calendar button, for development |

## How it is wired

`shell/bar.plm` includes `calendar/state.plm` and `calendar/popup.plm`. `shell/bar.luau` builds the month rail with `require("bar/rail")(1, 21)`, 21 px per month, and passes it to `require("calendar/controller")`. It uses the returned functions to close the calendar when another popup opens.

| Event | Effect |
|---|---|
| `calendar_toggle`, `calendar_open`, `calendar_close` | Toggle, open or close. The clock sends `calendar_toggle`; `tohru panel toggle calendar` and `tohru panel open calendar` send the first two |
| `calendar_previous`, `calendar_next`, `calendar_today` | Change month, or go back to today |
| `calendar_scroll <notches>` | Wheel over the grid or the buttons; positive is upwards, back in time |
| `calendar_month <1..12>` | Jump to that month of the year being viewed |
| `calendar_day <index>` | Select a day in the grid, including days of the adjacent months |
| `calendar_hover <index>`, `calendar_unhover <index>` | The pointer enters or leaves a day; leaving clears the hover only if no other day was entered first |
| `calendar_scrub_press <x>`, `calendar_scrub <x>`, `calendar_scrub_release` | Press, drag and release on the horizon, `x` in pixels from the rail's left end |

The `clock` service supplies today's date. Every open resets the view to today. A date change moves the today marker and the numeral without changing the month being viewed. Weeks start on Sunday; the names are English. Escape or a click elsewhere closes it.

While scrubbing, the popup's layout only grows (a six-week month makes it taller), so the horizon stays under the pointer; the release settles it on the shown month's height. The popup surface always has a six-week month's size, because resizing a popup closes and reopens it.

Facts survive a logic reload, so the viewed month and the selection stay put.

## Running and tests

```sh
lua tests/calendar/controller.lua    # month stepping, wheel, scrubbing, Today, hover text, year boundaries
lua tests/bar/controllers.lua        # calendar controller lifecycle, including reload
lua tests/bar/rail.lua               # rail goals, pitch, drag-then-settle, per-switch draws
cargo test --manifest-path tests/rust/Cargo.toml --test workspace-shader   # the ribbon shader
pleamar --check shell/calendar/entry.plm
TOHRU_SHELL=1 pleamar --scene shell/calendar/entry.plm --answer-as tohru-calendar
```

Run the Lua tests from the repository root with Lua 5.4. The standalone scene cannot require `bar/rail` from outside its folder, so it runs without the horizon's bead: the ribbon shows, the bead does not.

## Known limitations

When opened from the CLI, the calendar closes with Escape or `tohru panel close`, not with a click outside it.

In the sandbox's nested Hyprland, with a virtual pointer, a held button reaches every popup as a press repeated each frame for about a second before drag events start (the sound popup's rail does the same). A click on ‹ › there skips many months. Whether a real pointer on a real session does the same is unverified.
