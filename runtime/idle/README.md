# Native integrated idle

Source checkpoint; native compile/protocol acceptance outstanding. No activation.
The existing `tohru-shell` library owns policy/cookies/delay. Pinned Pleamar owns
native transport/renderer, without a new crate/daemon/backend.

Shared-owner wiring after review:

- Copy native.rs to upstream `src/tohru_idle.rs`, and observer.rs/screensaver.rs/
  logind.rs/sleep_guard.rs to `src/tohru_idle/`.
- Declare `mod tohru_idle;` in native lib and call
  `crate::tohru_idle::install(&lua, self.tx.clone(), self.to_logic.clone())?`
  beside native settings install, inside immutable root-only TOHRU_SHELL gate.
- Apply `runtime/idle/0004-idle-native.patch` after reviewed0002 and window0003.
  It only adds the native held transaction and callback, not auth behavior.
- Existing library fileset includes delay.rs/input.rs/logind_state.rs and their
  tests/session/idle_delay.rs/idle_input.rs/idle_logind.rs.
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
and reconfigures existing Policy without generation reset. Config queue8,
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

Native adapters are parse-checked only at this checkpoint. Core23CPU tests,
Clippy and actual Lua/private scene parser checks cover their respective scopes;
no native observer/logind/fade/screen-power/PAM/security acceptance inferred.
