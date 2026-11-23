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
