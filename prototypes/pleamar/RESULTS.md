# Pleamar results

The completed worker worktree has been removed after integration. Recordings and
raw evidence are preserved at the current `.tmp/prototypes/` paths below.
Worker executable paths and compositor PID/socket facts are historical; rebuild
from the integrated source and start a new isolated display for reruns.

Implemented and ran a genuine Pleamar Rust/wgpu scene with Luau logic; only the
workspace switcher is drawn in a 31px translucent top bar with 100px side margins.
PLAN.md is unchanged. Sources, exact dependency lock, commands, capture/measurement
scripts and upstream licenses are included in this directory.

**Framework:** Pleamar 0.7.27, pinned upstream revision
`dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`; explicit bounded scene-shader ABI
extension from 8 to 24 floats (element stride 64 to 80 floats), documented in
UPSTREAM.md and patches/framework.patch. Group shaders and blur retain their
8-value limit. No replacement renderer, browser, generic springs or frame playback.

**Verified:** release build, `cargo fmt --all --check`, release `cargo check`,
release Clippy all targets, 10 workspace simulation tests and 2 shader tests.
Scene compilation passes; ABI probes accept 24 and reject 25, and reject a
9-value group shader. Clippy succeeds with existing framework warnings (164
library / 162 test warnings); this is not a warnings-clean upstream fork.
Final release build took 29.69s incrementally with dependencies already built.
The test/format/build/check/Clippy logs are in `.tmp/`.

**Native integration:** actual Sway click dispatch to workspace 5 and subsequent
external `swaymsg workspace number 3` following both pass; see
`live-click-compositor.json`, `live-integration.log`, and `live-verification.md`.
Hyprland uses the upstream service path and was not runtime-tested here.
Live Sway changes are polled every 100ms through bounded native IPC, without
child processes. Idle demo mode installs zero animation timers; the framework
still has background wake-ups, visible as 84 render-loop rounds in 16 seconds.

**Measurement environment:** private sandbox Sway PID 15325, headless output
1280×240, scale 1, 60Hz; compositor pixman. App: wgpu/Vulkan llvmpipe
(LLVM 21.1.8, 256 bits), Bgra8Unorm premultiplied, 1280×42 layer-shell surface,
48 native process threads (including Luau and the software renderer), no helper
processes. CPU is AMD Ryzen 9 9950X3D; sandbox memory total approximately 31GiB.
The final video uses this same display and renderer. No sibling display or
process was changed. The historical measurement session used
`WAYLAND_DISPLAY=wayland-4`, `SWAYSOCK=/run/user/1000/sway-ipc.1000.15325.sock`.

Five-second warm-up, then at least 16 seconds per mode, without recording:

| Mode | Measured wall | App CPU, one core | RSS MiB | PSS MiB | Voluntary / involuntary switches, all app threads | Private Sway CPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Settled idle | 16.004s | 0.875% | 246.01 | 174.93 | 7,215 / 2 | 0.125% |
| Continuous active | 16.004s | 23.181% | 248.79 | 177.98 | 310,905 / 9 | 0.312% |

Active render-loop trace: 1022 records, median
16.300ms, p95
17.600ms, maximum
23.100ms. These are process render-loop
rounds, which can include skipped paintings; they are **not presentation feedback,
frame-delivery guarantees or end-to-end input latency**. Raw `/proc` snapshots,
wall times, CLK_TCK, context switches, memory and traces are preserved in
`benchmark-idle.json`, `benchmark-active.json` and matching `.log` files;
`measurement-method.md` gives commands. RSS/PSS are end snapshots, not peaks.
Software GPU CPU cost and shared driver PSS cannot predict hardware GPU results.

**Visual/numerical limits:** the reference CPU physics, 1/240 step, xorshift64*
seed `0x0123456789abcdef`, per-switch variation, interrupted and long travel,
palette, stretch, crash and wash are retained. WGSL implements the reference
28-segment sampled outline, ribbon coverage, two integrated Gaussian glows and
compositing order. It computes the profile in f32 from CPU state, so minute
rounding differences from f64→f32 sample upload remain. JS's exact square 16×4
rest shape overrides the primary Rust implementation's rounded end-slot bead.
The native Luau publication timer is 60Hz; replay physics runs on its canonical
240Hz timeline independent of render cadence. The 104×42 drawing box is centered
on the 31px bar, so the screen's top edge clips its uppermost 5.5px of glow; the
simple background is translucent with no compositor blur.

Executable:
`/home/jesse/orca/workspaces/tohru/workspace-pleamar/prototypes/pleamar/vendor/pleamar/target/release/pleamar`

Run on this private display (same controls work in another Wayland session):

```sh
WAYLAND_DISPLAY=wayland-4 SWAYSOCK=/run/user/1000/sway-ipc.1000.15325.sock /home/jesse/orca/workspaces/tohru/workspace-pleamar/prototypes/pleamar/run.sh replay --seconds 13
# replace replay with idle, active, or live; no source edits needed
```

Absolute host evidence paths:

- 1× bar MP4: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/pleamar-replay-1x.mp4` (1280×42, 30fps, 13s).
- Enlarged native crop MP4: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/pleamar-replay-8x.mp4` (832×336, 30fps, 13s; nearest-neighbor enlargement of actual captures).
- Bar screenshot: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/replay-motion-1x.png`.
- Interrupted motion screenshot: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/replay-interruption-8x.png`.
- Settled square screenshot: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/replay-settled-8x.png`.
- External workspace screenshot: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/live-workspace3.png`.
- Native run log: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/replay-native.log`.
- Raw idle/active measurements: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/benchmark-idle.json`, `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/benchmark-active.json`.
- Measurement commands/method: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/measurement-method.md`.
- Display facts: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/display-comparable.json`.
- Build/test/ABI logs: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/build-verified.log`, `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/tests-final.log`, `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/shader-tests.log`, `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/abi-check.log`.
