# Natural turns: preregistered lightweight gate

**Current revision:** see [decision-first follow-up](natural-turn-revision.md).
The results below describe the original `25a5c9b` candidate and remain unchanged
as historical evidence. Its four-second abstention and schema-2 design have been
superseded; semantic mode is still off by default.

Baseline: `337530073ae51c5f5e5a16c60c99ccd3776d2ddb`. Before runtime edits,
50 production voice fixture tests passed and the frozen 12-case authored replay
reported nine premature handoffs and zero missed requests. Source copies and raw
output are in `.tmp/natural-baseline/`. No recorded inputs or held-out split exist;
all cases are development fixtures, and cannot establish semantic model quality.

Acceptance fixed before implementation: mock WAIT/error/timeout must never speak;
RESPOND must preserve the existing acoustic minimum; stale decisions must not
authorize a changed transcript; resumed speech must invalidate prior authorization.
No new tools, commits, or speech before validated handoff; at most two speculative
calls per capture, with bounded capture duration. Completed playback segments may
be described as application playback evidence; fractional text must never be
described as definitely heard. Existing wake, follow-up and backchannel checks pass.

Semantic mode stays opt-in until actual tool-free provider quality and latency
evaluation, physical audio and subjective naturalness checks pass. CPU/RAM
admission is absent (`/etc/resource-scheduler/mode` and `resource-run` unavailable);
substantial benchmarks and model runs are blocked. Lightweight fixtures use no
models, provider accounts, GPU, microphone or speaker.

## Results and evidence limits

Raw bounded per-case data: [natural-turns.json](measurements/natural-turns.json).
The baseline file SHA-256 is
`288c42f82dd6cb19aa23a3302f7eff2518687098e13694a193fff76050085a38`.
Replay commands (from the worktree root). These commands use the Python voice service and its replay tools, which the native service replaced. Run them from a checkout of `e3161719` or earlier.

```sh
git show 337530073ae51c5f5e5a16c60c99ccd3776d2ddb:bin/tohru-voice > .tmp/baseline-voice
python3 tools/voice-turn-replay.py --module .tmp/baseline-voice
TOHRU_VOICE_SEMANTIC=on python3 tools/voice-turn-replay.py
python3 tools/voice-history-replay.py --module .tmp/baseline-voice
python3 tools/voice-history-replay.py
python3 tests/assistant/natural-turns.py
python3 tests/assistant/voice.py
cargo test --manifest-path desktop/Cargo.toml assistant
python3 tests/assistant/service.py
lua tests/assistant/controller.lua
lua tests/assistant/voice-controller.lua
```

Lua was `/nix/store/il4hkpqw0hzlax68ksl06y6znw4p6zan-lua-5.3.6/bin/lua`.
The original baseline and logs remain at the worktree's
`.tmp/natural-baseline/`, candidate data/logs under `.tmp/`.

| Fixed authored fixture metric | Baseline | Opt-in candidate |
| --- | ---: | ---: |
| Premature handoff among 9 WAIT/error/timeout cases | 9/9 | 0/9 |
| Missed handoff among 3 RESPOND cases | 0/3 | 0/3 |
| Handoff delay median, 3 requests | 480 ms | 480 ms |
| Handoff delay maximum / nearest-rank p95, 3 requests | 672 ms | 480 ms |
| Speculative calls emitted across 12 cases | 8 | 12 |
| Partial text presented as heard, 9 playback cuts | 6/9 | 0/9 |

Handoff clocks count 32 ms application capture windows after the last injected
speech-positive window, not physical speech end or measured ASR completion. Policy
answers arrive immediately after the first speculative request; full model response
latency is absent. The three-sample maximum is not a population tail estimate.
The shorter single-word-answer path is a changed draft threshold, not a measured
latency improvement. First useful audio was **not measured**. No filler counts.

Delay sensitivity uses the same corpus with `--decision-ms 1000` and
`--decision-ms 5000`. At 1000 ms the candidate still has zero premature/missed
handoffs, but request handoffs are 1280, 1280 and 480 ms (median/max/p95 1280 ms),
versus the unchanged baseline 480, 672 and 480 ms. At 5000 ms two of three requests
are missed within their three-second replay windows; only the explicit handoff
responds, at 480 ms. WAIT/error cases still do not seize the floor. These regressions
are retained in the raw data and are a reason to keep the feature gated.

Playback cuts span before synthesis consumption, beginning/middle/end of both
one-second synthetic clips and normal completion, using production `Speaker.play`
with a controlled pacing clock and fake pipe. Six baseline cases exposed partial
words; candidate context includes completed segments and an uncertainty note.
Neither pacat submission nor the pacing deadline proves actual hearing. A separate
queued-generation regression verifies that new synthesis cannot inherit old text.
Existing fixtures cover cancellation, backchannel resume, backend changes and
generation checks; the nine-cut replay does not independently time all those paths.

Validation: 50 existing voice fixtures, 29 assistant service fixtures (including
strict semantic protocol via fake Claude CLI), eight additional natural-turn
regressions, both Lua controller suites, and `git diff --check`. The new freshness
fixtures reject changed ASR text, stale tokens after resumed speech/end, and revised
final recognition; they exercise a long WAIT followed by explicit handoff. There
were no failing freshness assertions; this is not a measured live stale-response rate.

## Tradeoffs and unavailable acceptance gates

This is a gated implementation, **not evidence of improved classifier quality or
subjective naturalness**. The actual-provider semantic quality gate is unverified:
no authorized isolated provider evaluation route was established without touching
live accounts. No local model was selected, downloaded or loaded. Missing resource
admission blocks substantial CPU/RAM and model/TTS benchmarks. GPU work was not run.
Physical mic/speaker/echo, useful audio onset and human listening remain untested.

Costs can worsen: short answers and unfinished thoughts now request a semantic
draft, so this corpus emits 12 instead of eight calls. A decision is accepted only
after complete provider output, preserving marker validation but delaying handoff
for long answers. A provider taking over four quiet seconds is cancelled; unsupported
providers abstain on ordinary requests until a narrow explicit handoff. Two ASR
revisions can exhaust the per-capture draft budget. A changed final transcript is
retained for another utterance rather than automatically issuing a serial recheck.
These can miss intended handoffs in real use; none was estimated with an oracle.

No held-out tuning or recorded-data claim is made. The frozen twelve-case corpus
does not estimate quality on real transcripts or cover all acoustic gaps. Additional
regressions are implementation tests authored afterward, not a held-out set.
Further isolated live evaluation, realistic delays and larger admitted replays are
required before considering default enablement. Semantic mode therefore stays off.

Reference code read: n33tc0de's `VOICE_TURN_INSTRUCTIONS`, strict-prefix
`turn-output.ts`, `useVoiceConversation.ts` decision/reuse/history paths,
`lib/voice-turn.ts`, and the November 8 latency / November 10 turn-detection reports.
This change borrows contextual floor decisions and completed-segment evidence;
Tohru already had Silero/Smart Turn thresholds. It adds no n33tc0de dependency.

Merge touchpoints: voice process and both assistant Lua controllers, assistant
helper/schema migration, service fixtures and voice docs. SQLite advances to schema
2; older helper binaries refuse newer databases. Native provider histories retain
generated replies and receive corrective context; Orca remote history cannot be
rewritten by this implementation. Full chat text is preserved, with local delivery
metadata applied at the next committed request. No merge, deploy or host switch
was performed; the independent top-level worktree stays open.
