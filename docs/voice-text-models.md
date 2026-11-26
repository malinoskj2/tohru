# Spoken text model and effort comparison — 2026-11-22

**Decision:** use `gpt-6-luna` / **low** for Codex's ordinary tool-free spoken
drafts when neither model nor effort is configured. Keep `gpt-6.2-sol` / **low**
for optional semantic drafts, and `gpt-6.2-sol` / **medium** for typed or
tool-enabled work. Claude stays at its existing default pending an actual test.
This is the best measured split for these samples, not proof of a global optimum.
No host configuration, deployment, provider selection or TTS engine was changed.

The faster profile is deliberately limited to the path tested with tools disabled.
If an answer needs tools, the existing marker validation and ordinary fallback use
the regular model; no untested downgrade of desktop-action handling is introduced.
Short turns which do not start a speculative draft also keep the regular model.
Provider-only overrides bypass these profiles; specific `voice_models` or
`semantic_models` overrides can now select each scenario independently. This
keeps existing user pins authoritative; it does not silently replace a selected
Claude provider with Codex. Semantic detection remains off by default.

## Method and results

The production `provider_argv`, `run_provider`, stream parser, voice instruction
and tool-free draft instruction were used. Runs used ephemeral CLI sessions and
isolated scratch, with no MCP servers, audio, live conversation IDs, memory,
persona or action execution. Each call had a 25-second deadline. No local models
were loaded; CPU/RAM admission was not needed for these small cloud scripts.
The model cache listed low as the lowest supported effort for all tested models,
so unsupported `minimal` was not guessed or benchmarked.

Four authored prompts and rubrics were fixed in the runner before execution:
hash-map performance with its qualification, simple arithmetic, microphone-icon
uncertainty, and an action which must return `<<needs-tools>>`. Two repeats were
interleaved by model, reversing model order on the second repeat. The additional
Luna/Astra block ran afterward, so cross-block server/time variance remains a
limitation. No independent held-out claim is made.

Times below are medians for **six useful answer samples** per configuration
(three answer prompts × two repeats). The two tool-marker samples per configuration
are excluded from useful-answer timing, retained in raw evidence and checked for
false action claims. All timings include CLI startup and cloud transport.

| Codex model | Effort | First text | Completed turn | Maximum completed turn |
| --- | --- | ---: | ---: | ---: |
| gpt-6.2-sol | low | 4.662 s | 5.234 s | 9.052 s |
| gpt-6.2-sol | medium (regular default) | 4.722 s | 5.340 s | 5.837 s |
| gpt-6.1-sol | low | 4.607 s | 5.106 s | 5.732 s |
| gpt-6.1-sol | medium | 4.360 s | 4.873 s | 5.700 s |
| gpt-6-luna | low | **3.251 s** | **3.746 s** | 4.786 s |
| gpt-6-luna | medium | 3.636 s | 4.226 s | 4.642 s |
| gpt-6.2-astra | low | 4.873 s | 5.470 s | 6.128 s |

Luna/low reduced median completed text by **29.8%** and first-text arrival by
**31.1%** against the current SOL/medium default. Low effort alone was not a reliable
win: on SOL 6.2 it saved only 2% at the median and had a slower observed maximum;
on SOL 6.1 medium was faster than low. With n=6, nearest-rank p95 is the maximum,
not a credible estimate of population tail latency. These were fresh CLI sessions,
not cold model servers or controlled network conditions.

The current Codex adapter delivers `item.completed` assistant messages. “First
text” is therefore completed-message arrival, **not time to first model token**.
“Completed turn” is adapter/process completion. Silent speculative drafts still
must pass validation and commit before TTS. No sound was synthesized or played;
these numbers are not microphone-to-speaker latency or an acoustic-onset claim.
Claude's token-streaming path may have different latency and cannot be inferred
from these Codex results. Token usage, provider billing and physical TTS duration
were not measured.

## Quality checks and the rejected semantic shortcut

The core answers were factually suitable for these small prompts, with concise
speech and no false microphone-state guarantee. All fourteen initial action trials
returned the tool marker, never a false success claim. The small models sometimes
omitted helpful follow-up advice; one SOL 6.1 lookup answer mentioned resizing,
which is a less precise qualification for lookup than collisions. These samples
do not establish broad reasoning or tool-execution quality.

Eight further confirmation calls compared Luna/low with SOL 6.2/medium on probability
without replacement (30%), clock arithmetic (15:45), an undefined “blarg” mass unit,
and a requested mute action. Both got the arithmetic right, declined to invent a
conversion and returned the tool marker for the action. Both failed the stronger
rubric's request to ask a clarifying question about “blarg”: they stated there was
no known conversion and stopped. That limitation is retained rather than scored
as full rubric compliance. Confirmation prompts were authored after the first
comparison; they are additional development checks, not a blind hold-out.

Six actual contextual semantic samples with Luna/low matched **4/6** labels. It
incorrectly returned WAIT both for an answer to the assistant's previous question
and for a clear request after WAIT. That is an unacceptable turn-taking regression,
so the fast model is **not** the default semantic classifier/draft generator.
The preceding SOL 6.2/low sample matched **6/6** on these same fixed prompts;
its full evidence is retained, but six samples still do not prove semantic quality.
Semantic drafts reuse that SOL answer; no serial second model call was added.

One Claude Sonnet 5.5/low availability sample failed with the concrete message
“You've hit your weekly limit”. No more Claude calls were attempted in this task.
Therefore there is no evidence here to choose between Claude Opus/Sonnet or their
effort settings, and its default was not changed based on speculation.

## Reproduce, evidence and verification

[Raw authored prompts, replies, configuration and timing rows](measurements/voice-text-models.json)
include all outcomes, including the two Luna semantic mistakes and Claude failure.
Scratch evidence is under `.tmp/voice-models-*` and `.tmp/semantic-cloud-*` in this
worktree. This task made 71 bounded cloud attempts: 56 initial Codex text samples,
eight confirmation calls, six Luna semantic calls and one failed Claude probe.
The six earlier SOL semantic samples are reused evidence, not new calls.

```sh
# 32 Codex samples and one Claude availability probe:
python3 tools/voice-text-model-benchmark.py > .tmp/voice-model-benchmark.log
# 24 additional Codex samples:
python3 tools/voice-text-model-benchmark.py --extra > .tmp/voice-model-extra.log
# Eight confirmation samples:
python3 tools/voice-text-model-benchmark.py --confirm > .tmp/voice-model-confirm.log
# Six semantic samples; model is explicit and does not depend on changing defaults:
python3 tools/voice-semantic-sample.py --provider codex --model gpt-6-luna > .tmp/luna-semantic.log
cargo test --manifest-path desktop/Cargo.toml assistant
python3 tests/assistant/service.py
lua tests/assistant/controller.lua
```

The service regression checks cover regular, voice draft, semantic draft and
explicit-override resolution, provider discovery metadata and actual start-event
model/effort. Full service and Lua controller suites must pass before commit.
Broader quality, long/context-heavy conversations, persona-on performance, actual
tool use, real TTS and Claude comparisons remain separate gates. The code change
is reviewable in this branch; no merge, host switch or production rollout occurred.
