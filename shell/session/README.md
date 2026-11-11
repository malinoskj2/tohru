# Session menu

The Lock, Sleep, Restart, Shut Down and Log Out menu. It runs as its own process, `tohru-session`, started by `bin/tohru-session-run` with `TOHRU_SHELL=1` so it can read `bar.preferred_output`.

## Files

| File | Contents |
|---|---|
| `menu.plm` | A 1×1 px anchor surface and the 304×292 px menu popup, at the top left of the output below the bar. Permissions: `run` for `loginctl lock-session` and `systemctl suspend`, and the `window`, `session.reboot`, `session.poweroff` and `session.logout` services |
| `menu.luau` | Opening, choosing, the confirmation countdown and running the actions |
| `OWNERSHIP.md` | Which programs provide the lock screen, the polkit agent and notifications |

The menu opens on the output named by `bar.preferred_output`, or else on the focused output. The anchor surface moves only while the menu is closed.

## Opening it

| From | Sends |
|---|---|
| The bar's Mooncoil mark (`bar.mooncoil`) | The bar sends `emit menu_toggle` to `tohru-session` |
| `tohru session open`, `close`, `toggle` | `emit menu_open`, `menu_close`, `menu_toggle` |

Opening resets the selection and clears the last error. Toggling an open menu closes it and cancels any pending confirmation.

## Actions

| Item | Runs | Confirmation |
|---|---|---|
| Lock Screen | `loginctl lock-session` | No |
| Sleep | `loginctl lock-session`, then `systemctl suspend` only if that exited successfully | No |
| Restart… | Pleamar's `session.reboot` | Yes |
| Shut Down… | Pleamar's `session.poweroff` | Yes |
| Log Out… | Pleamar's `session.logout` | Yes |

Tohru has no lock screen: hypridle's `lock_cmd` answers `loginctl lock-session` by starting hyprlock, and its `before_sleep_cmd` locks too. If the lock request fails, Sleep shows an error and does not suspend.

A confirmation cancels itself after 60 seconds, and a logic reload cancels it. Up and Down move the selection, Return chooses or confirms, Escape cancels a confirmation and then closes the menu.

The `status` text says only whether the request was accepted; Pleamar does not report whether the action finished.

Other requests it accepts: `emit choose <1..5>` (in menu order), `emit accept_choice`, `emit cancel_choice`, and `get` on `open`, `confirming`, `remaining`, `status` and `error`.

## Running and tests

```sh
pleamar --check shell/session/menu.plm
lua tests/session/logic.lua
```

Run the Lua test from the repository root with Lua 5.4. It runs `menu.luau` against fake services and checks confirmation, cancellation, and the Lock and Sleep command order. `tests/session/native.sh --runtime <pleamar>` runs the menu on a private session, with fake `loginctl` and `systemctl` that only log their arguments; it has not been re-run since the repository was restructured.

## Known limitations

- The menu's header always reads "NixOS".
- The menu is always opened by a request from another process, never by a click in its own, so a click outside it does not close it. Escape, the Mooncoil mark or `tohru session close` does.
