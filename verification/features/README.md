# Tohru verification

Two drivers back the project `verify` skill. `tools/verify/verify.py` is the
lifecycle runner: `session-units` invokes the actual shipped CLI and verifies
persisted private files plus negative cases; `native-authority` is the distinct
native engineering suite. Other runner scenarios return blocked (3) without
effects. `tools/verify/desktop.py` drives the bar's public CLI on an owned
headless display; see [Desktop driver](#desktop-driver). A native pass proves the pinned test fixture's real CLI envelope,
schema, queue, RPC, SQLite journal and child IPC boundaries. Provider transports
and audio are scripted. It does not prove live Home, desktop UX or hearing.

| Map | Entrypoints | Current proof scope |
| --- | --- | --- |
| [Home conversation](home-conversation.md) | Desktop voice, typed popup, Orca GUI continuation | Unverified live journey |
| [Background work](background-work.md) | Native background admission, Home status, completion, visibility | Native fixture supported; actual Home unverified |
| [Capture and playback](voice-capture-playback.md) | Python/Rust replay, capture, player, retirement | Existing drivers; unified model/host journey unverified |
| [Screen captures](screens.md) | Chat popup open/close, runner crash, bar exit | Desktop driver `--feature screens`; summoned dragon and lock unverified |
| [Panels and calendar](panels.md) | Calendar CLI, clock click, popups, sound side effects | Calendar CLI open/toggle/close supported by the desktop driver |
| [Settings](settings.md) | `tohru config`, Settings window, managed overlays | CLI edit, TOML persistence and reload supported by the desktop driver |
| [Launcher](launcher.md) | Launcher window and search | Existing native harness; not driven here |
| [Notifications](notifications.md) | Notification daemon and popups | Existing private-bus harness; not driven here |
| [Desktop controls](desktop-controls.md) | Settings window, launcher, sound, session actions | Existing runners; unified journey unverified |
| [Install and recovery](install-recovery.md) | Shipped session-units CLI, packaged CLI, supervisor, GUI startup | Private CLI file effects supported; package/live recovery unverified |
| [Animation and motion](../motion/rejections.md) | Generated frames, sheets, native desktop captures | `tools/verify/motion.py` per-frame checks, approved side by side, separate reviewer; appeal stays the owner's |

## Desktop driver

`tools/verify/desktop.py run --feature calendar|settings|screens --runtime PLEAMAR` owns
launch, instance doctor, drive, capture and cleanup in one foreground process.
Run it inside `gpu-run --vram 4GiB` (or directly when `GPU_LEASE_ID` is set); it
refuses to drive without a lease. Pass the patched Pleamar 0.7.27 executable
explicitly, never the first store match or stock Pleamar.
`tools/verify/desktop.py doctor --runtime PLEAMAR` is the read-only prerequisite
check (tools, capability marker, hashes); it does not claim an instance is healthy.

Each run gets fresh HOME/XDG state with voice, Discord, polkit and corners off, a
private session bus, no system bus or audio server, an owned headless Sway with
the pixman renderer and private Pleamar IPC. Readiness requires parseable
`tohru status --json`. Never send commands to a pre-existing host session.
Evidence stays in `.tmp/verify-UNIQUE/`: `launch.json`, `commands.jsonl`, logs,
`doctor.json`, before/open/after screenshots with scene descriptions,
`cleanup.json` and `summary.json` with post-cleanup artifact hashes. Pair a
screenshot with its scene fact and the action transcript; neither alone is proof.
Software rendering cannot establish hardware rendering, performance or parity.

`tools/verify/runtime-source.py --runtime PLEAMAR` compares a Nix-built runtime's
declared source, patches and lock against this checkout. Require `result: passed`
before claiming they match; exit 2 is a provenance gap, not a reason to rebuild.

This driver lacks the runner's validated completion pointer and stale-run
rejection; use the exact directory it prints. Service logs stop at 4 MiB and
recorded command output at 64 KiB per stream. Cleanup signals each owned
process group even after its leader exits and fails the run if a member remains.

## Runner lifecycle: Launch, Doctor, Drive, Evidence, Cleanup

Preflight is read-only; it never imports a model, downloads dependencies, connects
to desktop sockets, or grants caller authority. A supplied source must resolve
inside this checkout's `.tmp`. `--target host` always blocks. No effects or caller
override flag exists. Tool presence is checked before running the native suite.
Namespace, boot, commit, dirty state, harness, source/lock and provenance hashes
are recorded. The source tree fingerprint covers src, packages and config.
Tool requirements and command-scoped environment guidance are in
tools/verify/environment.json. Runtime versions beyond pinned source/package
identity and exact executable paths are not yet probed; minimal-PATH package
and host socket/authority probes remain blocked with their later scenarios.
The final reviewed patch must reverse-apply cleanly. Installed and running
packages are explicitly unverified; their identity cannot be inferred from source.

Use the native map's exact preparation and launch commands. `run` composes doctor,
launch, drive, cleanup and collect. For inspecting each phase use:

```sh
python3 tools/verify/verify.py doctor --scenario native-authority --target sandbox --orca-source .tmp/native/source --json
python3 tools/verify/verify.py launch native-authority --target sandbox --orca-source .tmp/native/source
python3 tools/verify/verify.py drive --run /absolute/checkout/.tmp/verify/native-authority/RUN
python3 tools/verify/verify.py cleanup --run /absolute/checkout/.tmp/verify/native-authority/RUN
python3 tools/verify/verify.py collect --run /absolute/checkout/.tmp/verify/native-authority/RUN
python3 tools/verify/verify.py validate --run /absolute/checkout/.tmp/verify/native-authority/RUN
```

Replace RUN with the exact returned UUID; do not infer completion from launch.
The native driver uses an allowlisted environment with private HOME/XDG/TMPDIR,
no caller, credentials, host bus/display/audio sockets or inherited agent context.
It runs the fifteen-file admission and responsive Home suite and two typechecks with two native
workers. Every subprocess has its own session, a 180-second deadline per command
(override 0–1800 seconds), and a streaming 2 MiB combined output budget. Its entire
owned group is killed on completion/overflow/timeout, and its direct child reaped.
The runner does not acquire GPU leases for these CPU-only tests and disables
persistent sccache startup. It does not stop shared caches or leases.
Run locks prevent concurrent lifecycle changes; a source lock prevents two runs
from sharing writable native indexes/caches. Separate prepared trees can run apart.
An operation that cannot acquire the run lock returns incomplete without writing
recovery evidence or changing the owner's state. Exception recovery reacquires
the run lock before any state write or cleanup.

Cleanup is repeatable after a completed drive, preserves private files and all
attempts, and touches no host settings. An interrupted driver without durable
reaping proof returns cleanup_failed, never guesses PID ownership or reports
restoration. Abrupt-controller-death recovery, descendant reparenting/reaping,
and lease-held sccache lifetime proof remain prerequisites for future model/host
drivers. Do not use this runner for those workloads.

## Evidence contract

Each run owns `.tmp/verify/SCENARIO/UUID/` (0700). Collection snapshots live at
`collections/COLLECTION-UUID/`; every required artifact and bounded log is hashed,
then `collection-complete.json` is atomically written and validated, then
`latest-completed.json` is atomically replaced. Consumers validate the immutable
collection, including matching run/scenario/action bounds/cursor and all hashes.
A stale marker, missing artifact, partial collection, traversal or mismatch fails
with incomplete (4). If publication crashes after the marker but before the
pointer, the prior completed pointer remains valid; `collect` publishes a fresh
snapshot without changing the old one. Completed collections are never rewritten
by helpers. File hashes detect alteration, not malicious forgery or external trust.

| Artifact | Contract |
| --- | --- |
| manifest.json | Scenario/run, target, source/lock/harness/provenance hashes, exact argv, environment key names, namespace/boot/UID, explicit caller and effects limitations |
| actions.jsonl | One action per line: sequence, entrypoint, argv, wall end, monotonic duration, deadline, exit/reason and reaping receipt |
| observations.jsonl | Bounded native test log references, scripted adapter labels; no invented request/player observations |
| assertions.json | Required native boundaries, number of commands, actual scenario outcome, omitted boundaries |
| before.json / after.json | Selected source identity, no-host-effects declaration, owned process cleanup result; no invented host snapshots |
| resources.json | Owned CPU child receipts and cleanup phase; no GPU lease |
| timings.json | Wall action bounds and raw monotonic command durations; no latency or speedup claim |
| result.json | Outcome, proof scope, remaining gates and absolute run path |
| collection-complete.json | Schema v1 marker, exact run/collection/action bounds, cursor, all hashes, collector exit; see tools/verify/evidence.schema.json |

Exit codes: passed 0, behavior failed 1, invalid invocation 2, prerequisite blocked
3, incomplete evidence 4, cleanup failed 5, required external gate unverified 6.
Cleanup failure takes precedence and never makes a behavior failure disappear.
Logs may contain native fixture data; run directories are private. Do not supply
production transcripts/credentials or capture physical microphones by default.

## Maintenance and proof status

Run `python3 -m unittest discover -s tests/verification -v`. This checks wrapper/map
drift, actual subprocess overflow/deadline handling, immutable collection failure
cases and cleanup idempotency. Those tests are CPU evidence, separate from N.
Changed source, patches, lock, runner, caller schema, model, package or instance
invalidates affected prior proof. Preserve failed runs and distinguish reused
historical reports from fresh observations. Native integration artifacts belong
to the Orca integration maintainer; the runner does not change authority to pass.
Client discovery is verified structurally through both standard project paths;
interactive Codex/Claude discovery and full feature coverage remain unverified.

Later phases require actual Home/provider authority and private instance endpoints,
independent typed-retirement expiry and conflict-safe restoration, exact package
pins, scheduler-owned real model runs, supervisor lifetime proof and owner listening.
No mock, source test, submitted PCM or null monitor implies host UX acceptance.
