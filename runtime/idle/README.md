# Native integrated idle

Built into the Nix runtime. `tests/native-idle.py` checks it on a private Sway
display with a fake logind: screen-off fade, native output power-off, pointer
wake, a connected ScreenSaver inhibitor, and logind BlockInhibited=idle. Lock and
suspend actions, the dedicated locker's sleep delay and real logind are not
covered there.
The existing `tohru-shell` library owns policy/cookies/delay. Pinned Pleamar owns
native transport/renderer, without a new crate/daemon/backend.

Shared-owner wiring after review:

- Copy native.rs to upstream `src/tohru_idle.rs`, and observer.rs/screensaver.rs/
  logind.rs/sleep_guard.rs to `src/tohru_idle/`.
- Declare `mod tohru_idle;` in native lib and call
  `crate::tohru_idle::install(&lua, self.tx.clone(), self.to_logic.clone())?`
  beside native settings install, inside immutable root-only TOHRU_SHELL gate.
- Apply `runtime/idle/0004-idle-native.patch` after reviewed0002.
  It only adds the native held transaction and callback, not auth behavior.
- Existing library fileset includes configuration.rs/cookies.rs/delay.rs/input.rs/
  logind_state.rs/teardown.rs plus mod.rs and all matching tests listed below.
- Ordinary root includes `session/idle.part.plm` inside its scene; its logic requires private
  `session/idle.luau` via require("session/idle") and calls returned `configure([idle] typed table)` on startup and
  successful config reload. Keys/defaults match coordinator's chosen schema.
  Native global `tohru_idle` APIs: start(config), held(bool), resume(),
  caffeine(bool), stop(); locker alone calls start_guard()/stop_guard().
- Root events idle_lock_state(number0/1), idle_resume, idle_caffeine_toggle;
  idle_alpha number, idle_caffeine boolean and idle_status text are actual
  bindings. Dedicated locker emits fixed native `say:tohru` hint on native
  held changes/state_request. Native dedicated role remains separately launched.

One process Owner persists across VM replacement; config validates before enqueue
and reconfigures existing Policy without replacing its generation owner. Identical
config is a no-op across VM reload (no resume epoch, timer reset or rearm); changed
config is cached only after successful submission, and failed submissions retry. Config queue8,
action queue4, cookie limits32/client256total, signal queues16/32. Calloop ping
coalesces latest observations with monotonic cancellation/rearm edges; pulse
transitions cancel queued tasks immediately and survive until one policy drain. No timer at rest;
16ms scalar publication only while visible fade runs, existing native renderer
paints full-output overlay. Input cancels fade/rearms generation. Native callbacks
use proxy+generation currentness, replaced waits are destroyed, output hotplug
owns/destroys native power handles. Protocol failures report unavailable; native
power request is not a claim that hardware switched off. Unknown locker state
inhibits automatic actions until dedicated owner answers; hints never authorize
native release or final sleep. In-flight already delivered native actions cannot
be revoked by a later cancellation, consistent with accepted spawn-only API.

ScreenSaver owns both standard paths without replacing/queueing behind another
owner; bus authenticates unique-client cookie ownership and disconnect cleanup.
No application/reason strings retained. Logind idle-block observer conservatively
inhibits when state is unknown; caffeine owns independent close-on-exec idle-block
FD. Authenticated DBus name-owner subscription precedes initial state
publication; missing/replaced logind stays unknown/inhibited until a fresh query
to its exact unique owner succeeds. Old-owner or superseded queries cannot
overwrite current state. Separate locker delay subscribes before acquiring sleep-delay FD, marks sleep
intent then asks actual held transaction. Native held callback and immediate
check both use engaged→guard lock order. Only those callbacks close pending delay
FD while confirmed cycle remains held; no GPU/Wayland dispatch/wait/call under
that guard. Resume reacquires delay and cancels stale sleep request; failed
reacquisition reports absence accurately. Guard process/thread closes bus and
joins on explicit stop; no unlock/power action on teardown. Ordinary shell stop
closes own services, restores requested screen power and joins observer/actions;
it does not own or stop locker guard.

Native adapters are parse-checked only at this checkpoint. Core30CPU tests,
Clippy and actual Lua/private scene parser checks cover their respective scopes;
no native observer/logind/fade/screen-power/PAM/security acceptance inferred.

## Exact recovery and copy inputs

The composable part was committed at
`3c8beb7705778c8d41547f76ef7acb7dc18f812e`, owner-loss module/export/test fixes at
`9da8c41891bc42d30a03a52ebcae752ce8de1bf1`, and private probe at
`f7d56dddbee4879adfcf98e6e1bc72c6354b401f`. They are all present in this branch's
current source tree; relaying their hashes did not integrate them into main.
The native guard request also retains the separate synchronization correction
`56cd60d145f56ce976cae18a6317cd63364104e7` (ExternalFact, not Fact), and the actual
root all-target test initializer correction
`406c819d6b0dcbeb50f3b306cba92c625a119555`.

S owns shared prepare/root/schema edits. The complete idle copy portion of its
future preparation recipe is below, after the existing foundation/native source
layout exists; these commands do not install or activate anything:

```sh
mkdir -p "$destination/foundation/runtime/idle" \
    "$destination/foundation/tests/session" "$destination/src/tohru_idle"
cp "$root/runtime/idle/mod.rs" "$root/runtime/idle/configuration.rs" \
    "$root/runtime/idle/cookies.rs" \
    "$root/runtime/idle/delay.rs" "$root/runtime/idle/input.rs" \
    "$root/runtime/idle/logind_state.rs" "$root/runtime/idle/teardown.rs" \
    "$destination/foundation/runtime/idle/"
cp "$root/tests/session/idle.rs" "$root/tests/session/idle_cookies.rs" \
    "$root/tests/session/idle_delay.rs" "$root/tests/session/idle_input.rs" \
    "$root/tests/session/idle_logind.rs" "$root/tests/session/idle_configuration.rs" \
    "$root/tests/session/idle_teardown.rs" "$destination/foundation/tests/session/"
cp "$root/runtime/idle/native.rs" "$destination/src/tohru_idle.rs"
cp "$root/runtime/idle/observer.rs" "$root/runtime/idle/screensaver.rs" \
    "$root/runtime/idle/logind.rs" "$root/runtime/idle/sleep_guard.rs" \
    "$destination/src/tohru_idle/"
```

The foundation library's existing `pub mod idle` must still resolve to the copied
mod.rs, which exports configuration/cookies/delay/input/**logind_state**/teardown. Its test path must
resolve idle_logind.rs from the copied tests/session directory. Native module
registration/install and the full-path `runtime/idle/0004-idle-native.patch`
remain separate shared wiring, as above; copying files alone does not compile or
connect those hooks. Include `session/idle.part.plm` **inside** the ordinary root
scene; do not import the standalone idle.plm. The standalone idle.plm and
idle_probe.plm both include the same owned part, and root logic requires
`session/idle` and calls its configure function. Native adapters remain uncompiled.


## Orderly shutdown and feature boundary

S must call `crate::tohru_idle::shutdown()` on orderly native teardown, under the
same Luau feature cfg as the module declaration, before returning/exiting the
process and while its native output connection can restore requested power.
This Rust hook is terminal and idempotent, independently of Lua VM lifetime.
It stops only process-local registered resources; a bar without a guard cannot
stop a separate locker process. New ordinary/guard starts are rejected once
shutdown begins, and concurrent repeated shutdown calls serialize.

Shutdown cancels action/observer work first, then closes/joins logind and
ScreenSaver, joins the observer, joins actions and closes/joins the local guard
if owned. Resource slots are taken before cleanup and mutex poison is recovered;
each close/join failure is recorded while later cleanup is still attempted.
Lua stop closes only the ordinary owner; stop_guard closes only the guard, using
the same cleanup paths. Drop remains a partial-construction fallback. Native lock
surfaces/release permits/PAM/session commands are never used by teardown; output
power restoration on observer stop retains its existing behavior. No pending
lock/suspend request is newly dispatched by cleanup, though an already delivered
native request cannot be revoked. Opaque kernel/driver/bus calls can still delay
joining; this is orderly cleanup, not a hard shutdown deadline.

The idle held helper returns false without Luau, and the native held callback is
feature-guarded. `python3 tests/session/idle_cfg.py --security-source /absolute/source`
requires pinned native source with reviewed security0002 but before idle0004;
it applies the real patch and compiles/runs the actual helper/wrapper/callback
snippets with and without Luau against explicit std-only authority stubs.
It is not full native compilation, a display/bus test or authentication evidence.
The actual teardown/configuration helpers are exercised by existing-package CPU
tests for every ordered cleanup stage, failure continuation, poison recovery,
idempotence, no post-cancel action dispatch and reload configuration stability.

## Pinned SCTK delegation

SCTK0.21.1 uses `smithay_client_toolkit::delegate_dispatch2!(State)` for its
SeatData/OutputData/GlobalData dispatch, plus `delegate_registry!(State)` for
registry globals. The observer uses that exact API (SDK output.rs100,
dispatch2.rs25, seat/mod.rs428), as the pinned SDK examples do, rather than older
seat/output-specific delegation macros.

The inline native production-boundary test is
`tohru_idle::observer::tests::pinned_sctk_registry_seat_output_dispatch_bounds`;
it asserts the actual observer State implements Dispatch for WlSeat/SeatData,
WlOutput/OutputData, ZxdgOutput/OutputData, ZxdgOutputManager/GlobalData and the
registry. It needs no compositor connection when run, but compilation requires
the assembled native crate with pinned dependencies. After source review and
explicit build allocation, run its exact test filter in that native crate;
root foundation tests do not compile the SDK observer. No preparation copy
changes or extra fixture file are required because the test lives in observer.rs.
