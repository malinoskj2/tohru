# Bluetooth

The Bluetooth popup: adapter power, discovery, the device list, connecting, forgetting and pairing. It runs in the bar process and uses Pleamar's `bluetooth` service, which talks to BlueZ and acts as the pairing agent.

## Files

| File | Contents |
|---|---|
| `state.plm` | The `bluetooth` service as `bt` (adapter present and powered, discovering, up to 24 devices in `bt_devices`, and the current pairing question), popup and fold facts, what the controller works out (active device, counts, minis, the connecting row, the rail's stop), the fold and water springs, device icons and the `bluetooth_*` events |
| `popup.plm` | The Frosted Tide popup: a `260 × 190 px` tank that unfolds into `280 px` rows, and the `280 × 234 px` pairing question |
| `tank.plm` | The water group (`BluetoothTank`): the active device's battery as a large thin number under `../water.wgsl`, the water shared with the network popup |
| `controller.luau` | Turns popup actions into `bluetooth.*` calls, answers pairing questions, follows the service with its own watch, and drives the Off / On / Scan rail. Takes the rail (`bar/rail.luau`) and returns `{open, close, toggle, reconcile}` |
| `*.svg` | Lucide device icons (ISC; see `LICENSE.LUCIDE`) |
| `entry.plm`, `entry.luau` | A standalone scene with a Bluetooth button, for development. It has no rail logic, so its bead does not move |

The bar's Bluetooth button is in `shell/services.plm`. It is dimmed while the adapter is off, and is hidden when the output is too narrow for it.

## The panel

- **Folded** (`260 × 190`): the active device's name with a chevron and "N connected"; clear water over frost whose line cuts the battery number, full at the top of the digits and empty at their foot; and minis along the bottom, the active device on the left and up to three more of the user's own devices on the right, each with a battery bar. The active device is the first connected one that reports a battery, else the first connected one. With nothing connected the number is a thin "On"; connected without a battery, "—".
- **Off**: a large thin "Off", no water, and a two-stop rail. A click anywhere on the rail's row turns the adapter on; a slide turns it on if it ends past halfway.
- **Unfolded** (`280 px`, as tall as its rows): the Off / On / Scan rail, which is the workspace ribbon's rail and bead (rail 3 in `bar/rail-state.plm`, with fresh switch draws as `bar/rail.luau`'s `switch` gives them). A click picks the stop under it; a slide picks the nearest stop where it ends. The chosen stop is held until the adapter reports it, or for four seconds. Below it, the user's own devices (connected first, battery or "Paired"), then Nearby with Pair, six rows to a page with "1 of 2". The forget cross shows on a hovered or selected own device.
- **Connecting**: after a connect request the row is marked until the device reports connected, the attempt runs 30 s without a pairing question, the device goes or the adapter turns off. The water rises to that row and its line crosses it, as in concept 10, then drains away when it connects.
- **Pairing**: the device and its address, the prompt, a large thin code (`482 910`) or a secret field, and Pair / Continue / Allow and Cancel.

`bt_fold` springs between folded and unfolded with the sound popup's spring (ω 14, damping ratio 0.78), and the pairing question unfolds it too. The heading moves down to become the active device's row, as the sound popup's does. Unfolding parks the bead at its left end and leaves it 180 ms later.

## How it is wired

`shell/bar.plm` includes `bluetooth/state.plm` and `bluetooth/popup.plm`, with the `bluetooth.*` permission. Declaring the service with its pairing fields is what makes Pleamar offer the pairing agent. `shell/bar.luau` calls `require("bluetooth/controller")(make_rail(3))` and uses the returned functions to keep one popup open at a time. The controller also calls `sys.watch("bluetooth", …)`, since the counts, the active device, icon kinds and the connecting row cannot be worked out in the scene. The popup surface is 304 px wide and the panel is centred under `bt_button_x`, which `services.plm` sets, kept 8 px from the output's edges.

| Event | Effect |
|---|---|
| `bluetooth_open`, `bluetooth_close`, `bluetooth_toggle` | Open (folded), close or toggle. `tohru panel open bluetooth` and `tohru panel toggle bluetooth` send the first and last |
| `bluetooth_fold_toggle` | Fold or unfold |
| `bluetooth_row <row>` | Select a device and connect or disconnect it (a nearby one pairs first) |
| `bluetooth_select <row>`, `bluetooth_connect`, `bluetooth_activate` | Select a device; connect or disconnect the selected device |
| `bluetooth_forget`, `bluetooth_forget_row <row>` | Forget the selected device, or that row's own device |
| `bluetooth_rail <stop>` | Ask for Off (0), On (1) or Scan (2). Scan with the adapter off turns it on and scans once it reports power |
| `bluetooth_rail_press <x>`, `bluetooth_rail_drag <x>`, `bluetooth_rail_release` | Click or slide on the rail, in rail pixels |
| `bluetooth_power`, `bluetooth_scan` | Turn the adapter on or off, start or stop discovery |
| `bluetooth_answer`, `bluetooth_cancel` | Answer or cancel the pairing question |
| `bluetooth_question` | Sent when a new pairing question arrives; opens the popup |
| `bluetooth_page <delta>`, `bluetooth_move <delta>` | Page the rows; move the selection (unfolding) |

Tab folds and unfolds, Down and Up move the selection, Page Up and Page Down page, Return connects or disconnects, Escape closes.

## Discovery and pairing

- Discovery started from the popup stops when the popup closes. Discovery something else started is left running, and the rail's On stop says so rather than stopping it. If stopping fails, it is tried again on the next close or logic reload.
- A pairing question opens the popup. It asks for a PIN or passkey (in a secret field, cleared after use), asks to confirm a code, shows a code to type on the device, or asks to authorize the device.
- An answer is sent only if the question on screen is still the current one. Closing the popup, or Escape, cancels an unanswered question.

Each request reports only that it was sent; the device states shown always come from the service. A request that fails shows its error in red at the foot of the panel, the pairing question, or in place of the Off panel's hint.

## Running and tests

```sh
lua tests/bluetooth/controller.lua
lua tests/bluetooth/tank.lua
cargo test --manifest-path tests/rust/Cargo.toml --test water-shader
pleamar --check shell/bluetooth/entry.plm
pleamar --scene shell/bluetooth/entry.plm --answer-as tohru-bluetooth
```

Run the Lua tests from the repository root with Lua 5.4. `tank.lua` covers what the controller works out from its watch (active device, counts, minis, icon kinds), the rail's stops, slides and held stop, the folded power rail, the connecting state, the selection following its device, and the pairing code's grouping. `tests/water-shader.rs` validates the shared water with naga and checks that both popups hand it eight values. The standalone scene talks to the real BlueZ on the system bus. `python3 tests/bar/native.py --services-only --runtime <pleamar>` runs the popup, including all five pairing prompts, against a fake BlueZ on a private bus; it has not been re-run since the bar layout changed.

## Known limitations

When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
