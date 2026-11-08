# Ordinary conditional window lifecycle — source candidate

Authorized native Settings gap fix, separate from0002 native lock security.
`0003-window-lifecycle.patch` is based on pinned upstream +0001 settings +0002
cf347ff/a176c61 security. It is not yet in shared prepare/Nix patch lists and is
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
- Retirement first removes unpublished/pending NewSheet GPU owners before native
  role destruction. For a GPU owner already sent to render, native Placed is held
  until renderer SheetGone drops its swapchain and acknowledges sheet_released;
  destruction then occurs on Wayland's event loop. Reopening waits for old retirement.
  Unacknowledged handles at process teardown are intentionally retained rather than
  destroyed under an unproven renderer owner; process exit releases them.

Companion: copy `visibility.rs` to upstream `src/tohru_window.rs` after applying
0003. Pure intent regressions (`rustc --edition2024 --test -Dwarnings`) cover
initially closed, hide/reopen, WM-dismiss no immediate reopen and compound rearm.
Native source parse and patch apply-check pass. These do not prove typed native
compilation, event ordering, resource retirement or presentation behavior.

Next: independent source/lifetime review, one coordinated incremental jobs1 build
only after other build/GPU owners release, then private Sway native lifecycle:
initial false no blank window, explicit open/hide/reopen, WM close makes simple
factfalse, explicit reopen with unchanged output, close before/after configure,
rapid close/reopen with queued NewSheet, root settings managed override retained,
ordinary/lock role separation and native conditional compound latch. No production
activation or security acceptance inferred; do not edit0002 to implement this fix.
