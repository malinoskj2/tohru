# Migrating from j2bar to Tohru

Living checklist for replacing j2bar on the `home` (and `mac-vm`) desktop with
Tohru while keeping the behavior we still want. Scope decisions come from
[docs/reference/acceptance.md](docs/reference/acceptance.md); later decisions
recorded here override it.

Status: cut over on `home` on 2026-11-11. j2bar is stopped and Tohru runs from
its user units; the `~/nix` commit is local and Home Manager was activated
directly, so the next `nixos-rebuild switch` brings the system profile (and the
new packages on `PATH`) up to date. What is still open is unchecked below.

## 1. Scope changes since the acceptance inventory

| Area | Decision | Replacement |
|---|---|---|
| Lock boundary patch (0002) and Tohru's lock screen (row 23) | Back in Tohru as `tohru-lock` (patches 0026–0028, ten styles) | Started through `loginctl lock-session` and hypridle's `lock_cmd`; hyprlock stays on hosts without Tohru |
| Settings window lifecycle patch (0003) | Remove; Settings becomes its own process | `pleamar --scene` per open, like any Pleamar window app |
| Still-image wallpaper (row 26) | Remove from Tohru | Plain Hyprland configuration (no effects plugin) |
| Video wallpaper (row 26) | Remove from Tohru | mpvpaper, through the existing `~/nix` wallpaper units |
| Idle (row 24) | Remove from Tohru | hypridle, configured in `~/nix` |
| Polkit agent (row 25) | Removed, then brought back as an opt-in agent (`polkit.agent`, off by default) | hyprpolkitagent until `~/nix` switches; see [Polkit agent](#polkit-agent) |
| Region screenshot (row 28) | Remove from Tohru | grimblast (hyprwm/contrib): `grimblast --notify --freeze copysave area` |

Already dropped by the acceptance inventory and still dropped: dmenu picker
(06), notification history and styling (07, 08; Do Not Disturb came back with
the bell), balance/peak meter/overdrive
(10), network rates and VPN/Ethernet administration (12, 13), timezone editing
(19), user/storage/generation administration (21), Assistant (22), power
profiles and brightness (29), all game behavior (30, 31), Lua plugin API (37),
importing old state (40).

Accepted differences from j2bar that stay:

- Panels and OSD use Pleamar's native glass, springs, glow and blur.
  Dimensions, content and interactions follow j2bar; j2bar's custom
  Warp/turbulence/Slosh effects are not reproduced. The launcher is the
  exception: it draws j2bar's capsule, surge, reveal and Fill track in its own
  WGSL (`shell/launcher/README.md` lists what still differs).
- Popups opened from the CLI (no pointer click) close with Escape or
  `tohru panel close`, not an outside click: that needs an xdg_popup grab,
  and a grab needs a real input serial. Grabbing with a stale or zero serial
  was tried on 2026-11-11 in a nested Hyprland 0.56.2 and left Hyprland
  dismissing every later popup as soon as it opened, so it was dropped.
- Launcher search uses a simple fuzzy matcher and Pleamar's app catalog.
- Notifications are transient text plus one placeholder sound, with no
  history UI. The bar's bell shows whether a notification arrived unseen
  (saved across restarts) and Do Not Disturb, as j2bar's does; Do Not Disturb
  shows no OSD when toggled, and power warnings are the only notices that
  bypass it (there are no per-app filters).
- Network errors are as detailed as NetworkManager reports through Pleamar;
  requests report acceptance, not completion.
- `tohru` CLI commands report that Pleamar accepted the request, not that the
  action finished. If no Tohru runs on the caller's display, Pleamar may route
  to one on another display (with a warning).

## 2. Tohru work before cutover

- [ ] Removal tasks #1–#6 in the repo task list (lock boundary, window lifecycle,
      wallpaper, video, idle, capture), with their patch rebases.
- [x] Session menu Lock and Sleep call `loginctl lock-session` (and suspend for
      Sleep) so hyprlock, through hypridle's `lock_cmd`, does the locking.
- [ ] Bar visual parity with j2bar. Matched against j2bar on the live desktop:
      31px height, outline (flares, bottom corners, hairline weight at scale 1),
      icons and text on j2bar's pixel rows, rail width, clock padding, the body
      tint, the switcher (rail, lit segment with its rounded ends, glow), the
      button row at y=4, and the clock's tabular numerals (patch 0013; the
      date, time and tray now land within 1px of j2bar's).
      The switcher's motion now runs on Pleamar springs, with no Rust physics or
      shader-input patch; side-by-side recordings against j2bar on 2026-11-11
      match apart from a start 50 to 100 ms later (Pleamar's Hyprland event
      coalescing).
      Notification bell (left click marks seen, right click toggles Do Not
      Disturb, j2bar's 4px unread mark and glow) and adaptive light/dark glass
      are done. j2bar's adaptive ink is on its panels, not the bar, which stays
      dark there too: the panels and the session menu turn light above 55%
      bright and dark below 45% (patch 0020), and the calendar's month rail
      keeps its dark-glass track until the rail shader takes an ink.
      Not done, and why:
      - Text rasteriser: not worth a patch. cosmic-text already hints and
        snaps glyphs to quarter-pixel bins; after the tabular numerals,
        positions match j2bar within 1px. What is left is glyph shape
        (swash's hinter against FreeType's slight autohinter through
        Pango), which whole-pixel positioning would not change, at the cost
        of a patch on every text Pleamar draws.
- [x] Bar output policy: one bar on `bar.preferred_output`, every output while it
      is empty, no focused/first fallback.
- [x] CLI gap against current keybinds (section 4): `tohru launcher toggle|open|close`.
- [x] Process supervision: Tohru is several scenes (bar root, session menu,
      launcher, notifications, Settings on demand). Each long-lived role has a
      wrapper in `bin/`, and `programs.tohru.systemd.enable` starts them as user
      services bound to `graphical-session.target`, restarting on failure. The
      units have not run under a real systemd session yet.
- [x] Notifications: Tohru must be the only `org.freedesktop.Notifications`
      owner; verify banner text and the placeholder sound.
- [x] Polkit: hyprpolkitagent is the only polkit agent (stop j2bar polkit-agent).
- [x] Effects plugin: fix the first-pass `hl.animation` ordering (its animation calls run before
      `hl.plugin.load` registers them, so they only apply after a reload),
      and run its visual/unload/damage checks in a nested Hyprland. The plugin
      stays in this repository under `hyprland/`.
- [x] Settings window: it lays out at whatever size the compositor gives it
      (tiled or floating). It asks for 760×640, which Pleamar also makes its
      minimum size; no window rule is needed.
- [x] Re-run the private native checks (they own a private Sway, bus and
      Pulse; see `tests/WINDOW-RUNNER.md`). All passed on 2026-11-11 against
      the runtime with patches up to 0014:
      `tests/native-settings.sh --settings --sound --pointer --keyboard --wtype`,
      `tests/bar/native.py` in every mode, and `tests/session/native.sh
      --notifications` (Lock logs `loginctl lock-session`; Sleep logs it, then
      `systemctl suspend`). Clicks now find zones by name through
      `describe json` instead of fixed coordinates. Intermittent: in one of
      two runs, `--services-only` passed every check but its closing `quit`
      took longer than 10 s, and `--rails-only` timed out waiting for
      `ws_available` once. The CPU-only suites (`tests/*.lua` with Lua 5.4,
      `tests/cli.py`, `cargo test --manifest-path tests/rust/Cargo.toml`)
      pass too.

### Roles and processes

| Scene name (`--answer-as`) | Scene | Started by |
|---|---|---|
| `tohru` | `shell/bar.plm` (bar, popups, OSD, corners) | `bin/tohru-run` (sets `TOHRU_SHELL=1`) |
| `tohru-session` | `shell/session/menu.plm` | `bin/tohru-session-run` |
| `tohru-launcher` | `shell/launcher/entry.plm` | `bin/tohru-launcher-run` |
| `tohru-notifications` | `shell/notifications/notifications.plm` | `bin/tohru-notifications-run`; must be the only notification daemon |
| `tohru-settings` | `shell/settings/settings.plm` | `tohru settings open` runs `bin/tohru-settings-run` (sets `TOHRU_SHELL=1`) when no Settings answers |

Every role is its own process and can restart on its own. Scenes talk with
`sys.call("say", "<name>", "emit …")`, which needs `say:<name>` in the caller's
`permissions`. `TOHRU_RUNTIME` overrides which `pleamar` the wrappers and CLI use.

## 3. Nix changes (`~/nix`)

Remove j2bar:

- [x] `flake.nix`: drop the `j2bar` input (`git+file:///home/jesse/projects/j2bar`)
      and add a `tohru` input.
- [x] `hosts/common/optional/j2bar.nix`: replace with a Tohru module
      (`programs.tohru`, managed settings, units). Keep `programs.hyprlock.enable`;
      hyprlock is the locker. Add a hyprlock config that resembles j2bar's lock
      screen (wallpaper, clock, avatar/name, password field).
- [x] `hosts/home/configuration.nix`, `hosts/mac-vm/configuration.nix`: swap the
      import. Keep NetworkManager enabled; Tohru's network service also uses it.
- [x] `users/jesse/hyprland-desktop/j2bar.nix`: drop `J2BAR_BIN` and the plugin
      package; keep the `.wallpapers/video` link for mpvpaper.
- [x] `users/jesse/hyprland-desktop/hyprland/default.nix`: replace `nix.j2bar`
      and `nix.j2barPlugin` with the Tohru CLI and
      `${programs.tohru.hyprlandPlugin.package}/lib/libtohru-hyprland.so`.
- [x] `users/jesse/hyprland-desktop/hyprland/look.lua`: load `libtohru-hyprland.so`
      instead of `libj2bar-hyprland.so`; move `programs.j2bar.settings.effects`
      policy to the new plugin's configuration.
- [x] `hosts/home/gaming.nix`: remove `j2bar.gameEngine.enable` and every
      `j2bar game-run -- %command%` Steam launch option (game behavior is
      dropped).
- [x] Docs in `~/nix` that describe j2bar (`CLAUDE.md`, `README.md`,
      `docs/updating.md`, `docs/design.md`, `docs/game-latency.md`,
      `docs/mac-vm.md`, `docs/complexity-review.md`).

Wallpaper:

- [x] Still images: set them in the Hyprland configuration directly. Confirm the
      mechanism (hyprpaper launched from Hyprland config, or whatever the pinned
      Hyprland offers natively) and per-monitor assignment. Not needed: both
      monitors play videos, and Hyprland's own background is black.
- [x] Video: enable `wallpaper.launcher.enable` (`users/jesse/hyprland-desktop/wallpaper.nix`),
      which runs mpvpaper per output through `wallpaper-outputs` and `wallpaper@`.
- [x] `pkgs/wallpaper-autopause`: it reads `j2bar msg status | .locked` to pause
      behind the lock screen. Point it at hyprlock's state (`pidof hyprlock`,
      or logind's `LockedHint`). It reads `LockedHint`, which `session-lock`
      (hyprlock's wrapper in `~/nix`) sets and clears.
- [x] `pkgs/wallpaper-select`: it uses `j2bar dmenu`, which is dropped. Replace
      the picker (e.g. a small Tohru launcher mode, fuzzel/rofi) or drop the script.
      Dropped; nothing bound it. The bar's wallpaper popup (`shell/wallpaper/`)
      now does its job through `tohru-wallpaper`, with the same state file and IPC.
- [ ] Optional: keep a chosen wallpaper across logins. `wallpaper-outputs`
      randomizes every output when the session starts, so a choice made in the
      popup lasts until the next login, as one made with `wallpaper-select` did.
      To keep it, randomize only outputs without a playable assignment. In
      `pkgs/wallpaper-outputs/wallpaper-outputs.sh`, `follow_monitors`, replace
      `wallpaper-randomize "${monitors[@]}"` with:

      ```bash
      local -a unassigned=()
      local state="${XDG_STATE_HOME:-$HOME/.local/state}/wallpaper/assignments.json" path
      for monitor in "${monitors[@]}"; do
        path="$(jq -r --arg m "$monitor" '.assignments[$m] // empty' "$state" 2>/dev/null)"
        [[ -n "$path" && -f "$path" ]] || unassigned+=("$monitor")
      done
      ((${#unassigned[@]} == 0)) || wallpaper-randomize "${unassigned[@]}"
      ```

      `jq` is already in that package's runtime inputs. The popup's Randomize All
      still gives a fresh set on demand.
- [x] `pkgs/wallpaper-randomize`: check for j2bar references.

Screenshots (grimblast):

- [x] Add `grimblast` (pulls in grim, slurp, hyprpicker, wl-clipboard,
      libnotify) to the Hyprland desktop packages.
- [x] Set `XDG_SCREENSHOTS_DIR` in the session environment and create it.
- [x] Result notifications come through `notify-send`, so Tohru's notification
      daemon must be running.

Idle (hypridle):

- [x] Enable `services.hypridle` in Home Manager.
- [x] `general`: `lock_cmd = pidof hyprlock || hyprlock`, `before_sleep_cmd = loginctl lock-session`
      (lock before sleep), `after_sleep_cmd = hyprctl dispatch dpms on`; leave
      `ignore_dbus_inhibit` off so video players and `systemd-inhibit` still block idle.
- [x] Listeners matching the old timeouts (Tohru defaults: lock 30 min,
      suspend 2 h; j2bar's values are in `~/.config/j2bar/config.toml`):
      lock → `loginctl lock-session`; screen off → `hyprctl dispatch dpms off`
      with `on-resume = hyprctl dispatch dpms on`; suspend → `systemctl suspend`.
- [x] Gaps to accept or replace: hypridle has no pre-action fade and no
      caffeine toggle (use `systemd-inhibit --what=idle` from a keybind, or
      drop), and it has one set of timeouts rather than separate locked and
      unlocked ones (approximate by a shorter screen-off listener that checks
      whether the session is locked).

Lock screen (`tohru-lock`):

- [ ] After this branch is merged, `nix flake update tohru`, then apply the change
      below. It switches the locker on hosts that run Tohru and keeps hyprlock on
      the others (mac-vm). `security.pam.services.tohru-lock` gives the locker its
      PAM service (otherwise it uses hyprlock's, then `login`'s). `tohru-lock` sets
      and clears logind's `LockedHint` itself, so `session-lock` is not needed for
      wallpaper-autopause, and `lock.command --running` replaces `pidof hyprlock`
      in the listeners. Keep `misc.allow_session_lock_restore = true` in
      `hyprland.lua`: it is what lets the plain style take over after a crash.

```diff
--- a/hosts/common/optional/hyprland.nix
+++ b/hosts/common/optional/hyprland.nix
@@ -9,6 +9,8 @@
   programs.hyprland.enable = true;
   # The locker, with its PAM service; Home Manager configures it.
   programs.hyprlock.enable = true;
+  # Tohru's locker checks passwords against its own PAM service.
+  security.pam.services.tohru-lock = { };
 
   environment.sessionVariables = {
     GSETTINGS_SCHEMA_DIR = "${schemas}/share/gsettings-schemas/${schemas.name}/glib-2.0/schemas";
--- a/users/jesse/hyprland-desktop/lock.nix
+++ b/users/jesse/hyprland-desktop/lock.nix
@@ -41,12 +41,18 @@
     '';
   };
 
-  # Runs only while hyprlock is up, so the unlocked session keeps its own, longer timeouts.
-  whileLocked = command: "${pidof} hyprlock >/dev/null && ${command}";
+  # Tohru's locker where Tohru runs; hyprlock on hosts without it (mac-vm).
+  tohru = config.programs.tohru.enable;
+  tohruLock = config.programs.tohru.lock.command;
+
+  # Runs only while the session is locked, so the unlocked session keeps its own, longer timeouts.
+  whileLocked =
+    command:
+    if tohru then "${tohruLock} --running && ${command}" else "${pidof} hyprlock >/dev/null && ${command}";
 in
 {
   programs.hyprlock = {
-    enable = true;
+    enable = !tohru;
     # The NixOS module installs it with its PAM service.
     package = osConfig.programs.hyprlock.package;
     settings = {
@@ -124,7 +130,8 @@
     enable = true;
     settings = {
       general = {
-        lock_cmd = lib.getExe lock;
+        # tohru-lock sets logind's LockedHint itself, and asking again while locked does nothing.
+        lock_cmd = if tohru then tohruLock else lib.getExe lock;
         before_sleep_cmd = "loginctl lock-session";
         after_sleep_cmd = dpms "on";
         # Suspend waits for the lock surface, so the session never wakes unlocked.
@@ -157,5 +164,5 @@
     };
   };
 
-  home.packages = [ lock ];
+  home.packages = lib.optional (!tohru) lock;
 }
--- a/users/jesse/hyprland-desktop/tohru.nix
+++ b/users/jesse/hyprland-desktop/tohru.nix
@@ -18,6 +18,7 @@
     package = lib.mkIf (package != null) package;
     systemd.enable = true;
     settings.bar.preferred_output = config.desktop.monitors.main.output;
+    lock.style = "flooded-monolith";
   };
 
   services.hyprpolkitagent.enable = true;
```

Polkit (hyprpolkitagent):

- [x] Enable `services.hyprpolkitagent` in Home Manager (a user service bound to
      `graphical-session.target`), and remove `j2bar polkit-agent` so it is the
      only agent.

### Polkit agent

Tohru's agent (`tohru-polkit`, the Frosted Vessel dialog) can replace hyprpolkitagent. A session can have only one polkit agent: polkitd keeps the first registration and refuses the second, so hyprpolkitagent must be gone before `tohru-polkit` starts. Not done yet; `~/nix` still runs hyprpolkitagent.

1. Update the `tohru` flake input to a revision that contains the agent (this branch, once merged), so `programs.tohru.polkit.enable` exists.
2. In `users/jesse/hyprland-desktop/tohru.nix`, replace

   ```nix
   services.hyprpolkitagent.enable = true;
   ```

   with `polkit.enable = true;` inside the existing `programs.tohru` block:

   ```nix
   programs.tohru = {
     enable = package != null;
     package = lib.mkIf (package != null) package;
     systemd.enable = true;
     polkit.enable = true;
     settings.bar.preferred_output = config.desktop.monitors.main.output;
   };
   ```

   `polkit.enable` writes `[polkit] agent = true` into the managed settings file and adds the `tohru-polkit` user service (needs `systemd.enable`, which is already on). Nothing else in `~/nix` changes: NixOS's `security.polkit` already provides polkitd and the setuid `polkit-agent-helper-1` (or its socket) that the agent uses.
3. Switch Home Manager, then make sure only one agent runs: `systemctl --user stop hyprpolkitagent` if it survived the switch, or log out and in. While another agent holds the session, `tohru-polkit` logs `another agent is` and keeps asking again, with growing pauses; `systemctl --user restart tohru-polkit` makes it ask at once.
4. Check: `journalctl --user -u tohru-polkit` has no `cannot be the one asked` line; `pkexec true` shows the dialog; a wrong password shows "Incorrect password · 2 attempts remaining"; the right one returns 0.

To go back, remove `polkit.enable = true;` and restore `services.hyprpolkitagent.enable = true;`.

## 4. Keybinds (`hyprland/hyprland.lua`)

| Key | j2bar today | Tohru |
|---|---|---|
| Super+P | `msg screenshot-region` | `grimblast --notify --freeze copysave area` |
| Super+Escape | `msg panel-toggle menu` | `tohru session toggle` |
| Super+Shift+Escape | `msg session lock` | `loginctl lock-session` |
| Super+Space | `msg panel-toggle launcher` | `tohru launcher toggle` |
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
  Settings is a separate process, so its overrides live there; it forwards
  them to the bar, which then holds its own until it exits.
- Keys left after removals #3, #5 and #6: `appearance.palette`,
  `appearance.corners`, `appearance.corner_size` (1–100), `bar.mooncoil`,
  `bar.info_button`, `bar.about_button` (both off by default),
  `bar.preferred_output`. The bar, its popups and the volume display show only on
  the preferred output, and the workspace rail counts only its workspaces; empty
  means every output with one merged rail. Corners stay on every output.

- [x] Set `programs.tohru.settings.bar.preferred_output` to the main monitor
      (`DP-2` on `home`), where j2bar shows today.

- [x] Translate the values still in use from `~/.config/j2bar/config.toml` into
      `programs.tohru.settings` (Nix-managed) or `~/.config/tohru/config.toml`.
      Effects settings go to the effects plugin, not Tohru.
- [x] Start fresh for launcher pins/history and other mutable state (row 40).
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

Checked on `home` on 2026-11-11: bar on DP-2 only; launcher (with window
recession) and session menu through the commands their keybinds run; bar popups and the
session menu open on DP-2 from the CLI; a `notify-send` banner, with Tohru the
only notification owner; hyprpolkitagent the only polkit agent; volume up/down;
grimblast saving an output; both video wallpapers; the plugin's effects and its
animation leaves from a cold Hyprland start (in a nested instance).

Every retained row of [acceptance.md](docs/reference/acceptance.md) still needs
a live check. Things only the real session can prove:

- Bar on the preferred output only, and on each monitor with it unset; hotplug
  and reconnect, rotation and fractional scale.
- Glass and adaptive ink against j2bar screenshots over the same wallpaper.
- tohru-lock: session menu Lock and Sleep, Super+Shift+Escape, wrong password,
  lock before sleep, a monitor waking while locked, the real `tohru-lock` PAM
  service. hyprpolkitagent prompts with real PAM.
- hypridle: screen-off, lock, suspend, lock before sleep, inhibitors from
  video players, DPMS back on after resume.
- Notifications from real apps; exactly one notification daemon and one polkit agent.
- Volume keys while locked, OSD, sound panel with real devices.
- Network and Bluetooth panels against real NetworkManager/BlueZ.
- grimblast: freeze, region select, Escape cancel, save, clipboard, notification.
- Wallpapers (Hyprland stills, mpvpaper videos) pausing behind the lock screen.
- Performance and soak on the reference machine.
