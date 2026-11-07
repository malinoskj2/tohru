# Pleamar workspace switcher

Native Pleamar 0.7.27 scene + Luau logic + WGSL, with the original Rust
workspace simulation. No browser, frame playback, or generic spring substitution.

```sh
cd prototypes/pleamar
source ./env.sh                 # Nix sandbox native headers/loader paths; optional elsewhere
cargo build --release --locked --manifest-path vendor/pleamar/Cargo.toml
./run.sh live                  # Sway IPC, or Pleamar's upstream Hyprland service
./run.sh replay --seconds 13    # deterministic 12-second input; then stays settled
./run.sh idle                   # initially settled workspace 1; no animation timer
./run.sh active                 # [1,5,2,4,3], a target every 300ms, indefinitely
```

Run from a Wayland session supporting layer-shell. `WAYLAND_DISPLAY` and
`SWAYSOCK` select the compositor; never point these at a sibling's display.
`run.sh` accepts further upstream Pleamar options, including `--seconds N`,
`--record v1,v2`, and `--mouse '190,15@1500 click@1600'`. `v1` is bead x,
`v2` velocity. `--say workspace-pleamar 'get target'` queries the scene.

Dependencies are resolved exactly by the committed Cargo.lock; use `--locked`.
Linux development packages: Wayland, xkbcommon, fontconfig, PAM, C/C++ compiler,
pkg-config, Vulkan loader + a Vulkan ICD (Mesa lavapipe works). `env.sh` discovers
these development files in this sandbox's Nix store. Captures additionally need
Python 3, grim, and ffmpeg with libx264.

Replay events in elapsed milliseconds: `(0,1), (1000,2), (2200,3), (3400,4),
(4600,5), (5800,1), (7000,5), (7120,2), (7240,4), (9000,1)`; ends at 12000.
The simulation uses xorshift64* seed `0x0123456789abcdef`. Its 240 Hz canonical
clock consumes events before the first step at or after their timestamp; render
cadence cannot change the result. Live switches retain the original accumulator
and 50ms late-frame cap. The fixed-step springs, wave, wash, per-switch draws,
palette interpolation and shape constants come from j2bar's current Track.
At rest the JS reference's exact square 16x4 rectangle takes precedence over
the current Track's rounded end-slot bead. Five 16x22 hit cells, 104x42 drawing
bounds; a 31px translucent bar with 100px side margins. No hover animation.

```sh
cargo fmt --all --check --manifest-path vendor/pleamar/Cargo.toml
cargo clippy --release --locked --all-targets --manifest-path vendor/pleamar/Cargo.toml
cargo test --release --locked --manifest-path vendor/pleamar/Cargo.toml workspace_track::
cargo test --release --locked --manifest-path vendor/pleamar/Cargo.toml shaders::
python3 check_abi.py           # 24 accepted, 25 rejected, group shader stays bounded at 8
python3 capture.py             # actual native bar MP4, plus 8x nearest-neighbor crop
python3 measure.py idle --warmup 5 --compositor-pid <isolated-sway-pid>
python3 measure.py active --warmup 5 --compositor-pid <isolated-sway-pid>
```

All evidence is written to the worktree root's `.tmp/`, including the raw
benchmark JSON and native render-loop traces. `measure.py` waits 5 seconds,
then measures 16 seconds, without a recorder. CPU is process user+system ticks
(all threads) / CLK_TCK / elapsed wall time * 100, as percent of **one core**.
It sums task context switches and reads RSS/PSS from smaps_rollup. The app has
no child/helper processes; its Luau and software renderer threads belong to the
same process. The optional dedicated Sway process is measured separately.
Render-loop trace intervals are **not** display presentation or input latency.

See RESULTS.md for measurements, absolute evidence paths, and known limits;
UPSTREAM.md describes the bounded shader ABI patch and vendored revision.
