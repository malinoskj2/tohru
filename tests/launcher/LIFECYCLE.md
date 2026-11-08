# Finite native launcher lifecycle

Requires coordinator source assessment, cleared runner cleanup/isolation review,
shared patched runtime and explicit exclusive GPU/private-display allocation.
No heavy build needed. The unchanged source-only/CPU checks are not this run.

    python3 tests/bar/native.py --lifecycle-only --runtime ABS_PATCHED_RUNTIME

Uses reviewed runner ownership/private session bootstrap plus ad58142 fixed
system-bus containment and e4e09ab kill/reap-on-terminate-failure. All runtime/CLI
children use exact private PLEAMAR_SOCKETS and named Wayland; session override
and outside/symlink system endpoints rejected. No host network/radio/bus/desktop,
external app launch, portal, role owner, media or audio. Default private system
endpoint has no daemon for this workload; services cannot reach a live system bus.

Workload: exact private one-entry native catalog (no launch invocation), actual
new production entry+namespace, initial unmapped namespace/transparent1px bootstrap
and actual empty input region, real private virtual keyboard query/Escape, exact
role creation/destruction from raw Wayland protocol, rapid reopen/new close,
coalesced open/close, actual closing/visible VM reload. Private Sway second output
creation/disable/re-enable and own solid test window establish focused/first
fallback; this is a private protocol/topology test, not physical hotplug proof.
No broad pin/action/persistence rerun after those unchanged paths already passed.

Finite passive1second after settled close compares new wl_surface.frame requests,
not a sustained benchmark/FPS/power or physical/visual/plugin acceptance. Raw
lifecycle-summary explicitly carries these limits. Both app Vulkan and Sway GLES2
RTX5090 hardware logs checked before captures. Media/logs under tree-root.tmp
absolute host path printed by owner cleanup; all apps/own window/compositor/bus
stopped/reaped, failure preserves raw logs and still cleans owners first.

CPU evidence parser check: python3 tests/launcher/lifecycle.py --cpu. It exercises
Wayland role and region ID reuse so destruction cannot hide a later new role
with the same protocol ID; empty-input proof requires a real assigned empty
region, not merely absence of visible shapes. Actual native command/script
syntax and semantic failures are reported as failures, never replaced with a
workaround or visual/performance claim.

Input-region evidence tracks Wayland double-buffered state: a region assignment
is only pending until that surface commits. Region contents are copied at the
assignment, so destroying the region afterward does not discard pending state.
A committed nil region restores default full input; surface destroy/recreation
invalidates earlier evidence even when the numeric object ID is reused. The
CPU parser cases cover these ownership/commit transitions before native use.
