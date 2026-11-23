# Launcher

Users open the app launcher, search available applications and launch or dismiss
without affecting an unrelated session.

## Sub-features

- `launcher-open`: CLI open/toggle/close and bound shortcut.
- `launcher-search`: keyboard search and selection.
- `launcher-spawn`: selected application appears on the private display.
- `launcher-lifecycle`: reopening and role ownership remain consistent.

## How to get to it (user POV)

- Run `tohru launcher open|toggle|close`, commonly bound to Super+Space.
- Type a search query, select an application, press Return; Escape dismisses.

## Driving it with the native launcher harness

Preconditions: patched runtime, private bus/display, built pointer/keyboard
helpers as required by the suite and isolated application desktop entries. Read
`tests/bar/native.py` and `tests/launcher/native.py` before executing.

- **Existing harness.** The repository command is
  `python3 tests/bar/native.py --runtime "$verify_runtime" --launcher-only`;
  run under an exclusive GPU lease with its documented native loader setup.
  It owns its private transport; do not forward `tohru launcher` to host IPC.
- **Lifecycle variant.** `--lifecycle-only` selects
  `tests/launcher/lifecycle.py`. Inspect the suite's own evidence summary and logs.
- **Proof.** Require action transcript, observed visible launcher/search results,
  a private launched application and dismissal evidence. A fake desktop entry's
  launch proves its boundary only, not arbitrary installed application behavior.

## Gotchas

- This seed did not execute the launcher suites. `drive.py` has no launcher mode.
- Existing suites assert a specific NVIDIA adapter and need compiled helpers;
  software calendar rendering does not satisfy those gates.
- No app catalog or host desktop entries are copied into the baseline. Missing
  search targets must fail explicitly rather than weaken the launch journey.
