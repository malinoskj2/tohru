# Home conversation

## Sub-features

`home-direct` (N/H, M for capture) remains unverified by this runner. Entrypoints:
desktop voice, typed popup, Orca GUI continuation. All must use the same public
Home identity/provider history; private workers do not make Home inference concurrent.

## How to get to it (user POV)

Speak an authored synthetic phrase through private capture, type a direct popup
question, then continue in Orca GUI. Each must reach its authoritative final;
voice must bind the exact current request/generation to synthesis/player writes,
and typed answers stay text-only. Missing seed state fails the journey.

## Driving it with the shared runner

`python3 tools/verify/verify.py doctor --scenario home-direct --target sandbox --json`
reports blocked. Required before implementation: real local coordinating caller,
private bus/display/Pulse/IPC instance, history/registry identity, event subscription
before action, real provider and model readiness with scheduler reservation.
Reuse bin/tohru-voice-status and tests/voice-status.py for its bounded status boundary;
its simulated replies do not establish native admission or this journey.

## Gotchas

Negative gates: internal mail cannot become owner, queued request must resolve
through persisted canonical aliases, stale answer/player generation is rejected,
arbitrary newest answer is never selected. Preserve request/provider/final/player
correlation and routing identities in immutable evidence. Never copy authority
from historical reports. No physical hearing or owner satisfaction is established.
