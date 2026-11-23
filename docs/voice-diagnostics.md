# Voice diagnostics and background results

Inspect the running desktop voice service with:

```sh
journalctl --user -u tohru-bar.service --since '10 minutes ago' --no-pager
```

Voice diagnostic records have a session ID, a monotonic timestamp and `wall_ms` for correlation with other processes. Per-utterance events carry `turn_id`; provider and playback events also carry a speech generation.

To investigate a missed wake, follow `utterance_start`, `wake_decision`, `asr_finalize_end`, `conversation`, `ui_visibility_request` and `ui_state_observed`. The onset includes VAD probability and microphone RMS. A wake decision records acceptance or a reason such as `prefix_mismatch`, `decision_timeout`, `final_name_mismatch` or `kws_no_match`. Only a bounded leading name candidate is logged; the request and full transcript are excluded. Keyword-spotter confidence is unavailable when its API supplies only a detection result.

`capture_suppressed` distinguishes playback, echo cooldown, the thinking-state microphone floor and VAD rejection. It reports bounded scores and a count of suppressed windows, at most once per reason every five seconds. It does not produce per-frame logs or save microphone audio.

`assistant_request`, `speech_stream_start`, `first_provider_text`, `provider_done`, `synthesis_ready` and `sentence_first_write` separate recognition, provider, synthesis and playback delays. The UI events acknowledge the desktop controller's observed visibility and flight opacity. They are not proof that the compositor presented a particular frame. Set `TOHRU_VOICE_LOG=off` in the service environment to disable diagnostic output.

## Analyzed background results

An Orca worker completion can return the coordinating bot's final analyzed answer to the voice request that started the work. Raw worker reports, commentary and text-origin requests do not become speech. A new owner turn or voice interruption retires the older request's speech eligibility.

Tohru speaks a matching result only when it is already visible, the result belongs to its current voice request, and capture or another reply is not active. A popup showing another conversation blocks delivery. Hidden, superseded and duplicate results remain in Orca; they do not open Tohru or become a later spoken notification. Diagnostics record `continuation_speech` or `continuation_dropped` without logging the answer.

The return path uses live host-local completion events. Correlation is bounded and resets when Orca restarts; results missed during a restart remain in Orca. It does not poll chats or replay a backlog of speech.

Explicit [conversational task updates](task-speech.md) have a separate acknowledgement, progress, and completion endpoint. They require a producer that opts into that protocol. `task_update` and `task_update_dropped` diagnostics record the task ID, sequence, kind, and rejection reason without logging spoken content.
