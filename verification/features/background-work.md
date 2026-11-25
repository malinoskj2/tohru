# Background work

## Sub-features

`native-authority` (N) is the first supported journey. Prerequisites: pinned
Orca 1.4.268 with all reviewed integration patches, offline dependencies, generated
Nix modules and built packages. `job-conversation` and `task-floor` are unverified
H journeys. Shared job state survives cancellation of speech; accepted is not ready.

## How to get to it (user POV)

Ask Home for harmless long work, receive accepted before readiness, then continue
an unrelated direct conversation. Ask exact-job status while it is live; after
renewed interest, receive one analyzed completion. Hide/reveal and barge-in must
not replay a hidden/stale result. This live journey requires the authentic local
coordinating Home caller; a dispatched worker cannot supply that identity.

## Driving it with the shared runner

The usable isolated subset runs existing production admission tests rather than
submitting work to the owner's actual Home. See integrations/orca/README.md for
manual preparation. For an already available Nix source/dependency store:

```sh
nix derivation show /nix/store/SELECTED-orca-ade-1.4.268 > .tmp/orca-derivation.json
python3 tools/verify/prepare-native.py --derivation-json .tmp/orca-derivation.json --output .tmp/native --pnpm /nix/store/SELECTED-pnpm/bin/pnpm --zstd /nix/store/SELECTED-zstd/bin/zstd
python3 tools/verify/verify.py doctor --scenario native-authority --target sandbox --orca-source .tmp/native/source --json
python3 tools/verify/verify.py run native-authority --target sandbox --orca-source .tmp/native/source
```

Select existing immutable paths, never download or substitute an upstream version.
Preparation verifies the reviewed Tohru patches against the selected derivation,
copies the immutable upstream source and applies its patch stack privately,
extracts literal generated Nix modules, restores a writable offline dependency
store/SQLite index, installs with frozen lock and builds packages. Other Nix GUI
postPatch hooks are deliberately omitted and recorded. The preparation receipt
preserves exact inputs, hashes, commands and failed attempts. A new attempt needs
a fresh output path; no shared store is modified. Preparation alone is no pass.

The native suite drives actual CLI argument/envelope handling, background RPC,
native SQLite history, queued aliases and child IPC. With the quick-voice and automatic
steering patches applied, expect fourteen passing test
files and both typechecks, not merely exit0 from an accepted CLI. Its native
test action/log receipts and immutable collection supply the observable result.

Required negatives in the existing native suite: delayed readiness, cancellation,
durable acceptance replay, queued remint/canonical aliases, internal-mail owner
isolation, stale owner, invalid speech, context retirement, terminal/revision races
and foreign/remote caller rejection. See integrations/orca/verify-owner-admission.py
for the exact maintained file list; transport-fixture.mjs uses additional fake
host/database/journal adapters and is weaker, separately labeled evidence.

## Gotchas

Native fixtures use scripted providers and audio. They never impersonate a real
Home caller or establish real Home conversation. The H gate needs a real wait
(75 seconds), exact accepted dispatch/run retained, actual provider turns,
one `bin/tohru-voice-status` call for each status, current interest, one terminal
IPC, source/runtime IDs, typed retirement and completed restoration observation.
Home ends explicit acknowledged status with `NO_REPLY`; a worker-show read alone
does not renew speech. Missing caller/source is blocked, never worked around by
setting ORCA_AGENT_SESSION_ID or copying a voice request. H remains unimplemented.
