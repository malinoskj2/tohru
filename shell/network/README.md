# Network

The bar's network icon and the network popup. Both run in the bar process and use Pleamar's `network` service, which talks to NetworkManager.

## Files

| File | Contents |
|---|---|
| `state.plm` | The `network` service as `net` (online, kind, name, strength, Wi-Fi on, error, up to 64 networks in `net_rows`), native icons, the fold and view springs, popup facts and `network_*` events |
| `popup.plm` | The adaptive-glass popup: the `272 × 84 px` compact pair of Wi-Fi and Ethernet tiles, unfolding into the `288 px` segmented `Wi-Fi \| Ethernet` header over the chosen view |
| `controller.luau` | Turns popup actions into `network.*` calls and manages fold, views, tile focus, the power switch, selection, pagination and password lifecycle. Takes the power switch's rail (`bar/rail.luau`) and returns `{open, close, toggle}` |
| `scan.svg`, `lock.svg` | Lucide icons for scanning and secured networks (ISC; see `LICENSE.LUCIDE`) |
| `entry.plm`, `entry.luau` | A standalone scene with a Network button, for development |

The bar's network button and its Wi-Fi/Ethernet icons are in `shell/services.plm`. The icon shows Ethernet, Wi-Fi at four signal strengths, or Wi-Fi off, dimmed while offline.

## Layout

- **Compact** (`272 × 84 px`): two `122 × 64 px` interface tiles with 10 px around them. Wi-Fi shows its signal glyph, `On` or `Off`, and the network's name, `Not connected` or `Disabled`; Ethernet shows `Off` and `Unplugged`, or the connection's name. The connected tile is brighter and carries the glowing 4 px active mark. There is no percentage anywhere. Clicking a tile unfolds into its view.
- **Unfolded** (`288 px`): the `Wi-Fi | Ethernet` segmented header, whose thumb glides between the segments while the views crossfade and the panel's height follows; the fold chevron returns to the tiles.
- **Wi-Fi view**: the power switch in the header, a two-stop ribbon rail with its bead (rail 8 of `bar/rail-state.plm`, drawn with `bar/ribbon.wgsl`: Off at its left end, On at its right; a click toggles, a slide picks the nearest stop; the chosen stop is held until the service reports it, or for four seconds), and the scan button. Below, nearby networks strongest first (7 to a page), each with its leading signal arc, `Saved` and the lock, the active one with its mark and Disconnect, a selected saved one with Forget. A new secured network opens the password card. The footer shows the last request, or `Connected to …`, `Not connected` or `Wi-Fi is off`.
- **Ethernet view**: the service reports only `online`, `kind` and `name` for a wired link, so the view shows the connection's name and `Connected` with `No further details available`, or `Ethernet`, `Not connected` and `No wired connection reported`.

## How it is wired

`shell/bar.plm` includes `network/state.plm` and `network/popup.plm`, with the `network.*` permission. `shell/bar.luau` calls `require("network/controller")(make_rail(8))` and uses the returned functions to keep one popup open at a time. The popup surface is 312 px wide and centred under `net_button_x`, kept 8 px from the output's edges, and opens folded (`net_unfolded = false`). `net_fold` springs between compact and unfolded and `net_view_k` between the Wi-Fi (`net_view = 0`) and Ethernet (`1`) views (ω 14, damping ratio 0.78, matching `sound_fold`).

| Event | Effect |
|---|---|
| `network_open`, `network_close`, `network_toggle` | Open, close or toggle. `tohru panel open network` and `tohru panel toggle network` send the first and last |
| `network_open_view <view>` | Unfold into the Wi-Fi (`0`) or Ethernet (`1`) view, as a click on that tile does |
| `network_view <view>` | Switch the unfolded view, as the segments do. Leaving Wi-Fi cancels password entry |
| `network_fold` | Fold back to the two tiles |
| `network_focus_move <delta>` | Move the keyboard between the compact tiles |
| `network_select <row>` | Connect to that network, or open the Wi-Fi view and ask for its password first |
| `network_join`, `network_cancel` | Submit or cancel the password |
| `network_scan`, `network_power` | Scan, or turn Wi-Fi on or off |
| `network_power_press <x>`, `network_power_drag <x>`, `network_power_release` | The power switch's click or slide, `x` along its rail |
| `network_observed` | Sent when the reported Wi-Fi power changes, so the switch's bead follows it |
| `network_disconnect`, `network_disconnect_active` | Disconnect the selected or active Wi-Fi network |
| `network_forget`, `network_forget_row <row>` | Forget the selected or row-targeted saved network |

Choosing a saved or open network connects at once. A new secured network opens the Wi-Fi view and asks for a password in a secret field that is cleared on submit, cancel, fold, a switch to Ethernet and close.

Compact, `Tab`, `Left` and `Right` move between the tiles (a ring shows the keyboard's tile) and `Return` or `Space` opens it. Unfolded, `Tab` switches views, `Down`/`Up` move the Wi-Fi selection (from the tiles they open the Wi-Fi view), `Page Up`/`Page Down` page the list and `Return` activates. `Escape` cancels password entry, then folds back to the tiles, then closes.

Each request reports only that it was sent ("Request sent — check observed status"); the status shown always comes from the service.

## Running and tests

```sh
lua tests/network/controller.lua
python3 tests/network/strength.py
pleamar --check shell/network/entry.plm
pleamar --check shell/bar.plm
pleamar --scene shell/network/entry.plm --answer-as tohru-network
```

Run them from the repository root; the Lua test needs Lua 5.4. The standalone scene cannot require `bar/rail` from outside its folder, so its power switch's bead stays at Off; the switch itself works. It talks to the real NetworkManager on the system bus. `python3 tests/bar/native.py --services-only --runtime <pleamar>` runs the popup against a fake NetworkManager on a private bus.

## Known limitations

- Error detail is only what NetworkManager reports through Pleamar.
- There are no IP address, VPN or network rate details.
- When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
