# Ordinary conditional window lifecycle — source candidate

Authorized native Settings gap fix, separate from0002 native lock security.
`0003-window-lifecycle.patch` is based on pinned upstream +0001 settings +current
0002 including L90234c6/1c1a5a2 security ownership fixes. It is not yet in shared prepare/Nix patch lists and is
not compiled, GPU-tested or accepted. No new package/dependency/helper or owner.

Proposed implementation:
- Only ordinary `kind: window` with `open:` participates. Unconditional windows,
  layers, native lock faces and authenticated-release paths keep their behavior.
- Conditional windows initially create no native role until render evaluates
  desired open. Event-loop callbacks create or retire windows without polling or
  an output reconnect. Ordinary view data/Settings Owner remain in the process.
- Native WM close sends WindowClosed before retirement; renderer clears a simple
  boolean open fact and forwards that fact to Lua. Compound expressions latch
  closed until their requested condition has been false then true; no guessing
  how to reverse an arbitrary expression or immediate unsolicited reopen.
- Pending GPU targets keep a final owning SCTK Window clone in Placed after the
  pending field. Configured NewSheet carries another owning clone through its
  PlatformWindow; target precedes window in NewSheet/Sheet drop order. Native own
  frames and their queued worker retain this clone too. This covers output removal,
  event-loop disconnect and active shutdown as well as explicit retirement, without
  reinterpreting State.announced (which remains an output announcement list).
- For a GPU owner already sent to render, native Placed is held
  until renderer SheetGone drops its swapchain and acknowledges sheet_released;
  destruction then occurs on Wayland's event loop. Reopening waits for old retirement.
  On disconnect/shutdown the actual queued/installed/frame-worker owning clones
  survive State instead; no accumulating leak-on-shutdown guard is used.
- Desired visibility runs before the GPU-none gate. Static window-only scenes use
  the existing message/appointment rest mechanism while awaiting first open;
  existing time-dependent advancement is retained. No new timer or polling owner.

Companion: copy `visibility.rs` to upstream `src/tohru_window.rs` after applying
0003. Pure intent regressions (`rustc --edition2024 --test -Dwarnings`) cover
initially closed, hide/reopen, WM-dismiss no immediate reopen and compound rearm.
`python3 tests/window-lifecycle.py PREPARED_SOURCE` additionally compiles the actual
coordination block and actual Placed/NewSheet/Role declarations with CPU native/GPU
test resources: first open without sheets/GPU, WM-dismiss/rearm, pending shutdown,
queued configure followed by output removal/disconnect and either teardown order.
It asserts native constructor/worker lease forwarding and placement above the GPU
gate. Eight tests pass including Intent's three tests. These source-extraction
fixtures intentionally substitute driver/native types: they are not a Wayland or
wgpu integration typecheck. Native source parse and patch apply-check pass. These do not prove typed native
compilation, event ordering, resource retirement or presentation behavior.

Next: independent source/lifetime review, one coordinated incremental jobs1 build
only after other build/GPU owners release, then private Sway native lifecycle:
initial false no blank window, explicit open/hide/reopen, WM close makes simple
factfalse, explicit reopen with unchanged output, close before/after configure,
rapid close/reopen with queued NewSheet, root settings managed override retained,
ordinary/lock role separation and native conditional compound latch. No production
activation or security acceptance inferred; do not edit0002 to implement this fix.
