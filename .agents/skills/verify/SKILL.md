---
name: verify
description: Verify Tohru's Wayland desktop shell through its public CLI on an owned headless display, or check standalone service staging without activating services. Use after shell, settings or CLI changes; consult the feature map for unsupported desktop entry points.
---

# Verify Tohru

Tohru is a Pleamar desktop shell, not a web app. Its CLI reports request
acceptance; a zero exit is insufficient evidence of completion. The primary
surface is the bar and its panels. Secondary surfaces include Settings, launcher,
notifications, session/lock/greeter, voice and the standalone setup CLI.

Read [features/README.md](features/README.md) before selecting a journey. This seed
covers five areas; it does not certify the whole desktop. The executable helper is
`./.agents/skills/verify/scripts/drive.py` (Python 3.11+). Run from the repository
root. The helper resolves its checkout from its own path, never another worktree.

## Launch

Prerequisites: Python 3.11+, Bash, the checkout's `bin/`, `dbus-daemon`, `sway`,
`swaymsg`, `grim`, system fonts, a working Vulkan loader/driver and the **patched
Pleamar 0.7.27** runtime. Supply the runtime explicitly; do not auto-select the first
store match or use stock Pleamar. For a fresh build use the source instructions in
`runtime/README.md` (`prepare.sh`, dev environment, locked Cargo build). Building
is a separate prerequisite; this skill neither downloads models nor installs tools.

Set `verify_runtime` to the absolute built `pleamar` executable. This environment's
tested immutable artifact was
`/nix/store/i956in9f7akaaj040z291dndgwakx7m6-pleamar-tohru-0.7.27/bin/pleamar`;
its presence elsewhere is not guaranteed. Record the runtime's identity separately
from the checkout HEAD. The capability marker alone does not prove source identity.

```sh
verify_runtime=/nix/store/i956in9f7akaaj040z291dndgwakx7m6-pleamar-tohru-0.7.27/bin/pleamar
./.agents/skills/verify/scripts/drive.py doctor --runtime "$verify_runtime"
gpu-run --vram 4GiB --name 'Tohru isolated calendar verification' -- \
  ./.agents/skills/verify/scripts/drive.py run --feature calendar --runtime "$verify_runtime"
```

`run` owns launch → instance doctor → drive → capture → cleanup in one foreground
process. It prints the absolute evidence directory immediately and a final JSON
result. Keep the reservation through cleanup. If `GPU_LEASE_ID` is already set,
invoke the driver directly without another reservation. A small 1280×720 bar run
budgets 4 GiB; larger native workloads need their own conservative budget. Missing
scheduler/driver/runtime is a blocker, never a reason to bypass admission.

Every run creates an empty disposable HOME and XDG directories, a private session
bus, an unavailable private system bus/audio endpoint, and private Pleamar IPC.
Native runs start an owned headless Sway with the pixman compositor renderer.
The bar uses the real checkout scene and runtime, without edited scenes or
mock IPC. Verification config explicitly disables voice, Discord, polkit and
corners. No credentials or models are copied. This setup differs from the host:
no system services, physical audio/input, host display, wallpaper owner or real
application catalog. Software rendering is allowed here and is recorded in
`bar.log`; it cannot establish hardware rendering, performance or visual parity.

Usable readiness requires parseable `tohru status --json`, not merely a socket or
exit code. The first attempt exposed Pleamar's exit-zero “the render does not
answer” response during startup; the helper waits through that response.

For the short-lived standalone CLI path there is no server/build step:

```sh
./.agents/skills/verify/scripts/drive.py run --feature session-units
```

## Doctor

The standalone `doctor --runtime` above is a read-only prerequisite check: required
tools, executable runtime, capability schema/version and SHA-256. It does not
claim an instance is healthy. Each native `run` additionally calls its **instance
doctor** after launch, checking its owned processes, private runtime/IPC routing,
active `HEADLESS-1`, parseable status and live clock. It records these observations
in `doctor.json`. If startup fails, inspect `failure.txt`, `commands.jsonl`,
`bar.log` and `sway.log`; rerun in a new run directory after fixing the cause.

The public `tohru doctor --json` diagnoses a complete installed desktop, including
systemd/logind/system services deliberately absent here. Do not replace the private
instance doctor with a successful host probe, or call that public doctor healthy
when required checks fail. No services are activated to improve its result.

For a Nix-built runtime, the optional read-only provenance helper compares its
declared source, patches, Cargo lock and upstream archive against this checkout:

```sh
./.agents/skills/verify/scripts/runtime-source.py --runtime "$verify_runtime" \
  > .tmp/runtime-source.json
```

Require exit zero and `result: passed` before claiming declared runtime inputs
match this checkout. Both file sets are checked using the derivation's selectors:
Rust files under `runtime/src`, all files under tests/bindings, and explicit
manifest/license/role-test inputs. A removed checkout file retained in the build
inputs is a mismatch. Exit 1 (`failed`) identifies differing files or pins and
recommends selecting/rebuilding a matching runtime; exit 2 (`unavailable`) names
missing tools, paths or metadata and how to restore the check. This needs `nix`,
`nix-store` and surviving derivation/input
paths. Missing metadata is a provenance gap, not a reason to rebuild/install
without admission. The helper does not perform an independent rebuild. The
native preflight records both a wrapper hash and its `.pleamar-wrapped` payload
hash when present. A different hash or capability marker needs investigation.

## Drive

The runnable journeys are:

```sh
# Calendar's public open/toggle/close commands and observed panel state:
gpu-run --vram 4GiB --name 'Tohru calendar' -- \
  ./.agents/skills/verify/scripts/drive.py run --feature calendar --runtime "$verify_runtime"
# Public settings mutation, persisted TOML and independent status observation:
gpu-run --vram 4GiB --name 'Tohru settings' -- \
  ./.agents/skills/verify/scripts/drive.py run --feature settings --runtime "$verify_runtime"
# Actual generated unit files and refusal to overwrite, with no activation:
./.agents/skills/verify/scripts/drive.py run --feature session-units
```

`commands.jsonl` records exact public commands, timestamps, stdout, stderr, exits
and read-only runtime observations. `get` and `describe json` are observers, never
mutation substitutes. The driver does not use native setters, synthetic voice
callers, PCM submission or test-only endpoints. It intentionally offers no arbitrary
CLI forwarding that could reach power actions or start unowned processes.

Pointer, keyboard, launcher and notifications journeys are mapped but **not covered
by this helper**. Their files name the required existing harness and missing gates.
Do not report those entry points passed because the calendar CLI passed.

## Evidence

Each invocation preserves `.tmp/verify-<unique>/` with `launch.json` (source HEAD,
dirty paths, runtime marker/hash, start time), `commands.jsonl`, logs, feature
snapshots/screenshots or generated files, `cleanup.json`, and `summary.json` with
end time, outcome and artifact SHA-256. Failures preserve `failure.txt` and all
available evidence. The helper verifies artifacts still exist by hashing them
**after cleanup**. A successful staging run is evidence of file generation only.

For native proof inspect the before/open/after screenshots and scene descriptions
alongside the action transcript and observed facts. A screenshot alone does not
prove the action. A scene fact alone does not prove visible pixels. Settings proof
must include the actual config file and parsed public status; successful request
acceptance alone is inadequate. Do not claim persistence across process restarts
from config reload alone. For new work record the exact committed HEAD used by a
clean rerun; preserve earlier dirty runs and failed iterations too.

When presenting multiple images follow the workspace media-gallery instructions.
Evidence paths are absolute shared host paths. Internal reports belong in
`.tmp/docs/`, never the distributable `docs/` directory.

## Cleanup

The helper's `finally` block terminates and reaps only its own Popen process groups
in reverse order, then removes its disposable state. It does not kill by name or
restart the host bar/compositor/audio. `cleanup.json` names owned PIDs, exit codes
and the removed scratch directory. Any cleanup failure makes the run fail and
leaves scratch for inspection. Verify `summary.json.result`, cleanup failures and
surviving artifacts before claiming success. On interruption, let the foreground
driver finish cleanup; never recover by killing every `pleamar`/`sway` process.

Use `/maintain-verification-skill` to update this map as entry points and contracts
change. Broader verification must preserve isolation and report unavailable
hardware, credentials, source or authority as gaps. No formal companion skill is
justified by this initial desktop driver; it would not prove rendered UX.
