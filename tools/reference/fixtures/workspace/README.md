# Exact workspace port reference

These nine files are byte-identical excerpts of the deleted prototype at
`9119cfc23911e7c29bee66ee0ad459c303bbce5e`, not a runtime, supported API or build
package. `provenance.json` records original paths, byte lengths and SHA-256;
`test_tooling.py` verifies the files against both manifest and Git source blobs.
The complete prototype and dependency locks remain in Git history.

- `workspace_motion.rs`: exact 1/240s step, 50ms live accumulator cap, 6e-3 rest
  energy, xorshift64* generator and per-switch draws.
- `workspace_track.rs`: spring/wave/wash, palette/bead/ribbon state, exact square
  rest, shader scalar layout and embedded deterministic/interruption/rest tests.
  The 12-second EVENTS replay and seed `0x0123456789abcdef` are authoritative;
  publication cadence must not change simulation results.
- `workspace_ipc.rs`: historical bounded Sway IPC (200ms timeout, 1MiB reply cap).
  Its presence does not authorize a new adapter or prototype polling in production.
- `ribbon.wgsl`: precise ribbon/bead coverage, 28-segment outline, glows and
  compositing; f32 evaluation may differ slightly from reference f64 samples.
- `workspace.plm`, `workspace.luau`: scene geometry, five hit cells and 24-value
  binding/publication example, not production scene/event contracts. Historical
  60Hz publication is separate from 240Hz physics; settled motion stops timers.
- `framework.patch`: bounded 8-to-24 scalar ABI, upload/layout and thin Luau bridge
  changes, isolated from the full vendor tree. Historical patch is reference only;
  do not apply wholesale to the production pin or retain its unrelated startup
  choice. Preserve only the necessary accepted extension when implementing.
- `J2BAR-LICENSE`: MIT notice for derived motion/shader work; `PLEAMAR-LICENSE`:
  BSD-3-Clause notice for framework patch context.

The future port must retain exact interrupted motion, canonical seed and square
16×4 rest, with 240Hz simulation independent of display rate and no simulation or
animation frames at rest. The prototype has 19 meaningful scalar values including
RGB, although its ABI allocates 24; reducing redundant inputs is allowed only
without changing behavior. Native-glass and full shell equivalence remain open.

No Cargo manifest or executable is provided here deliberately. To inspect any
removed file without restoring a vendor tree:

```sh
git show 9119cfc23911e7c29bee66ee0ad459c303bbce5e:prototypes/pleamar/UPSTREAM.md
git show 9119cfc23911e7c29bee66ee0ad459c303bbce5e:prototypes/pleamar/vendor/pleamar/Cargo.lock
```

Preservation checks prove byte identity, not a new Rust build/test or GPU run.
