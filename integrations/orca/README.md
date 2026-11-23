# Test the isolated Orca task speech patch

This artifact adds the explicit producer and desktop registration handshake to Orca 1.4.268. It depends on the existing desktop routing patch identified in `provenance.json`. It does not install Orca or change a running service.

## Prepare source

From this Tohru worktree, supply the unpacked upstream source and the existing routing patch:

```sh
python3 integrations/orca/prepare.py \
  --upstream /path/to/orca-1.4.268-source \
  --routing-patch /path/to/desktop-bot-chat.patch \
  --output .tmp/orca-progress/review
```

The output must be a fresh directory inside this worktree's `.tmp`. The script verifies the upstream package manifest, the routing patch, and each modified base file. A mismatch requires reviewing and rebasing the artifact. The inputs remain unchanged.

## Install isolated test dependencies

Use the source's pinned package manager, Node.js, Python 3, Git, and Lua 5.4. Prepare the source's dependencies in the isolated output directory:

```sh
cd .tmp/orca-progress/review
pnpm install --frozen-lockfile --ignore-scripts
pnpm run build:packages
node node_modules/vitest/vitest.mjs run --config config/vitest.config.ts \
  src/cli/orchestration/identity/orchestration-compatibility-envelope.test.ts \
  src/main/bots/desktop-voice-task-queue.test.ts \
  src/main/bots/desktop-voice-continuations.test.ts \
  src/main/runtime/rpc/orchestration/callers/orchestration-session-voice-caller.test.ts \
  --maxWorkers=2
```

These tests need no Electron window, microphone, model, or GPU. An existing Nix dependency store can be used instead of downloading the locked packages. Restore its writable permissions and SQLite index using the Nix package's dependency preparation workflow.

## Exercise producer through consumer

Return to the Tohru worktree root and run:

```sh
python3 integrations/orca/verify-transport.py \
  --orca-source .tmp/orca-progress/review \
  --lua lua5.4
```

The test bundles the patched Orca CLI evidence envelope, server caller-admission selection, CLI handlers, RPC handlers, origin correlation, queue, and process delivery. Host session-record lookup remains an adapter; the fixture never inserts a caller unless the actual envelope and admission code request resolution. It invokes the real Tohru registration helper and `tohru assistant voice-task` endpoint, then drives the production voice controller. Fake host/journal/database and desktop IPC adapters isolate the test. A fake speech adapter records text instead of playing audio.

The test verifies acknowledgement before completion, both sides of the registration handshake, progress, worker-origin correlation, analyzed completion, suppression of the legacy spoken final, replay rejection, caller and remote rejection, raw-mail rejection, supersession, hiding, and text-origin exclusion. The trace and producer log are saved under `.tmp/orca-progress/integration/`.

## Review producer behavior

The patched bot instructions tell the coordinator to acknowledge a slow voice task before executing it, run long work asynchronously, and publish only confirmed milestones. The explicit command is:

```sh
orca bot voice-task --task workspace --sequence 1 --kind ack \
  --text "I'll create that workspace now." --json
```

Retain the returned `result.request`. Later updates require that request ID, the same task ID, and an increasing sequence:

```sh
orca bot voice-task --task workspace --request REQUEST --sequence 2 --kind progress \
  --text "The workspace exists. The agent is starting." --json
```

Use `done` only after verifying the result, `failed` for confirmed failure, and `cancelled` to retire task speech. The endpoint does not cancel underlying work. Updates are explicit bot actions. Tool commentary and raw worker mail cannot enter through this command.

The desktop calls `orca bot voice-ready --chat CHAT --request REQUEST` after the production voice controller registers `start`. Until that callback, the producer retains bounded pending updates. The producer supports 64 requests and eight task IDs per request. It coalesces pending progress, retains sequence tombstones, expires requests after ten minutes, and expires queued delivery after 30 seconds. IPC runs through Orca's process wrapper with a five-second timeout and bounded output. Ambiguous failures are consumed rather than automatically replayed.

Both RPC methods participate in Orca's injected caller resolution. A producer must be the coordinating bot's local session. Readiness requires the local owner socket without an agent caller. Every update resolves the exact voice origin using the existing routing provenance. New owner turns retire old task speech eligibility. A task-mode request suppresses the legacy spoken-final route while keeping shared history intact.

## Limits of this verification

The integration test uses fake task and speech adapters. It does not prove audible playback, an actual voice-created worktree, or an LLM's compliance with the bot instructions. No installed application or live desktop service was activated. Existing final-only behavior remains the fallback when either side lacks the new handshake.

The source `check:changed` gate passes all 30 checks. The patch removes the obsolete `worker-auto-release.test.ts` entry from the test-runtime list because that file is absent from the pinned upstream source. No exception is added to the check.

## Shared Home owner and background admission corrections

`desktop-voice-owner-admission.patch` applies after the deployed task-speech and host-native bot patches. Its exact base-file hashes are in `owner-admission-provenance.json`. Apply it only to an isolated current integration source tree:

```sh
python3 integrations/orca/apply-owner-admission.py --source .tmp/current-orca-source
python3 integrations/orca/verify-owner-admission.py --source .tmp/current-orca-source
```

The tree needs its pinned dependencies, built workspace packages and the Nix-generated settings modules. The verifier performs real SQLite journal operations, native session admission and independent provider queues through the RPC dispatcher; provider transports and physical audio are fixtures. A real child process records desktop IPC. Tests hold native readiness unresolved, continue Home input, replay durable acceptance, cancel before task delivery, consume a queued draft into its canonical ID, reject invalid task speech, distinguish internal mail, and fence a cleared context. This is stronger boundary coverage than the earlier adapter-only transport fixture; neither establishes physical hearing.

The new optional `worker-start --background` returns a persisted `accepted` receipt, distinct from readiness. Existing calls retain their wait-for-ready behavior. Launch/failure cleanup remains runtime-owned after the RPC returns; shutdown drains it, and a restart exposes the same durable Dispatch rather than relaunching blindly. Automatic acceptance/readiness speech uses that dispatch’s request correlation and the existing visibility/interruption arbiter. Shared work status is read through the existing bot items and worker-show APIs in the one Home conversation; no transcript copy transfers approvals.

With `--ids`, native history now carries typed owner/internal/answer authority and the original queued request alias. Only the final assistant entry of an explicitly successful turn carries its completion identity, so an unrelated later mail turn cannot hide that completed answer. The helper registers desktop speech only after this real admission is observed. Home is the default public route; historical experimental foreground chats remain intact for reference.

The follow-up `desktop-voice-background-protocol.patch` makes the native bot instructions explicitly choose nonblocking admission and uses the pinned Node executable for IPC recorders inside a Nix build sandbox. Apply it after the owner correction with `apply-owner-admission.py --stage background --source .tmp/current-orca-source`, then rerun the same verifier. A failed full Nix build exposed the original recorder’s unavailable `/usr/bin/env` shebang; no delivery success is inferred from that failed test.

`desktop-voice-current-interest.patch` applies next (`--stage interest`). It preserves successful CLI admission as `ok:true, state:accepted`, and attributes an answer to actual owner input even when control mail opened that provider turn. A later status question can deliberately follow its verified active dispatch: Home reads `worker-show`, then publishes one truthful `bot voice-task --task dispatch:<id> --sequence 1 --kind ack` under the current request and ends with `NO_REPLY`. Admission verifies that dispatch belongs to this Home and is still active; replay and a changed owner revision cannot rebind it. Its later analyzed completion then uses the current task speech arbiter once. Ordinary status reads and unrelated later owner turns do not revive old audio.

The final `desktop-voice-completion-observability.patch` (`--stage completion`) records bounded completion identity, eligibility reasons and pending/dispatched/replay outcomes without speech text. Its native regression uses the runtime's actual `answerEnded` subscription instead of invoking speech delivery manually after a journal flush. Diagnostics do not retry delivery or authorize a stale result.

## Independently supervised host reload

`restart-supervisor.py` separates the installed CLI launcher from the GUI executable and queues a controller plus a separate GUI systemd user unit. It never stops the persistent PTY daemon. It verifies the prepared PID/runtime/process birth, preserves the desktop/profile, supplies an explicit essential-tool PATH, and records shutdown, launch exit, ownership and readiness separately. A ready process from manual recovery does not count as an automatic launch. A failed launch gets at most one retry; an active launch with uncertain readiness is retained for inspection.

Prepare and rehearse without stopping the running app:

```sh
python3 integrations/orca/restart-supervisor.py prepare \
  --cli /etc/profiles/per-user/jesse/bin/orca-ide --output .tmp/orca-reload-plan
python3 integrations/orca/restart-supervisor.py rehearse --plan .tmp/orca-reload-plan/plan.json
python3 integrations/orca/test-restart-supervisor.py
```

The read-only rehearsal reproduces the historical missing-`dirname` failure under an insufficient PATH, then checks the installed CLI with the corrected environment and confirms the current app is unchanged. It does not claim a live GUI relaunch. GUI processes may scrub their environment, so preparation uses the user manager’s desktop environment and uid identity when original variables are absent, preserving explicit process values and profile arguments.

GUI units explicitly unset unsafe Node, library and agent handoff variables at the systemd service boundary, including values inherited from the user manager. External launch and unit queries have bounded waits. An uncertain launch reply retains the original unit; only confirmed termination permits a second attempt.

`queue --plan ...` prints the independent systemd command. Only an explicitly coordinated `queue --plan ... --execute` submits it, and the controller rechecks the exact rehearsed plan before any signal. Keep the prepared private directory and its logs through recovery. Do not queue another reload to diagnose an already failed one.
