# Panels and calendar

Users open a popup from the bar or a bound CLI command, inspect its contents and
close it without leaving a stale popup behind.

## Sub-features

- `calendar-cli`: open, toggle off/on and close with populated calendar.
- `calendar-pointer`: clock click and outside dismissal.
- `calendar-input`: previous/next month, month rail, today and Escape.
- `panel-routing`: opening a second panel closes the first.
- `sound-effects`: per-app/output/input controls change a private audio endpoint.

## How to get to it (user POV)

- Click the clock in the bar; use calendar arrows/month rail/Back to today.
- Run `tohru panel open calendar`, `tohru panel toggle calendar`, `tohru panel close`.
- Press Escape to dismiss a CLI-opened popup; outside dismissal requires a real
  compositor input serial from the bar click path.
- Click corresponding status icons or run `tohru panel open|toggle NAME`, where
  NAME is sound, network, media, bluetooth, wallpaper, tray, info or about.
- Sound controls also have `tohru audio` commands documented by `tohru help`.

## Driving it with the desktop driver

Preconditions: baseline private native instance and runtime/GPU lease from the [index](README.md#desktop-driver).

- **CLI journey.** Run `tools/verify/desktop.py run --feature calendar --runtime "$verify_runtime"`
  inside the lease. It drives the actual `bin/tohru panel` commands; `cal_open`
  must become false → true → false → true → false, with nonempty `cal_title`.
- **Visible proof.** Inspect `calendar-before.png`, `calendar-open.png`,
  `calendar-after.png` and matching JSON descriptions. Open must show the month
  grid; before/after must show only the bar. The command transcript records actions.
- **Other entries.** The private pointer/keyboard harness in
  `tests/WINDOW-RUNNER.md` and `tests/native-settings.sh --sound` drives root
  sound/calendar interaction; it requires compiled pointer/keyboard helpers and
  an owned Pulse null sink. It is not run by `tools/verify/desktop.py` and must be separately
  checked for its hardware adapter prerequisites. Do not direct it at host audio.

## Gotchas

- CLI acceptance can precede rendering; the driver waits for usable status.
- CLI toggle/open does not prove the clock click, month navigation or Escape.
- The driver deliberately has no audio server. Sound changes are blocked, not
  successful mute/volume operations. Other panel data needs real private services.
- Private Sway has no fabricated workspaces and does not certify Hyprland UX.
