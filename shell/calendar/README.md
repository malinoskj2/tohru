# Calendar

A month calendar that opens from the bar's clock. It runs in the bar process; the popup hangs from the bar surface.

## Files

| File | Contents |
|---|---|
| `state.plm` | Facts, texts, the `cal_days` model (up to 42 days) and the calendar events |
| `popup.plm` | The 320 px popup at `screen.width - 430, 36`: month title, previous and next, day grid, selected date and week number |
| `controller.luau` | Date arithmetic, navigation and selection. Returns `{open, close, toggle}` |
| `rail.plm`, `rail-state.plm`, `rail-publisher.luau` | The row of twelve month cells, drawn with the workspace switcher's ribbon shader and `tohru_workspace` physics |
| `entry.plm`, `entry.luau` | A standalone scene with a Calendar button, for development |

## How it is wired

`shell/bar.plm` includes `calendar/state.plm` and `calendar/popup.plm`. `shell/bar.luau` builds the month row with `require("calendar/rail-publisher")(tohru_workspace, "calendar", "cal_rail_")` and passes it to `require("calendar/controller")`. It uses the returned functions to close the calendar when another popup opens.

| Event | Effect |
|---|---|
| `calendar_toggle`, `calendar_open`, `calendar_close` | Toggle, open or close. The clock sends `calendar_toggle`; `tohru panel toggle calendar` and `tohru panel open calendar` send the first two |
| `calendar_previous`, `calendar_next`, `calendar_today` | Change month, or go back to the current one |
| `calendar_month <1..12>` | Jump to that month of the year being viewed |
| `calendar_day <index>` | Select a day in the grid, including days of the adjacent months |

The `clock` service supplies today's date. Every open resets the view to today. A date change moves the today marker without changing the month being viewed. Weeks start on Sunday; the footer shows the selected date and its week number. The layout follows j2bar's calendar.

Facts survive a logic reload, so the viewed month and the selection stay put.

## Running and tests

```sh
lua tests/bar/controllers.lua        # calendar controller, including reload
lua tests/bar/rail.lua               # month-row timer
pleamar --check shell/calendar/entry.plm
TOHRU_SHELL=1 pleamar --scene shell/calendar/entry.plm --answer-as tohru-calendar
```

Run the Lua tests from the repository root with Lua 5.4. Without `TOHRU_SHELL=1` the standalone scene has no `tohru_workspace` and skips the month-row motion.

## Known limitations

When opened from the CLI, the calendar closes with Escape or `tohru panel close`, not with a click outside it.
