# Conversational task updates

Tohru accepts explicit task acknowledgements, progress, and verified completion separately from ordinary assistant replies. An acknowledgement can speak while the task runs. It releases the desktop reply poller so you can ask another question. The task continues in Orca and its messages remain in the shared conversation history.

The [isolated Orca producer patch](../integrations/orca/README.md) implements the task-update protocol and desktop registration handshake. This checkout does not install or activate that patch. The existing final-result route continues to work without it. Tohru does not interpret tool commentary or raw worker messages as task updates.

## Foreground conversation

Voice defaults to `assistant.orca_voice_chat = "home"`. Desktop voice, desktop text and Orca’s Home GUI use the same public conversation and history. Private workers execute long jobs; their histories do not become a second conversation the owner must navigate. Task items, dispatches and approval records remain authoritative in Orca.

On an Orca runtime supporting this integration, `orca orchestration worker-start --background ... --json` returns durable Task and Dispatch IDs with `state: "accepted"` before setup or readiness. Acceptance means the job was recorded, not that a worker is running. Inspect `worker-show` for current starting, ready, failed or interrupted state. Repeating the same mutation request ID replays its receipt rather than launching another worker. A cancellation during launch fences task delivery and reports cleanup separately from proven exit.

The runtime produces bounded, truthful acceptance and readiness updates independently of the Home model. Deliberate diagnostic/progress summaries still use the explicit task producer, and analyzed completion follows the verified dispatch state. Ordinary commentary and raw worker mail stay text-only. A newer owner turn retires stale speech eligibility while the job and its result remain in Home.

Home must finish its short conversational turn after delegating. A long tool executed directly in Home still occupies its provider; this design does not run two uncontrolled writers in one history or promise concurrent inference within a single provider turn.

The earlier `foreground` route remains an explicit compatibility option. It uses a separate saved chat with bounded Home excerpts and does not share the complete history. Switching back to `home` preserves that experimental chat and its registry for reference; no transcript is merged, erased or reset. `new` also remains an explicit option. Managed settings take precedence over local settings.

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

The first accepted acknowledgement detaches the matching desktop poller and releases foreground continuation even when speech is hidden or the owner is speaking. This state transition preserves an active capture. If a completed follow-up is already queued, it is submitted once after the old request is retired, and the old acknowledgement does not speak over it. An in-progress cancellation retains its own completion callback. It does not send a cancellation to the Orca bot. Task completion then uses this task-update channel exclusively. The producer must suppress a second spoken final result for that request. The final analyzed answer still belongs in shared Orca history.

## Speech ordering and lifetime

The desktop task speech owner has one active utterance and at most eight queued utterances. It accepts at most eight task IDs per voice request, retaining terminal IDs until the request retires. Replayed, duplicate, and older sequence numbers cannot reopen a task.

Completion and failure take priority over acknowledgements, progress, and waiting remarks. They preempt active task speech. An acknowledgement precedes queued progress. New progress replaces older queued progress for the same task and preempts a waiting remark. Queued completion expires after 30 seconds. Other queued speech expires after 15 seconds. Every active task utterance has a 30-second deadline.

Home's next owner turn, an explicit hush or dismissal, hiding, conversation end, backend change, or voice shutdown retires task speech eligibility and cancels queued and active task speech. A spoken turn the quick responder answers never reaches Home, so it only cancels queued updates and a late acknowledgement: the task's later progress and completion still speak once the floor is free. The owner taking the floor only stops the update that is playing: its origin and task records stay, so later progress and completion still speak. An update arriving while the owner speaks is recorded and not replayed. The voice service reports `hearing` once speech yields words, not on voice-activity onset alone, so a wordless blip neither takes the floor nor silences speech. During a pending barge-in, a backchannel keeps the floor with the speaker, which resumes. A later update cannot reopen Tohru. A popup displaying another conversation also blocks speech. Paused voice and Do Not Disturb suppress task speech. Background task synthesis releases foreground thinking before audio is ready, so a new question is not joined onto the accepted task request.

Retiring speech does not transfer or cancel task ownership in Orca. Results after retirement remain text in the shared history. This deliberately prevents an old task from speaking over a new conversation.

## Verification

These commands exercise production scheduling and desktop adapters with fake clocks, task events, and speech effects:

```sh
lua5.4 tests/assistant/task-speech.lua
lua5.4 tests/assistant/controller.lua
lua5.4 tests/assistant/voice-controller.lua
cargo test --manifest-path voice/Cargo.toml
python3 -B tests/cli.py
cargo test --locked --manifest-path runtime/Cargo.toml waiting_chatter
```

`lua5.4 tools/task-speech-replay.lua 12345` prints a deterministic synthetic task conversation. It opens no microphone, starts no service, and produces no audio. These tests do not establish actual audible playback or automatic Orca producer delivery.
