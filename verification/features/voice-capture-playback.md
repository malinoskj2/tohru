# Voice capture and playback

## Sub-features

`capture-parity` (C/M/H) and `expiry-restoration` (C/N/H) remain unverified by this
runner. Replay, native/Python contract, capture, synthesis, player, barge-in and
typed-retirement cleanup are separate boundaries.

## How to get to it (user POV)

Capture authored audio through private null input/output, observe exact recognized
text, authoritative reply and request/generation player binding. Interrupt it,
retire the synthetic origin through actual typed input and verify a fresh completed
post-restoration observation interval with zero later writes for retired IDs.

## Driving it with the shared runner

`python3 tools/verify/verify.py doctor --scenario capture-parity --target sandbox --json`
reports blocked. tools/voice-native-contract.py and `tohru-voice-native --replay`
provide finite CPU/model entrypoints.
Before choosing a model read MODEL_ROOT/catalog.json, readiness.json and README.md,
check files plus runtime/base/adapter compatibility, then use a foreground gpu-run
reservation including teardown (reuse GPU_LEASE_ID, never nest). Missing models
need host `systemctl --user start local-models-sync.service`, never a downloader.
Private Pulse/display and authored corpus are prerequisites; no physical audio.

## Gotchas

Ground truth differs from recognized baseline. Finite waveform equality is not
universal parity; eSpeak fallback uses semantic/format/process criteria, unavailable
KWS stays unverified. Evidence needs WAV/text/runtime/model hashes, format/rate/frame
count/nonzero PCM, exact IDs and missing clocks recorded as null. Submitted PCM
and nonzero null monitor prove no human hearing. Expiry must admit typed retirement
before restoration, preserve isolation on rejection, tolerate partial setup/cleanup
twice and reject external routing/drop-in edits. These operations are not implemented
by the CPU runner. Host defaults/device/scheduler settings must not be changed.
