# System information and power warnings

A read-only System Information popup with seven pages, and the battery warnings sent to the notification daemon. Both run in the bar process. The popup opens from the bar's information button, which `bar.info_button` turns on, from About's More Info, or with `tohru panel toggle info`.

## Files

| File | Contents |
|---|---|
| `state.plm` | Popup facts, the visible `info_rows` (8) and cached `info_cache` (256) models, the `clock` service and the `info_*` events |
| `power-state.plm` | Battery facts (`power_present`, `power_known`, `power_percent`, `power_charging`), `power_warning_level` and `power_error` |
| `popup.plm`, `content.plm` | The 600×520 px popup, centred horizontally 48 px from the top, with page tabs, rows, paging and Refresh |
| `controller.luau` | Builds each page's rows from a `tohru_info` snapshot. Returns `{open, close, toggle, refresh}` |
| `power.luau` | Watches `battery` and sends power warnings |
| `entry.plm`, `entry.luau` | A standalone scene with an information button, for development |

## Pages

| Page | Shows |
|---|---|
| 0 System | OS, version, host name, kernel, processor, graphics, memory, available memory, uptime, active generation, configuration revision (`/run/current-system/configuration-revision`) |
| 1 Displays | Output name and logical size, if the controller is given an outputs function. The bar gives it none, so this page reads Unknown |
| 2 Date/Time | Date and time from the `clock` service. Time zone reads Unknown |
| 3 Users | Accounts from `/etc/passwd` (up to 256), the current one first, with UID, description, home and shell |
| 4 Storage | Block devices (capacity, model, removable) and mounts (source, filesystem, read-only). Free space reads Unknown |
| 5 Generations | NixOS system generations (`/nix/var/nix/profiles/system-*-link`), with the one matching `/run/current-system` marked active |
| 6 Power | Battery presence, percentage and charging state from the `battery` service |

Pages 0 and 3 to 5 read a snapshot only when the page opens or Refresh is pressed; rows are cached, so scrolling and logic reloads do not read again. Up and Down scroll one row, Page Up and Page Down eight, and Escape closes. A fact the snapshot lacks reads Unknown, and the popup says when some sources reported problems.

## How it is wired

`shell/bar.plm` includes `info/state.plm` and `info/popup.plm`, and its permissions include `battery` and `say:tohru-notifications`. `shell/bar.luau` calls `require("info/controller")(tohru_info.snapshot)` and `require("info/power")()`.

| Event | Effect |
|---|---|
| `info_open [page]`, `info_close`, `info_toggle` | Open (on page 0 by default), close or toggle |
| `info_page <page>` | Switch page |
| `info_refresh` | Read the snapshot again |
| `info_scroll <rows>` | Scroll |
| `power_warning_retry` | Resend a power warning that failed |

## Power warnings

`power.luau` sends `emit power_warning_<level> <charge>` to `tohru-notifications`, with the charge from 0 to 1, whenever the warning level changes:

| Level | When discharging at or below |
|---|---|
| `low` | 20% |
| `critical` | 5% |
| `empty` | 2% |
| `clear` | Charging, above 20%, or no battery |

A level is kept until the charge rises 2 percentage points above its threshold, so it does not flicker. If sending fails, `power_error` shows the error and the warning is retried on the next battery report or `power_warning_retry`. Nothing suspends or shuts down on a low battery.

## Running and tests

```sh
lua tests/info/controllers.lua
lua tests/info/power.lua
pleamar --check shell/info/entry.plm
TOHRU_SHELL=1 pleamar --scene shell/info/entry.plm --answer-as tohru-info
```

Run the Lua tests from the repository root with Lua 5.4. `python3 tests/bar/native.py --services-only --runtime <pleamar>` exercises this popup, About, network and Bluetooth on a private Sway session against fake services; it has not been re-run since the bar layout changed.

## Known limitations

When opened from the CLI, the popup closes with Escape, its close button or `tohru panel close`, not with a click outside it.
