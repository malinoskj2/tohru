# Workspace switcher comparison

Both native implementations were built by separate Codex workers on sibling Orca
worktrees and integrated here on 7 November 2026. The workers and their compiled
applications remain available in `/home/jesse/orca/workspaces/tohru/workspace-amane`
and `/home/jesse/orca/workspaces/tohru/workspace-pleamar`.

Start with [Amane](amane/README.md) or [Pleamar](pleamar/README.md) for build,
live workspace, replay, idle and active commands. Both preserve the reference's
240Hz fixed-step physics, seeded variation, interruptions, palette, wave and
square resting segment. Clicking and external workspace following were verified
on isolated Sway sessions. Amane currently supports Sway only; Pleamar also wires
upstream Hyprland services, but that path has not been runtime-tested.

## Recordings and personal visual review

- Combined, labelled, enlarged video: `/home/jesse/projects/tohru/.tmp/workspace-comparison.mp4`
- Review timeline: `/home/jesse/projects/tohru/.tmp/comparison-timeline.png`
- Native Amane bar: `/home/jesse/orca/workspaces/tohru/workspace-amane/.tmp/amane-replay-1x.mp4`
- Amane enlarged: `/home/jesse/orca/workspaces/tohru/workspace-amane/.tmp/amane-replay-6x.mp4`
- Native Pleamar bar: `/home/jesse/orca/workspaces/tohru/workspace-pleamar/.tmp/pleamar-replay-1x.mp4`
- Pleamar enlarged: `/home/jesse/orca/workspaces/tohru/workspace-pleamar/.tmp/pleamar-replay-8x.mp4`

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

## Performance evidence

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

For this workspace switcher, I would continue with Amane: close visual results,
lower idle activity and memory, and no framework patch. Pleamar is viable and
its lower measured active CPU is encouraging, but the bounded shader ABI change
adds maintenance and its idle wake-ups deserve investigation. This establishes
that both can reproduce this effect; it does not select a complete desktop shell
or prove how either performs on the physical high-refresh GPU display.
