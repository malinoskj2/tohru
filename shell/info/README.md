# System information and power warnings

A read-only System Information popup, a glass capsule whose overview of four tiles unfolds into seven pages, and the battery warnings sent to the notification daemon. Both run in the bar process. The popup opens from the bar's information button, which `bar.info_button` turns on, from About's More Info, or with `tohru panel toggle info`.

## Files

| File | Contents |
|---|---|
| `state.plm` | Popup facts (`info_visible`, `info_unfolded`, `info_page`), each page's hero texts (`info_big.N`, `info_unit.N`, `info_note.N`), the tile captions, the visible `info_rows` (8) and cached `info_cache` (256) models, the `clock` service, the `info_*` events and the `info_fold` spring |
| `power-state.plm` | Battery facts (`power_present`, `power_known`, `power_percent`, `power_charging`), `power_warning_level` and `power_error` |
| `popup.plm` | The glass capsule, centred horizontally 37 px from the top, and its keys. It grows from the 312×330 px overview to the 456×432 px page |
| `frost.plm` | The frosted, drifting palette light behind the glass: `network/water.wgsl` at rest unfolded, with no water or numeral |
| `overview.plm` | The folded "System Overview" bento: System memory, Storage, Generations and Power tiles, each a large thin number with a caption |
| `page.plm` | The unfolded page: "‹ Overview", title, refresh, the hero tile (large thin number, unit, summary), up to eight rows, and the footer with row paging |
| `rail.plm` | The seven-stop page rail (Sys, Disp, Time, Users, Disk, Gen, Pwr): rail 3 of `bar/rail-state.plm`, drawn with `bar/ribbon.wgsl` |
| `controller.luau` | Builds each page's rows and the overview from a `tohru_info` snapshot and moves the rail. Returns `{open, overview, close, toggle, refresh, unfold, fold, step}` |
| `power.luau` | Watches `battery` and sends power warnings |
| `refresh.svg` | The refresh icon |
| `entry.plm`, `entry.luau` | A standalone scene with an information button, for development |

## Folded and unfolded

The popup opens folded on the overview. Choosing a tile, a stop on the rail, Return or Tab unfolds a page: the chosen tile grows into the page's hero while the other tiles recede and fade, and the page's rows fade in. Pages without a tile (Displays, Date & Time, Users) grow from their stop on the rail. "‹ Overview" or Tab folds back. `info_fold` springs between the two (ω 14, damping ratio 0.78, as the network and sound popups' fold).

The rail's bead marks the chosen page and glides between stops with the workspace ribbon's draws. Folded, it sits under the tiles with the page names below it; while unfolding it fades there and comes back under the page's header. On light glass an ink track is drawn under the ribbon, whose own track is white.

| Key | Folded | Unfolded |
|---|---|---|
| Left, Right | Choose the previous or next page | Turn to the previous or next page |
| Return, Space | Unfold the chosen page | |
| Tab | Unfold the chosen page | Fold back to the overview |
| Up, Down | | Scroll one row |
| Page Up, Page Down | | Scroll eight rows |
| Escape | Close | Close |

## Pages

| Page | Hero | Rows |
|---|---|---|
| 0 System | Memory in GiB; available memory and OS | OS, version, host name, kernel, processor, graphics, memory, available memory, uptime, active generation, configuration revision (`/run/current-system/configuration-revision`) |
| 1 Displays | Number of outputs; the first one's name and size | Output name and logical size, if the controller is given an outputs function. The bar gives it none, so this page reads Unknown |
| 2 Date & Time | The time, and the date, from the `clock` service | Time zone, which reads Unknown |
| 3 Users | Number of accounts; the current one | Accounts from `/etc/passwd` (up to 256), the current one first, with UID, description, home and shell |
| 4 Storage | Total block device capacity in TB or GB; device and mount counts | Block devices (capacity, model, removable) and mounts (source, filesystem, read-only). Free space reads Unknown |
| 5 Generations | The active generation; how many there are | NixOS system generations (`/nix/var/nix/profiles/system-*-link`), with the one matching `/run/current-system` marked active |
| 6 Power | Battery percentage and charging state, live | Battery presence, charge, state, the warning thresholds and the current warning, from the `battery` service |

The overview tiles show pages 0, 4, 5 and 6's heroes, with captions: the processor's short name and kernel version, the disk and mount counts, the number of generations, and the charging state.

Opening the overview reads a snapshot; so do opening pages 0 and 3 to 5 and Refresh. Unfolding the page the overview has already read does not read again. Rows are cached, so scrolling and logic reloads do not read again. A fact the snapshot lacks reads Unknown (a hero reads "—"); when some sources reported problems the overview says "Partial facts" and the page says how many.

## How it is wired

`shell/bar.plm` includes `info/state.plm` and `info/popup.plm`, and `bar/rail-state.plm`, which holds rail 3. Its permissions include `battery` and `say:tohru-notifications`. `shell/bar.luau` calls `require("info/controller")(tohru_info.snapshot, nil, make_rail(3))` and `require("info/power")()`.

| Event | Effect |
|---|---|
| `info_open [page]` | Open: on the folded overview, or with a page, unfolded on that page |
| `info_close`, `info_toggle` | Close, or toggle the overview |
| `info_page <page>` | Unfold that page |
| `info_unfold`, `info_fold`, `info_fold_toggle` | Unfold the chosen page, fold back, or either |
| `info_step <delta>` | Choose (folded) or turn to (unfolded) the previous or next page |
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
lua tests/info/overview.lua
lua tests/info/power.lua
pleamar --check shell/info/entry.plm
TOHRU_SHELL=1 pleamar --scene shell/info/entry.plm --answer-as tohru-info
```

Run the Lua tests from the repository root with Lua 5.4. `python3 tests/bar/native.py --services-only --runtime <pleamar>` exercises this popup, About, network and Bluetooth on a private Sway session against fake services; it has not been re-run since the bar layout changed.

## Known limitations

- When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
- The frosted backdrop is the network popup's `water.wgsl`; if that shader moves, `frost.plm` and About's popup follow it.
