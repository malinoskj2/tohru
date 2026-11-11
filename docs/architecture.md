# Architecture

## Pleamar

[Pleamar](https://github.com/k4ditano/pleamar) describes itself as "a desktop shell language and runtime where animation does not depend on logic". A scene (`.plm`) declares surfaces, shapes, text, popups, facts, events and the system services it may use. Luau scripts react to events and update facts. WGSL shaders draw custom effects. Pleamar provides the native services Tohru uses: audio over the PulseAudio protocol, NetworkManager, BlueZ, MPRIS media, application discovery and launch, workspaces, battery, notifications and session actions.

Tohru is a set of Pleamar scenes plus a small set of runtime patches and Luau bindings.

## Processes

Each role is its own `pleamar` process, started by a wrapper in `bin/`. Each can restart without the others.

| Role name | Scene | Wrapper | Notes |
|---|---|---|---|
| `tohru` | `shell/bar.plm` | `tohru-run` | Bar, all popups, volume OSD and screen corners in one process. Sets `TOHRU_SHELL=1` |
| `tohru-session` | `shell/session/menu.plm` | `tohru-session-run` | Session menu |
| `tohru-launcher` | `shell/launcher/entry.plm` | `tohru-launcher-run` | Launcher, a full-screen layer named `tohru-launcher-backdrop` |
| `tohru-notifications` | `shell/notifications/notifications.plm` | `tohru-notifications-run` | Owns `org.freedesktop.Notifications`; must be the only notification daemon |
| `tohru-settings` | `shell/settings/settings.plm` | `tohru-settings-run` | Settings window, started on demand. Sets `TOHRU_SHELL=1` |

The wrappers run `pleamar --scene <scene> --no-hud --stall 0 --answer-as <role>`. The role name is what other processes and the CLI address.

Tohru has no lock screen, idle handling, polkit agent, wallpaper or screenshot tool. The session menu's Lock runs `loginctl lock-session`, which hypridle's `lock_cmd` answers by starting hyprlock. Sleep runs `loginctl lock-session` and, only if that succeeds, `systemctl suspend`. Restart, Shut Down and Log Out use Pleamar's session services.

## How the parts talk

Processes send each other requests with `pleamar --say <role> "<request>"`, or from Luau with `sys.call("say", "<role>", "<request>")`. A request is `emit <event> [args]` to fire an event, `get <name>` to read a text value, `text <name> <value>` to set one, or `quit`. A scene may only `say` to roles listed in its `permissions` (`say:<role>`).

- The `tohru` CLI is a shell script that turns each command into a `--say` request. For example `tohru launcher toggle` sends `emit launcher_toggle` to `tohru-launcher`, and `tohru status --json` sends `get settings_status` to `tohru`.
- The bar sends `emit menu_toggle` to `tohru-session` when its session button is used, and power warnings to `tohru-notifications`.
- Settings sends `emit config_reload` and `setting_*` events to `tohru` after each accepted edit.

Requests report that Pleamar accepted them, not that the action finished. If no Tohru process runs on the caller's display, Pleamar may route the request to one on another display, with a warning.

## Runtime patches

`runtime/pleamar/pin.json` pins Pleamar 0.7.27 at `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`. Both `runtime/pleamar/prepare.sh` and `nix/default.nix` apply the patches in `runtime/pleamar/patches/` in name order:

| Patch | Change |
|---|---|
| `0001-shell-settings` | Adds the `tohru-shell` library as a dependency and installs the `tohru_settings` Luau global, only when `TOHRU_SHELL=1` |
| `0002-shader-inputs` | Raises the number of values a scene can pass to a shader from 8 to 16, for the workspace switcher's ribbon shader |
| `0004-workspace-binding` | Installs the `tohru_workspace` Luau global |
| `0005-launcher-namespace` | Adds an optional static `layer_namespace` surface property. The launcher uses it so the Hyprland plugin can match its layer. Without it, layers are named `pleamar` |
| `0006-info-binding` | Installs the `tohru_info` Luau global |
| `0010-key-rule-order` | Keys that arrive in the same frame as a key that triggers a rule, such as typing right after Ctrl+F, are handled after the rule has acted |
| `0011-focus-round` | A keyboard leave and enter in the same frame, such as a popup grab moving focus between Tohru's own surfaces, no longer counts as losing focus |

Numbers 0003 and 0007 to 0009 belonged to features that were removed.

## Luau bindings and the Rust library

The bindings in `runtime/pleamar/bindings/` are copied into the Pleamar source as `src/tohru_settings.rs`, `src/tohru_workspace.rs` and `src/tohru_info.rs`. They wrap the `tohru-shell` crate in `runtime/`:

| Global | Module | Purpose |
|---|---|---|
| `tohru_settings` | `runtime/src/settings.rs` | Layered TOML settings: defaults, writable file, managed file, runtime overrides. Writes keep comments and unknown keys. Lives outside the Luau state, so it survives logic reloads |
| `tohru_workspace` | `runtime/src/workspace/` | Spring physics for the workspace switcher, stepped at a fixed 1/240 s and stopped at rest |
| `tohru_info` | `runtime/src/info/` | Read-only snapshot for the information popup: OS, kernel, CPU, GPUs, memory, uptime, accounts, disks and system generation |

## Hyprland plugin

`hyprland/` is a separate, optional compositor plugin, `libtohru-hyprland.so`. It does not link to or talk to the shell. It combines four effects: focus dips (from hyprfocus), thin titlebars (from hyprbars), file chooser sheets attached to their parent window, and a tilt, shrink, shade and blur of windows behind the launcher's `tohru-launcher-backdrop` layer. It is built for Hyprland 0.56.2 and checks the compositor's API hash before installing any hook. The plugin is planned to move to its own repository.

## Repository layout

| Path | Contents |
|---|---|
| `shell/bar.plm`, `shell/bar.luau`, `shell/services.plm` | Root scene: bar layout and service buttons |
| `shell/bar/` | Bar contents and the workspace switcher |
| `shell/sound/`, `shell/calendar/`, `shell/network/`, `shell/bluetooth/`, `shell/info/`, `shell/about/` | Popups shown by the bar |
| `shell/osd/`, `shell/corners/` | Volume OSD and screen corners, also in the bar process |
| `shell/session/`, `shell/launcher/`, `shell/notifications/`, `shell/settings/` | Scenes that run as their own processes |
| `shell/service-assets/` | Shared icons |
| `bin/` | `tohru` CLI and launch wrappers |
| `runtime/` | `tohru-shell` crate (`src/`, `tests/`) |
| `runtime/pleamar/` | Pin, patches, bindings, lock file, `prepare.sh` |
| `hyprland/` | Optional compositor plugin |
| `nix/` | Package, runtime wrapper, Home Manager module and its test |
| `tests/` | Logic tests (`lua`), CLI tests and native tests that run on a private Sway session |
| `tools/reference/` | Frozen reference fixtures and replay tooling from j2bar |

Each `shell/*/README.md` holds the development notes for that feature.
