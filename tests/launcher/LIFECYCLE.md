# Finite native launcher lifecycle

Requires coordinator source assessment, cleared runner cleanup/isolation review,
shared patched runtime and explicit exclusive GPU/private-display allocation.
No heavy build needed. The unchanged source-only/CPU checks are not this run.

    source runtime/dev-env.sh
    # Include installed wayland-scanner, cc/pkg-config and known setsid/sh in PATH.
    TOHRU_SHELL=1 python3 -B tests/bar/native.py --lifecycle-only --runtime ABS_PATCHED_RUNTIME

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

The keyboard is ONE persistent test-only virtual-keyboard client, owned by the
reviewed runner until cleanup. It uses a fixed US XKB keymap and accepts only
fixed `query` (fixture), `escape`, and `quit` commands over its owned stdin pipe.
There is no arbitrary key/command forwarding. Each request waits for the helper
acknowledgment; each Escape additionally requires a new compositor-to-app raw
Wayland Escape press before checking query/visibility. Startup requires actual
keyboard focus. One-shot wtype devices are not used here: their removal dropped
the only headless keyboard, causing a native blur/close before the next process
could send Escape in the failed1794116866071766575 run. Production blur and
Escape semantics remain unchanged.

The tiny client is compiled from keyboard.c + virtual-keyboard.xml (retained MIT
protocol notice) with wayland-scanner/cc/pkg-config and existing wayland-client/
xkbcommon headers/libraries. This is a bounded fixture compile, not a runtime
build or new dependency/backend for the production shell. Generated sources and
binary stay in the evidence keyboard-build directory. Preparation requires the
installed wayland-scanner bin directory in PATH (this sandbox:
/nix/store/2a96lbjxyq7yiaf5zb013kshf8p8nigx-wayland-scanner-1.25.0-bin/bin).
The client rejects mismatched private runtime/IPC, named-display escapes,
WAYLAND_SOCKET and CUA_INJECT_SOCKET before opening a display. All helper owners
are registered through start(); cleanup kill/reap covers failures and stdin EOF
also terminates the device. CPU parser fixtures distinguish Escape presses from
releases/other codes. CPU hostile-env cases must exit2 before a connection.

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
