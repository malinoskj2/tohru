# Desktop controls

## Sub-features

`desktop-controls` (C/N) remains unverified by this runner. Entrypoints: Settings
CLI/window, launcher, sound popup and session actions.

## How to get to it (user POV)

Open real Settings, edit a value, close and reopen, observe persisted configuration.
Use Ctrl+F to focus the actual input; invoke the launcher and sound popup. Open
twice to check no duplicate instance. Power actions use recorded stand-ins.

## Driving it with the shared runner

`python3 tools/verify/verify.py doctor --scenario desktop-controls --target sandbox --json`
reports blocked. Follow tests/WINDOW-RUNNER.md, tests/native-settings.sh and
tests/session/native.sh only with their private bus/display/Pulse/config and owned
processes. Prerequisites include sandbox display and virtual input; ordinary browser
or compositor does not require GPU reservation. This wrapper does not yet compose them.

## Gotchas

Native widget actions and observable config/state/exit outcomes are required;
internal setters cannot stand in for input. Private endpoints must not escape
through inherited file descriptors. Preserve screenshots/recordings under this
checkout .tmp with host absolute paths. Use media-gallery for review batches.
Missing display/hardware/seed state is blocked, never a reason to use the host.
