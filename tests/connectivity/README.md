# Private native service UI workload

Prepared only; requires coordinator source assessment, cleared runner cleanup/
isolation recheck, explicit GPU/private-display allocation and S shared runtime
with real info binding. No browser/audio/live role/radio/network/admin operation.

    python3 tests/bar/native.py --services-only --runtime ABS_RUNTIME

The reviewed factory pins system bus to owned runtime_dir/system-bus. This mode
starts its own unprivileged dbus-daemon with fixed private address+EXTERNAL policy
and a synthetic Gio NetworkManager/BlueZ server. The fixture independently checks
exact private system/IPC selectors, no Wayland FD/injection selector, and never
connects through a default/system path. It owns only these well-known names on
this private bus; no native production backend/agent/transport is replaced.
Native Pleamar uses its actual existing NM and BlueZ implementations to speak to
that test server. Runner tracks/stops/kills/reaps app/server/daemon/Sway and the
parent session bus. No temporary backend is packaged for production.

Finite scope: native declarative network list/status, open/password/saved state,
real virtual keyboard synthetic password submit, native disconnect/forget/scan/
radio request+observed fake status; native Bluetooth discovery ownership through
actual VM reload, all five pairing prompt kinds through native agent calls and
PIN/passkey input/confirmation/display/cancel, unchanged-ID reload, native device
connect/disconnect/forget. Synthetic records include only matching-fixture booleans,
not typed passwords/codes. No outside app/device/interface exists in the fixture.

Generated private root entry copies actual feature modules/state/popup parts and
calls their actual lifecycle functions directly. It also exercises real Rust info
binding (actual public hostname), snapshot-on-open/refresh count and retained
cached rows across VM reload/scroll; About six specs/direct MoreInfo, native clock.
Power PNG is EXPLICIT injected private presentation (20%,charging), not real
battery device/unit/warning delivery evidence; those controller boundaries have
separate CPU fixtures. L cross-role banner/tone not started in this workload.

Five labelled captures only: network, Bluetooth display prompt, info, About and
injected power presentation; all under root.tmp absolute host evidence path.
App RTX5090/Vulkan and Sway RTX5090/GLES2 logs required. No visual parity,
performance, actual radio/network/physical/hardware/policy acceptance implied.

CPU protocol.py parses actual fixture introspection XML with GLib, constructs
actual object-tree variants, NM property/settings branches WITHOUT executing any
bus bootstrap/connection/owner/mainloop. PASS12 interface schemas; AST syntax
also checked. This is schema validation, not a passing native backend workload.

Protected input `get` must return the EXACT native denial
`? secret input cannot be read` before and after submit. Empty/None/arbitrary
errors/masked guesses or any disclosure/echo fail; transport exit0 with native
question-mark denial is handled deliberately. CPU secret.py covers the actual
assertion and hostile attempted disclosures, not a substitute secrecy check.

Service records publish by same-directory atomic replacement, never truncate the
reader's live JSON path. Actual extracted save helper concurrent-reader CPU
regression validates complete snapshots. The reader stays strict: malformed
persistent JSON is an error, not an unbounded retry or ignored observation.
