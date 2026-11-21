# Wayland Compositor CI Investigation for Tohru

## Executive Summary

Testing Tohru's bar (`shell/bar.plm`, launched via `bin/tohru-run` on the patched Pleamar 0.7.27 runtime) across common Wayland compositors in CI is **practical, fast, and GPU-less**.

During this investigation we empirically probed **10 compositor targets** from the flake's pinned `nixpkgs` (`4fe25e12…`) and `hyprland-nixpkgs` (`8ce4ef6c…`) without touching the host display or GPU, using CPU software rendering (`pixman` / `llvmpipe` for compositors and **Mesa Lavapipe** software Vulkan for Pleamar's `wgpu` renderer). We also built and ran a working two-compositor `nixosTest` spike in [`nix/tests/compositors.nix`](../nix/tests/compositors.nix) wired into `flake.nix` (`checks.x86_64-linux.compositor-sway` and `compositor-weston`), which passed end-to-end under KVM in **30 seconds** total (including VM boot, Lavapipe shader compilation, bar rendering, exclusive-zone verification, title/workspace updates, screenshot pixel validation, and clean exit).

The investigation and spike immediately uncovered **six concrete cross-compositor and runtime defects** in the current codebase:

1. **Workspace clicking is denied by `shell/bar.plm` permissions on all compositors**: `shell/bar.plm` declares `services: "workspaces"` (read-only) without `"workspaces.focus"`. Emitting `workspace_select` fails inside Pleamar's permission gate (`src/logic_luau.rs:322`) and records a permission error in `ws_error`.
2. **Hyprland workspace focus is hardcoded to a custom Lua dispatcher**: Pleamar's `src/platform/hyprland.rs:238` sends `dispatch hl.dsp.focus({ workspace = N })` (upstream author's `hyprlua` config) instead of stock Hyprland's `dispatch workspace N`.
3. **The bar never maps on compositors without `ext-workspace-v1` unless `bar.preferred_output` is set**: `shell/bar.plm` declares `screens: bar_outputs`, and `shell/bar/workspace.luau:28` derives `bar_outputs` exclusively from `workspaces` monitor names. When `workspaces` is unavailable (verified on **KWin 6.8.2** and **River 0.3/0.5**) and `bar.preferred_output` is empty, `bar_outputs` stays `""`, which Pleamar's `rescreen()` (`src/platform/wayland.rs:1378`) interprets as `Screens::Named([])` ("no monitor").
4. **Missing `zwlr_foreign_toplevel_manager_v1` is silently masked when `ext-workspace-v1` exists**: In `src/platform/compositor.rs:350`, `compositor::service("window", ...)` returns `true` without checking `e.lists_windows`. Verified on **COSMIC 1.11.0**: `bar_error` stays `""` while window titles never arrive.
5. **Missing `zwlr_layer_shell_v1` causes a raw Rust panic (exit 101)**: `src/platform/wayland.rs:2100` calls `LayerShell::bind(&globals, &qh).expect("the compositor has no layer-shell")`, verified on **Weston 16.0.0** and **Mutter 50.5**.
6. **Backdrop glass capture ignores `ext-image-copy-capture-v1`**: Pleamar's `wayland.rs:2093` only binds `zwlr_screencopy_manager_v1` for lens glass captures, even though `capture.rs` and `thumbnails.rs` already support `ext_image_copy_capture_manager_v1` (which **COSMIC 1.11.0** provides instead of `wlr-screencopy`).

---

## 1. Protocols Relying on the Compositor

Inspecting Pleamar 0.7.27 (`src/platform/wayland.rs`, `compositor.rs`, `hyprland.rs`, `capture.rs`, `thumbnails.rs`, `mod.rs`) plus `runtime/pleamar/patches/` shows the exact protocol surface Tohru's bar uses:

| Protocol / Transport | Used By | Required? | Failure / Degradation Today |
|---|---|---|---|
| `wl_compositor`, `wl_output`, `wl_seat`, `wl_shm` | Core surface, output, input | **Hard requirement** | Fatal if absent |
| `zwlr_layer_shell_v1` | Bar surface (`anchor: top`, `reserve: 31`, `level: top`/`overlay`), OSD, corners | **Hard requirement** | **Panics (exit 101)** at `wayland.rs:2100` (`"the compositor has no layer-shell"`) |
| `xdg_wm_base` (`xdg-shell`) | Bar popups (`sound`, `calendar`, `network`, `media`, `bluetooth`, `info`, `about`, `wallpaper`) and test windows | Soft (warns if absent) | Warns on stderr; popups disabled |
| `ext_workspace_manager_v1` (or Hyprland `.socket.sock`/`.socket2.sock`) | Workspace switcher (`shell/bar/workspace.luau`) **and** default `bar_outputs` output discovery | Soft (currently broken fallback) | Sets `ws_available = false`, `ws_error = "This compositor does not report workspaces"`, **and leaves `bar_outputs = ""` so bar does not map** |
| `zwlr_foreign_toplevel_manager_v1` (or Hyprland IPC) | Centered focused window title & app icon (`shell/bar/controller.luau`) | Soft | Sets `bar_error = "Focused-window service unavailable"` (except bug #4 on COSMIC) |
| `wp_fractional_scale_manager_v1` + `wp_viewporter` | Crisp rendering at fractional scales (e.g. 1.25×, 1.5×) | Soft | Warns on stderr; paints at integer `wl_output` scale |
| `ext_background_effect_manager_v1` | Compositor-side blur for non-lens `glass: 100%; lens: false` (`shell/bar.plm:46`) | Optional | Silently skipped; bar renders translucent `#140a1e` fill + `fill.wgsl` + hairline |
| `zwlr_screencopy_manager_v1` (+ `zwp_linux_dmabuf_v1`) | Lens glass backdrop capture on popups (`bright: panel_bright`, patches `0020`, `0021`, `0023`); also used by `grim` in tests | Optional | Popup lens glass gets no backdrop (`panel_bright` stays `0`, dark ink fallback) |
| `ext_image_copy_capture_manager_v1` & `ext_foreign_toplevel_list_v1` | Pleamar `capture.rs` & `thumbnails.rs` (not wired into bar lens or `window` service yet) | Unused by bar today | Could unlock COSMIC title & lens glass if wired up |
| `zxdg_output_manager_v1` | Logical output geometry for `capture.rs` | Optional | Falls back to `wl_output` |
| `wp_cursor_shape_manager_v1` | Pointer hover cursors (`cursor: pointer`) | Optional | Falls back to default cursor |
| `ext_session_lock_manager_v1` | `tohru-lock` only (not `shell/bar.plm`) | Separate role | Out of scope for bar tests; relevant for future locker CI |

---

## 2. Compositor Coverage & Empirical Protocol Matrix

Every row below was verified against the pinned `nixpkgs` (`4fe25e12eb9f4c744966dd3de5a05246cf098601`) and `hyprland-nixpkgs` (`8ce4ef6cb6f871616146b9fe26d2a5ae594e94fe`) binaries using `wayland-info` in live headless/nested probes (except Hyprland 0.56.2, whose compiled protocol globals were verified from its store binary because Aquamarine requires DRM/dmabuf; see §3).

| Compositor (pinned version) | Family | `layer-shell` | `ext-workspace` | `wlr-foreign-toplevel` | `ext-toplevel-list` | `wlr-screencopy` | `ext-copy-capture` | `ext-bg-effect` | `fractional-scale` | Expected Bar Verdict |
|---|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|---|
| **Hyprland 0.56.2** (0.57.1 in `nixpkgs`) | Aquamarine / custom | Yes (IPC too) | Yes (IPC too) | Yes (IPC too) | Yes | Yes | Yes | Yes | Yes | **Full support** (primary target; test both native IPC and `PLEAMAR_GENERIC=1`) |
| **Sway 1.12** (`wlroots 0.19.3`) | wlroots (tiling) | Yes (v5) | **Yes (v1)** | Yes (v3) | Yes (v1) | Yes (v3) | Yes (v1) | No | Yes (v1) | **Full bar + workspaces + titles** (no compositor blur); *verified in spike* |
| **labwc 0.20.2** (`wlroots 0.20.2`) | wlroots (stacking) | Yes (v4) | **Yes (v1)** | Yes (v3) | Yes (v1) | Yes (v3) | Yes (v1) | No | Yes (v1) | **Full bar + workspaces + titles** (no compositor blur); *verified in probe* |
| **niri 26.04** | Smithay (scrollable) | Yes (v5) | **Yes (v1)** | Yes (v3) | Yes (v1) | Yes (v3) | No | **Yes (v1)** | Yes (v1) | **Full bar + workspaces + titles + blur**; *verified in probe* |
| **COSMIC `cosmic-comp` 1.11.0** | Smithay (DE) | Yes (v5) | **Yes (v1)** | **No** | Yes (v1) | **No** | Yes (v1) | **Yes (v1)** | Yes (v1) | **Degrades**: bar + workspaces + blur work; titles & lens glass missing today |
| **KDE Plasma `kwin_wayland` 6.8.2** | KWin (DE) | Yes (v5) | **No** *(KDE proto)* | **No** *(KDE proto)* | No | **No** *(PipeWire)* | No | **Yes (v1)** | Yes (v1) | **Degrades**: bar + blur work once `bar_outputs` fallback is fixed; no workspaces/titles/lens |
| **river-classic 0.3.17** | wlroots (tag tiling) | Yes (v4) | **No** *(river tags)* | Yes (v3) | Yes (v1) | Yes (v3) | Yes (v1) | No | Yes (v1) | **Degrades**: bar + titles work once `bar_outputs` fallback is fixed; no workspaces |
| **river 0.5.0** | wlroots (non-monolithic) | Yes (v4) *if WM binds `river_layer_shell_v1`* | **No** | Yes (v3) | Yes (v1) | Yes (v3) | Yes (v1) | No | Yes (v1) | **Needs WM client**: closes layer surfaces without a `river-window-management-v1` WM; low CI priority vs `river-classic` |
| **GNOME `mutter` 50.5** | Mutter (DE) | **No** | No | No | No | No | No | No | Yes (v1) | **Cannot run**: must fail cleanly with clear missing `layer-shell` message |
| **Weston 16.0.0** | libweston (reference) | **No** | No | No | No | No | No | No | No | **Cannot run**: lightweight negative test for missing `layer-shell`; *verified in spike* |

### Important Ecosystem Observations

- **Ubuntu 24.04 `apt` packages are too old for accurate coverage**: Ubuntu Noble ships `sway` 1.9 (no `ext-workspace-v1`), `labwc` 0.7.1 (no `ext-workspace-v1`), `kwin-wayland` 5.27 (Plasma 5, not Plasma 6), `mutter` 46, and has **no official packages at all** for `hyprland`, `niri`, `river`, or `cosmic-comp`. Using the flake's pinned `nixpkgs` gives all 10 compositors at current 2026 versions, pre-built on `cache.nixos.org`.
- **River 0.4+/0.5.0 architectural split**: Empirically, `river` 0.5.0 logged `window manager did not bind river_layer_shell_v1, closing layer surface` and immediately closed every surface because River ≥0.4 delegates all layout and layer-shell policy to an external window manager process. For wlroots tiling coverage without maintaining a custom River WM fixture, test **`river-classic` 0.3.17** (monolithic) and optionally add `river` 0.5 + a packaged WM client (e.g., `kwm` / `rivercarro` equivalent) later.
- **KWin 6.8.2** implements `zwlr_layer_shell_v1` v5 and `ext_background_effect_manager_v1` v1, but still exposes `org_kde_plasma_virtual_desktop_management` and `org_kde_plasma_window_management` instead of `ext-workspace-v1` or `zwlr_foreign_toplevel_manager_v1`. That makes KWin an essential degradation target: once `bar_outputs` falls back to all outputs, Tohru's bar will map cleanly with compositor blur on Plasma 6, while clearly reporting unavailable workspaces and window titles.

---

## 3. Running Each Compositor Headlessly in CI & GPU-less Rendering

### 3.1 GPU-less Rendering Stack

Pleamar renders every scene via `wgpu` (`src/lib.rs:268`, `src/gpu.rs:3528`):
- By default it selects Vulkan (`wgpu::Backends::PRIMARY`), falling back to OpenGL/EGL (`wgpu::Backends::GL`) if no Vulkan adapter exists.
- **Recommended CI renderer for Pleamar**: **Mesa Lavapipe** (CPU Vulkan):
  ```bash
  export VK_DRIVER_FILES="${MESA}/share/vulkan/icd.d/lvp_icd.x86_64.json"
  export VK_ICD_FILENAMES="$VK_DRIVER_FILES"
  export WGPU_BACKEND=vulkan
  export PLEAMAR_BACKEND=vulkan
  ```
  Empirically verified on Sway 1.12, labwc 0.20.2, Niri 26.04, COSMIC 1.11.0, and KWin 6.8.2:
  - Pleamar selects `llvmpipe (LLVM 21.1.8, 256 bits) (Vulkan) · Bgra8Unorm · PreMultiplied`.
  - Executes the **exact same Vulkan + WGSL shader pipelines** (`fill.wgsl`, `ribbon.wgsl`, `backdrop_check.wgsl`) used on real GPUs—unlike `PLEAMAR_BACKEND=gl`, which disables `backdrop_check.wgsl` (`src/lens.rs:25`).
  - First-frame compile + paint latency on CPU: **~1.3–1.5 s** with `appearance.corners = false`, or **~3.4–4.7 s** with all four corner surfaces enabled.
- **Compositor renderers**:
  - **wlroots (`sway`, `labwc`, `river-classic`, `river`)**: `WLR_RENDERER=pixman` — fastest, zero GL/DRM dependency, works identically inside containers, bare runners, and VMs.
  - **Weston**: `--renderer=pixman` — boots in < 0.2 s.
  - **KWin**: `KWIN_COMPOSE=Q` (QPainter software) or `LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe` with `--virtual`.
  - **Mutter / Niri / COSMIC / Hyprland**: `LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe`.

### 3.2 Exact Headless Launch Recipes

| Compositor | Headless / CI Invocation | Screenshot Capture | Measured Startup + Bar Test |
|---|---|---|---|
| **Sway 1.12** | `WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman sway -d -c sway.conf` | `grim -o HEADLESS-1 out.png` | ~5–7 s bare; ~25–30 s in `nixosTest` |
| **labwc 0.20.2** | `WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman labwc -d` | `grim out.png` | ~5 s bare |
| **river-classic 0.3.17** | `WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman river -no-xwayland -c true` | `grim out.png` | ~5 s bare |
| **niri 26.04** | Nested inside headless Sway (`WAYLAND_DISPLAY=<sway> EGL_PLATFORM=wayland LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe niri -c niri.kdl`) **or** NixOS VM with `virtio-gpu-pci` (`-vga none -device virtio-gpu-pci`) | `grim out.png` (niri supports `zwlr_screencopy_manager_v1`) | ~5.5 s nested bare |
| **COSMIC 1.11.0** | Nested inside headless Sway (`WAYLAND_DISPLAY=<sway> EGL_PLATFORM=wayland COSMIC_DISABLE_XWAYLAND=1 cosmic-comp --no-xwayland`) **or** NixOS VM with `virtio-gpu-pci` | Parent Sway `grim` (nested) or small `ext-image-copy-capture-v1` Python/Rust helper | ~7.4 s nested bare |
| **KWin 6.8.2** | `KWIN_COMPOSE=Q kwin_wayland --virtual --socket wayland-kwin --width 1280 --height 720 --no-lockscreen --no-global-shortcuts --no-kactivities` | `spectacle -b -n -o out.png` or KWin `org.kde.KWin.ScreenShot2` D-Bus API (no `wlr-screencopy`) | ~4.4 s bare |
| **Hyprland 0.56.2** | **Requires DRM or dmabuf-capable Wayland** (bare `CBackend::create()` throws without DRM/dmabuf). Run in a **NixOS VM** with `-vga none -device virtio-gpu-pci` (or `vkms`), `LIBGL_ALWAYS_SOFTWARE=1`, `GALLIUM_DRIVER=llvmpipe`, `AQ_NO_MODIFIERS=1`, plus `hyprctl output create headless HEADLESS-1` or the virtual connector | `grim out.png` (Hyprland supports `zwlr_screencopy_manager_v1`) | ~30–40 s in `nixosTest` |
| **Mutter 50.5** | `mutter --wayland --headless --no-x11 --wayland-display wayland-mutter --virtual-monitor 1280x720@60` | Not needed (negative test) | ~0.3 s bare |
| **Weston 16.0.0** | `weston --backend=headless --renderer=pixman --width=1280 --height=720 --socket=wayland-weston --no-config` | Not needed (negative test) | ~0.2 s bare; ~18 s in `nixosTest` |

### 3.3 NixOS VM Tests (`nixosTest`) vs Plain GitHub Runners

| Dimension | Bare Nix-store Harness on `ubuntu-24.04` | NixOS VM Tests (`pkgs.testers.runNixOSTest`) |
|---|---|---|
| **Where it runs** | Plain GitHub Actions `ubuntu-24.04` runner (no VM needed), using Nix-fetched compositors & `tohru` | Any Linux machine with `/dev/kvm`—including GitHub Actions `ubuntu-24.04` x64 runners (nested KVM is available once `/dev/kvm` permissions are set or via Determinate Nix) |
| **Compositors supported** | Sway, labwc, river-classic, Weston, Mutter, KWin (`--virtual`), nested Niri, nested COSMIC | **All of the above PLUS DRM-requiring compositors (Hyprland, DRM Niri/COSMIC, full Plasma 6 / GNOME sessions)** via `-vga none -device virtio-gpu-pci` and real `systemd-logind`/`seatd` |
| **Isolation & reproducibility** | Good (private `XDG_RUNTIME_DIR`, private `dbus-daemon`, `PLEAMAR_SOCKETS`), shares host kernel & `/etc` | Hermetic: pinned kernel, fontconfig, D-Bus, udev, logind, Mesa; identical locally (`nix flake check`) and in CI |
| **Measured wall-clock runtime** | **0.2–7.5 s** per compositor (warm Nix cache) | **~18–30 s** per VM (14–15 s VM boot + 1–10 s test); parallelizable across matrix jobs |
| **Download / cache footprint (from `cache.nixos.org`)** | ~100 MiB for Sway + labwc + river + Niri + Weston + Mesa; +58 MiB Mutter; +44 MiB COSMIC; +253 MiB KWin | Same compositor closures plus ~150 MiB minimal NixOS VM kernel/initrd/QEMU closure (shared across all VM tests) |
| **GitHub Actions cost (public repo)** | **$0** (standard `ubuntu-24.04` runners are free for public repos; 4 vCPU, 16 GiB RAM, 14 GiB SSD) | **$0** on `ubuntu-24.04` x64 with KVM. Watch the 14 GiB runner disk limit when pulling KWin + GNOME + Hyprland in a single job—split into per-compositor matrix jobs! |

**Recommendation — Use a Two-Layer Strategy**:
1. **Shared Python test runner (`tests/compositors/runner.py`)** driven by Nix-provided binaries (zero Ubuntu `apt` compositor drift). It owns private `dbus-daemon`, `XDG_RUNTIME_DIR`, `PLEAMAR_SOCKETS`, compositor lifecycle, assertions, and screenshot capture.
2. **Expose it in two ways**:
   - **Fast PR checks (`ubuntu-24.04` Nix runner, no VM)**: runs the runner directly against Sway, labwc, nested Niri, nested COSMIC, KWin (`--virtual`), Mutter, and Weston in **< 60 s total**.
   - **NixOS VM `checks` (`nix/tests/compositors.nix` via `nix flake check`)**: wraps the same runner in `pkgs.testers.runNixOSTest` for hermetic local verification and DRM-dependent compositors (**Hyprland** with `virtio-gpu-pci`, plus Sway and Weston as baseline VM checks). On GitHub Actions, run the VM matrix on `main` pushes, nightly schedules, and PRs touching `runtime/`, `nix/`, or `shell/bar*`.

---

## 4. What Each Test Should Assert

A compositor test should prove **five distinct layers of behavior**, degrading gracefully where protocols are absent rather than skipping silently:

1. **Layer Surface Placement & Output Geometry**
   - Query `pleamar --say tohru "describe json"` and assert panel `main` exists with `size == [1280, 42]` and expected `scale` (`1.0`, plus a `1.5` fractional-scale subtest on compositors advertising `wp_fractional_scale_manager_v1`).
   - Verify expected zones (`bar_clock_hit`, `root_network`, `sound_button`, `bar_bell_hit`, `root_wallpaper`, and `workspace_cell.1` when `ws_available == true`) have non-zero bounding boxes within `[0, 0, 1280, 42]`.
   - Verify `bar_outputs` matches the compositor's output name (`HEADLESS-1`, `Virtual-0`, `WINIT-0`, etc.) **without** requiring `preferred_output` in `config.toml` (once bug #3 is fixed), plus an explicit `preferred_output` targeting check on a 2-output headless session.
2. **Exclusive Zone (`reserve: 31`)**
   - Spawn a real Pleamar XDG window (`surface { kind: window; ... }`) on a tiling compositor with borders disabled (`default_border none` on Sway; `gaps_in=0, gaps_out=0, border_size=0` on Hyprland; `gaps 0` on Niri).
   - Assert via compositor IPC (`swaymsg -t get_tree -r`, `hyprctl clients -j`, `niri msg -j windows`) that the window's top edge is `y == 31` (or `height == 720 - 31`), proving the compositor honored `zwlr_layer_surface_v1.set_exclusive_zone(31)`.
3. **Visual Rendering & Screenshot Comparison**
   - Capture the output (via `grim` on `wlr-screencopy` compositors, parent `grim` on nested Smithay compositors, or `ScreenShot2` D-Bus on KWin).
   - **Tier A — Structural pixel invariants (always-on, zero-flake)**:
     - Dimensions match output (`1280×720` at scale 1).
     - Outside the 100 px left/right bar margins (`x ∈ [10, 85], y ∈ [2, 28]`): pixels match the solid desktop background within ±2 RGB (proves bar pill inset and alpha transparency).
     - Inside the bar pill (`x ∈ [110, 1170], y ∈ [2, 31]`): > 20,000 pixels differ from background (translucent dark fill rendered) and > 250 pixels exceed luminance 180 (white hairline, workspace bead, title text, service icons, clock rendered).
   - **Tier B — Masked reference comparison (visual regression)**:
     - Pin `fonts.packages = [ pkgs.dejavu_fonts ]`, disable animations, use a fixed solid background `#406080`, and **mask out the `bar_clock_hit` bounding box** (read live from `describe json`) so the current wall-clock time never causes false diffs.
     - Compare the remaining bar crop (`y=0..42`) against a per-compositor-class reference PNG (one for non-blur compositors like Sway/labwc, one for blur compositors like Niri/Hyprland) with per-channel tolerance `±4` and max `0.5%` differing pixels (accounting for Lavapipe / font rasterizer minor updates).
     - Always upload the captured PNG + diff heatmap as a GitHub Actions artifact on failure.
4. **Live Compositor Reactivity & CLI / IPC**
   - **CLI**: `tohru status --json` returns valid JSON with `"schema": 7`; `tohru config set bar.mooncoil true` updates `bar_mooncoil` to `true` and exposes `bar_mark_hit` in `describe json`.
   - **Window title** (where `zwlr_foreign_toplevel_manager_v1` or Hyprland IPC exists): spawn a window with `title: "Tohru CI Window"` → assert `get bar_title == "Tohru CI Window"`, close it → assert `get bar_title == ""`. Where absent (KWin, COSMIC until `ext-foreign-toplevel-list` is added), assert `get bar_error == "Focused-window service unavailable"`.
   - **Workspaces** (where `ext_workspace_manager_v1` or Hyprland IPC exists): create/switch to workspace 2 from the compositor → assert `ws_advertised.2 == "true"` and `workspace_cell.2` is active; emit `workspace_select 1` from the bar → assert compositor's active workspace switches to `1` (after fixing bugs #1 and #2). Where absent (KWin, `river-classic`), assert `ws_available == "false"` and `ws_error == "This compositor does not report workspaces"`.
   - **Clean shutdown**: `pleamar --say tohru quit` exits `tohru-run` with status `0`.
5. **Clean Failure on Unsupported Compositors**
   - On **Weston 16.0.0** and **Mutter 50.5**, assert `tohru-run` exits promptly (< 5 s) with a **non-zero, non-panic exit code** (`1`, no `"panicked"` backtrace once bug #5 is fixed) and a clear diagnostic explaining that the compositor does not support `zwlr_layer_shell_v1`.

---

## 5. Degrading Hyprland-Specific Features Elsewhere & Implied Code Changes

| Feature / Path | Current Behavior on Hyprland | Current Behavior Elsewhere | Required Code Change (Small & Targeted) |
|---|---|---|---|
| **Workspace focus permission** (`shell/bar.plm:3`) | Denied by Pleamar (`services` only lists `"workspaces"`) | Denied by Pleamar everywhere | Add `"workspaces.focus"` to `permissions { services: ... }` in `shell/bar.plm` (1-line change). |
| **Hyprland workspace focus IPC** (`src/platform/hyprland.rs:238`) | Sends `dispatch hl.dsp.focus({ workspace = N })` — works only on upstream author's `hyprlua` config, fails on stock Hyprland | Uses standard `ext_workspace_handle_v1::activate` + `commit` (`compositor.rs:402`) | Add Pleamar patch (`0031-hyprland-workspace-dispatch.patch`) changing `hyprland.rs:238` to `dispatch workspace <N>` (standard Hyprland IPC). |
| **Default output placement (`bar_outputs`)** (`shell/bar/workspace.luau:28`, `src/platform/wayland.rs:1378`) | Populated from `j/workspaces` monitor names | On KWin / River / Sway <1.12 (no `ext-workspace-v1`), `bar_outputs` is `""` → `Screens::Named([])` → **bar never maps!** | Add Pleamar patch (`0032-rescreen-all-fallback.patch`) so `rescreen` maps a sentinel (e.g. `"*"` or `"all"`, keeping `""` for intentional empty) to `Screens::All`, and have `workspace.luau` write `"*"` when `preferred == ""` and no monitors were reported. |
| **Window service availability check** (`src/platform/compositor.rs:350`) | Uses Hyprland `j/activewindow` | On COSMIC (`ext-workspace` present, `wlr-foreign-toplevel` absent), `service("window")` falsely returns `true` and hides `bar_error` | Add Pleamar patch (`0033-compositor-window-availability.patch`): return `false` from `compositor::service("window", ...)` when `!e.lists_windows`. Optionally follow up with `ext_foreign_toplevel_list_v1` read-only fallback for COSMIC. |
| **Missing `zwlr_layer_shell_v1` exit** (`src/platform/wayland.rs:2100`) | Present | On Mutter / Weston: Rust panic (`exit 101`) with backtrace | Add Pleamar patch (`0034-clean-missing-layer-shell.patch`): replace `.expect(...)` with `eprintln!("tohru: this Wayland compositor does not implement zwlr_layer_shell_v1 (required for desktop shell panels)"); std::process::exit(1);` (when the scene requests any layer surface). |
| **Glass / lens layer positioning** (`src/platform/wayland.rs:436-467`, patch `0005`) | Polls `j/layers` + `j/monitors` once/sec so layers displaced by another bar capture the right region | Falls back cleanly to `position_on_output(anchor, margin, size, monitor)` — exact for a top-anchored bar (`margin: 0`) and its popups | **No change required** for the bar. Document as expected degradation only when stacking multiple third-party bars on non-Hyprland compositors. |
| **Backdrop blur & lens capture** (`wayland.rs:1048, 2093`) | Both `ext_background_effect_manager_v1` (blur) and `zwlr_screencopy_manager_v1` (lens) work | Blur degrades silently to translucent `#140a1e` on wlroots (Sway/labwc/River); lens capture degrades on KWin and COSMIC | Bar already degrades well without blur. **Optional follow-up**: wire `ext_image_copy_capture_manager_v1` into `wayland.rs` backdrop capture to enable popup lens glass on COSMIC. |
| **Wallpaper popup (`bin/tohru-wallpaper`)** | Calls `hyprctl` + `mpvpaper` | Fails if `hyprctl` is absent | Guard `command -v hyprctl` with a clear non-Hyprland message (already isolated from bar startup). |
| **Optional Hyprland plugin (`hyprland/`)** | Tilts/blurs windows behind `tohru-launcher-backdrop` | Unused; `layer_namespace` is a harmless standard string on other compositors | **No change required**; already cleanly decoupled. |

---

## 6. Compositor × Test Matrix

Legend: **P** = Pass (full feature assertion), **D** = Pass (assert explicit graceful degradation), **F** = Pass (assert clean failure with diagnostic), **P\*** = Pass after Phase 0 fixes, **—** = Not applicable.

| Compositor | Map & `describe json` | Exclusive Zone (`31 px`) | Screenshot & Pixel Check | Fractional Scale (`1.5×`) | CLI (`status`, `config set`, `quit`) | Window Title Change | Workspace Observe & Select | Popup Open (`calendar`) | Missing Layer-Shell Clean Exit |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Hyprland 0.56.2** (IPC) | P | P | P | P | P | P | P\* | P | — |
| **Hyprland 0.56.2** (`PLEAMAR_GENERIC=1`) | P | P | P | P | P | P | P\* | P | — |
| **Sway 1.12** | **P** *(spike)* | **P** *(spike)* | **P** *(spike)* | P | **P** *(spike)* | **P** *(spike)* | P\* *(observe in spike)* | P | — |
| **labwc 0.20.2** | P | P | P | P | P | P | P\* | P | — |
| **niri 26.04** | P | P | P | P | P | P | P\* | P | — |
| **COSMIC 1.11.0** | P | P | P *(nested)* | P | P | D\* | P\* | P | — |
| **KWin 6.8.2** | P\* | P\* | P\* *(D-Bus)* | P\* | P\* | D | D | P\* | — |
| **river-classic 0.3.17** | P\* | P\* | P\* | P\* | P\* | P\* | D | P\* | — |
| **Mutter 50.5** | — | — | — | — | — | — | — | — | **F\*** |
| **Weston 16.0.0** | — | — | — | — | — | — | — | — | **F\*** *(spike)* |

---

## 7. Recommended Implementation Plan & Rough Effort

| Phase | Scope | Effort |
|---|---|---|
| **Phase 0: Fix the 5 blockers uncovered by this investigation** | 1. Add `"workspaces.focus"` to `shell/bar.plm`.<br>2. Patch `hyprland.rs` to use `dispatch workspace N`.<br>3. Patch `wayland.rs` + `workspace.luau` so missing workspaces fall back to `Screens::All` (`bar_outputs = "*"`).<br>4. Patch `compositor.rs` so `window` returns `false` when `!lists_windows`.<br>5. Patch `wayland.rs` to exit cleanly (`1`) without panicking when `zwlr_layer_shell_v1` is absent.<br>6. Tighten the two assertions in `nix/tests/compositors.nix` (`workspace_select` switches Sway workspace; Weston exits `1` without `"panicked"`). | **1–2 days** (small patches + existing unit/Lua tests + Pleamar rebuild) |
| **Phase 1: PR-Gating Headless CI (Tier 1)** | Factor the Python test logic out of `nix/tests/compositors.nix` into a reusable `tests/compositors/runner.py` with per-compositor adapters. Run **Sway**, **labwc**, **nested Niri**, **Weston**, and **Mutter** on `ubuntu-24.04` in `.github/workflows/compositors.yml` using Nix-cached compositors + Lavapipe (~60 s wall-clock, free runners). Upload screenshots on failure. | **2–3 days** |
| **Phase 2: NixOS VM Matrix & DE Coverage (Tier 2)** | Add `compositor-hyprland` (NixOS VM with `virtio-gpu-pci` testing both IPC and `PLEAMAR_GENERIC=1`), `compositor-kwin` (`kwin_wayland --virtual`), `compositor-cosmic` (nested or VM), and `compositor-river-classic` to `nix/tests/compositors.nix`. Run Hyprland + Sway + Weston VMs in CI on `ubuntu-24.04` with KVM on PRs touching `shell/bar*`, `runtime/`, or `nix/`, and the full matrix nightly. | **3–4 days** |
| **Phase 3: Visual Regression, Fractional Scale & Popup Input** | Add `1.5×` fractional-scale test cases, clock-masked reference PNG comparison (with update script), and `calendar`/`sound` popup open + Escape dismissal across compositors (using `pleamar --say tohru "press bar_clock_hit"` so input tests do not depend on `wlr-virtual-pointer`). | **2–3 days** |

Total estimated effort for full multi-compositor CI: **~1.5–2 weeks** of focused work, delivering high value as soon as Phase 0 + Phase 1 land (~3–5 days).

---

## 8. Working Spike & CI Workflow Sketches

### 8.1 Working Spike (Committed in This Branch)

[`nix/tests/compositors.nix`](../nix/tests/compositors.nix) (wired into [`flake.nix`](../flake.nix) under `checks.x86_64-linux`) implements two working GPU-less NixOS VM tests:

- **`compositor-sway`**: Boots a minimal NixOS VM (`graphics = false`, 2 vCPU, 2 GiB RAM), starts private `dbus-daemon` + headless Sway 1.12 (`WLR_RENDERER=pixman`), launches `tohru-run` on Mesa Lavapipe **without** `preferred_output`, verifies `wayland-info` globals, `tohru status --json`, `describe json` (`[1280, 42]`, zones), window title updates, `y == 31` exclusive zone via `swaymsg -t get_tree`, workspace 2 observation via `ext-workspace-v1`, captures `$out/sway-bar.png` via `grim` and validates bar pill + margin + bright text/hairline pixels with Pillow, and asserts clean exit `0` on `quit`.
- **`compositor-weston`**: Boots a minimal NixOS VM with headless Weston 16.0.0 (`--backend=headless --renderer=pixman`), verifies `zwlr_layer_shell_v1` is absent, and asserts `tohru-run` exits non-zero with `"no layer-shell"` (recording `exit_code: 101, panicked: true` until Phase 0 lands).

Run locally (requires KVM):
```bash
nix build --print-build-logs .#checks.x86_64-linux.compositor-sway .#checks.x86_64-linux.compositor-weston
```

### 8.2 Sketch of `.github/workflows/compositors.yml`

```yaml
name: Compositor Integration

on:
  pull_request:
    paths:
      - 'shell/**'
      - 'bin/**'
      - 'runtime/**'
      - 'nix/**'
      - 'flake.*'
      - 'tests/compositors/**'
  push:
    branches: [main]
  schedule:
    - cron: '0 6 * * *' # Nightly full matrix

permissions:
  contents: read

jobs:
  # Tier 1: Fast headless checks directly on ubuntu-24.04 using Nix-pinned compositors (~60 s)
  headless:
    name: Headless (${{ matrix.compositor }})
    runs-on: ubuntu-24.04
    strategy:
      fail-fast: false
      matrix:
        compositor: [sway, labwc, niri, cosmic, kwin, river-classic, weston, mutter]
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1
        with:
          persist-credentials: false

      - uses: cachix/install-nix-action@3dc140bfc76c49fbd4dd958429132e3746bcb086
      - uses: nix-community/cache-nix-action@7df957e333c1e5da7721f60227dbba6d06080569
        with:
          primary-key: nix-comp-${{ matrix.compositor }}-${{ hashFiles('flake.lock', 'nix/**', 'runtime/pleamar/**') }}
          restore-prefixes-first-match: |
            nix-comp-${{ matrix.compositor }}-
            nix-Linux-

      - name: Run ${{ matrix.compositor }} bar integration test
        run: |
          nix run .#compositor-test-${{ matrix.compositor }} -- --out artifacts/${{ matrix.compositor }}

      - uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9
        if: always()
        with:
          name: compositor-${{ matrix.compositor }}
          path: artifacts/${{ matrix.compositor }}
          retention-days: 7

  # Tier 2: Hermetic NixOS VM tests (required for DRM Hyprland + flake check verification)
  nixos-vm:
    name: NixOS VM (${{ matrix.check }})
    runs-on: ubuntu-24.04
    strategy:
      fail-fast: false
      matrix:
        check: [compositor-sway, compositor-weston, compositor-hyprland]
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1
        with:
          persist-credentials: false

      - name: Enable /dev/kvm access for NixOS VM tests
        run: |
          echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules
          sudo udevadm control --reload-rules
          sudo udevadm trigger --name-match=kvm
          ls -l /dev/kvm

      - uses: cachix/install-nix-action@3dc140bfc76c49fbd4dd958429132e3746bcb086
      - uses: nix-community/cache-nix-action@7df957e333c1e5da7721f60227dbba6d06080569
        with:
          primary-key: nix-vm-${{ matrix.check }}-${{ hashFiles('flake.lock', 'nix/**', 'runtime/pleamar/**') }}
          restore-prefixes-first-match: |
            nix-vm-${{ matrix.check }}-
            nix-Linux-

      - name: Build and run NixOS VM check
        run: |
          nix build --print-build-logs --out-link result-vm .#checks.x86_64-linux.${{ matrix.check }}

      - uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9
        if: always()
        with:
          name: vm-${{ matrix.check }}
          path: result-vm/
          retention-days: 7
```
