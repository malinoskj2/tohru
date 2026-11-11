# Private session menu entry point

This is an independent development scene, not root imports or production activation.
Scene-only changes reuse the compiled runtime. After coordinator GPU allocation,
on private compositor and private buses only:

```sh
pleamar --scene shell/session/menu.plm --answer-as tohru-session --no-hud
```

Run it in its own dedicated process. Never launch it against the user's display
or buses. See OWNERSHIP.md for required lifetimes.

| Native name | Request | Observable status |
| --- | --- | --- |
| tohru-session | `emit menu_open`, `emit menu_close`, `emit menu_toggle` | `open`, `confirming`, `remaining`, `status`, `error` |
| tohru-session | `emit choose N` (1 lock, 2 sleep, 3 restart, 4 shutdown, 5 logout) | 3–5 open confirmation; `emit accept_choice` or `emit cancel_choice` |

Tohru has no lock screen. Lock runs `loginctl lock-session`, which hypridle's
`lock_cmd` answers by starting hyprlock. Sleep runs `loginctl lock-session` and,
once that command exits successfully, `systemctl suspend`; if the lock request
fails, it reports the error and does not suspend. hypridle's `before_sleep_cmd`
also locks. The scene declares exactly those two commands in `permissions { run: … }`.

Restart, Shut Down and Log Out use the native `session.*` services. They spawn
systemctl/loginctl, do not observe child exit or asynchronous completion, and may
fail after request acceptance. Logout uses native terminate-session when
XDG_SESSION_ID is available and otherwise native terminate-user fallback. Text
status says request acceptance, never power success. Tohru has no polkit agent;
hyprpolkitagent is the session's agent.

CPU validation:

```sh
pleamar --check shell/session/menu.plm
lua tests/session/logic.lua
```

The test executes the actual Lua-compatible entry script with fake native boundaries
and a deterministic timer. It checks confirmation expiration/cancellation, the
Lock and Sleep command order, Sleep's refusal to suspend after a failed lock
request. It is not a native
Luau/compositor/helper integration test. Native popup click grab/Escape and CLI
idle Escape behavior are accepted.

`menu_toggle` closes an open menu and cancels any confirmation; opening resets
selection and errors. The root owns its visible menu control: this feature scene
has only a 1px transparent anchor surface, without a duplicate visible button.

After coordinator GPU allocation, `tests/session/native.sh --runtime /absolute/pleamar`
runs private menu protocol checks. Its `systemctl` and `loginctl` are
fakes that only append their arguments to `power.log` in the evidence directory,
and the run asserts Lock and Sleep produce `loginctl lock-session` and
`loginctl lock-session`, `systemctl suspend`. `--notifications` adds the
notification workload on private buses and an owned null audio sink.
