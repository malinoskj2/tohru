# Conversational task updates

Tohru accepts explicit task acknowledgements, progress, and verified completion separately from ordinary assistant replies. An acknowledgement can speak while the task runs. It releases the desktop reply poller so you can ask another question. The task continues in Orca and its messages remain in the shared conversation history.

The [isolated Orca producer patch](../integrations/orca/README.md) implements the task-update protocol and desktop registration handshake. This checkout does not install or activate that patch. The existing final-result route continues to work without it. Tohru does not interpret tool commentary or raw worker messages as task updates.

## Waiting conversation controls

`assistant.waiting_chatter` defaults to `false`. The **Conversation while waiting** switch in Settings enables it. The same setting is available through `tohru config set assistant.waiting_chatter true` or `false`.

`assistant.waiting_seed` accepts integers between 0 and 2147483646 and defaults to 1. A seed of 0 uses the same sequence as 1. The sampler uses a local Park-Miller PRNG. It does not call a model or a random-number service.

Waiting conversation uses four short questions or thoughts. Silence is a fifth outcome. Each request allows at most two remarks, with at least 45 seconds between speech starts. Candidate times are 45 to 75 seconds apart. Consecutive remarks cannot repeat the same topic. Waiting conversation stops after ten minutes, even if the task has not finished. Disabling the setting cancels a waiting remark that is playing or queued.

## Task-update protocol

The host-local endpoint is:

```text
tohru assistant voice-task --chat CHAT --request REQUEST --task TASK --sequence N --kind KIND -- TEXT
```

The producer supplies the current voice conversation and its stable Orca prompt item ID. Each task has a stable ID and strictly increasing sequence numbers. `TEXT` is a user-facing statement of at most 320 UTF-8 bytes without control characters. Shell arguments must remain separate arguments. A successful CLI exit means the update reached desktop IPC, not that the user heard it.

After the voice controller registers a request, it invokes the separate `tohru-assistant voice-ready --conversation CHAT --request REQUEST` helper. The helper calls the patched Orca readiness endpoint without inherited agent identity. This callback releases queued task acknowledgements. It does not depend on a timer or the task finishing.

| Kind | Meaning |
| --- | --- |
| `ack` | Intent to perform an accepted task. This must precede progress and completion. |
| `progress` | A confirmed intermediate observation, such as a created workspace or a blocker. |
| `done` | A verified successful result. |
| `failed` | A confirmed failure, with a useful next step when available. |
| `cancelled` | Retires pending speech for this task. The endpoint does not cancel the underlying task. |

The first accepted acknowledgement detaches the matching desktop poller. It does not send a cancellation to the Orca bot. Task completion then uses this task-update channel exclusively. The producer must suppress a second spoken final result for that request. The final analyzed answer still belongs in shared Orca history.

## Speech ordering and lifetime

The desktop task speech owner has one active utterance and at most eight queued utterances. It accepts at most eight task IDs per voice request, retaining terminal IDs until the request retires. Replayed, duplicate, and older sequence numbers cannot reopen a task.

Completion and failure take priority over acknowledgements, progress, and waiting remarks. They preempt active task speech. An acknowledgement precedes queued progress. New progress replaces older queued progress for the same task and preempts a waiting remark. Queued completion expires after 30 seconds. Other queued speech expires after 15 seconds. Every active task utterance has a 30-second deadline.

A new owner turn, active hearing, interruption, hiding, conversation end, backend change, or voice shutdown retires task speech eligibility. Hiding or interruption cancels queued and active task speech. A later update cannot reopen Tohru. A popup displaying another conversation also blocks speech. Paused voice and Do Not Disturb suppress task speech.

Retiring speech does not transfer or cancel task ownership in Orca. Results after retirement remain text in the shared history. This deliberately prevents an old task from speaking over a new conversation.

## Verification

These commands exercise production scheduling and desktop adapters with fake clocks, task events, and speech effects:

```sh
lua5.4 tests/assistant/task-speech.lua
lua5.4 tests/assistant/controller.lua
lua5.4 tests/assistant/voice-controller.lua
python3 -B tests/assistant/voice.py
python3 -B tests/cli.py
cargo test --locked --manifest-path runtime/Cargo.toml waiting_chatter
```

`lua5.4 tools/task-speech-replay.lua 12345` prints a deterministic synthetic task conversation. It opens no microphone, starts no service, and produces no audio. These tests do not establish actual audible playback or automatic Orca producer delivery.
