# Integrated idle boundary

`mod.rs` is pure policy for the existing `tohru-shell` package, not another crate,
service or daemon. It does not activate a bus/display or run system commands.
Defaults preserve retained j2bar timers: lock1800s, unlocked screen-off disabled,
locked screen-off60s, suspend7200s unlocked/5400s locked, fade2s. Settings schema
and process integration remain S-owned and are not wired by this checkpoint.

A native process-owned observer must execute `arm()` effects once and own exactly
three ext-idle-notify subscriptions. Destroy replaced handles and attach the opaque
Subscription to callbacks; old generations are ignored after rearm. Standard
compositor inhibitors apply to those notifications. Feed independent Screensaver
cookie, logind-idle and caffeine observations via `inhibited`; releasing one source
cannot clear another. Any active inhibition cancels fades and pending sleep.

Execute `Fade`/`ClearFade` with native Pleamar rendering; schedule only `deadline()`
for fade completion/5s locker timeout, no recurring policy polling. Lock and suspend
coalesce into one lock request. Feed actual native compositor held state into
`session`; request acceptance does not qualify. Route RequestSuspend through the
locker so its current native held state is revalidated at execution, never blindly
call session.suspend from a stale ordinary-shell observation. `resumed()` cancels
pending work, restores powered-off screens and restarts locked/unlocked timers;
it never unlocks. Screen power adapters remain isolated by compositor/protocol.

Required shared integration request: declare this module in existing src/lib.rs
with `#[path = "../runtime/idle/mod.rs"] pub mod idle;` and include runtime/idle
plus tests/session/idle.rs in runtime prepare/Nix foundation sources. No dependency
changes needed (existing thiserror). Tests are included by the module under cfg(test).
The adapter/owned protocol threads, logind prepare-for-sleep delay inhibitor,
notification/cookie owner and native fade scene are still unimplemented; this is
an independently checked policy boundary, not complete idle integration.
