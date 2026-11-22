# Architecture

## Pleamar

[Pleamar](https://github.com/k4ditano/pleamar) describes itself as "a desktop shell language and runtime where animation does not depend on logic". A scene (`.plm`) declares surfaces, shapes, text, popups, facts, events and the system services it may use. Luau scripts react to events and update facts. WGSL shaders draw custom effects. Pleamar provides the native services Tohru uses: audio over the PulseAudio protocol, NetworkManager, BlueZ, MPRIS media, application discovery and launch, workspaces, battery, notifications and session actions.

Tohru is a set of Pleamar scenes plus a small set of runtime patches and Luau bindings.

## Processes

Each role is its own `pleamar` process, started by a wrapper in `bin/`. Each can restart without the others.

| Role name | Scene | Wrapper | Notes |
|---|---|---|---|
| `tohru` | `shell/bar.plm` | `tohru-run` | Bar, all popups, volume OSD and screen corners in one process. Sets `TOHRU_SHELL=1` |
| `tohru-session` | `shell/session/menu.plm` | `tohru-session-run` | Session menu, on the bar's output. Sets `TOHRU_SHELL=1` to read it |
| `tohru-launcher` | `shell/launcher/entry.plm` | `tohru-launcher-run` | Launcher: a full-screen scrim layer named `tohru-launcher-backdrop` under a full-screen layer with the deck (a frosted tab sidebar beside a clearer grid stage) |
| `tohru-notifications` | `shell/notifications/notifications.plm` | `tohru-notifications-run` | Owns `org.freedesktop.Notifications`; must be the only notification daemon. Keeps whether something arrived unseen (saved across restarts) and Do Not Disturb (not saved) |
| `tohru-settings` | `shell/settings/settings.plm` | `tohru-settings-run` | Settings window, started on demand. Sets `TOHRU_SHELL=1` |
| `tohru-lock` | `shell/lock/lock.plm`, else `shell/lock/plain.plm` | `tohru-lock` | The lock screen, started by the on-demand user unit, an external idle manager or `tohru lock`. Sets `TOHRU_SHELL=1` and `TOHRU_LOCKER=1`. Exits when unlocked |
| `tohru-greeter` | `shell/greeter/greeter.plm`, else `shell/greeter/plain.plm` | `tohru-greeter` | Pre-session `greetd` login screen, started inside a minimal compositor by `services.tohru-greeter`. Sets `TOHRU_SHELL=1`, `TOHRU_GREETER=1`, `PLEAMAR_NO_PORTAL=1`, `PLEAMAR_GENERIC=1`. Speaks `GREETD_SOCK` natively and exits on `start_session` |
| `tohru-polkit` | `shell/polkit/polkit.plm` | `tohru-polkit-run` | Registers as the session's polkit authentication agent when `polkit.agent` is true, and shows the Frosted Vessel dialog on a keyboard-exclusive overlay layer. Talks to PAM only through polkit's helper. Sets `TOHRU_SHELL=1` |

The wrappers run `pleamar --scene <scene> --no-hud --stall 0 --answer-as <role>`. The role name is what other processes and the CLI address.

Standalone integration is described in [standalone.md](standalone.md). Wallpaper
ownership is off by default; its foreground supervisor owns only its child
mpvpaper processes and private session sockets. Optional swayidle ownership is
independent of the shell. Session Lock/Sleep invokes `tohru-session-action`, which
starts the on-demand systemd locker and requires compositor-confirmed readiness
before requesting suspend. PAM/native unlock authority and plain-style recovery
remain in the existing locker. There is no screenshot tool. Polkit stays opt-in.
The shared service specification under `share/tohru/session` is consumed by both
Home Manager and standalone generation.

## Lock screen

`tohru-lock` locks with ext-session-lock-v1 and checks the password with PAM. The service is the first of `tohru-lock`, `pleamar` and `hyprlock` in `/etc/pam.d`, else `login`. Failures keep the session locked:

- One locker per display/session (`flock` on a private hashed runtime path). Missing required flock is an explicit failure. The compositor also refuses a second lock (exit 3, not retried).
- Before locking, the wrapper runs `pleamar --check` on the styled scene, which compiles its shaders. A scene that fails locks with `plain.plm`, which has no shaders and no screen capture.
- Any other exit than 0 (a crash, a kill, exit 75 for a quit while still locked) starts the plain scene again. The compositor keeps the session locked meanwhile; Hyprland needs `misc:allow_session_lock_restore = true` to let the new locker take over.
- Only the native side reads the password field. Luau sees bullets and a verdict (`lock_auth`: 0 idle, 1 checking, 2 accepted, 3 rejected, 4 PAM unavailable). The password is zeroed once PAM has answered.
- Unlocking needs a PAM success for the lock cycle that is still held; a permit does not carry over to the next lock. `--say` requests to a locker are refused, it does not hot-reload, and it does not relaunch itself when its binary changes.
- A monitor plugged in while locked gets its own face. A stalled locker leaves the compositor's lock in place.

The styles are parts in `shell/lock/styles/`, all in one scene and picked by the `style` fact (`lock.style`; 0 is plain). They draw a blurred capture of the screen taken before locking, never a live one. Limits: each face lays the style out in a 1920 × 1080 box centred on its monitor, unscaled, so a smaller monitor crops it and a larger one shows more backdrop; the capture comes from one output (`bar.preferred_output`, else the focused one) and every face shows it; the bar drawn on the lock is a still copy with the workspaces as they were at locking.

## How the parts talk

Processes send each other requests with `pleamar --say <role> "<request>"`, or from Luau with `sys.call("say", "<role>", "<request>")`. A request is `emit <event> [args]` to fire an event, `get <name>` to read a text value, `text <name> <value>` to set one, or `quit`. A scene may only `say` to roles listed in its `permissions` (`say:<role>`).

- The `tohru` CLI is a shell script that turns each command into a `--say` request. For example `tohru launcher toggle` sends `emit launcher_toggle` to `tohru-launcher`, and `tohru status --json` sends `get settings_status` to `tohru`.
- The bar sends `emit menu_toggle` to `tohru-session` when its session button is used, and power warnings to `tohru-notifications`.
- The bell: the bar sends `emit notices_seen` (left click), `emit notices_dnd 2` (right click, toggle) and, when it starts, `emit notices_report` to `tohru-notifications`, which answers each change with `emit notices_state N` to `tohru` (1 unread, 2 Do Not Disturb, added).
- Settings sends `emit config_reload` and `setting_*` events to `tohru` after each accepted edit.
- The launcher sends `emit launcher_bar N` to `tohru` (0 closed, 1 open, 2 open and mapped, 3 closing), and the bar tints sky and rises above the launcher from it. The bar sends `emit launcher_report` to `tohru-launcher` when it starts and every second while the launcher is not closed; the launcher answers with its state, and a bar that cannot reach it takes it as closed.

Requests report that Pleamar accepted them, not that the action finished. If no Tohru process runs on the caller's display, Pleamar may route the request to one on another display, with a warning.

## Runtime patches

`runtime/pleamar/pin.json` pins Pleamar 0.7.27 at `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`. Both `runtime/pleamar/prepare.sh` and `nix/default.nix` apply the patches in `runtime/pleamar/patches/` in name order:

| Patch | Change |
|---|---|
| `0001-shell-settings` | Adds the `tohru-shell` library as a dependency and installs the `tohru_settings` Luau global, only when `TOHRU_SHELL=1` |
| `0005-launcher-namespace` | Adds an optional static `layer_namespace` surface property. The launcher uses it so the Hyprland plugin can match its layer. Without it, layers are named `pleamar`. It also makes Pleamar's lookup of where a layer sits on Hyprland match the layer's own namespace, so glass on a named layer captures the right part of the screen |
| `0006-info-binding` | Installs the `tohru_info` Luau global |
| `0010-key-rule-order` | Keys that arrive in the same frame as a key that triggers a rule, such as typing right after Ctrl+F, are handled after the rule has acted |
| `0011-focus-round` | A keyboard leave and enter in the same frame, such as a popup grab moving focus between Tohru's own surfaces, no longer counts as losing focus |
| `0012-popup-root-parent` | Until the pointer enters one of its surfaces, a scene's popups hang from its main surface instead of whichever surface mapped first (such as a screen corner on another monitor) |
| `0013-text-font-features` | A text's `family:` may name OpenType features after a colon (`"SF Pro Text:tnum"`). The bar's clock uses it for j2bar's tabular numerals |
| `0014-secret-get` | `--say … get` answers `? secret input cannot be read` for the text of a `secret: true` field, as `describe` and `watch` already withhold it. Without it any process of the user could read the Wi-Fi password while it is typed |
| `0015-watch-patience` | Time spent starting a service in `sys.watch`, such as asking the system bus whether BlueZ or NetworkManager is there, no longer counts against a handler's 2 s limit, as with `sys.call` and `sys.ask`. Without it a slow bus at login cut off the bar's whole script, leaving the bar empty |
| `0016-field-arrow-rules` | Plain Left and Right go to an active `on key` rule of the scene before the focused field moves its caret, as j2bar's launcher grid has them; with Shift they still select text |
| `0017-lens-paint-without-waiting` | A popup with a `lens` body and moving contents paints on every refresh. Before, it skipped the round while a capture was on its way and painted the capture the moment it arrived, which with Hyprland answering the frame one refresh late lost one round in three (39 a second at 60 Hz). The lens keeps a third canvas so the one a capture was asked against is never painted over, and a frame that has been on its way for three quarters of a refresh no longer holds back the next one, once per unanswered frame |
| `0018-popup-keyboard-before-grab` | A popup opened by a click commits its parent layer's pending keyboard change before asking for the grab. A parent with frames of its own committed `keyboard: exclusive while …` after its queued frames, so it often reached Hyprland after the popup's grab; Hyprland then moved the keyboard back to the bar and ended the grab, and the panel closed the instant it opened |
| `0019-apps-j2bar-catalog` | The `apps` catalog leaves out entries whose `TryExec` program is not installed, and entries whose `OnlyShowIn`/`NotShowIn` exclude the desktops in `XDG_CURRENT_DESKTOP`, as j2bar's catalog did. Each record also says, in `icon_found`, whether its icon would load, so the launcher can draw j2bar's placeholder for one that would not, and carries every `Name[…]` (`names`) and `Exec=` as written (`exec_line`), which j2bar's search matched |
| `0020-backdrop-bright-share` | Adds an optional `bright: fact` popup property. After each capture its lens takes in, the card counts the cells of a 16 × 16 grid over the popup's glass whose Rec. 709 luma is over 55 %, and only that count is read back into the fact (0 to 1). It returns to 0 when the popup opens. The panels and the session menu use it for j2bar's light and dark glass |
| `0021-card-capture-without-shm` | When the compositor offers its captures in a shared-memory format Pleamar cannot read (a headless Sway offers 24-bit BGR888) but also offers a dmabuf, the next capture goes onto the card instead of none ever arriving. Without it no lens glass works on such a Sway |
| `0023-surface-bright` | Lets a layer `surface` take the same `bright: fact` as a popup (patch 0020), counted over all of its lens glass. The fact may be declared inside the surface, and it keeps its last value while the surface is closed instead of going back to 0, so the launcher reopens over the same desktop in the same ink |
| `0024-media-players-volume` | The `media` report gives each player's title, artist and artwork in `players`, and the reported player's MPRIS `Volume` as `volume` (`-1` when it has none); `media.volume <0..1>` sets it, and fails for a player without one. The media popup lists the players and sets the player's own volume with it |
| `0025-notification-hold` | The `notifications` service reports each notification's `timeout` and the `left` of it, in milliseconds (0 for one that does not expire), and `notifications.hold(id, true/false)` stops and restarts its expiry where it was. The notification daemon's capillary line shows that time and pauses it while the pointer is on a toast, and an expiry is still closed by the server with reason 1 |
| `0026-lock-authority` | Tohru's lock authority (`tohru_lock`, from `bindings/lock.rs`): the native password check, the per-cycle unlock permit that the render and Wayland code check before releasing the lock, and the locker mode (`TOHRU_LOCKER=1`) that refuses `--say`, hot reload and relaunch |
| `0027-lock-pam` | PAM picks the `tohru-lock` service first, the password copy is zeroed with volatile writes on every path, and the `tohru-test-pam` feature (never in a shipped build) lets `TOHRU_TEST_PAM_CONFDIR` point the check at a private service for nested-compositor tests |
| `0028-lock-views-together` | An element on more than one view (a lock face per monitor) is clipped to all of them together. Clipped to the first alone, a face drew the backdrop only as large as a smaller monitor plugged in while locked |

| `0030-polkit-agent` | Completes the `polkit` service into an agent: it refuses `BeginAuthentication` and `CancelAuthentication` from anyone but the owner of `org.freedesktop.PolicyKit1`, lists unix-user and unix-group identities, queues requests, relays the helper's PAM prompts, info and errors, switches identity, and reports the requesting app, command line, tries and outcome. Only the helper's `SUCCESS` line grants; a panic, a lost helper or a closed connection fails the request. The answer is zeroed after it is written. `polkit.answer`, `polkit.identity` and `polkit.cancel` are the scene's commands. Also sends the agent's path as an object path when registering. See `shell/polkit/README.md` |
| `0031-audio-default-input` | Adds explicit `audio.default_output(id)` and `audio.default_input(id)` commands to the PulseAudio and wpctl backends so selecting a default microphone never collides with an output sink ID, while ignoring monitor sources |
| `0032-greeter-authority` | Tohru's pre-session greeter authority (`tohru_greeter`, from `bindings/greeter.rs`): guards secret fields with volatile zeroing, speaks `greetd`'s length-prefixed JSON IPC over `GREETD_SOCK`, refuses `--say`, disables hot-reload/relaunch/skill-refresh and `right_click_quits`, and tolerates missing user settings when `TOHRU_GREETER=1` |
| `0033-keyboard-indicators` | Exposes `caps_lock` and `num_lock` facts plus the active XKB `kb_layout` text from `wl_keyboard` keymap/modifiers to scenes that declare them (used by the greeter and available to the lock screen) |

Numbers 0002 to 0004 and 0007 to 0009 belonged to features that were removed or moved into the scenes.

## Luau bindings and the Rust library

New portability patches 0034–0036 add independent `outputs` discovery (wl_output/xdg-output), explicit generic workspace activation capability, native configurable text/display/monospace font roles, compositor-confirmed systemd locker readiness, and the read-only `--tohru-capabilities` runtime marker. Output topology is never derived from workspaces.

The bindings in `runtime/pleamar/bindings/` are copied into the Pleamar source as `src/tohru_settings.rs` and `src/tohru_info.rs`. They wrap the `tohru-shell` crate in `runtime/`:

| Global | Module | Purpose |
|---|---|---|
| `tohru_settings` | `runtime/src/settings.rs` | Layered TOML settings: defaults, writable file, managed file, runtime overrides. Writes keep comments and unknown keys. Lives outside the Luau state, so it survives logic reloads |
| `tohru_info` | `runtime/src/info/` | Read-only snapshot for the information popup: OS, kernel, CPU, GPUs, memory, uptime, accounts, disks and system generation |

## Hyprland plugin

`hyprland/` is a separate, optional compositor plugin, `libtohru-hyprland.so`. It does not link to or talk to the shell. It combines four effects: focus dips (from hyprfocus), thin titlebars (from hyprbars), file chooser sheets attached to their parent window, and a tilt, shrink, shade and blur of windows behind the launcher's `tohru-launcher-backdrop` layer. It is built for Hyprland 0.57.2 and checks the compositor's API hash before installing any hook.

## Repository layout

| Path | Contents |
|---|---|
| `shell/bar.plm`, `shell/bar.luau`, `shell/services.plm` | Root scene: bar layout and service buttons |
| `shell/bar/` | Bar contents and the workspace switcher |
| `shell/sound/`, `shell/calendar/`, `shell/network/`, `shell/media/`, `shell/bluetooth/`, `shell/wallpaper/`, `shell/info/`, `shell/about/`, `shell/assistant/` | Popups shown by the bar |
| `shell/osd/`, `shell/corners/` | Volume OSD and screen corners, also in the bar process |
| `shell/session/`, `shell/launcher/`, `shell/notifications/`, `shell/settings/`, `shell/polkit/` | Scenes that run as their own processes |
| `shell/water/` | Shaders shared between scenes (the water vessel of the polkit dialog and the lock screen) |
| `shell/service-assets/` | Shared icons |
| `bin/` | `tohru` CLI and launch wrappers |
| `runtime/` | `tohru-shell` crate (`src/`, `tests/`) |
| `runtime/pleamar/` | Pin, patches, bindings, lock file, `prepare.sh` |
| `hyprland/` | Optional compositor plugin |
| `nix/` | Package, runtime wrapper, Home Manager module and its test |
| `tests/` | Logic tests (`lua`), CLI tests and native tests that run on a private Sway session |
| `tools/reference/` | Frozen reference fixtures and replay tooling from j2bar |

Each `shell/*/README.md` holds the development notes for that feature.
