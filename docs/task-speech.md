# Conversational task updates

Tohru accepts explicit task acknowledgements, progress, and verified completion separately from ordinary assistant replies. An acknowledgement can speak while the task runs. It releases the desktop reply poller so you can ask another question. The task continues in Orca and its messages remain in the shared conversation history.

The [isolated Orca producer patch](../integrations/orca/README.md) implements the task-update protocol and desktop registration handshake. This checkout does not install or activate that patch. The existing final-result route continues to work without it. Tohru does not interpret tool commentary or raw worker messages as task updates.

## Foreground conversation

Orca voice uses `assistant.orca_voice_chat = "foreground"` by default. This selects a separate, persistent bot chat so Home Chat's long tool calls do not occupy the voice provider queue. The microphone, speech scheduler, and background workers keep their existing owners. No additional local model is loaded.

Home Chat and its full history remain intact. Each foreground request includes Home's ID, current working state, and up to eight recent text excerpts of 500 characters each. These excerpts are reference material. They can include unfinished commentary and do not authorize actions or establish completion. The foreground bot can read Home and task items for more context. It must check existing task ownership and approvals before starting work. Raw worker mail and tool output are excluded from the excerpts.

The foreground bot receives instructions to delegate long work to asynchronous Orca workers, acknowledge accepted tasks, and publish verified progress through the task endpoint. Deliberate conversational updates use this endpoint too. Ordinary commentary stays text-only. Each worker belongs to the foreground request that launched it. A newer owner turn retires older speech eligibility, while task results remain in that chat.

This separation provides independent provider queues. It does not enforce a tool allowlist: a long tool call executed directly by the foreground bot still blocks that actor. Delegation compliance and audible delivery need live verification.

The desktop popup can open the foreground conversation from History. Typed replies in that conversation share its context. A new typed Home conversation stays in Home. Tohru does not copy every message between chats or transfer existing approvals to another task.

The helper retains the foreground chat identity in its private scratch directory, keyed by bot and Home IDs. Worker mail, another wake, or a helper restart does not select a different chat. A missing Home, unreadable handover, invalid identity, or closed foreground chat fails without sending a new request. Restore a closed foreground chat to resume it.

Existing explicit `home` and `new` settings remain authoritative. Switching to `foreground` retires desktop speech for the old route, but leaves Home and its tasks intact. Switching back to `home` restores Home routing; the foreground history remains available. Managed settings take precedence over user settings.

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

A new owner turn, active hearing, interruption, hiding, conversation end, backend change, or voice shutdown retires task speech eligibility. Hiding or interruption cancels queued and active task speech. A later update cannot reopen Tohru. A popup displaying another conversation also blocks speech. Paused voice and Do Not Disturb suppress task speech. Background task synthesis releases foreground thinking before audio is ready, so a new question is not joined onto the accepted task request.

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
