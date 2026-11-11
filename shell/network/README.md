# Network

The Wi-Fi popup and the bar's network icon. Both run in the bar process and use Pleamar's `network` service, which talks to NetworkManager.

## Files

| File | Contents |
|---|---|
| `state.plm` | The `network` service as `net` (online, kind, name, strength, Wi-Fi on, error, up to 64 networks in `net_rows`), popup facts and the `network_*` events |
| `popup.plm` | The 390×490 px popup: status, Wi-Fi on/off, Scan, networks seven to a page, the password form, Disconnect and Forget |
| `controller.luau` | Turns popup actions into `network.*` calls. Returns `{open, close, toggle}` |
| `entry.plm`, `entry.luau` | A standalone scene with a Network button, for development |

The bar's network button and its icons are in `shell/services.plm`. The icon shows Ethernet, Wi-Fi at four signal strengths, or Wi-Fi off, dimmed while offline.

## How it is wired

`shell/bar.plm` includes `network/state.plm` and `network/popup.plm`, with the `network.*` permission. `shell/bar.luau` calls `require("network/controller")()` and uses the returned functions to keep one popup open at a time. The popup is centred under the network button, using `net_button_x`, which `services.plm` sets, and kept 8 px from the output's edges.

| Event | Effect |
|---|---|
| `network_open`, `network_close`, `network_toggle` | Open, close or toggle. `tohru panel open network` and `tohru panel toggle network` send the first and last |
| `network_select <row>` | Connect to that network, or ask for its password first |
| `network_join`, `network_cancel` | Submit or cancel the password |
| `network_scan`, `network_power` | Scan, or turn Wi-Fi on or off |
| `network_disconnect`, `network_forget` | Disconnect the selected active network, or forget a saved one |

Choosing a saved or open network connects at once. A new secured network asks for a password in a secret field that is cleared on submit, cancel and close. Down and Up move the selection, Return activates it, Escape closes.

Each request reports only that it was sent ("Request sent — check observed status"); the status shown always comes from the service.

## Running and tests

```sh
lua tests/network/controller.lua
python3 tests/network/strength.py
pleamar --check shell/network/entry.plm
pleamar --scene shell/network/entry.plm --answer-as tohru-network
```

Run them from the repository root; the Lua test needs Lua 5.4. The standalone scene talks to the real NetworkManager on the system bus. `python3 tests/bar/native.py --services-only --runtime <pleamar>` runs the popup against a fake NetworkManager on a private bus; it has not been re-run since the bar layout changed.

## Known limitations

- Error detail is only what NetworkManager reports through Pleamar.
- There are no Ethernet, IP address, VPN or network rate details.
- When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
