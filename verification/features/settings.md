# Settings

Users change desktop appearance through Settings or CLI and can read back the
effective value and the writable configuration.

## Sub-features

- `settings-cli`: validated edit, persistence and reload.
- `settings-ui`: open/reopen/close, search and edit appearance.
- `settings-managed`: display and enforce managed values.
- `settings-restart`: retain writable settings after process restart.

## How to get to it (user POV)

- Run `tohru settings open`; select appearance controls or press Ctrl+F to search.
- Close with `tohru settings close`, the window manager or Escape.
- Run `tohru config set appearance.corner_size 29`, then `tohru config reload`.
- Read `tohru status --json` and `tohru settings-error` for results.

## Driving it with the desktop driver

Preconditions: fresh baseline with writable disposable configuration, patched
runtime and a GPU lease. No managed overlay is installed in this journey.

- **CLI edit.** Run `tools/verify/desktop.py run --feature settings --runtime "$verify_runtime"`
  inside the lease. The public config command must write corner size 29.
- **Second views.** The driver captures actual persisted `config.toml` and public
  `settings-status.json` after reload; both must show 29 and no settings error.
  Inspect `settings.png` and `settings.json` for the actual bar identity.
- **UI path.** Use the existing private `tests/native-settings.sh --settings`
  harness with required pointer/keyboard/wtype arguments from
  `tests/WINDOW-RUNNER.md`. It drives an evidence-only focus canary for Ctrl+F;
  document that scene change. `tools/verify/desktop.py` never opens an unowned Settings process.

## Gotchas

- The native helper proves only the CLI entry. A visible bar screenshot is not
  evidence that the Settings window/search controls worked.
- Corner size is persisted even with corners disabled for this isolated run.
- Reload is not a restart proof. Managed policy, atomic invalid reload and
  restart persistence remain additional native-suite gates.
- Do not run the public config command in the inherited live-session environment.
