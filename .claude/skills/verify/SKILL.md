---
name: verify
description: Verify Tohru through the shipped session-units CLI in private state, isolated native admission, truthful prerequisites and immutable evidence. Use after relevant changes or when asked to verify Tohru behavior; live Home, model, desktop and recovery gates remain separately unverified.
---

# Verify Tohru

Use the checkout's shared runner; load only the relevant feature map from
[the index](../../../verification/features/README.md). A CPU/native pass cannot
establish host UX, physical hearing or owner acceptance.

## Launch

For the supported shipped command journey, follow
[installation and recovery](../../../verification/features/install-recovery.md).
Run `python3 tools/verify/verify.py doctor --scenario session-units --target sandbox --json`,
then `python3 tools/verify/verify.py run session-units --target sandbox` to launch,
generate actual private session files with bin/tohru, inspect effects, exercise
refusal/invalid-target negatives, clean up owned processes and collect evidence.
No native source, caller identity, display or service manager is needed.

Select the exact checkout/source. Follow
[background work](../../../verification/features/background-work.md) for pinned
offline native preparation. `python3 tools/verify/verify.py launch native-authority --target sandbox --orca-source .tmp/native/source`
creates a private UUID run and records its read-only preflight. No host launch.

## Doctor

Run `python3 tools/verify/verify.py doctor --scenario native-authority --target sandbox --orca-source .tmp/native/source --json`
before effects. Missing source/tools/authority is blocked, not a reason to fabricate
caller environment, weaken isolation, download models or restart the host.

## Drive

`python3 tools/verify/verify.py drive --run ABSOLUTE_RUN` runs the declared native
actions with streaming limits/deadlines. Or compose every phase with
`python3 tools/verify/verify.py run native-authority --target sandbox --orca-source .tmp/native/source`.
The distinct native-authority journey remains an engineering test suite; its pass
does not replace session-units command-effect proof. Other declared scenarios
currently block; read their maps for remaining prerequisites.

## Evidence

After cleanup, `python3 tools/verify/verify.py collect --run ABSOLUTE_RUN`, then
`python3 tools/verify/verify.py validate --run ABSOLUTE_RUN`. Report immutable
collection absolute paths, exact commit/commands, scenario outcome and omitted
boundaries. Accepted receipts are not completed evidence. Preserve failed attempts.

## Cleanup

`python3 tools/verify/verify.py cleanup --run ABSOLUTE_RUN` is repeatable and keeps
evidence. A crashed driver without reaping proof reports cleanup_failed; do not
signal guessed PIDs or restore someone else's state. No host/model cleanup exists.

## Helpers

Shared implementation: tools/verify/verify.py, tools/verify/process.py,
tools/verify/evidence.py, tools/verify/prepare-native.py, tools/verify/scenarios.json,
tools/verify/evidence.schema.json. Check drift and failure boundaries with
`python3 -m unittest discover -s tests/verification -v`.

Maps: [Home](../../../verification/features/home-conversation.md),
[background work](../../../verification/features/background-work.md),
[voice](../../../verification/features/voice-capture-playback.md),
[desktop](../../../verification/features/desktop-controls.md),
[installation/recovery](../../../verification/features/install-recovery.md).
