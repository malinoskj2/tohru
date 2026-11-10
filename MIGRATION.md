# Migrating from j2bar to Tohru

Living checklist for replacing j2bar on the `home` (and `mac-vm`) desktop with
Tohru while keeping the behavior we still want. Scope decisions come from
[docs/reference/acceptance.md](docs/reference/acceptance.md); later decisions
recorded here override it.

Status: draft, started 2026-11-10. Nothing below has been done yet.

## 1. Scope changes since the acceptance inventory

| Area | Decision | Replacement |
|---|---|---|
| Lock boundary patch (0002) and Tohru's lock screen (row 23) | Remove from Tohru | hyprlock, started through `loginctl lock-session` |
| Settings window lifecycle patch (0003) | Remove; Settings becomes its own process | `pleamar --scene` per open, like any Pleamar window app |
| Still-image wallpaper (row 26) | Remove from Tohru | Plain Hyprland configuration (no effects plugin) |
| Video wallpaper (row 26) | Remove from Tohru | mpvpaper, through the existing `~/nix` wallpaper units |
| Idle (row 24) | Remove from Tohru | hypridle, configured in `~/nix` |
| Region screenshot (row 28) | Remove from Tohru | grimblast (hyprwm/contrib): `grimblast --notify --freeze copysave area` |

Already dropped by the acceptance inventory and still dropped: dmenu picker
(06), notification history/DND/styling (07, 08), balance/peak meter/overdrive
(10), network rates and VPN/Ethernet administration (12, 13), timezone editing
(19), user/storage/generation administration (21), Assistant (22), power
profiles and brightness (29), all game behavior (30, 31), Lua plugin API (37),
importing old state (40).

Accepted differences from j2bar that stay:

- Panels, launcher and OSD use Pleamar's native glass, springs, glow and blur.
  Dimensions, content and interactions follow j2bar; j2bar's custom
  Warp/turbulence/Slosh effects are not reproduced.
- Popups opened from the CLI (no pointer click) may need Escape to close;
  outside-click dismissal relies on the click's popup grab.
- Launcher search uses a simple fuzzy matcher and Pleamar's app catalog.
- Notifications are transient text plus one placeholder sound; Pleamar keeps
  its own internal expiry bookkeeping but no history, unread or DND UI.
- Network errors are as detailed as NetworkManager reports through Pleamar;
  requests report acceptance, not completion.
- `tohru` CLI commands report that Pleamar accepted the request, not that the
  action finished. If no Tohru runs on the caller's display, Pleamar may route
  to one on another display (with a warning).

## 2. Tohru work before cutover

- [ ] Removal tasks #1–#6 in the repo task list (lock boundary, window lifecycle,
      wallpaper, video, idle, capture), with their patch rebases.
- [ ] Session menu Lock and Sleep call `loginctl lock-session` (and suspend for
      Sleep) so hyprlock, through hypridle's `lock_cmd`, does the locking.
- [ ] Bar visual parity with j2bar (started: shape, row layout, glyphs, SF Pro
      Text). Still missing: notification bell, adaptive light/dark glass.
- [ ] Bar output policy: acceptance row 03 asks for one bar on a preferred output
      with focused/first fallback; Tohru currently draws `screens: all`. Pick one.
- [ ] CLI gap against current keybinds (section 4): launcher toggle.
- [ ] Process supervision: Tohru is several scenes (bar root, session menu,
      locker, launcher, notifications, polkit, Settings on demand). Nothing
      starts them yet; `nix/home-manager.nix` only installs the package. Add
      systemd user units (bound to `graphical-session.target`) or Hyprland
      `exec-once`, with restart policy for each long-lived role.
- [ ] Notifications: Tohru must be the only `org.freedesktop.Notifications`
      owner; verify banner text and the placeholder sound.
- [ ] Polkit: Tohru's agent must be the only agent (j2bar currently runs
      `j2bar polkit-agent`).
- [ ] Effects plugin: fix the first-pass `hl.animation` ordering (its animation calls run before
      `hl.plugin.load` registers them, so they only apply after a reload),
      run its visual/unload/damage checks in a nested Hyprland, then move
      `effects/` to its own repository as planned.
- [ ] Settings window: decide whether it floats or tiles (it renders 800 px wide
      inside whatever size the compositor gives it).
- [ ] Re-run the private native checks that survive the removals against
      `nix build --print-out-paths .#pleamar-runtime` (they own a private Sway,
      bus and Pulse; see `tests/WINDOW-RUNNER.md`):
      `tests/native-settings.sh --windows --sound --pointer --keyboard --wtype`
      (rework for the Settings process, #2), `tests/bar/native.py` in every mode
      (needs `setsid`, `wayland-scanner`, xkbcommon pkg-config; update for the
      new bar layout), and `tests/session/native.sh` with `--notifications`
      (its lock and PAM modes go with #1).

### Roles and processes

| Scene name (`--answer-as`) | Scene | Started by |
|---|---|---|
| `tohru` | `shell/bar.plm` (bar, popups, OSD, corners) | `bin/tohru-run` (sets `TOHRU_SHELL=1`, clears `TOHRU_LOCKER`) |
| `tohru-session` | `shell/session/menu.plm` | `pleamar --scene … --answer-as tohru-session --no-hud` |
| `tohru-launcher` | `shell/launcher/entry.plm` | `pleamar --scene … --answer-as tohru-launcher --stall 0` |
| `tohru-notifications` | `shell/notifications/notifications.plm` | own process; must be the only notification daemon |
| `tohru-polkit` | `shell/polkit/polkit.plm` | own process; must be the only polkit agent |
| `tohru-settings` | Settings scene | spawned on open after #2 |

Every role is its own process and can restart on its own. Scenes talk with
`sys.call("say", "<name>", "emit …")`, which needs `say:<name>` in the caller's
`permissions`. `TOHRU_RUNTIME` overrides which `pleamar` the wrappers and CLI use.

## 3. Nix changes (`~/nix`)

Remove j2bar:

- [ ] `flake.nix`: drop the `j2bar` input (`git+file:///home/jesse/projects/j2bar`)
      and add a `tohru` input.
- [ ] `hosts/common/optional/j2bar.nix`: replace with a Tohru module
      (`programs.tohru`, managed settings, units). Keep `programs.hyprlock.enable`;
      hyprlock is the locker. Add a hyprlock config that resembles j2bar's lock
      screen (wallpaper, clock, avatar/name, password field).
- [ ] `hosts/home/configuration.nix`, `hosts/mac-vm/configuration.nix`: swap the
      import. Keep NetworkManager enabled; Tohru's network service also uses it.
- [ ] `users/jesse/hyprland-desktop/j2bar.nix`: drop `J2BAR_BIN` and the plugin
      package; keep the `.wallpapers/video` link for mpvpaper.
- [ ] `users/jesse/hyprland-desktop/hyprland/default.nix`: replace `nix.j2bar`
      and `nix.j2barPlugin` with Tohru CLI and effects plugin paths.
- [ ] `users/jesse/hyprland-desktop/hyprland/look.lua`: load the Tohru effects
      plugin instead of `libj2bar-hyprland.so`; move `programs.j2bar.settings.effects`
      policy to the new plugin's configuration.
- [ ] `hosts/home/gaming.nix`: remove `j2bar.gameEngine.enable` and every
      `j2bar game-run -- %command%` Steam launch option (game behavior is
      dropped).
- [ ] Docs in `~/nix` that describe j2bar (`CLAUDE.md`, `README.md`,
      `docs/updating.md`, `docs/design.md`, `docs/game-latency.md`,
      `docs/mac-vm.md`, `docs/complexity-review.md`).

Wallpaper:

- [ ] Still images: set them in the Hyprland configuration directly. Confirm the
      mechanism (hyprpaper launched from Hyprland config, or whatever the pinned
      Hyprland offers natively) and per-monitor assignment.
- [ ] Video: enable `wallpaper.launcher.enable` (`users/jesse/hyprland-desktop/wallpaper.nix`),
      which runs mpvpaper per output through `wallpaper-outputs` and `wallpaper@`.
- [ ] `pkgs/wallpaper-autopause`: it reads `j2bar msg status | .locked` to pause
      behind the lock screen. Point it at hyprlock's state (`pidof hyprlock`,
      or logind's `LockedHint`).
- [ ] `pkgs/wallpaper-select`: it uses `j2bar dmenu`, which is dropped. Replace
      the picker (e.g. a small Tohru launcher mode, fuzzel/rofi) or drop the script.
- [ ] `pkgs/wallpaper-randomize`: check for j2bar references.

Screenshots (grimblast):

- [ ] Add `grimblast` (pulls in grim, slurp, hyprpicker, wl-clipboard,
      libnotify) to the Hyprland desktop packages.
- [ ] Set `XDG_SCREENSHOTS_DIR` in the session environment and create it.
- [ ] Result notifications come through `notify-send`, so Tohru's notification
      daemon must be running.

Idle (hypridle):

- [ ] Enable `services.hypridle` in Home Manager.
- [ ] `general`: `lock_cmd = pidof hyprlock || hyprlock`, `before_sleep_cmd = loginctl lock-session`
      (lock before sleep), `after_sleep_cmd = hyprctl dispatch dpms on`; leave
      `ignore_dbus_inhibit` off so video players and `systemd-inhibit` still block idle.
- [ ] Listeners matching the old timeouts (Tohru defaults: lock 30 min,
      suspend 2 h; j2bar's values are in `~/.config/j2bar/config.toml`):
      lock → `loginctl lock-session`; screen off → `hyprctl dispatch dpms off`
      with `on-resume = hyprctl dispatch dpms on`; suspend → `systemctl suspend`.
- [ ] Gaps to accept or replace: hypridle has no pre-action fade and no
      caffeine toggle (use `systemd-inhibit --what=idle` from a keybind, or
      drop), and it has one set of timeouts rather than separate locked and
      unlocked ones (approximate by a shorter screen-off listener that checks
      whether the session is locked).

## 4. Keybinds (`hyprland/hyprland.lua`)

| Key | j2bar today | Tohru |
|---|---|---|
| Super+P | `msg screenshot-region` | `grimblast --notify --freeze copysave area` |
| Super+Escape | `msg panel-toggle menu` | `tohru session toggle` |
| Super+Shift+Escape | `msg session lock` | `loginctl lock-session` |
| Super+Space | `msg panel-toggle launcher` | missing: add `tohru launcher toggle` (`--say tohru-launcher 'emit launcher_toggle'`) |
| Super+S | `msg panel-toggle control-center` | j2bar has no such panel (the bind is already dead); remove or repoint |
| Super+A | `msg panel-toggle assistant` | Assistant dropped; remove the bind |
| XF86AudioRaiseVolume | `msg volume-up` | `tohru audio volume up` |
| XF86AudioLowerVolume | `msg volume-down` | `tohru audio volume down` |
| XF86AudioMute | `msg volume-mute` | `tohru audio mute toggle` |

Keep `misc.allow_session_lock_restore = true` so a replacement locker can take
over if the first one dies.

Other j2bar commands: `caffeine-*` and `dpms-*` belong to idle and go to
hypridle/`hyprctl dispatch dpms` (no Tohru command); `status` has
`tohru status --json`.

## 5. Settings and state

How Tohru settings work:

- Writable file `$XDG_CONFIG_HOME/tohru/config.toml` (else
  `~/.config/tohru/config.toml`); Nix supplies a read-only managed file through
  `TOHRU_MANAGED_CONFIG` (`programs.tohru.settings`).
- Precedence: defaults < user TOML < Nix-managed < runtime overrides. Editing a
  Nix-managed value in Settings or with `tohru config set` applies at once and
  survives `tohru config reload`, but is never written and ends with the process.
  Unmanaged edits are written to the TOML file. Settings marks managed rows.
- Keys left after removals #3, #5 and #6: `appearance.palette`,
  `appearance.corners`, `appearance.corner_size` (1–100), `bar.mooncoil`.
  `bar.preferred_output` is accepted but ignored (bars use every output).

- [ ] Translate the values still in use from `~/.config/j2bar/config.toml` into
      `programs.tohru.settings` (Nix-managed) or `~/.config/tohru/config.toml`.
      Effects settings go to the effects plugin, not Tohru.
- [ ] Start fresh for launcher pins/history and other mutable state (row 40).
      Leave `~/.config/j2bar` and j2bar's state in place for rollback.

## 6. Cutover

1. Build and switch with Tohru installed but not started, j2bar still running.
2. Stop j2bar (the bar, `j2bar polkit-agent`, its mpvpaper children) and start
   the Tohru units and the wallpaper units.
3. Check every row of section 7 on the real session.
4. Make the Tohru units start with the session; remove the j2bar module.

Rollback: revert the `~/nix` commit and switch; j2bar's config and state were
left untouched.

## 7. Verification on the real session

Every retained row of [acceptance.md](docs/reference/acceptance.md) still needs
a live check. Things only the real session can prove:

- Bar on each monitor, hotplug and reconnect, rotation and fractional scale.
- Glass and adaptive ink against j2bar screenshots over the same wallpaper.
- Real PAM: lock/unlock, wrong password, lock before sleep, polkit prompts.
- hypridle: screen-off, lock, suspend, lock before sleep, inhibitors from
  video players, DPMS back on after resume.
- Notifications from real apps; exactly one notification daemon and one polkit agent.
- Volume keys while locked, OSD, sound panel with real devices.
- Network and Bluetooth panels against real NetworkManager/BlueZ.
- grimblast: freeze, region select, Escape cancel, save, clipboard, notification.
- Wallpapers (Hyprland stills, mpvpaper videos) pausing behind the lock screen.
- Performance and soak on the reference machine.
