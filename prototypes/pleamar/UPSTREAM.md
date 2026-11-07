# Pinned sources and changes

Framework: https://github.com/k4ditano/pleamar
Revision: `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86` (0.7.27, inspected 2026-11-07).
Vendored build source, embedded docs/skill/UI library, lockfile and original
BSD-3-Clause license are included; example assets/editor/tools are not needed.
The vendored Rust is rustfmt-formatted. No upstream service or other shell UI
is used by the prototype.

Reference: `/home/jesse/projects/j2bar` revision
`3a8daa5e077f4085b5262041ea4832373b8754ca`, specifically `src/track.rs`,
`src/kit/motion.rs`, `src/shaders/track.frag`, GPU track upload, and
`docs/design/workspace-indicator/{README.md,workspace-indicator.js,demo.html}`.
`workspace_track.rs` preserves that simulation, with the requested seed and
JS square end segment at rest. `workspace_motion.rs` keeps its exact RNG and
constants. Replay and correctness tests are appended beside the simulation.

## Explicit framework patch

Upstream's scene shader `values:` accepts eight numbers (`s.a`, `s.b`), plus
two colors. This cannot carry the full spring/wave/shape state without lossy
packing. We add four vec4s (`s.c` through `s.f`), raising the checked bound to
24, and extend the element stride from 64 to 80 floats (256 to 320 bytes).
CPU upload and WGSL storage structs agree; the GPU's element store still has
an explicit fixed stride. Other element kinds get zeroed extra fields. Group
shader parameters remain the upstream eight; this prototype uses a plain scene
shader. No arbitrary buffer, shader bindings, or replacement renderer is added.
`patches/framework.patch` records the semantic framework changes before rustfmt;
the three `workspace_*.rs` modules and scene files are separate additions.

The native Lua globals `workspace_tick`, `workspace_external`, `workspace_select`,
`workspace_sway`, and `workspace_mode` expose this specific prototype. The first
owns the exact Rust simulation. Sway requests use bounded native IPC reads with
200ms timeouts and a 1MiB response cap; live external changes are polled at 100ms.
Hyprland uses upstream `sys.watch("workspaces")` and `sys.call` (not runtime-tested
here). The normal-mode prototype intentionally does not auto-install upstream
editor skills on launch. Added error boundaries use thiserror and anyhow.

WGSL ports the reference ribbon pixel coverage, 28-segment outline, palette,
two integrated Gaussian drop shadows and compositing order. The wave profile
is evaluated in the shader from exact CPU simulation state, using WGSL f32
rather than CPU f64 before sampling, so there can be tiny numerical differences.
The shader reads no time and the idle mode has no timer, so settled state sleeps.
The prototype's animation publication is on Luau's 60Hz timer; the canonical
physics is still 240Hz. It is not Pleamar's built-in spring animation path.
