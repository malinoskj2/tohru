# Exact workspace physics preparation

This is the approved exact-motion boundary, ready for S integration with the authorized cancellable native60Hz
publication timer (AUTONOMOUS_DECISIONS.md). It is not active in the shell yet. No crate,
backend, IPC, thread, timer, frame handler or polling adapter is added here.

`mod.rs` exposes one retained `Indicator`, `observe`/`advance`/`sample`, and typed
`Activity::{Animating,Resting}`. Initialize it outside reloadable Luau state.
Native workspace reports supply highest numeric workspace and active slot;
calendar uses `Indicator::calendar()` with the same retained twelve-slot rail.
Native `sys.watch("workspaces")`/`workspaces.focus` already work on private Sway
through ext-workspace; no historical Sway IPC/polling copy is needed.

The motion/track math derives from byte-frozen tools/reference fixtures (MIT
notice here). Preserve 1/240 step, 50ms live catch-up cap, 6e-3 energy, canonical
seed/per-switch draws, interrupted momentum and exact square16x4 rest. Shader
publication is a fixed array, with no sample allocation or CPU geometry rebuild.
CPU geometry remains test-only as an independent reference for shape/rest values.
The canonical replay helper is test-only, never a production unbounded loop.

`shader-inputs.patch` is a **proposed isolated integration patch**, against clean
Pleamar dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86. It extends direct scene shader
values from8 to16 (s.a–s.d), Element stride64→72 floats and two extra vec4 fields.
It does not alter group shader limits, frame subscriptions, startup, services or
bindings. `git apply --check` passes against the clean prepared source. It is not
applied to any runtime or shared patch file by B, and no native build was run.

`shell/bar/ribbon.wgsl` adapts the preserved five-slot shader to a dynamic5–12-slot width
and resident native color inputs. Sixteen shape scalars stay bit-identical to
retained shader_values[0..16]. First native color is sample.color RGB; second
native color is white when sample.color alpha==1, black when absent. Its red
channel gates the bead/glow, preserving the ribbon when no workspace is active.
This removes3 redundant scalar color inputs without changing the physics. The
shape box width is sample.width+24, height42; origin is12px before the row and
10px above it. Color components for .plm rgb(...) are sample.color*255.
The original native colors API sets opaque tones; do not assume it forwards
sample alpha automatically. Full visual parity/dynamic-width shader pixels still
require native build/render comparison after shared patch integration.

S wiring request (do not implement a second package): expose this module from
the existing library with `#[path = "../runtime/workspace/mod.rs"] pub mod workspace;`
and copy runtime/workspace into the prepared foundation library path. A narrow
binding should retain one Indicator per workspace/calendar role, accept native
observations, advance from monotonic Instant, publish shape/color and moving status.
The feature rail-publisher starts at most one native every(1000/60) callback,
cancels it at Resting/error/close, and resumes on a native workspace/month event.
No frame listener/unsubscribe patch is used.
S owns manifest/preparation/binding/root hooks and final patch inclusion. No new
crate dependency is needed for this module or approved scalar ABI extension.

Checked CPU commands from repository root:

```sh
rustfmt --check --edition 2024 runtime/workspace/*.rs
rustc --edition 2024 --crate-type lib -D warnings runtime/workspace/mod.rs -o .tmp/libworkspace.rlib
rustc --edition 2024 --test -D warnings runtime/workspace/mod.rs -o .tmp/workspace-tests
.tmp/workspace-tests
```

15 tests pass: canonical replay render-cadence independence, interruption/seed,
late-frame cap, native observation capacity, invisible bead, calendar twelve-slot
shape, view reads cannot advance, square rest and matching CPU geometry samples.
The production WGSL also parses/validates using the pinned cached naga library
via `tests/bar/workspace-shader.rs`; this is a lightweight CPU test, not a native
runtime/GPU build or pixel/performance acceptance.

Binding contract: tohru_workspace.observe(role, highestOrNil, activeOrNil),
tick(role), reset(role), with roles workspace/calendar and sample
{shape=array16,color=array4,width=number,moving=boolean}. Retain Indicators
outside reloadable Luau. reset(calendar) creates Indicator::calendar() on open.
Common timer factory lives in shell/calendar/rail-publisher.luau; calendar state
includes rail-state automatically. Root adds bar/rail-state + bar/workspace
and initializes bar/workspace with the factory/binding. S owns all shared hooks.
