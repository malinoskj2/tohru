# Development

Working on Tohru from a checkout, without rebuilding or switching for every edit. Everything here is for contributors; none of it is a supported way to customise an installed shell.

## Live scenes with `tohru dev`

Scenes under `shell/` (`.plm`, `.luau`, `.wgsl`) load when a role starts, so a restart picks up an edit. `tohru dev` restarts roles against the checkout's `shell/` and puts the packaged scenes back when it exits:

```sh
./bin/tohru dev --watch              # every running tohru-*.service unit
./bin/tohru dev bar launcher --watch # only these
```

By default it works on the real session's systemd user units (`programs.tohru.systemd.enable`): it points the user manager's environment at the checkout, restarts the units, and on Ctrl-C (or TERM, or a second Ctrl-C during cleanup) clears that environment and restarts them again. The installed package must already include `tohru dev` support; a session started from an older build ignores the checkout.

| Option | Effect |
|---|---|
| `COMPONENT...` | `bar`, `session`, `launcher`, `notifications`, `polkit`; `settings` with `--nested` |
| `--watch` | Restart roles when files under the scene directory change. A change inside `shell/launcher/`, `shell/session/`, `shell/notifications/`, `shell/polkit/` or `shell/settings/` restarts that role; anything else restarts every selected role. Uses `inotifywait`, or `nix shell nixpkgs#inotify-tools` when it is not installed |
| `--nested` | Leave systemd alone and run the roles in the foreground on the nested Hyprland at `WAYLAND_DISPLAY=hyprland-1`, stopping them on exit. Defaults to `bar session launcher notifications` |
| `--runtime PATH` | Run this `pleamar` instead of the packaged one |
| `--shell DIR` | Scene directory; defaults to the checkout's `shell/`, found from the script's location or the current directory |

The settings window is started on demand rather than by a unit, so in a real session close it (`tohru settings close`) and reopen it to see changes; use `--nested settings` to iterate on it directly.

Lock-screen scenes are read each time the locker starts. While `tohru dev` runs, a locker started by the systemd user manager's environment (hypridle as a user service, for example) uses the checkout too.

## A local runtime

Changes to the Pleamar patches or bindings need a runtime build:

```sh
nix build .#pleamar-runtime -o .tmp/runtime
./bin/tohru dev --runtime .tmp/runtime/bin/pleamar --watch
```

## The Hyprland plugin

Build it against the running compositor's Hyprland and swap it in place:

```sh
nix develop path:./hyprland -c make -C hyprland -j1
hyprctl plugin unload "$PWD/hyprland/libtohru-hyprland.so"
hyprctl plugin load "$PWD/hyprland/libtohru-hyprland.so"
```

If the session loaded the packaged plugin, unload that path first (`hyprctl plugin list` shows it). A plugin built against another Hyprland version refuses to load.

## Switching to the checkout

To run the whole checkout as the installed shell, including uncommitted changes, override the flake input when switching instead of pushing:

```sh
home-manager switch --flake ~/your-config#you --override-input tohru "path:$PWD"
```

`tohru` is whatever name your configuration gives this flake input. A `path:` input copies the working tree as it is, so the next ordinary switch returns to the pinned revision.
