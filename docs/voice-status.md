# Report a background task's voice status

From the coordinating Home bot, run one tool invocation:

```sh
tohru-voice-status --dispatch ctx_DISPATCH --run run_RUN
```

Use the exact dispatch and run from the task's accepted receipt. The command uses the bot's existing injected caller identity and snapshots the latest actual desktop voice owner before reading worker status. It acknowledges only a confirmed ready, live, working worker, through the existing native task-speech producer. A new owner turn, foreign task or terminal race can reject that acknowledgement.

The returned `pending`, `dispatched` or `replay` delivery is a native admission result, not proof of hearing. Finish the Home turn with `NO_REPLY` after a successful explicit status update so the same status is not repeated in a final answer. A repeated call in the same owner request is deduplicated by the native task arbiter. A later relevant voice question can renew interest in that exact task; an unrelated owner turn retires it.

Unknown, starting, idle, stopped or completed work does not receive a running-status acknowledgement. On failure, inspect and answer from authoritative state; do not retry speech automatically or claim the task is running. Hidden desktop and owner interruption retain their normal audio policy. The underlying background job continues independently.

This command combines verification and acknowledgement in one tool slot. It does not make Home's own inference or synchronous tools concurrent, periodically chatter, change models or create another public conversation. Long work should use persisted `worker-start --background` admission and private executors. Acceptance is distinct from readiness; verified completion remains a separate, once-only analyzed result.

The command is shipped in the Tohru package and can also be invoked as `python3 /path/to/tohru/bin/tohru-voice-status` from a committed checkout. It requires the existing local coordinating-bot environment and supported Orca voice APIs; it never creates or overrides a caller or voice origin.
