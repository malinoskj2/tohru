---
name: verify
description: Verify Tohru changes in isolation before reporting them done. Drives the public CLI against private state - standalone session-unit staging, the bar's calendar and settings on an owned headless display, and the native Orca admission suite - and records hashed evidence. Use after changes to the shell, settings, CLI, session integration or Orca voice integration, or when asked to verify Tohru behaviour.
---

# Verify Tohru

Tohru is a Pleamar desktop shell. A zero CLI exit only means a request was
accepted; prove the resulting state. Read [the feature index](../../../verification/features/README.md),
then only the map for the area you changed. Maps list unverified entry points;
never report those as passed because a neighbouring journey passed.

Run from the checkout under test. Pick the smallest journey that covers the change:

| Change touches | Journey | Needs |
| --- | --- | --- |
| CLI, session integration, anything | `python3 tools/verify/verify.py run session-units --target sandbox` | Python 3.11+, bash |
| Bar scenes, panels, settings | `tools/verify/desktop.py run --feature calendar` or `--feature settings` | Patched Pleamar runtime, sway, grim, dbus-daemon, GPU lease |
| Orca integration, voice admission | `python3 tools/verify/verify.py run native-authority --target sandbox --orca-source .tmp/native/source` | Prepared Orca source ([background work](../../../verification/features/background-work.md)) |

## Launch

Desktop runs take the patched Pleamar 0.7.27 executable explicitly and a GPU
reservation held through cleanup:

```sh
verify_runtime=/absolute/path/to/patched/bin/pleamar
gpu-run --vram 4GiB --name 'Tohru desktop verification' -- \
  tools/verify/desktop.py run --feature calendar --runtime "$verify_runtime"
```

Invoke it directly when `GPU_LEASE_ID` is already set. Build a runtime with
`runtime/README.md`; this skill never downloads, installs or rebuilds anything.

## Doctor

`tools/verify/desktop.py doctor --runtime "$verify_runtime"` and
`python3 tools/verify/verify.py doctor --scenario SCENARIO --target sandbox --json`
are read-only. A missing tool, runtime, source or authority is a blocker to
report, never a reason to use the host session, fabricate a caller or bypass the
GPU scheduler. `tohru doctor --json` checks an installed desktop, not these runs.

## Drive

Each driver launches, checks its own instance, drives the public commands and
cleans up in one foreground process. The runner also exposes `launch`, `drive`,
`cleanup`, `collect` and `validate` phases for longer or resumable runs.
Other runner scenarios exit 3 (blocked) by design.

## Evidence

Report the exact commit, command, outcome and absolute evidence path, plus what
stayed unverified. The runner writes `.tmp/verify/SCENARIO/UUID/`; check it with
`verify.py validate --run PATH`. The desktop driver prints `.tmp/verify-UNIQUE/`;
check `summary.json`, `cleanup.json` and the screenshots against `commands.jsonl`.
Preserve failed runs. Show three or more images with the media gallery.

## Cleanup

Both drivers reap only their own process groups and keep evidence;
`verify.py cleanup --run PATH` is repeatable. Never kill `pleamar`, `sway` or
Orca by name, restart the host bar or touch host settings.

## Helpers

Code lives in `tools/verify/`. After changing it or a map, run
`python3 -m unittest discover -s tests/verification -v`. This file and
`.claude/skills/verify/SKILL.md` must stay byte-identical.
