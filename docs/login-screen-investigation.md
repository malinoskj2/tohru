# Tohru Login Screen (Greeter) Investigation

## Executive Summary

**Recommendation:** Build a dedicated `tohru-greeter` mode for Tohru backed by **`greetd`** on NixOS, hosted inside a **minimal, isolated bare Hyprland session** (with **Sway** as an optional lightweight/VM backend, and **`cage`** only if we add an optional-layer-shell patch to Pleamar).

Key findings from inspecting Tohru's lock screen (`0026-lock-authority`, `0027-lock-pam`, `0028-lock-views-together`, `bin/tohru-lock`, `shell/lock/`), Pleamar 0.7.27's source, NixOS 26.05's display-manager modules, and the host configuration in `~/nix`:

1. **`greetd` is the only natural fit.** SDDM is coupled to Qt/QML (`sddm-greeter-qt6`) and GDM is coupled to GNOME Shell/Mutter; neither can render a Pleamar scene. `greetd` is a tiny Rust daemon that owns VT1 and PAM, hands the greeter a UNIX socket (`GREETD_SOCK`), and launches the authenticated session once the greeter exits.
2. **Pleamar 0.7.27 currently panics under `cage`.** `src/platform/wayland.rs` unconditionally calls `LayerShell::bind(&globals, &qh).expect("the compositor has no layer-shell")`, and `cage` does not implement `zwlr_layer_shell_v1`. Even after patching that, `cage` exposes only one maximized `xdg_toplevel` (no per-monitor layout or output rotation for `home`'s portrait `DP-1` + 360 Hz `DP-2` pair, and Pleamar popups do not yet attach to `kind: window`). A bare Hyprland session (or Sway) already provides `zwlr_layer_shell_v1`, per-output placement/rotation, and full GPU/dmabuf parity with the real desktop.
3. **Lock screen reuse is high for visuals and security design, zero for `libpam` calls.**
   - **Not reused:** `0027-lock-pam` / `platform/auth.rs` — a `greetd` greeter runs as the unprivileged `greeter` user and must **not** call PAM directly; `greetd` owns PAM as root and exposes it over `GREETD_SOCK`.
   - **Reused verbatim:** All 5 WGSL shaders (`tank.wgsl`, `glass.wgsl`, `veil.wgsl`, `bar/ribbon.wgsl`, `bar/fill.wgsl`), the `LockDots` component, animation springs, palette tokens, and the static-file loading path of `backdrop.plm` (`image lock_shot = from shot, 0, 0` already loads absolute PNG/JPEG/WebP/SVG paths via `src/text.rs:load_live`).
   - **Reused as architecture:** `0026-lock-authority` (`bindings/lock.rs`) is the exact blueprint for `tohru_greeter` — native-only password buffer with volatile zeroing, bullets-only text to Luau, refused `--say` orders, no hot-reload or self-relaunch, and `pleamar --check` preflight with a zero-shader `plain.plm` fallback.
4. **Seven concrete runtime/Pleamar gaps** must be closed: a `greetd` IPC + authority module (`serde_json` is already in `runtime/Cargo.toml`), Wayland/X11 session `.desktop` discovery, Caps Lock / XKB layout exposure in `wayland.rs`, hardened `TOHRU_GREETER=1` guards (including suppressing `right_click_quits` and missing-`HOME` settings errors), multi-output centering on `kind: panel` surfaces, static wallpaper luma sampling for light/dark glass polarity (`pk`), and NixOS Home Manager login-shell environment handoff.
5. **Estimated effort:** ~5.5–8 focused engineering days for the full implementation, NixOS module, and VM test suite (~3.5–4.5 days for an MVP with `flooded-monolith` + `plain` on Hyprland).

---

## 1. How a Login Screen Runs on NixOS

### 1.1 Display Manager Comparison: `greetd` vs. SDDM vs. GDM

| Criterion | `greetd` | SDDM (`wayland.enable`) | GDM |
|---|---|---|---|
| **Greeter UI tech** | Any process (Pleamar, GTK, TUI) speaking `GREETD_SOCK` | Qt6/QML (`sddm-greeter-qt6`) | GNOME Shell / GJS / Mutter |
| **Fits a Pleamar scene?** | **Yes — native fit** | No (would require QML rewrite or unsupported external helper) | No (hardwired to GNOME Shell) |
| **Auth & session owner** | `greetd` (root) owns PAM (`security.pam.services.greetd`), logind seat, and user session spawn | `sddm` daemon (root) | `gdm` daemon (root) |
| **Wayland compositor** | Pluggable (`Hyprland`, `sway`, `cage`, etc.) | `weston` or `kwin_wayland` | `mutter` |
| **NixOS 26.05 footprint** | Tiny Rust binary; creates `greeter` user; enables `services.displayManager` session data | Pulls Qt6 + kwin/weston + sddm PAM stack | Pulls GNOME Shell, Mutter, AccountsService, PulseAudio/PipeWire |
| **TTY / VT handling** | Fixed to `tty1` (`Conflicts=getty@tty1.service`, `autovt@tty1` off); `tty2–tty6` stay live | Dynamic VT allocation | Dynamic VT allocation |

**Verdict:** `greetd` is unambiguously the right choice. On NixOS 26.05 (`nixos/modules/services/display-managers/greetd.nix`):

- `services.greetd.enable = true` automatically enables `services.displayManager.enable = lib.mkDefault true`, which aggregates `.desktop` files from `services.displayManager.sessionPackages` into `${config.services.displayManager.sessionData.desktops}/share/{wayland-sessions,xsessions}`.
- `programs.hyprland.enable = true` (already enabled in `~/nix/hosts/common/optional/hyprland.nix`) already appends `cfg.package` to `services.displayManager.sessionPackages`, producing `hyprland.desktop` (`Exec=.../bin/start-hyprland`) and `hyprland-uwsm.desktop` (`TryExec=.../bin/uwsm`).
- `services.greetd.vt` was **removed** in NixOS 26.05 — VT is fixed to `tty1` (`settings.terminal.vt = 1`), `autovt@tty1` is disabled, and `systemd.services.greetd.restartIfChanged = false` prevents `nixos-rebuild switch` from killing an active desktop session.
- `security.pam.services.greetd` is configured with `startSession = true` and `allowNullPassword = true`, so `pam_systemd.so` creates a real logind session with `XDG_RUNTIME_DIR=/run/user/<greeter-uid>` and `uaccess` ACLs on `seat0` devices, and `pam_env.so` reads `/etc/pam/environment` (`environment.sessionVariables`).
- `users.users.greeter` is created automatically as `{ isSystemUser = true; group = "greeter"; }` (note: its home defaults to `/var/empty`, which is read-only).

### 1.2 Wayland Compositor Under `greetd`: `cage` vs. Minimal `sway` vs. Bare `Hyprland`

`greetd` does not run a compositor itself; `default_session.command` launches a Wayland compositor that runs the greeter client and exits when the greeter exits.

| Criterion | `cage` | Minimal `sway` | Bare `Hyprland` |
|---|---|---|---|
| **`zwlr_layer_shell_v1`** | **No** — Pleamar 0.7.27 panics on startup (`wayland.rs:2100`) | **Yes** — Pleamar works unmodified | **Yes** — Pleamar works unmodified |
| **Pleamar surface kind** | Requires `kind: window` + optional `LayerShell` patch; no `xdg_popup` on windows (`wayland.rs:2347`) | `kind: panel` (`size: full, full; level: overlay; keyboard: exclusive`) | `kind: panel` (`size: full, full; level: overlay; keyboard: exclusive`) |
| **NVIDIA 615.78.08 (`home`)** | wlroots on NVIDIA; no per-output config | wlroots on NVIDIA; requires `--unsupported-gpu` flag | **Aquamarine — already validated on `home` and `katana`** |
| **Multi-monitor (`DP-1` portrait 1080×1920 + `DP-2` 1920×1080@360)** | Poor: `-m last` blanks other screens (and may pick unrotated `DP-1`); `-m extend` stretches one window across both | Good: explicit `output` transform/mode/pos config | **Best:** same monitor syntax & transforms as the real desktop |
| **Keyboard layout & repeat** | Env vars only (`XKB_DEFAULT_LAYOUT`, etc.) | Explicit `input type:keyboard` config + `swaymsg` layout switching | Explicit `input { kb_layout = ... }` + `hyprctl switchxkblayout` |
| **VT switch (`Ctrl+Alt+F2`)** | Only with `-s` flag | Built-in | Built-in |
| **Sandbox / VM testing** | Possible if patched | **Easiest** (`WLR_BACKENDS=headless` / `virtio-gpu` / already used by `tests/bar/native.py`) | Works in QEMU VM (`virtio-gpu`) and nested sandbox |
| **Extra dependency** | Adds `pkgs.cage` + wlroots | Adds `pkgs.sway` + wlroots | **Zero extra compositor** (already installed by `programs.hyprland`) |

**Recommendation:**
- **Default on Hyprland hosts (`home`, `katana`):** **Bare `Hyprland`** launched with a minimal, Nix-store-generated config file (`Hyprland -c ${greeterHyprConf}`), disabling the Hyprland logo/splash, disabling all user plugins and app keybinds, applying only the declared `monitor` and `input` settings, and running `exec-once = ${greeterScript}; hyprctl dispatch exit`. Because `home` runs NVIDIA 615.78.08 with a portrait side monitor (`DP-1`, `transform = 1`) and a 360 Hz main monitor (`DP-2`), bare Hyprland guarantees identical DRM/Vulkan/dmabuf behavior with zero wlroots-on-NVIDIA surprises.
- **Secondary / VM / non-Hyprland option:** **Minimal `sway`** (`sway --config ${greeterSwayConf}`, adding `--unsupported-gpu` if NVIDIA is present). Because Tohru's native test suite (`tests/bar/native.py`) already runs Pleamar on headless Sway, supporting both `hyprland` and `sway` in the NixOS module is trivial (~15 lines of Nix) and makes QEMU VM and CI testing rock-solid.
- **`cage`:** Not recommended as the primary target because of the `LayerShell::bind` panic and weak multi-monitor support, though making `LayerShell` optional in Pleamar is a small patch (~30 lines) if a single-monitor kiosk option is ever wanted later.

### 1.3 What a Greeter Must Do

#### A. Authentication Over `greetd` IPC (`GREETD_SOCK`)
`greetd` passes the socket path in `GREETD_SOCK`. Every message in both directions is framed as:

```text
[ 4-byte native-endian u32 byte length ][ UTF-8 JSON payload ]
```

The four client requests and three daemon responses are:

1. **`create_session`**: `{"type":"create_session","username":"jesse"}`
   - Starts a PAM transaction in `greetd`.
   - Returns `auth_message` (most commonly `auth_message_type: "secret"`, `auth_message: "Password: "`), `success` (e.g., passwordless/nullok login), or `error` (`error_type`: `"auth_error"` or `"error"`, `description`).
2. **`post_auth_message_response`**: `{"type":"post_auth_message_response","response":"..."}` (or `null` for `info`/`error` prompts)
   - May return another `auth_message` (multi-step PAM: U2F/FIDO2 touch prompt as `info`, TOTP/2FA as `visible`/`secret`, password-expiry warning), `success`, or `error`.
3. **`start_session`**: `{"type":"start_session","cmd":[...],"env":[...]}`
   - Tells `greetd` what command and extra environment variables to run once authenticated.
   - Returns `success` or `error`.
   - **Critical lifecycle rule:** `greetd` does **not** launch the user session until the greeter session exits! After `start_session` returns `success`, `tohru-greeter` must finish its short exit transition (≤ 400–500 ms) and exit `0`, which triggers `hyprctl dispatch exit` (or `swaymsg exit`) so the compositor terminates cleanly and hands VT1 back to `greetd`.
4. **`cancel_session`**: `{"type":"cancel_session"}`
   - Must be sent when switching users, pressing Escape mid-conversation, or resetting after an `auth_error` before calling `create_session` again.

#### B. Session Selection & the NixOS + Home Manager Environment Gotcha
- **Discovery:** Scan `${config.services.displayManager.sessionData.desktops}/share/wayland-sessions` (and optionally `xsessions`), plus any directories in `TOHRU_GREETER_SESSIONS_DIRS` / `XDG_DATA_DIRS`. Parse INI fields `Name`, `Exec`, `TryExec`, `DesktopNames`, `Hidden`, `NoDisplay`; skip entries whose `TryExec` binary does not exist on disk or `PATH` (for example, `hyprland-uwsm.desktop` specifies `TryExec=.../bin/uwsm`, which should only appear if UWSM is installed).
- **Environment variables on `start_session`:** Populate `XDG_SESSION_TYPE=wayland` (or `x11` for `xsessions`), plus `XDG_CURRENT_DESKTOP` and `XDG_SESSION_DESKTOP` from `DesktopNames`.
- **Important NixOS + Home Manager handoff detail:** `greetd` spawns `start_session.cmd` directly via `execve` after PAM `open_session` — **not** through a login shell. On NixOS, `pam_env.so` reads `/etc/pam/environment`, which provides `environment.sessionVariables` (`GBM_BACKEND=nvidia-drm`, `NIXOS_OZONE_WL=1`, `LIBVA_DRIVER_NAME=nvidia`, etc.), **but does not source Home Manager's `hm-session-vars.sh`** (`TOHRU_MANAGED_CONFIG`, `XDG_SCREENSHOTS_DIR`, `TERMINAL`, `home.sessionPath`). Furthermore, `nix/home-manager.nix` (`programs.tohru.systemd.enable = true`) requires `WAYLAND_DISPLAY`, `PATH`, and `XDG_DATA_DIRS` to be imported into `systemd --user` before `graphical-session.target` starts.
  - **Two clean solutions:**
    1. **Recommended (for `~/nix`):** Enable `programs.hyprland.withUWSM = true` and select `Hyprland (uwsm-managed)` — UWSM sources the user's login shell environment and imports `WAYLAND_DISPLAY`, `PATH`, and `XDG_DATA_DIRS` into `systemd --user` automatically.
    2. **General fallback in `tohru-greeter`:** Wrap non-UWSM session `Exec` lines in a login shell wrapper (e.g., `["/bin/sh", "-lc", "exec ..."]` or the user's login shell) so `~/.profile` / `/etc/profile` and Home Manager session variables are always loaded even without UWSM.
- **Persistence:** Remember the last successfully logged-in `(username, session_id)` in `/var/lib/tohru-greeter/state.json` (directory created by `systemd.tmpfiles.rules`, owned `greeter:greeter`, mode `0750`).

#### C. User List
- Tohru **already has** a bounded, safe `/etc/passwd` reader in `runtime/src/info/system.rs` (`accounts()`, capped at 128 KiB / 256 accounts, never reads `/etc/shadow`).
- For the greeter, filter entries with:
  - `uid >= 1000` (or `UID_MIN` from `/etc/login.defs`) and `uid < 30000` (critical on NixOS: `nixbld*` build users start at UID `30001`, which `sddm.nix` handles via `MaximumUid = config.ids.uids.nixbld`, and `nobody` is `65534`).
  - Shell not in `/run/current-system/sw/bin/nologin`, `/sbin/nologin`, `/usr/sbin/nologin`, `/bin/false`, `/usr/bin/false`.
  - Username not in `services.displayManager.hiddenUsers` (e.g., `nobody`, `greeter`).
- **UX:** On a single-user workstation like `home` (`jesse`), pre-select `jesse` (or the last remembered user) immediately and focus the password field on startup — zero extra clicks, matching the lock screen's instant-typing flow. Support cycling/clicking users when multiple accounts exist, plus an "Other user…" manual username field so hidden/custom accounts are always reachable.

#### D. Power Actions (Sleep, Restart, Shut Down)
- The lock screen currently supports Sleep (`systemctl suspend`) and two-click Restart (`systemctl reboot`). A login screen should add **Shut Down** (`systemctl poweroff`) with the same two-click arming (or confirmation modal like `shell/session/menu.plm`).
- **NixOS Polkit requirement:** Running as `greeter`, `systemctl suspend/reboot/poweroff` calls `org.freedesktop.login1` over the system D-Bus, which checks Polkit. If another user or SSH/TTY session is active (or under stricter logind seat checks), `*-multiple-sessions` is denied without interactive auth. The NixOS module must install a scoped `security.polkit.extraConfig` rule granting `org.freedesktop.login1.{suspend,reboot,power-off}` (and optionally `*-multiple-sessions`, behind a module option) to `subject.user == "greeter" && subject.local && subject.active`.

#### E. Running Before Any User Session as `greeter`
- **No user session bus:** `DBUS_SESSION_BUS_ADDRESS` is unset; there is no desktop portal, no MPRIS media, no PipeWire/PulseAudio user daemon, no `hypridle`, and no notification daemon.
- **No access to `/home/jesse`:** `/home/jesse` has permissions `0700` (`drwx------`). The `greeter` user cannot read `~/.config/tohru/config.toml`, `~/.wallpapers/`, or `~/.local/share/fonts/`. Every asset the greeter reads (config, wallpaper, fonts, scenes) must live in `/nix/store`, `/etc`, or `/run/current-system/sw`.
- **Read-only `HOME`:** `users.users.greeter` defaults to `home = /var/empty`. Writable caches (e.g., Mesa/NVIDIA shader cache, fontconfig cache if needed) should use a private runtime dir under `$XDG_RUNTIME_DIR` or `/var/cache/tohru-greeter`.
- **GPU access:** `pam_systemd` grants `uaccess` ACLs on VT1, and adding `extraGroups = [ "video" "render" ]` on `users.users.greeter` provides belt-and-suspenders DRI/Vulkan access.

---

## 2. Lock Screen Reuse & Pleamar/Runtime Gaps

### 2.1 Component-by-Component Reuse Audit

| Lock screen component | Reusable? | Details |
|---|---|---|
| **`0027-lock-pam` (`platform/auth.rs`)** | **No** | Do not reuse for login auth. `greeter` cannot verify other users' passwords via `/etc/shadow`; `greetd` owns PAM as root over `GREETD_SOCK`. |
| **`0026-lock-authority` (`bindings/lock.rs`)** | **Architecture reused** | Mirror its exact security model in a new `bindings/greeter.rs` (`TOHRU_GREETER=1`): pre-allocated 1024-byte secret buffer in `render.rs`, volatile zeroing on clear/submit/drop, bullets-only `••••` sent to Luau, async worker thread talking to `GREETD_SOCK`, refuse `--say`, disable file watch & binary relaunch. |
| **`0028-lock-views-together` (`gpu.rs`)** | **Yes (already in runtime)** | Already handles clipping across multiple heterogeneous output views (e.g., portrait `DP-1` + landscape `DP-2`). |
| **WGSL shaders (`tank.wgsl`, `glass.wgsl`, `veil.wgsl`, `bar/ribbon.wgsl`, `bar/fill.wgsl`)** | **100% reusable** | Pure procedural / group-input shaders with zero session or compositor dependencies. |
| **`shell/lock/state.plm` & `field.plm` & `styles/common.plm`** | **~85% reusable** | Reuse all springs (`unlock`, `wake`, `err`, `shake`, `kick`, `dots`), palette tokens, `LockDots`, icons, and hidden secret `input password`. Add greeter facts/texts (`user_count`, `session_name`, `prompt_text`, `prompt_visible`, `caps_lock`, `kb_layout`, `power_armed`) and drop `lock.held`, `engaged`, `ws_*`, `media_*`. |
| **`shell/lock/backdrop.plm`** | **Yes (minor adaptation)** | `image lock_shot = from shot, 0, 0` **already supports static absolute file paths** (`src/text.rs:547`) and uploads large wallpapers to their own GPU texture (`self.picture(...)`). Set `text.shot` to a static wallpaper path in `/nix/store` or fall back to the built-in dusk gradient (`not has_shot`). Note: Pleamar's `image` crate supports JPEG, PNG, WebP, GIF, and SVG — **not `.mp4`**. For Jesse's video wallpapers (`~/nix/users/jesse/hyprland-desktop/wallpaper.nix`), extract a still frame with `ffmpeg` at Nix build time or point to a static image. |
| **`bin/tohru-lock` wrapper pattern** | **Pattern reused** | Reuse `pleamar --check` preflight + fallback to zero-shader `plain.plm`, plus a third-tier TUI fallback (`tuigreet`/`agreety`) if graphics fail altogether. Drop `flock` / `SetLockedHint` / `ext-session-lock` retry semantics. |

#### Per-Style Reuse Audit (`shell/lock/styles/`)

| Style | Reuse Status | Required Adaptation for Greeter |
|---|---|---|
| `flooded-monolith` *(owner's current `lock.style`)* | **Direct reuse** | Change `"Type password to unlock"` / `"Press Return to unlock"` copy; mount shared greeter chrome (user/session/power/CapsLock bar). |
| `still-glass` | **Direct reuse** | Change `"Unlocking…"` copy; mount greeter chrome. |
| `ribbon-key` | **Direct reuse** | Change `"Type to unlock"` / `"Unlocked"` copy; mount greeter chrome. |
| `drifting-frost-veil` | **Direct reuse** | Works cleanly over static wallpaper (`has_shot`) or dusk gradient; mount greeter chrome. |
| `horizon-meniscus` | **Direct reuse** | Works cleanly over static wallpaper (`has_shot`) or dusk gradient; mount greeter chrome. |
| `refractive-typographic-lens` | **Direct reuse** | Works cleanly over static wallpaper; change `"Lens clearing…"` copy; mount greeter chrome. |
| `corner-anchor` | **Direct reuse** | Change `"Locked"` / `"Authenticated"` copy; ensure responsive anchoring so portrait monitors don't clip `x = 104`. |
| `plain` | **Direct reuse (essential)** | Zero-shader fallback; add minimal clickable user/session/power text rows so recovery mode stays fully functional. |
| `unfolding-glass-capsule` | **Moderate edit** | Already has Sleep + Restart buttons; replace the `"NOW PLAYING"` / `media_title` section with User + Session selectors and add Shut Down. |
| `edge-glass-drawer` | **Moderate edit** | Replace `"{host} · {output_name} · Session locked"` and `"NOW PLAYING"` / `media_title` with host, session selector, user switcher, and power controls. |
| `bar-drop-curtain` + `styles/bar.plm` | **Moderate edit** | `bar.plm` paints a fake desktop workspace/status bar (`ws_active`, wifi/volume/bell/bluetooth). Replace with a real greeter top bar (hostname, keyboard layout, session pill, battery if present, power buttons), or omit `bar.plm` in greeter mode. |

**Shared layout note across all 10 styles:** Every lock style uses fixed `1920 × 1080` coordinates (`960`, `1920`, `1080`). Under `kind: lock`, Pleamar automatically centers that box on each monitor via `face.view_origin` (`wayland.rs:2313`). Under `kind: panel` (`size: full, full`), `view.origin` is `(0, 0)`, so the greeter scene must wrap the shared style parts in a centered group (`move: (screen.width - 1920) / 2, (screen.height - 1080) / 2`) while anchoring greeter chrome (session/user/power bar) to `screen.width` and `screen.height`.

### 2.2 Concrete Gaps in Pleamar 0.7.27 and Tohru Runtime

| # | Gap | Exact Source Location | What Needs to Change |
|---|---|---|---|
| **1** | **No `greetd` IPC client or greeter authority** | New: `runtime/src/greeter.rs`, `runtime/pleamar/bindings/greeter.rs`; patch `lib.rs`, `render.rs`, `logic_luau.rs` | Implement `GREETD_SOCK` length-prefixed JSON protocol (`std::os::unix::net::UnixStream` + existing `serde_json` — **zero new crates**). Gate on `TOHRU_GREETER=1`: guard secret input in `render.rs`, volatile-zero passwords, expose `tohru_greeter` Luau API (`users()`, `sessions()`, `start_auth(user)`, `respond_visible(text)`, `select_session(id)`, `cancel()`, `last_state()`) and facts (`greeter_state`, `greeter_prompt_type`). |
| **2** | **No Wayland/X11 session `.desktop` discovery** | `runtime/src/greeter.rs` (Pleamar's `apps` only scans `applications/`) | Bounded scanner for `{wayland-sessions,xsessions}/*.desktop` across `TOHRU_GREETER_SESSIONS_DIRS` and `XDG_DATA_DIRS`, honoring `TryExec`, `DesktopNames`, `Hidden`, `NoDisplay`. |
| **3** | **Caps Lock / Num Lock / XKB keyboard layout discarded** | `wayland.rs:2641–2661`, `scene.rs:1435–1440` | `KeyboardHandler::update_modifiers` receives `Modifiers` (which includes `caps_lock` and `num_lock`) and the active XKB `layout: u32`, and `update_keymap` receives the XKB keymap string — but `Mods` only stores `{ ctrl, alt, shift, logo }` and discards the rest. Patch to publish `keyboard.caps_lock` (fact) and `keyboard.layout` (text, parsed from XKB group names). This benefits `tohru-lock` too. |
| **4** | **Hardened greeter mode & no-user-session guards** | `lib.rs:303, 382, 439`, `wayland.rs:2402`, `logic_luau.rs:1404`, `bindings/settings_luau.rs:15–24`, `from_file.rs:249` | Extend the `tohru_lock::locker()` guards to `tohru_greeter::greeter()`: refuse `--say` (or skip creating the command socket), disable hot-reload and binary self-relaunch, skip `skill::refresh_quietly()`, **suppress `right_click_quits` (`wayland.rs:2402`)** so a stray right-click never kills a `kind: panel` greeter, and tolerate unset/read-only `HOME` in `tohru_settings` (loading `TOHRU_MANAGED_CONFIG` directly when `HOME` is unset or `/var/empty`). Wrapper also sets `PLEAMAR_NO_PORTAL=1` and `PLEAMAR_GENERIC=1`. |
| **5** | **Multi-output handling on non-lock surfaces** | `wayland.rs:1245–1331`, `render.rs:1045–1125`, `shell/greeter/` | `kind: panel` with `screens: all` mirrors one scene on every monitor without `face.view_origin` centering. Use either (a) a backdrop+clock surface (`screens: all; keyboard: none`) plus a primary interactive surface (`screens: preferred_output` or `screens: each` where pointer/focus activates the card; `keyboard: exclusive`), so `DP-1` portrait (`1080×1920`) shows a clean backdrop/clock without cropping edge styles while `DP-2` (`1920×1080`) hosts the full login UI. |
| **6** | **Static wallpaper luma sampling (`pk`)** | `shell/lock/controller.luau:96–113` | `lock` uses `capture.pixel` on a screencopy frame to decide light vs. dark ink (`pk`). For a static wallpaper file, add a tiny native helper (sampling 5 points from the image file via `image` crate, or read an explicit `greeter.light_backdrop` setting) so `pk` adapts automatically to light/dark wallpapers. |
| **7** | **Optional `LayerShell` *(only if supporting `cage`)*** | `wayland.rs:1278–1294, 2100` | Change `State.layers` from `LayerShell` to `Option<LayerShell>`, avoid pinning `set_min_size` to `1920×1080` on `kind: window`, and optionally call `window.set_fullscreen(None)`. Not needed when using bare Hyprland or Sway. |

**Running without Hyprland IPC:** Verified clean — `platform/hyprland.rs` is guarded by `hyprland::is_present() && env("PLEAMAR_GENERIC").is_none()`. With `PLEAMAR_GENERIC=1` and no `workspaces`/`window`/`capture`/`media` in the greeter scene's `permissions` block, Pleamar never opens Hyprland IPC sockets or session D-Bus connections.

---

## 3. NixOS Module Sketch

Tohru currently exports only `homeManagerModules.tohru` (`docs/nix.md`: *"There is no NixOS module"*). A greeter runs before any user logs in and configures system services (`greetd`, `polkit`, `tmpfiles`, `users`), so it belongs in a new NixOS module — e.g., `nix/nixos.nix`, exported from `flake.nix` as `nixosModules.tohru-greeter` (and `nixosModules.default`).

### 3.1 Sketch of `nix/nixos.nix`

```nix
{ config, lib, pkgs, ... }:
let
  cfg = config.services.tohru-greeter;
  format = pkgs.formats.toml { };

  # Read-only greeter config in /nix/store (world-readable by the `greeter` user).
  managedConfig = format.generate "tohru-greeter.toml" (lib.recursiveUpdate cfg.settings {
    greeter = lib.filterAttrs (_: v: v != null) {
      style = cfg.style;
      wallpaper = if cfg.wallpaper != null then toString cfg.wallpaper else null;
      preferred_output = cfg.preferredOutput;
      default_user = cfg.defaultUser;
      default_session = cfg.defaultSession;
      allow_power_actions = cfg.allowPowerActions;
      avatars = lib.mapAttrs (_: toString) cfg.avatars;
    };
  });

  sessionsDir = "${config.services.displayManager.sessionData.desktops}/share";

  # Wrapper that runs tohru-greeter with isolated env and plain/TUI fallbacks.
  greeterRun = pkgs.writeShellScript "tohru-greeter-run" ''
    export TOHRU_SHELL=1
    export TOHRU_GREETER=1
    export PLEAMAR_NO_PORTAL=1
    export PLEAMAR_GENERIC=1
    export TOHRU_MANAGED_CONFIG=${managedConfig}
    export TOHRU_GREETER_SESSIONS_DIRS=${lib.escapeShellArg sessionsDir}
    export TOHRU_GREETER_STATE_DIR=/var/lib/tohru-greeter
    export XDG_CACHE_HOME="''${XDG_RUNTIME_DIR:-/tmp}/tohru-greeter-cache"
    mkdir -p "$XDG_CACHE_HOME"
    exec ${cfg.package}/bin/tohru-greeter
  '';

  # Minimal isolated Hyprland config: no user plugins, no app keybinds, no logo.
  hyprlandConf = pkgs.writeText "tohru-greeter-hyprland.conf" ''
    misc {
      disable_hyprland_logo = true
      disable_splash_rendering = true
      force_default_wallpaper = 0
    }
    animations {
      enabled = false
    }
    ${lib.concatMapStringsSep "\n" (m:
      "monitor = ${m.output}, ${m.mode}, ${m.position}, ${toString m.scale}, transform, ${toString m.transform}"
    ) cfg.monitors}
    input {
      kb_layout = ${cfg.keyboard.layout}
      kb_variant = ${cfg.keyboard.variant}
      kb_options = ${cfg.keyboard.options}
      repeat_delay = ${toString cfg.keyboard.repeatDelay}
      repeat_rate = ${toString cfg.keyboard.repeatRate}
    }
    ${cfg.extraCompositorConfig}
    exec-once = ${greeterRun}; ${lib.getExe' cfg.hyprlandPackage "hyprctl"} dispatch exit
  '';

  swayConf = pkgs.writeText "tohru-greeter-sway.conf" ''
    ${lib.concatMapStringsSep "\n" (m:
      "output ${m.output} mode ${m.mode} pos ${lib.replaceStrings ["x"] [" "] m.position} scale ${toString m.scale} transform ${toString (m.transform * 90)}"
    ) cfg.monitors}
    input type:keyboard {
      xkb_layout ${cfg.keyboard.layout}
      xkb_variant "${cfg.keyboard.variant}"
      xkb_options "${cfg.keyboard.options}"
      repeat_delay ${toString cfg.keyboard.repeatDelay}
      repeat_rate ${toString cfg.keyboard.repeatRate}
    }
    ${cfg.extraCompositorConfig}
    exec "${greeterRun}; ${lib.getExe' pkgs.sway "swaymsg"} exit"
  '';

  compositorCmd =
    if cfg.compositor == "hyprland" then
      # Guard with a TUI fallback if the compositor itself fails to start (e.g. broken GPU driver).
      pkgs.writeShellScript "tohru-greeter-session" ''
        export XDG_CONFIG_HOME="''${XDG_RUNTIME_DIR:-/tmp}/tohru-greeter-xdg"
        mkdir -p "$XDG_CONFIG_HOME"
        ${lib.getExe cfg.hyprlandPackage} -c ${hyprlandConf}
        status=$?
        if [ "$status" -ne 0 ]; then
          exec ${lib.getExe pkgs.tuigreet} --time --sessions ${lib.escapeShellArg "${sessionsDir}/wayland-sessions"}
        fi
      ''
    else
      pkgs.writeShellScript "tohru-greeter-session" ''
        ${lib.getExe pkgs.sway} ${lib.optionalString cfg.swayUnsupportedGpu "--unsupported-gpu"} --config ${swayConf}
        status=$?
        if [ "$status" -ne 0 ]; then
          exec ${lib.getExe pkgs.tuigreet} --time --sessions ${lib.escapeShellArg "${sessionsDir}/wayland-sessions"}
        fi
      '';
in {
  options.services.tohru-greeter = {
    enable = lib.mkEnableOption "Tohru Wayland login screen via greetd";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./default.nix { };
    };
    compositor = lib.mkOption {
      type = lib.types.enum [ "hyprland" "sway" ];
      default = if config.programs.hyprland.enable then "hyprland" else "sway";
    };
    hyprlandPackage = lib.mkOption {
      type = lib.types.package;
      default = config.programs.hyprland.package or pkgs.hyprland;
    };
    swayUnsupportedGpu = lib.mkOption {
      type = lib.types.bool;
      default = lib.elem "nvidia" config.services.xserver.videoDrivers;
    };
    style = lib.mkOption {
      type = lib.types.enum [
        "still-glass" "ribbon-key" "flooded-monolith" "drifting-frost-veil"
        "unfolding-glass-capsule" "edge-glass-drawer" "horizon-meniscus"
        "refractive-typographic-lens" "corner-anchor" "bar-drop-curtain" "plain"
      ];
      default = "flooded-monolith";
    };
    wallpaper = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Static image (PNG/JPEG/WebP/SVG) copied to the Nix store; null uses the dusk gradient.";
    };
    avatars = lib.mkOption {
      type = lib.types.attrsOf lib.types.path;
      default = { };
      description = "Optional per-user avatar images copied to the Nix store (overrides AccountsService / ~/.face).";
    };
    preferredOutput = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
    };
    defaultUser = lib.mkOption { type = lib.types.nullOr lib.types.str; default = null; };
    defaultSession = lib.mkOption { type = lib.types.nullOr lib.types.str; default = null; };
    monitors = lib.mkOption {
      type = lib.types.listOf (lib.types.submodule {
        options = {
          output = lib.mkOption { type = lib.types.str; };
          mode = lib.mkOption { type = lib.types.str; default = "preferred"; };
          position = lib.mkOption { type = lib.types.str; default = "auto"; };
          scale = lib.mkOption { type = lib.types.number; default = 1; };
          transform = lib.mkOption { type = lib.types.ints.between 0 7; default = 0; };
        };
      });
      default = [ ];
    };
    keyboard = {
      layout = lib.mkOption { type = lib.types.str; default = config.services.xserver.xkb.layout or "us"; };
      variant = lib.mkOption { type = lib.types.str; default = config.services.xserver.xkb.variant or ""; };
      options = lib.mkOption { type = lib.types.str; default = config.services.xserver.xkb.options or ""; };
      repeatDelay = lib.mkOption { type = lib.types.ints.positive; default = 260; };
      repeatRate = lib.mkOption { type = lib.types.ints.positive; default = 24; };
    };
    allowPowerActions = lib.mkOption { type = lib.types.bool; default = true; };
    allowMultipleSessionPower = lib.mkOption { type = lib.types.bool; default = false; };
    extraCompositorConfig = lib.mkOption { type = lib.types.lines; default = ""; };
    settings = lib.mkOption { type = format.type; default = { }; };
  };

  config = lib.mkIf cfg.enable {
    services.greetd = {
      enable = true;
      restart = true;
      settings.default_session = {
        user = "greeter";
        command = toString compositorCmd;
      };
    };

    # Belt-and-suspenders DRI/Vulkan access alongside pam_systemd uaccess.
    users.users.greeter.extraGroups = [ "video" "render" ];

    # Enable AccountsService so users can also set avatars imperatively (`SetIconFile`).
    services.accounts-daemon.enable = lib.mkDefault true;

    # Persist last-selected (user, session) cleanly outside /var/empty, and mirror declarative avatars.
    systemd.tmpfiles.rules = [
      "d /var/lib/tohru-greeter 0750 greeter greeter -"
    ] ++ lib.mapAttrsToList (user: path:
      "L+ /var/lib/AccountsService/icons/${user} - - - - ${path}"
    ) cfg.avatars;

    # Allow the local active greeter seat to suspend, reboot, or power off.
    security.polkit.enable = true;
    security.polkit.extraConfig = lib.mkIf cfg.allowPowerActions ''
      polkit.addRule(function(action, subject) {
        var actions = [
          "org.freedesktop.login1.suspend",
          "org.freedesktop.login1.reboot",
          "org.freedesktop.login1.power-off"
          ${lib.optionalString cfg.allowMultipleSessionPower '',
          "org.freedesktop.login1.suspend-multiple-sessions",
          "org.freedesktop.login1.reboot-multiple-sessions",
          "org.freedesktop.login1.power-off-multiple-sessions"''}
        ];
        if (subject.user == "greeter" && subject.local && subject.active && actions.indexOf(action.id) >= 0) {
          return polkit.Result.YES;
        }
      });
    '';
  };
}
```

### 3.2 Greeter User Access to Fonts, Wallpaper, and Config

- **Fonts:** Pleamar uses `cosmic-text` with the `fontconfig` feature (`Cargo.toml:54`). In `~/nix/hosts/common/optional/fonts.nix`, `sf-pro`, `sf-compact`, `sf-mono`, `ny`, `lato`, and `fira-code` are already installed via NixOS `fonts.packages` and registered system-wide in `/etc/fonts/fonts.conf`. The `greeter` user gets `SF Pro Display` and `SF Pro Text` with zero extra configuration. *(For external users of Tohru's NixOS module, document that required fonts must be in NixOS `fonts.packages`, not Home Manager `home.packages`.)*
- **Wallpaper:** Passing a path to `services.tohru-greeter.wallpaper` copies it into `/nix/store` (world-readable `0444`). For `home`, where desktop wallpapers are videos under `~/nix/users/jesse/hyprland-desktop/wallpapers/`, either point to a curated static image or derive a PNG frame at build time with `pkgs.runCommand` + `ffmpeg-headless`. Never reference a path under `/home/jesse/` (`0700`).
- **Config:** Generated into `/nix/store/...-tohru-greeter.toml` and injected via `TOHRU_MANAGED_CONFIG`. Runtime state (last user/session) lives in `/var/lib/tohru-greeter/` via `systemd.tmpfiles.rules`.

### 3.3 Consumer Snippet (Reference for `~/nix/hosts/home/configuration.nix` — Do Not Edit Yet)

```nix
{ inputs, ... }:
{
  imports = [ inputs.tohru.nixosModules.tohru-greeter ];

  services.tohru-greeter = {
    enable = true;
    style = "flooded-monolith";
    preferredOutput = "DP-2";
    defaultUser = "jesse";
    monitors = [
      { output = "DP-1"; mode = "1920x1080@144"; position = "0x0"; transform = 1; }
      { output = "DP-2"; mode = "1920x1080@360"; position = "1080x0"; }
    ];
    # Optional: static image or ffmpeg-extracted frame from desktop wallpapers
    # wallpaper = ./greeter-wallpaper.png;
  };
}
```

---

## 4. Risks, Lockout Recovery & Safe Testing

### 4.1 Failure Modes That Could Lock Out the Machine

| Risk | How It Happens | Mitigation |
|---|---|---|
| **GPU / Vulkan init crash under `greeter`** | Missing `render`/`video` ACL, broken NVIDIA ICD env, or wgpu panic on early boot → black screen or VT1 respawn loop. | Three-tier fallback in `tohru-greeter` + compositor wrapper: (1) styled scene after `pleamar --check`, (2) zero-shader `plain.plm`, (3) `exec tuigreet` TUI fallback on compositor/runtime non-zero exit. |
| **Scene / shader compile failure** | Typo or WGSL error in a style breaks styled scene startup. | Run `pleamar --check` before launching the styled scene (identical to `bin/tohru-lock`), falling back to `plain.plm`. |
| **Compositor fails to exit after `start_session`** | `greetd` waits indefinitely for the greeter session to terminate before launching Hyprland. | `tohru-greeter` exits `0` unconditionally after a bounded transition grace (≤ 600 ms, enforced natively in Rust just like `RELEASE_GRACE` in `bindings/lock.rs`), and the compositor config chains `; hyprctl dispatch exit` / `; swaymsg exit`. |
| **Session exits immediately after login** | Missing Home Manager env (`TOHRU_MANAGED_CONFIG`, `PATH`) or broken `Exec=` bounces user back to greeter. | Wrap non-UWSM `Exec` in a login shell (or use `withUWSM = true`), log session stderr to journal (`services.displayManager.logToJournal = true`), and verify in VM before host rollout. |
| **Stray right-click kills greeter** | Pleamar's `right_click_quits` (`wayland.rs:2402`) defaults to `true` for `kind: panel` scenes without a right-click rule. | Suppress `right_click_quits` whenever `TOHRU_GREETER=1`. |
| **Broken keyboard focus or Caps Lock confusion** | `keyboard: exclusive` not set, or Caps Lock invisibly on during password entry. | Enforce `keyboard: exclusive` on the primary greeter surface, expose `keyboard.caps_lock` indicator, and verify virtual keyboard input in native tests. |
| **Greeter user can launch apps or read user files** | Misconfigured compositor config leaves default Hyprland binds (`Super+Return` → terminal) active as `greeter`. | Minimal isolated `-c` compositor config with **no** default keybinds, no plugins, and `TOHRU_GREETER=1` refusing `--say`. |

### 4.2 Recovery Paths

1. **TTY2–TTY6 fallback (always available):** NixOS 26.05 binds `greetd` strictly to `tty1` (`Conflicts=getty@tty1.service`, `autovt@tty1.enable = false`). Pressing **`Ctrl+Alt+F2`** (through `F6`) switches to a standard Linux `getty` console where `jesse` can log in, inspect `journalctl -u greetd`, stop `greetd` (`sudo systemctl stop greetd`), or run `start-hyprland` directly.
2. **TUI greeter fallback on VT1:** If the Wayland compositor or `plain.plm` exits non-zero, the session wrapper falls back straight to `tuigreet` on VT1 so login remains possible without even switching TTYs.
3. **Limine boot-generation rollback:** `home` retains `maxGenerations = 10` in `~/nix/hosts/home/boot.nix`. If a boot ever wedged before `Ctrl+Alt+F2`, selecting the previous generation in Limine immediately restores TTY login.
4. **Safe rollout discipline:** Because `greetd.nix` sets `restartIfChanged = false`, even switching won't kill an active session — and using `nixos-rebuild boot` on first adoption lets you test a reboot cleanly with one-key rollback.

### 4.3 Testing Without Touching the Real Host

1. **Layer 1 — CPU-only unit & protocol tests (fast, no GPU/VM):**
   - Rust tests in `runtime/` using `std::os::unix::net::UnixStream::pair()` as an in-process mock `greetd`: test `create_session`, single- and multi-step `auth_message` (`secret`/`visible`/`info`/`error`), `auth_error` + `cancel_session` recovery, `start_session`, oversized/malformed JSON rejection, and volatile password zeroing.
   - Rust tests for `.desktop` session discovery (`TryExec` filtering, `DesktopNames`, `Hidden`/`NoDisplay`) and `/etc/passwd` user filtering (including excluding UID `30001` `nixbld` and `nologin` shells).
   - `mlua` controller tests (mirroring `tests/lock/logic.rs`) and `naga` WGSL shader checks (`tests/lock/shaders.rs`).
2. **Layer 2 — Headless Wayland native tests in the sandbox (mirroring `tests/bar/native.py`):**
   - Start a private headless `sway` (`WLR_BACKENDS=headless`, testing both single `1920×1080` and dual `1080×1920` + `1920×1080` outputs) and a mock `GREETD_SOCK` UNIX socket server in Python.
   - Launch `bin/tohru-greeter` with `TOHRU_GREETER=1`, drive typing/user-switch/session-switch/Escape/Return through the existing virtual keyboard (`tests/launcher/keyboard.c` + `wtype`) and virtual pointer (`tests/bar/pointer.c`), capture screenshots with `grim` into `.tmp/`, and assert:
     - Wrong password triggers shake + error state without exiting.
     - Right password sends the real password over `GREETD_SOCK` (never logged or exposed to Luau), sends `start_session` with the chosen session's `cmd` and `env`, and exits `0` within the transition grace.
     - `--say` commands are refused; right-click does not exit; broken styled scene falls back to `plain.plm`.
3. **Layer 3 — Full NixOS VM end-to-end test (`nixos-rebuild build-vm` / `pkgs.testers.runNixOSTest`):**
   - Define a `nix/tests/greeter-vm.nix` test (run via `resource-run --cpu 4 --ram 8GiB --project tohru --name "greeter VM test" --category test -- nix build ...`) booting a QEMU NixOS guest with real `systemd`, real `greetd`, real `greeter` user, real PAM, real Polkit, and `sway` (or `Hyprland`) on `virtio-gpu`.
   - Verify: VT1 displays `tohru-greeter` → `Ctrl+Alt+F2` reaches `getty@tty2` → wrong password fails cleanly → valid login launches the target Wayland session with `WAYLAND_DISPLAY` and `TOHRU_MANAGED_CONFIG` → logout returns to `tohru-greeter` → Polkit permits `systemctl poweroff` as `greeter`.
   - For interactive visual testing without touching the host display, `nix build .#checks.x86_64-linux.greeter-vm` / `nixos-rebuild build-vm` can run headless in QEMU with VNC/SPICE or inside the sandbox Wayland display (`sandbox-wayland-display`).

---

## 5. Recommended Implementation Plan

| Phase | Scope | Key Files | Rough Effort |
|---|---|---|---|
| **1. Native greeter core & `greetd` IPC** | Implement `GREETD_SOCK` client, session `.desktop` scanner, `/etc/passwd` user filter, and last-state persistence in `runtime/src/greeter.rs`. Implement `runtime/pleamar/bindings/greeter.rs` + `0031-greeter-authority.patch` (`TOHRU_GREETER=1`, guarded secret buffer + volatile zeroing, refused `--say`, no hot-reload/relaunch, no `right_click_quits`, missing-`HOME` settings grace, static wallpaper luma helper). Add full Rust unit tests with mock `UnixStream::pair()` `greetd`. | `runtime/src/greeter.rs`, `runtime/src/lib.rs`, `runtime/pleamar/bindings/greeter.rs`, `runtime/pleamar/patches/0031-greeter-authority.patch` | **1.5–2 days** |
| **2. Keyboard indicators *(+ optional `cage` patch)*** | Expose `keyboard.caps_lock` (and `num_lock`) and active XKB `keyboard.layout` in `wayland.rs` + `render.rs` (also improving `tohru-lock`). Optionally make `LayerShell::bind` non-fatal for `kind: window` if `cage` support is desired. | `runtime/pleamar/patches/0032-keyboard-indicators.patch` *(optional `0033-optional-layer-shell.patch`)* | **0.5–1 day** |
| **3. Greeter scenes & shared lock-style refactor** | Extract lock-agnostic parts of `shell/lock/styles/*` (parameterize copy strings like `"Unlock"` vs `"Sign in"` and replace `media_title`/`bar.plm` slots cleanly). Build `shell/greeter/` (`greeter.plm`, `plain.plm`, `controller.luau`, greeter top/bottom chrome for user picker, session picker, Caps Lock/layout pill, and Sleep/Restart/Shut Down controls, plus multi-output primary/secondary handling and centered 1920×1080 wrapper). | `shell/greeter/*`, shared parts under `shell/lock/styles/*`, `tests/greeter/logic.rs` | **1.5–2 days** |
| **4. Launch wrapper & NixOS module** | Add `bin/tohru-greeter` (with `pleamar --check` + `plain.plm` fallback), package it in `nix/default.nix`, author `nix/nixos.nix`, and export `nixosModules.tohru-greeter` + `nixosModules.default` in `flake.nix`. Update `docs/nix.md` and `docs/architecture.md`. | `bin/tohru-greeter`, `nix/default.nix`, `nix/nixos.nix`, `flake.nix`, `docs/nix.md` | **1 day** |
| **5. Automated native & NixOS VM verification** | Add headless Sway + mock-`GREETD_SOCK` native test (single + portrait/landscape dual-output screenshots, virtual keyboard auth flow, security assertions) and a NixOS VM integration test (`nix/tests/greeter-vm.nix`) verifying real `greetd`, PAM, Polkit power actions, and TTY2 fallback. | `tests/greeter/native.py`, `nix/tests/greeter-vm.nix` | **1–1.5 days** |
| **6. Host adoption *(separate change in `~/nix`)*** | Enable `services.tohru-greeter` on `home` (and optionally `katana`), decide UWSM vs. login-shell session handoff, pick/extract a static wallpaper, verify with `nixos-rebuild build-vm`, then adopt via `nixos-rebuild boot`. | `~/nix/hosts/home/configuration.nix` (out of scope for this repo) | **0.5 day** |

**Total rough effort:** **~5.5–8 focused days** end-to-end (or **~3.5–4.5 days** for an MVP supporting `flooded-monolith` + `plain` on bare Hyprland before adapting all 10 styles and adding Sway/multi-style polish).

### Open Decisions — Resolved

1. **Scope:** Adapt **all 10 lock styles + `plain`** (full parity with `tohru-lock`, not an MVP subset).
2. **Session handoff:** Support both cleanly — recommend enabling **`programs.hyprland.withUWSM = true`** on NixOS hosts (upstream Hyprland's cleanest systemd lifecycle: imports `WAYLAND_DISPLAY`/`PATH`/`XDG_DATA_DIRS` and tears down `graphical-session.target` cleanly on logout so re-login from `tohru-greeter` leaves no stale services), while having `tohru-greeter` automatically wrap non-UWSM `.desktop` `Exec` commands in a login shell so Home Manager session variables (`TOHRU_MANAGED_CONFIG`, `home.sessionPath`) still work on non-UWSM hosts.
3. **Backdrop:** Default to Tohru's built-in **dusk gradient** (`wallpaper = null`), keeping static image `wallpaper` as an optional NixOS module setting for later.
4. **Optional user avatars:** Add first-class optional avatar support (detailed in Section 6 below).

---

## 6. Optional User Avatars

### 6.1 How Other Linux GUI Shells & Display Managers Handle Avatars

Across Linux desktop environments and Wayland shells, two mechanisms dominate:

| Mechanism | Used By | How It Is Set | Works Across `0700` `$HOME` for `greeter`? |
|---|---|---|---|
| **AccountsService** (`org.freedesktop.Accounts`, `/var/lib/AccountsService/icons/<user>`) | GNOME/GDM, KDE Plasma/SDDM, ReGreet, Noctalia (`noctalia-greeter`), DankMaterialShell (`dms-greeter`) | Desktop Settings UI or `busctl` calls `org.freedesktop.Accounts.User.SetIconFile(path)`. `accounts-daemon` (root, gated by Polkit `org.freedesktop.accounts.change-own-user-data`) copies the image to world-readable `/var/lib/AccountsService/icons/<user>` and records `Icon=` in `/var/lib/AccountsService/users/<user>`. | **Yes** — specifically designed to bridge the unprivileged greeter user and private home directories. |
| **`~/.face` / `~/.face.icon`** | Quickshell rices (Caelestia, Noctalia in-session), Hyprlock, legacy LightDM/SDDM | User copies a square PNG/JPG to `~/.face` (or manages it via Home Manager `home.file.".face"`). | **No** when `/home/<user>` is `0700` (like `home`'s `/home/jesse`); works inside the user's own session (`tohru-lock`) or on `0711`/`0755` homes. |
| **Declarative system path** (`/nix/store` or `FacesDir`) | NixOS SDDM/ReGreet configs (`systemd.tmpfiles.rules` symlinking `/var/lib/AccountsService/icons/<user>`) | Set in NixOS configuration (`avatars.<user> = ./avatar.png;`). | **Yes** — `/nix/store` files are world-readable (`0444`). |

**Key takeaway:** Modern Wayland shells with greeters (Noctalia, DankMaterialShell) pair **AccountsService** for system-wide/pre-login persistence with **`~/.face`** as an in-session convenience, while NixOS users also expect a **declarative Nix option**.

### 6.2 Recommended Avatar Design for Tohru

#### Resolution Order (First Readable Valid Image Wins; Absent = No Avatar Drawn)
Resolve per username in `runtime/src/greeter.rs` (and share with `tohru-lock`):

1. **Declarative NixOS config** (`services.tohru-greeter.avatars.<user>` → path in `/nix/store` written into `tohru-greeter.toml`).
2. **AccountsService** — parse `Icon=` from `/var/lib/AccountsService/users/<user>`, else check `/var/lib/AccountsService/icons/<user>`. *(Plain file reads — no D-Bus required in the greeter, works even if `accounts-daemon` is stopped!)*
3. **System faces directory** — `/etc/tohru/avatars/<user>.{png,jpg,jpeg,webp,svg}`.
4. **Home directory fallback** — `<home>/.face` and `<home>/.face.icon` (succeeds in `tohru-lock`, and in `tohru-greeter` if home is traversable; silently skipped on `EACCES`).

#### How Users Set an Avatar
Support both NixOS-declarative and imperative workflows cleanly:

- **Declarative (NixOS):**
  ```nix
  services.tohru-greeter.avatars.jesse = ./avatar.png;
  ```
  The NixOS module records the store path in `tohru-greeter.toml` and (when `services.accounts-daemon.enable` is on) links `/var/lib/AccountsService/icons/jesse` via `systemd.tmpfiles.rules` so other Linux apps and `tohru-lock` automatically see the same avatar.
- **Imperative (CLI & Settings UI):**
  Enable `services.accounts-daemon.enable = lib.mkDefault true;` in `nix/nixos.nix` (matching NixOS's `programs.regreet` module). Provide `tohru avatar set <file>` / `tohru avatar clear` (and an optional Appearance/Account row in `tohru-settings`), calling `org.freedesktop.Accounts.User.SetIconFile` over system D-Bus (plus optionally writing `~/.face` for compatibility with non-AccountsService tools). No custom privileged helper is required.

#### Rendering in Pleamar (Zero Runtime Patches Needed)
- **Completely optional:** When no avatar is found (`fact.has_avatar == false`), **no placeholder silhouette is drawn** — every style preserves its exact clean typography and layout.
- **When set (`fact.has_avatar == true`):** Pleamar's `image user_avatar = from avatar_path, 0, 0` (`src/text.rs:load_live`) already loads arbitrary PNG/JPEG/WebP/SVG files, and `clip box { from: x, y; size: d, d; corner: d / 2 }` with `fit: cover` clips it into a crisp circle with a subtle `1 px` `ink` border:
  - **Centered / typographic styles** (`still-glass`, `ribbon-key`, `horizon-meniscus`, `refractive-typographic-lens`, `bar-drop-curtain`, `plain`): compact `24–28 px` circular avatar inline beside the username (or `48 px` above the field in the user-switcher pill).
  - **Card / drawer / corner styles** (`flooded-monolith`, `unfolding-glass-capsule`, `edge-glass-drawer`, `corner-anchor`, `drifting-frost-veil`): compact circular avatar paired directly with the card's `user` header.
- **Bonus:** Sharing the same `AvatarBadge` component lets `tohru-lock` display the exact same optional user avatar consistently.

