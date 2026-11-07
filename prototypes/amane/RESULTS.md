# Amane results — 7 November 2026

Implemented, built and ran genuine Amane at pinned revision `544cbfc23af37f84ac237cd9bec53173981772b3`, with no upstream patches. The public shader API accommodates the effect. Native release executable: `/home/jesse/orca/workspaces/tohru/workspace-amane/prototypes/amane/target/release/workspace-amane` (16 MiB). Run instructions and exact replay controls are in README.md; PLAN.md is untouched.

Validation passed: `cargo build --release --locked`, `cargo fmt --check`, `cargo test --release --locked` (10 tests), and `cargo clippy --release --locked --all-targets -- -D warnings`. Tests cover independent seeded JS numerical agreement, full replay determinism, exact settled geometry, interrupted velocity, variation ranges, long travel, wave generation and late-frame clamping. Final logs: `.tmp/build-final.log`, `.tmp/test-final.log`, `.tmp/clippy-final.log`, `.tmp/fmt.log`. Earlier failed attempts remain separately named in `.tmp`; they are not the final results.

External focus to workspace 4 was observed by native Sway subscription, producing lavender; clicking cell 2 through a persistent virtual pointer changed Sway's actual focused workspace to 2 and the bead to yellow. Evidence: `.tmp/live-external-command.json`, `.tmp/live-after-click-final.json`, `.tmp/live-pointer-final.log` (native Wayland enter/press/release trace). Initial one-shot wlrctl/sway cursor attempts had no persistent pointer capability; `.tmp/click.c` and `.tmp/click` retain the successful persistent input helper. The application itself requires no helper process.

All evidence is host-shared beneath `/home/jesse/orca/workspaces/tohru/workspace-amane/.tmp/`:

- `amane-replay-1x.mp4`: 1080×42, 30 fps, 390 captures / 13 seconds, bar only.
- `amane-replay-6x.mp4`: same native recording, widget cropped and enlarged to 624×252 using nearest-neighbour sampling.
- `amane-frame-15.png`, `amane-frame-40.png`, `amane-frame-220.png`, `amane-frame-375.png`: native bar screenshots throughout the replay.
- `amane-motion-6x.png`: enlarged motion screenshot.
- `amane-live-external-4.png`, `amane-live-click-2.png`: settled external focus and actual click results.
- `replay-native.log`, `live-final.log`: clean native startup and renderer logs.
- `capture-seconds.csv`: actual capture wall offsets; video uses a nominal 30 fps stream and includes app startup.

The app and recordings use a private Sway 1.12 headless output, HEADLESS-1, 1280×240 logical/physical pixels, scale 1, normal transform, nominal 60000 mHz. Sway uses pixman; Amane uses Vulkan llvmpipe (LLVM 21.1.8, 256 bits), device type CPU. This is software rendering inside the sandbox, not a hardware GPU comparison. The isolated compositor has PID 1006, socket `/run/user/1000/sway-ipc.1000.1006.sock`, display `wayland-4`; configuration and facts are `.tmp/sway-amane.conf`, `.tmp/sway.log`, `.tmp/display.json`. No host display or sibling process was used.

Release measurements below are the final independent no-recorder run: five-second warm-up followed by 15.047 seconds idle and 15.048 seconds active. CPU is process user+system tick delta / CLK_TCK / elapsed ×100, expressed as percent of **one** core; CLK_TCK=100. Context-switch counters sum every task/thread in each process. RSS/PSS are final `/proc/PID/smaps_rollup` snapshots. All llvmpipe worker threads belong to the app and are included; the required compositor is measured separately.

| Mode | App CPU | App RSS / PSS | App voluntary / involuntary switches | Compositor CPU | Compositor RSS / PSS |
|---|---:|---:|---:|---:|---:|
| Settled idle | 0.066% | 198232 / 163699 KiB | 2 / 0 | 0.000% | 18016 / 5016 KiB |
| Continuous active, target every 300 ms | 27.711% | 197748 / 162889 KiB | 264364 / 19 | 0.399% | 18384 / 5200 KiB |

Compositor context switches: idle 2 / 0; active 2701 / 0. App threads: 37; compositor threads: 2. Idle produced **zero view invocations** during the observed settled interval. Active produced 896 view invocations over the middle 15 seconds, median interval 16.040 ms and p95 16.154 ms. These are view-build counts and intervals, not GPU completion, compositor commit counts, end-to-end latency or presentation latency. Tiny idle CPU includes one 10 ms accounting tick; the shader/service does not continue animating. The view observation interval is relative to first view, while CPU samples start five seconds after launch; startup offsets make those windows differ slightly.

Raw/method evidence: `.tmp/benchmark-idle-raw.json`, `.tmp/benchmark-active-raw.json`, `.tmp/benchmark-summary.json`, `.tmp/benchmark-final.log`, `.tmp/benchmark-idle.log`, `.tmp/benchmark-active.log`, `.tmp/idle-views.ns`, `.tmp/active-views.ns`, and `scripts/benchmark.py`. Earlier benchmark output in `.tmp/benchmark.log` is superseded by the final run; the raw JSON files contain the final data. No claim is made about physical-display latency or hardware GPU cost.

Limits: Sway workspaces 1–5 only; no Hyprland adapter. The deliberately basic bar has translucent fill but no backdrop blur or shaped glass. Top-edge glow overhang is clipped by the screen boundary. Shader profile reconstruction uses f32 math and may differ minutely from CPU f64 sampling; the integration equations themselves match the seeded JS golden within 1e-10. Square first/last resting endpoints follow the explicit task/design contract. No custom renderer, browser surface or frame playback replaces Amane.
