# Polkit agent

Tohru's polkit authentication agent and its dialog, the Frosted Vessel: a glass card whose thin numeral sits in clear water over drifting frost, cut by the waterline. It runs as its own process, `tohru-polkit`, and is off until `polkit.agent` is set (see [docs/configuration.md](../../docs/configuration.md)).

## Files

| File | Contents |
|---|---|
| `polkit.plm` | The scene: a full-screen overlay layer (keyboard exclusive, a faint scrim that takes the pointer) that exists only while a request or its outcome is shown, and a centred `popup` with adaptive light and dark glass from `../panel-ink.plm` |
| `polkit.luau` | Reads `polkit.agent` and `bar.preferred_output`, and starts the controller |
| `controller.luau` | Turns the native agent's reports into the card's states, carries the answer to the agent and clears it, and handles the identity picker. Returns `{start, report}` |
| `../water/vessel.wgsl` | The water: a group shader shared with the lock screen (see below) |

The agent itself is Pleamar's `polkit` service, extended by `runtime/pleamar/patches/0030-polkit-agent.patch`.

## How a request goes

polkitd calls `BeginAuthentication` on the agent. The agent turns away calls from anyone but the current owner of `org.freedesktop.PolicyKit1`, lists the identities polkit would take (unix users, and the members of unix groups, one each, with `superuser`, `wheel` or the primary group), picks the caller's own identity if it is among them and otherwise the first, and queues the request behind any other. For the request being attended it reaches polkit's setuid helper (the `/run/polkit/agent-helper.socket` socket of polkit 126 and later, otherwise `polkit-agent-helper-1`) for the chosen user and cookie, and relays the helper's conversation: `PAM_PROMPT_ECHO_OFF`/`ON` become the prompt, `PAM_TEXT_INFO` becomes `info` and `PAM_ERROR_MSG` becomes `warning`. The helper alone talks to PAM and tells polkitd; the agent never holds privileges.

| Card state | When | Numeral | Water |
|---|---|---|---|
| Request | A request is asking, nothing typed | Tries left, `03 ATTEMPTS` | Low (24 %) |
| Typing | Characters in the field | Their count, `08 CHARS` | Rises 4 % a character, to 64 % |
| Checking | Submitted, the helper has not answered | Unchanged | 40 % |
| Wrong password | The helper said `FAILURE`, tries remain | `02 TRIES LEFT`, red | Red, 44 %, sloshing |
| Success | The request ended granted | `✓ UNLOCKED`, green; "Authenticated as …", "Continuing…" | Green, 78 % |
| Failed | Three wrong answers, or the helper went away | `00 TRIES LEFT`; "Authentication failed" | Red, 30 % |
| Cancelled | Cancel, Escape, or polkit cancelled | `— DISMISSED`; "Authentication cancelled" | Quiet, 12 % |

Success shows for 1.1 s, cancellation for 0.9 s and failure for 1.6 s; a request queued behind is shown after it. The footer says `N admins · Tab to unfold`, or PAM's warning, PAM's info, "Incorrect password · N attempts remaining", "Checking…" and how many requests wait.

**Identity picker.** Tab, or a click on the identity chip, unfolds the card into the full message, the command line (`command_line` or `program` from the request's details), the "Authenticate as" list with group and uid, and a rail whose lit segment sits at the chosen row. Up and Down move, Return or a click chooses, Escape or Tab folds. Choosing another identity lets go of the helper and asks again for the new one without spending a try.

**The requesting app** is the nearest ancestor of polkit's subject process that is not a shell or a pass-through tool (`pkexec`, `sudo`, `env`…), so `pkexec` in Alacritty reads "Alacritty".

**Keys.** Return submits, Escape cancels (or folds the picker), Tab unfolds.

## Password field

The field has the same states as the lock screen's: rest (ink 8 %, hairline 12 %), typing (ink 12 %, hairline 22 %, a character count at the right), wrong (red fill and hairline), success (green, with the result in place of the field) and cancelled (dimmed). It is a `secret: true` input marked `agent: no`, so `pleamar --say … get` and `describe` never return it (patch 0014) and an automation agent cannot type into it or press Authenticate.

## Security

- **No success without the helper.** The agent ends a request granted only on the helper's own `SUCCESS` line, never after a cancellation; the card shows success only when the agent reports a request that ended granted. polkitd itself only authorises after the setuid helper has told it so, whatever the agent answers.
- **Failure is the default.** A helper that goes away, cannot be reached or answers anything else fails the request. If the thread attending a request panics, a guard fails it, and the state lock is recovered rather than left poisoned. If the process dies, its bus connection goes, polkitd drops the agent and the helper's input closes: the request fails.
- **The password** goes from the field to `polkit.answer` and straight to the helper, up to its first line break; the Rust copy is overwritten with zeroes once written, and the field is cleared on submit, cancel, identity change, the request ending, and any typing while nothing is asked. It is never logged and never in what the scene or `describe` is told. The Luau string that carried it is not overwritten; it is released to Luau's collector.
- **Only polkit may ask.** `BeginAuthentication` and `CancelAuthentication` from any sender other than the owner of `org.freedesktop.PolicyKit1` are refused with `NotAuthorized`, so another program on the system bus can neither put the dialog up nor take it down.
- **Keyboard.** The overlay layer is created for each request, exclusive from its first commit, and destroyed when the card hides (its output is emptied), so there is never a moment it is mapped without the keyboard, and it sits above windows and full-screen windows. If the field loses focus inside the scene, it takes it back. The scrim takes the pointer, and the surface is marked `captures: hidden` where the compositor supports it.
- `PLEAMAR_POLKIT_HELPER` points the agent at a stand-in helper for tests. A stand-in cannot grant anything: polkitd only listens to the real helper.

## Running and tests

```sh
cargo test --manifest-path tests/rust/Cargo.toml --test polkit --test water-vessel-shader
pleamar --check shell/polkit/polkit.plm
TOHRU_SHELL=1 pleamar --scene shell/polkit/polkit.plm --answer-as tohru-polkit
```

The agent's own tests run with the patched runtime (`cargo test --lib tohru_polkit` in the prepared source, and in the Nix build): a stand-in helper on a socket and a stand-in polkitd on a private `dbus-daemon` cover success, a wrong answer costing a try, three wrong answers, polkit cancelling, the scene cancelling, queued requests and cancelling one still queued, switching identity, a helper that dies, nothing but the helper's exact `SUCCESS` granting, calls from another bus client refused, the agent's connection going away mid-request, the panic guard, and the password being zeroed and never reaching the scene.

`python3 tests/polkit/stand-in.py RUNDIR` runs a stand-in polkitd and helper on a private bus (password `right`) for trying the dialog where no polkitd runs; it writes the environment the agent needs to `RUNDIR/env` and takes `begin COOKIE` and `cancel COOKIE` on `RUNDIR/control`.

## Shared water

`shell/water/vessel.wgsl` is a group shader for any card whose numeral sits in water: it refracts and frost-blurs whatever the group holds below a meniscus across the whole card. Its values are `level, slosh, pk, quiet, clock, tint, unfold, corner` (`tint` −1 green, 0 lavender, +1 red). WGSL has no includes, so the sound, network and OSD shaders keep their own copies of the same noise, wave and plume functions; the lock screen should use this one rather than another copy.

## Trying it in the nested Hyprland

The nested Hyprland's seat has no keyboard of its own. `wtype` makes a one-shot keyboard per call, and a layer that maps while no keyboard exists is not given focus, so `wtype` gives misleading results here. Keep one virtual keyboard open for the whole run instead (as `tests/launcher/keyboard.c` does). Verified that way: four requests in a row each take the keyboard, nothing typed into the dialog reaches the focused terminal, and the terminal gets keys again once the card hides.

## Differences from the mockup

- No application icon in the header, only the app's name.
- The message is polkit's own, often longer than the mockup's short one, and wraps to two lines.
- The field is emptied after a wrong password; the mockup keeps the dots.
- The identity list shows whatever identities polkit offers (two on a single-user machine: the user and root).

## Known limitations

- No application icon in the header; the app's name only.
- A prompt asked with `PAM_PROMPT_ECHO_ON` is still masked.
- Changing `polkit.agent` takes effect when `tohru-polkit` restarts.
- Only Hyprland and other layer-shell compositors; `captures: hidden` is honoured by pleamar-wm only.
