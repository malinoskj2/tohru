# Bluetooth

The Bluetooth popup: adapter power, discovery, the device list, connecting, forgetting and pairing. It runs in the bar process and uses Pleamar's `bluetooth` service, which talks to BlueZ and acts as the pairing agent.

## Files

| File | Contents |
|---|---|
| `state.plm` | The `bluetooth` service as `bt` (adapter present and powered, discovering, up to 24 devices in `bt_devices`, and the current pairing question), popup facts and the `bluetooth_*` events |
| `popup.plm` | The 390×520 px popup: Power, Discover, devices six to a page, Connect / Disconnect, Forget and the pairing prompt |
| `controller.luau` | Turns popup actions into `bluetooth.*` calls and answers pairing questions. Returns `{open, close, toggle}` |
| `entry.plm`, `entry.luau` | A standalone scene with a Bluetooth button, for development |

The bar's Bluetooth button is in `shell/services.plm`. It is dimmed while the adapter is off, and is hidden when the output is too narrow for it.

## How it is wired

`shell/bar.plm` includes `bluetooth/state.plm` and `bluetooth/popup.plm`, with the `bluetooth.*` permission. Declaring the service with its pairing fields is what makes Pleamar offer the pairing agent. `shell/bar.luau` calls `require("bluetooth/controller")()` and uses the returned functions to keep one popup open at a time. The popup is centred under the Bluetooth button, using `bt_button_x`, which `services.plm` sets, and kept 8 px from the output's edges.

| Event | Effect |
|---|---|
| `bluetooth_open`, `bluetooth_close`, `bluetooth_toggle` | Open, close or toggle. `tohru panel open bluetooth` and `tohru panel toggle bluetooth` send the first and last |
| `bluetooth_select <row>` | Select a device |
| `bluetooth_connect`, `bluetooth_activate` | Connect or disconnect the selected device |
| `bluetooth_forget` | Forget the selected device |
| `bluetooth_power`, `bluetooth_scan` | Turn the adapter on or off, start or stop discovery |
| `bluetooth_answer`, `bluetooth_cancel` | Answer or cancel the pairing question |
| `bluetooth_question` | Sent when a new pairing question arrives; opens the popup |

Down and Up move the selection, Return connects or disconnects, Escape closes.

## Discovery and pairing

- Discovery started from the popup stops when the popup closes. Discovery something else started is left running. If stopping fails, it is tried again on the next close or logic reload.
- A pairing question opens the popup. It asks for a PIN or passkey (in a secret field, cleared after use), asks to confirm a code, shows a code to type on the device, or asks to authorize the device.
- An answer is sent only if the question on screen is still the current one. Closing the popup, or Escape, cancels an unanswered question.

Each request reports only that it was sent ("Request sent — check observed status"); the device states shown always come from the service.

## Running and tests

```sh
lua tests/bluetooth/controller.lua
pleamar --check shell/bluetooth/entry.plm
pleamar --scene shell/bluetooth/entry.plm --answer-as tohru-bluetooth
```

Run the Lua test from the repository root with Lua 5.4. The standalone scene talks to the real BlueZ on the system bus. `python3 tests/bar/native.py --services-only --runtime <pleamar>` runs the popup, including all five pairing prompts, against a fake BlueZ on a private bus; it has not been re-run since the bar layout changed.

## Known limitations

When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
