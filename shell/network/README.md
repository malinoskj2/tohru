# Network

The bar's network icon and the network popup (Atlas Water Chamber). Both run in the bar process and use Pleamar's `network` service, which talks to NetworkManager.

## Files

| File | Contents |
|---|---|
| `state.plm` | The `network` service as `net` (online, kind, name, strength, Wi-Fi on, error, up to 64 networks in `net_rows`), native icons, fold/water spring state, popup facts and `network_*` events |
| `popup.plm` | The adaptive-glass popup: `272 × 200 px` compact Atlas Water Chamber (Wi-Fi & Ethernet interface tiles over the refracted signal water chamber), unfolding smoothly into the `288 px` segmented `Wi-Fi \| Ethernet` header, nearby list (7 to a page), password card, Disconnect and Forget |
| `water.plm`, `water.wgsl` | Clear partly-frosted water (`NetworkWater`) drawn from `net_water`, `net_big`, `net_fold`, `net_quiet` and `pk`: refracts and blurs the signal numeral in compact mode and transitions into drifting workspace-palette light plumes behind the unfolded list |
| `controller.luau` | Turns popup actions into `network.*` calls and manages fold, selection, pagination and password lifecycle. Returns `{open, close, toggle}` |
| `scan.svg`, `lock.svg` | Lucide icons for scanning and secured networks (ISC; see `LICENSE.LUCIDE`) |
| `entry.plm`, `entry.luau` | A standalone scene with a Network button, for development |

The bar's network button and its Wi-Fi/Ethernet icons are in `shell/services.plm`. The icon shows Ethernet, Wi-Fi at four signal strengths, or Wi-Fi off, dimmed while offline.

## How it is wired

`shell/bar.plm` includes `network/state.plm` and `network/popup.plm`, with the `network.*` permission. `shell/bar.luau` calls `require("network/controller")()` and uses the returned functions to keep one popup open at a time. The popup surface is 312 px wide and centred under `net_button_x`, kept 8 px from the output's edges, and opens folded (`net_unfolded = false`). `net_fold` springs between compact and unfolded (ω 14, damping ratio 0.78, matching `sound_fold`), and `net_water` springs smoothly to live Wi-Fi signal strength (`1.0` when wired, `0.0` when offline).

| Event | Effect |
|---|---|
| `network_open`, `network_close`, `network_toggle` | Open, close or toggle. `tohru panel open network` and `tohru panel toggle network` send the first and last |
| `network_fold_toggle` | Fold or unfold between the compact Atlas Water Chamber and the nearby list |
| `network_select <row>` | Connect to that network, or unfold and ask for its password first |
| `network_join`, `network_cancel` | Submit or cancel the password |
| `network_scan`, `network_power` | Scan, or turn Wi-Fi on or off |
| `network_disconnect`, `network_disconnect_active` | Disconnect the selected or active Wi-Fi network |
| `network_forget`, `network_forget_row <row>` | Forget the selected or row-targeted saved network |

Choosing a saved or open network connects at once. A new secured network unfolds the panel and asks for a password in a secret field that is cleared on submit, cancel, fold and close. `Tab` folds/unfolds, `Down`/`Up` unfold and move selection, `Page Up`/`Page Down` page the list, `Return` activates, and `Escape` cancels password entry or closes the popup.

Each request reports only that it was sent ("Request sent — check observed status"); the status shown always comes from the service.

## Running and tests

```sh
lua tests/network/controller.lua
python3 tests/network/strength.py
pleamar --check shell/network/entry.plm
pleamar --check shell/bar.plm
pleamar --scene shell/network/entry.plm --answer-as tohru-network
```

Run them from the repository root; the Lua test needs Lua 5.4. The standalone scene talks to the real NetworkManager on the system bus. `python3 tests/bar/native.py --services-only --runtime <pleamar>` runs the popup against a fake NetworkManager on a private bus.

## Known limitations

- Error detail is only what NetworkManager reports through Pleamar.
- There are no IP address, VPN or network rate details.
- When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
