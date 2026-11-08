# Workspace switcher comparison

Both native implementations were built by separate Codex workers on sibling Orca
worktrees and integrated here on 7 November 2026. The completed worker worktrees
were subsequently removed at the user's request. All recordings and raw evidence
were copied and hash-verified under this checkout's `.tmp/prototypes/amane/` and
`.tmp/prototypes/pleamar/`. Rebuild from the integrated source for further runs;
old worker executable paths and compositor sessions are historical.

Start with [Amane](amane/README.md) or [Pleamar](pleamar/README.md) for build,
live workspace, replay, idle and active commands. Both preserve the reference's
240Hz fixed-step physics, seeded variation, interruptions, palette, wave and
square resting segment. Clicking and external workspace following were verified
on isolated Sway sessions. Amane currently supports Sway only; Pleamar also wires
upstream Hyprland services, but that path has not been runtime-tested.

## Recordings and personal visual review

- Combined, labelled, enlarged video: `/home/jesse/projects/tohru/.tmp/workspace-comparison.mp4`
- Review timeline: `/home/jesse/projects/tohru/.tmp/comparison-timeline.png`
- Native Amane bar: `/home/jesse/projects/tohru/.tmp/prototypes/amane/amane-replay-1x.mp4`
- Amane enlarged: `/home/jesse/projects/tohru/.tmp/prototypes/amane/amane-replay-6x.mp4`
- Native Pleamar bar: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/pleamar-replay-1x.mp4`
- Pleamar enlarged: `/home/jesse/projects/tohru/.tmp/prototypes/pleamar/pleamar-replay-8x.mp4`

The combined video uses actual recorded pixels enlarged 6x with nearest-neighbour
sampling. Pleamar's recording is trimmed by 0.4 seconds to approximately align
the replay motion after startup; this alignment is editorial and says nothing
about startup speed or input latency. Original recordings are unchanged.

I inspected frames across the recordings, including long jumps, rapid
interruptions, changing colours, the crashing crest and final rest. Both capture
the distinctive motion well. Neither offers a compelling visual advantage in
this test. The enlarged frames show small differences in wave phase, glow and
background tone; these are not proof of pixel equivalence. Recordings are 30fps
and their capture starts and native scheduling differ. Both have the exact
square resting bead, without a perpetual idle animation. Both clip the upper
5.5 pixels of the drawing bounds at the screen edge and omit compositor glass
blur in this deliberately basic bar. Full Liquid34 fidelity is unproven.

## Hardware GPU retest

The sandbox does expose an NVIDIA GeForce RTX 5090. The initial tests below
used a pixman compositor, whose surface was incompatible with NVIDIA Vulkan;
the apps therefore selected llvmpipe. That setup was a testing mistake, not
evidence that the sandbox lacks a GPU. The coordinator reran both unchanged
release binaries using NVIDIA Vulkan and an NVIDIA GLES2 headless Sway compositor
on `/dev/dri/renderD128`, driver 615.78.08. Native adapter logs confirm the RTX 5090
for both apps and the compositor.

Each cell below represents three sequential 15-second samples after separate
5-second warm-ups. App order alternates between rounds. The same 1280x240,
scale-1, 60Hz output is used, with no recorder or per-frame tracing during
measurement. CPU includes all process threads, as percent of one core. Median
app-plus-compositor CPU is calculated from each paired sample, not by adding
independent medians. No source or shader optimisation was made for this retest.

| Hardware observation | Amane | Pleamar |
| --- | ---: | ---: |
| Idle app CPU median (range) | 0.00% (0.00-0.07%) | 0.20% (0.20-0.33%) |
| Active app CPU median (range) | 1.67% (1.60-1.73%) | 1.13% (0.93-1.27%) |
| Active compositor CPU median | 2.53% | 0.33% |
| Active app + compositor CPU median | 4.13% | 1.47% |
| Idle app PSS median | 71.66 MiB | 81.49 MiB |
| Active app PSS median | 73.90 MiB | 85.31 MiB |

0.00% means no accounted CPU tick in these short samples, not literally zero
work. Driver background wake-ups remain visible in context-switch counts.
These are real hardware-rendered application tests, still on a headless 60Hz
output; they do not measure GPU execution time, GPU power or physical-display
latency. Other host activity is not isolated. Surface sizes and independently
ported shaders still differ, so this compares these prototypes rather than
proving a general framework advantage. In particular, Amane's shader scans 81
columns to select four ribbon samples and recomputes adjacent bead endpoints;
Pleamar evaluates four samples directly and reuses adjacent endpoints. Compiler
optimisation and the larger compositor CPU difference have not been profiled.

Use `benchmark_gpu.py --help` for the common sequential measurement runner.
It requires explicit compiled binary/run-script paths and the isolated
compositor PID, and fails if an app log does not confirm the RTX 5090. In this
sandbox, source `amane/dev-env.sh`, prepend `/run/opengl-driver/lib` to
`LD_LIBRARY_PATH`, set `VK_DRIVER_FILES` to
`/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json` and `WGPU_BACKEND=vulkan`.
Start private Sway with `WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS=1
WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128
WLR_LIBINPUT_NO_DEVICES=1`, using `pleamar/sway-headless.conf`. Resolve its
`WAYLAND_DISPLAY` and `SWAYSOCK` before launching either app. Preserve existing
desktop sessions.

Raw paired samples, summary, GPU enumeration, environment facts and all native
logs are in `/home/jesse/projects/tohru/.tmp/gpu-benchmark/` (`raw.json`,
`summary.json`, `vulkaninfo.txt`, `environment.json` and per-trial `.log` files).
Fresh hardware-rendered videos are `amane-gpu-1x.mp4`, `amane-gpu-6x.mp4`,
`pleamar-gpu-1x.mp4`, and `pleamar-gpu-6x.mp4` in that same directory. Recording
is performed only after measurements. One initial Amane `--seconds 4` hardware
probe segfaulted at exit; the sustained benchmark runs completed their measured
windows successfully and use external process termination. Timed shutdown on
NVIDIA needs follow-up before treating this as a production-ready shell.

I also inspected the new hardware recordings. Both animate, but the enlarged
Amane recording exposes a one-pixel step in the resting grey rail around its
middle (`amane-rest.png`); Pleamar's rail stays flat. This was not apparent in
the software-rendered review and needs a shader/coordinate investigation.
The benchmark deliberately retains this unchanged shader; no fix is hidden in
the measured result. The GPU retest therefore changes the visual assessment too:
Pleamar currently has the cleaner resting rail on this GPU.

## Initial software-rendering evidence

These are single worker-measured release runs on the same kind of isolated
1280x240, scale-1, nominal 60Hz headless Sway output, with pixman compositing and
Vulkan llvmpipe software rendering. Each measurement follows a 5-second warm-up
and runs without recording, for 15 seconds (Amane) or 16 seconds (Pleamar).
CPU includes all app threads and is expressed as percent of one core. PSS is an
end snapshot, not peak usage. These are comparable prototype observations,
not statistically repeated or hardware-GPU results. Layer surface extents differ:
Amane uses the 1080px inset surface; Pleamar uses a 1280px surface with inset bar.

| Observation | Amane | Pleamar |
| --- | ---: | ---: |
| Settled app CPU | 0.066% | 0.875% |
| Continuous active app CPU | 27.711% | 23.181% |
| Settled app PSS | 159.86 MiB | 174.93 MiB |
| Active app PSS | 159.07 MiB | 177.98 MiB |
| Settled compositor CPU | 0.000% | 0.125% |
| Active compositor CPU | 0.399% | 0.312% |
| Settled activity evidence | 0 view builds | 84 render-loop rounds / 16s |
| Framework changes | None | 8-to-24 scalar scene-shader ABI patch |

Pleamar used about 16% less app CPU during this active sample. Amane was quieter
at idle and used roughly 15-19 MiB less PSS. The probes differ: Amane's active
view intervals have median/p95 16.040/16.154ms; Pleamar's render-loop rounds have
16.300/17.600ms. Those are different instrumentation points and must not be ranked
as presentation smoothness or end-to-end latency. No physical-display or GPU
power measurements were made, and there is no matched original-j2bar benchmark.

Raw measurements, methods, runtime facts and validation logs are linked from
[Amane results](amane/RESULTS.md) and [Pleamar results](pleamar/RESULTS.md).
Both passed release builds, formatting, simulation tests and Clippy. Amane's
Clippy was warning-clean; Pleamar retains existing upstream warnings. Pleamar
also passed shader and ABI-bound tests. The coordinator inspected these reports
and raw measurement summaries rather than repeating completed builds.

## Recommendation

Amane remains the simpler integration: close visual results, lower idle activity
and memory, and no shader-interface patch. The corrected hardware tests give
Pleamar a stronger measured active-performance advantage, particularly including
compositor CPU. For an active-performance priority, Pleamar currently wins this
prototype test; for the smaller patch burden and settled desktop, Amane retains
advantages. Equalise shaders and investigate Amane's compositor cost and timed
shutdown before making a framework decision from these numbers. This establishes
that both can reproduce this effect; it does not select a complete desktop shell
or prove how either performs on the physical high-refresh display.
