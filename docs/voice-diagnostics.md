# Voice diagnostics and background results

Inspect the running desktop voice service with:

```sh
journalctl --user -u tohru-bar.service --since '10 minutes ago' --no-pager
```

Voice diagnostic records have a session ID, a monotonic timestamp and `wall_ms` for correlation with other processes. Per-utterance events carry `turn_id`; provider and playback events also carry a speech generation.

To investigate a missed wake, follow `utterance_start`, `wake_decision`, `asr_finalize_end`, `conversation`, `ui_visibility_request` and `ui_state_observed`. The onset includes VAD probability and microphone RMS. A wake decision records acceptance or a reason such as `final_name_mismatch`, `empty_final` or `kws_no_match`. Only the matched name form, or the bounded leading word when none matched, is logged; the request and full transcript are excluded. Keyword-spotter confidence is unavailable when its API supplies only a detection result.

`capture_suppressed` distinguishes playback, echo cooldown, the thinking-state microphone floor and VAD rejection. It reports bounded scores and a count of suppressed windows, at most once per reason every five seconds. It does not produce per-frame logs or save microphone audio.

`assistant_request`, `speech_stream_start`, `first_provider_text`, `provider_done`, `synthesis_ready` and `sentence_first_write` separate recognition, provider, synthesis and playback delays. The UI events acknowledge the desktop controller's observed visibility and flight opacity. They are not proof that the compositor presented a particular frame. Set `TOHRU_VOICE_LOG=off` in the service environment to disable diagnostic output.

## First audio

`first_audio` marks the first write of each reply's audio, with the voice profile and `onset_ms`, the offset of its first audible sample. `speech_stream_start` and `committed_answer_ready` name the reply's path: `draft`, `normal`, `quick`, `home`, `ack` (an acknowledgement), `notice`, `greeting`, `task`, `alert` or `result`. `ack_decision` records the acknowledgement model's `when` (`now`, `late` or `none`) for each turn, and whether it was drafted while the owner was talking (`speculative`). `smart_turn` records the end-of-turn model's score and `endpoint` the hold that ended the turn. `speech_yield` records how long speech ran on to reach a gap between words after the owner interrupted it, and `high_stream_gap` any late chunk while the high voice streams.

Summarize end of speech to first audible sound per path and profile, at p50 and p95:

```sh
journalctl --user -u tohru-bar.service --since today -o cat | tohru-voice --first-audio-report
```

End of speech is the last window the voice detector heard as speech, about 80 ms after the last audible sample. For an acknowledged turn, `first_audio_ms` is the acknowledgement and `answer_audio_ms` the answer behind it.

`tools/voice-first-audio.py` measures the same interval end to end with the real models. It plays recorded utterances through a fake microphone in real time and timestamps the first audible sample written to a fake speaker. Device and room latency are excluded.

Measured on an RTX 5090 desktop with synthesized utterances, in milliseconds (p50 / p95):

| Path | Before | After |
| --- | --- | --- |
| Canned reply, Kokoro (voice pipeline alone) | 1030 / 1658 | 1216 / 1324 |
| Canned reply, Tomoko | 1866 / 2324 | 750 / 971 |
| Quick answer, Kokoro | 4125 / 4849 (Claude CLI) | 2201 / 2984 (Messages API) |
| Request for work, 4 s reply: first sound | 5034 / 5602 | 831 / 1376 (acknowledgement) |
| Other turn, 4 s reply: first sound | 5043 / 5155 | 2313 / 2339 (acknowledgement) |
| Home turn, Opus through the CLI | 4569 / 6669 | unchanged |

The canned-reply p95 fell because a lagging transcript no longer triggers the 1050 ms unfinished-phrase hold.

Model-written acknowledgements against the fixed lines they replaced, Kokoro, eight turns each, with the acknowledgement model reached through the Claude CLI (no API key was available for the measurement):

| Turns | Fixed lines: first sound, acknowledged | Model-written: first sound, acknowledged |
| --- | --- | --- |
| Request for work, 4 s reply | 843 / 1433, 8 of 8 | 2391 / 3513, 8 of 8 |
| Other turn, 4 s reply | 2326 / 2346, 8 of 8 | 4823 / 5022, 4 of 8 |
| Quick answer (Sonnet through the CLI) | 2335 / 2375, 8 of 8 | 3327 / 10702, 4 of 8 |

Through the CLI the line is ready 1.8 to 3.0 s after end of speech, later than the turn starts (about 580 ms), so a work turn's first sound waits for it. The other turns acknowledged were "late" lines; the rest were judged to need none and wait silently for the answer. Over the Messages API the call is expected to finish before the turn starts, but that path was not measured.

## Analyzed background results

An Orca worker completion can return the coordinating bot's final analyzed answer to the voice request that started the work. Raw worker reports, commentary and text-origin requests do not become speech. A new owner turn or voice interruption retires the older request's speech eligibility.

Tohru speaks a matching result only when it is already visible, the result belongs to its current voice request, and capture or another reply is not active. A popup showing another conversation blocks delivery. Hidden, superseded and duplicate results remain in Orca; they do not open Tohru or become a later spoken notification. Diagnostics record `continuation_speech` or `continuation_dropped` without logging the answer.

The return path uses live host-local completion events. Correlation is bounded and resets when Orca restarts; results missed during a restart remain in Orca. It does not poll chats or replay a backlog of speech.

Explicit [conversational task updates](task-speech.md) have a separate acknowledgement, progress, and completion endpoint. They require a producer that opts into that protocol. `task_update` and `task_update_dropped` diagnostics record the task ID, sequence, kind, and rejection reason without logging spoken content.

Task records also include the request, speech generation, and capture ID. Hidden and active-hearing rejections are logged before admission. `speech_stream_start.request` links direct provider speech to its Orca request. `waiting_config.enabled` records whether optional chatter is configured when voice starts or the setting changes. These records establish scheduling and pipe submission, not audible delivery.
