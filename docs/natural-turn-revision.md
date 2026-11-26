# Decision-first semantic turns: bounded follow-up

This follows `25a5c9b6a64b7e6d9b53407aa7c5e66e70d130ac`, retaining its exact
source in `.tmp/revision-baseline/` and its original frozen twelve-case evidence
in `docs/measurements/natural-turns.json`. The pre-feature source remains
`337530073ae51c5f5e5a16c60c99ccd3776d2ddb`. Semantic mode remains **off**.

## Changes

The tool-free draft emits a strict newline-terminated WAIT/RESPOND first-line
decision independently of answer completion. WAIT stops generation early; a bare
WAIT at EOF is also valid. RESPOND may schedule acoustic handoff, but no answer
delta, commit or speech is permitted until full protocol/tool-marker validation.
The existing controller promotion, cancellation and rollback gates remain in charge.
Malformed final output after an early RESPOND falls back through the ordinary path.
Semantic provider work is bounded at 30 seconds; cancellation cannot publish a
partial answer. Promoted stopped/error drafts use the existing ordinary fallback once.

On malformed output, exhausted two-draft budget, or unsupported backend, capture
uses ordinary endpointing once, cancels speculative work, and reports the fallback
reason. Unsupported routes (including Orca) display a capability warning. Absent
intent gets six quiet seconds, then ordinary fallback. That deadline was changed
from four after the actual development sample below; this is tuning, not held-out
validation. A confirmed WAIT is never overridden by the deadline or an explicit
handoff phrase on the same transcript. It releases capture while retaining bounded
continuation text for new speech. Final ASR revisions cancel obsolete drafts and
fall back once instead of requiring another utterance. Narrow explicit hold phrases
remain listening even in a revised final. Errors are not represented as confident
semantic RESPOND decisions. This degradation favors responsiveness when semantic
evidence is unavailable and can interrupt thinking aloud; it is observable and gated.

Completed-segment interruption behavior is retained unchanged. Full chat text stays
intact; delivery notes are corrective context, not rewritten native provider history
or physical-hearing proof. Orca receives a note in its next request but has no
separately reconciled remote delivery field. Local evidence is saved on the next
committed request; if there is no subsequent request it is not durable.

The unnecessary hard schema bump was removed. The optional default-empty delivery
column is added under a transaction while retaining schema version 1. Older helpers
insert explicit columns and ignore extra read columns. A real pre-feature helper
successfully opened a fresh isolated database created by the new helper; service
fixtures also exercise an old-style insert. Older prompts/exports may omit delivery
metadata. No user database was opened or downgraded. Unshipped schema-2 scratch data
from the first candidate must be exported with that candidate and imported into a
fresh database, not downgraded by editing PRAGMA values. Ordinary rollback to the
pre-feature helper is compatible with new schema-1 databases, though it loses the
context benefit; retain backups/exports when testing changes.

## Measurement

[Raw evidence](measurements/natural-turns-revision.json) retains every matrix row,
including failures. The expanded **development** matrix has 12 scenarios × four
policy timings (immediate, 1 s, 5 s, absent), or 48 capture outcomes. Each is repeated
with simulated 0.6 s, 6 s and 31 s full-answer durations (144 rows; not 144 independent
capture samples). It covers short answers, prior WAIT continuation, multi-revision
ASR, revised final, error, unsupported local/Orca capability and thinking/self-question
cases. The old twelve-case labels/data were not changed. The expanded set is not
independently held out; no generalization claim is made.

| Capture outcome, same 48 cases | Earlier candidate | Revised candidate |
| --- | ---: | ---: |
| Missed intended handoffs at full 36 s observation bound (32 request cases) | 26 | 0 |
| Handoffs later than 3 s | 0 | 17 |
| Premature takeover among 16 WAIT-labeled cases | 0 | 3 |
| Takeover after an accepted WAIT | 0 | 0 |

The three WAIT regressions are thinking, self-question and ambiguous transcripts
with **absent** decisions: ordinary fallback takes the floor at the six-second
deadline. They are not relabeled as correct. Explicit hold language still waits.
Previously, silence on errors/timeouts was counted together with WAIT success;
this report separates them. Rows outside three seconds are delayed, not missed.
The old matrix's missed count uses the whole bound, including capture termination.

Full-answer validation deadlines are separately simulated, not measured speech.
Early policy handoff does not wait for the simulated six-second answer. The 31 s
answer condition is explicitly marked as exceeding the semantic provider's 30 s
deadline; it is not counted as useful output. Existing Lua tests now assert that
an early decision and even buffered answer text do not commit or notify answer
listeners until done/validation. A real fake-CLI transport test separates the early
header from delayed full output by over 150 ms. No first-useful-audio metric is claimed.
Resource cost remains at most two speculative attempts per capture plus one ordinary
fallback, not an unbounded classifier/retry loop; unsupported attempts spawn no draft.

## Actual isolated cloud sample

Authentication probes found configured Claude and Codex CLIs. Six fixed labeled
Claude attempts failed before classification; one bounded diagnosis captured the
concrete error: **weekly quota reached**. This was not an approval or CPU-admission
blocker. No further Claude requests were sent. Six Codex calls then completed via
the production helper: requested model `gpt-6.2-sol`, effort `low`, draft/read-only
mode, MCP and tools disabled, persona/memory/Orca context disabled, fresh isolated
SQLite and scratch directories, no live conversation IDs. Context for the two
follow-up samples was seeded only in that isolated local database. Credentials
were not read, exported or printed; existing CLI authentication was used.

Codex matched **6/6** authored labels: three requests (including a short answer to
a prior question and a request after WAIT), three WAIT cases. This tiny development
sample is encouraging but is not a robust quality gate. Decision time from helper
invocation ranged **3416.60–4822.77 ms**, median **4410.60 ms** (n=6; max equals
nearest-rank p95 and is not a population-tail estimate). For the three responses,
decision preceded validated completion by **687.87, 511.00 and 464.60 ms**. That
separation is real transport evidence, not a claim of equivalent audio-latency gain.
In particular, the current Codex adapter emits text on `item.completed`, not on
individual model tokens: this interval includes completion/CLI shutdown after a
completed assistant message. Claude supports token deltas, but its quota prevented
an equivalent live measurement. See [model/effort follow-up](voice-text-models.md).

Replaying those actual decisions and timings through production capture against
the same six authored transcripts yields:

| Sample replay | Pre-feature acoustic baseline | Revised semantic candidate |
| --- | ---: | ---: |
| Premature handoff, WAIT cases | 3/3 | 0/3 |
| Missed requests within 36 s | 0/3 | 0/3 |
| Request handoff times | 480, 672, 480 ms | 4640, 3680, 4736 ms |
| Request median / maximum | 480 / 672 ms | 4640 / 4736 ms |

These combine recorded cloud timing with simulated capture; physical input, ASR
and output are absent. Invocation includes CLI startup/transport, not just model
compute. The request handoff penalty is substantial, so this is a correctness and
floor-control improvement under available decisions, **not a net latency win**.
No filler or cancelled output is counted. Only thirteen cloud attempts total were
made: seven quota failures and six successful Codex calls; no subscriptions or
provider settings were changed. Billing/token usage was not measured.

## Reproduction and remaining gates

From this worktree (all evidence lives beneath `.tmp/`). These commands use the Python voice service and its replay tools, which the native service replaced. Run them from a checkout of `e3161719` or earlier.

```sh
git show 25a5c9b:bin/tohru-voice > .tmp/voice-v1
TOHRU_VOICE_SEMANTIC=on python3 tools/voice-turn-matrix.py --module .tmp/voice-v1 > .tmp/matrix-v1.json
TOHRU_VOICE_SEMANTIC=on python3 tools/voice-turn-matrix.py > .tmp/matrix-v2.json
TOHRU_VOICE_SEMANTIC=on python3 tools/voice-turn-replay.py > .tmp/original-cases-v2.json
# Explicitly spends at most six isolated cloud calls:
python3 tools/voice-semantic-sample.py --provider codex > .tmp/cloud.log
# Use the results.json path printed by the preceding command:
TOHRU_VOICE_SEMANTIC=on python3 tools/voice-sample-replay.py PATH_TO_RESULTS_JSON
python3 tests/assistant/voice.py
python3 tests/assistant/natural-turns.py
cargo test --manifest-path desktop/Cargo.toml assistant
python3 tests/assistant/service.py
lua tests/assistant/controller.lua
lua tests/assistant/voice-controller.lua
```

Validation: 50 voice tests, 11 natural-turn tests, 31 service tests, both Lua
controller suites, `git diff --check`, plus isolated old-binary schema compatibility.
The unchanged completed-segment evidence is in the original report; no new physical
playback claim is made. Existing cancellation/backchannel/backend tests still pass.

Missing CPU/RAM admission still blocks substantial builds/benchmarks; it did not
block the small cloud sample. GPU/model/TTS, physical mic/speaker/echo, subjective
naturalness, and useful-audio onset remain unmeasured. Default enablement still needs
larger independent/recorded evaluation and acceptable real turn latency. No model
downloads, production integration, merge, host switch or other-worktree edits occurred.

Main has independently gained portability merge
`563d48a3d2abfd1030bdaf736d6bbf9944db84fa`. Eventual integration must preserve its
interpreter/environment re-exec block and both offline `en_core_web_sm` preflight
checks in `bin/tohru-voice`; this branch did not alter its original measurement base
or merge those changes. The independent top-level card remains open.
