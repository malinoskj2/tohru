# Amane workspace switcher prototype

Native Rust/Amane layer surface containing only a workspace switcher and a basic bar. Amane is pinned to `544cbfc23af37f84ac237cd9bec53173981772b3`; `Cargo.lock` pins all transitive dependencies. No framework patches. Tested with Rust 1.95.0 on Linux.

From this directory:

```sh
# Needed in the supplied Nix sandbox; ordinary Linux installations can omit this.
source ./dev-env.sh
cargo build --release --locked
./target/release/workspace-amane --mode live
./target/release/workspace-amane --mode replay --seconds 12
./target/release/workspace-amane --mode active --seconds 30
./target/release/workspace-amane --mode idle --seconds 30
cargo fmt --check
cargo test --release --locked
cargo clippy --release --locked --all-targets -- -D warnings
```

Native development dependencies are Wayland, xkbcommon, fontconfig, PulseAudio, PAM, a Vulkan loader and driver. PAM/PulseAudio are linked by upstream even though this prototype uses neither service. `dev-env.sh` exposes already installed Nix development outputs and selects the sandbox's compatible 64-bit PAM; it is a sandbox convenience, not a portable Nix derivation. On another Nix machine provide those packages through its development shell. Run with `WGPU_BACKEND=vulkan` to match the measured renderer.

`live` is the default and requires Sway's `SWAYSOCK` plus the intended display's `WAYLAND_DISPLAY`. It reads initial focus, subscribes to workspace changes over native Sway IPC, and sends a workspace command when any of the five 16×22 cells is clicked. It follows compositor confirmation rather than changing focus optimistically. This prototype supports numbered Sway workspaces 1–5; focus outside this range hides the bead. Hyprland integration is not implemented. Demo modes are explicit, require no compositor workspace events, and clicks retarget their simulation locally.

`replay` uses elapsed milliseconds `(0,1), (1000,2), (2200,3), (3400,4), (4600,5), (5800,1), (7000,5), (7120,2), (7240,4), (9000,1)`, then rests through 12000. It does not quit automatically unless `--seconds` is supplied. `active` cycles `[1,5,2,4,3]` every 300 ms forever. `idle` starts settled on workspace 1. All modes use xorshift64* seed `0x0123456789abcdef` and keep drawing and motion state separate.

The domain simulation is copied from j2bar `src/track.rs` and its motion constants/RNG at reference commit `3a8daa5e077f4085b5262041ea4832373b8754ca`. It preserves 1/240 integration, the 50 ms late-frame cap, all tuning ranges, carried velocity/amplitude/wash on interruption, per-switch draws, sampled wave, interpolated palette, 28-section superellipse, and two Gaussian glows. The explicit square-at-rest requirement takes precedence over current j2bar's rounded first/last bead endpoints; the JS design's square bead is used instead. A seeded, independently executed copy of the JS simulation provides a numerical golden test.

The public Rectangle shader API receives six vec4 rows. The GLSL shader adapts j2bar's analytic coverage/glow code and reconstructs its pixel-column profile from frozen wave state, using f32 shader math rather than uploading all sampled columns. The shader composites internally in premultiplied alpha, then divides RGB by alpha for Amane's straight-alpha shader API. Its source contains no automatic animation token. A dedicated service advances simulation, wakes only while moving or awaiting a scheduled input, and blocks indefinitely at idle; views only read state. The service publishes approximately every 16.667 ms while moving, and Amane presents through its normal compositor frame handling.

The bar is 31 logical pixels high, with 100-pixel side margins and dark translucent fill; the widget retains its 104×42 drawing bounds and 80×22 hit area. Its center is at bar y=15.5, so the physical top screen edge clips the uppermost 5.5 pixels of drawing overhang. This deliberately basic bar does not implement glass blur, flares, panels or unrelated shell features.

For the existing isolated sandbox display:

```sh
export WAYLAND_DISPLAY=wayland-4
export SWAYSOCK=/run/user/1000/sway-ipc.1000.1006.sock
export WGPU_BACKEND=vulkan
./target/release/workspace-amane --mode replay --seconds 12
```

The compositor can be reproduced with `WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1 WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c <config>`, where the config sets `output HEADLESS-1 mode 1280x240@60Hz`, `output * bg #38242e solid_color` and `seat seat0 fallback true`. Resolve the newly created display/socket from its log/runtime directory rather than assuming the names above.

From the repository root, `python3 prototypes/amane/scripts/capture.py` records the actual native app with repeated grim captures and derives a 6× nearest-neighbour crop. `python3 prototypes/amane/scripts/benchmark.py --compositor-pid PID` performs two separate five-second warm-ups plus at least fifteen seconds of accounting without a recorder. Both inherit the display/renderer environment. All media, raw measurements and logs go under the repository's `.tmp/`. `AMANE_FRAME_LOG=/absolute/path` optionally records view invocation offsets in nanoseconds; these are diagnostic view builds, not presentation timestamps.
