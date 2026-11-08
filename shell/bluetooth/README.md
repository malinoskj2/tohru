# Native Bluetooth popup and pairing

Production: include bluetooth/state.plm + bluetooth/popup.plm in same root as its
bar button, permission bluetooth.*, initialize require("bluetooth/controller")().
Retain returned open/close/toggle for root direct coordination. Public native
bluetooth_open/close/toggle events; private entry scene/button is test-only.

Uses pinned platform/bluez.rs native declarative service: adapter present/powered,
discovering, capped24 connected/paired/nearby devices, and pairing kind/id/address/
name/code. Declaring pairing explicitly plus answer/confirm/cancel permission is
required by native logic_luau::offer_pairing; raw sys.watch alone is insufficient.
No second raw observer, custom agent, BlueZ binding or polling is used.

Power/discovery/connect/disconnect/forget are native commands. Discovery started
here is tracked by process-retained scene fact bt_scan_owned across VM reload
and stops on close only after a successful native stop; failures retain the
obligation for retry. An already running outside scan is not stopped just because
this popup closes. Device actions reject stale address/index changes. PIN and
passkey preserve leading zeros; secret native input agent:no clears after send,
question change, creation/reload reconciliation and close. Confirm/authorize reply only to the question displayed
by this controller, checking current native id/kind; display code waits for the
device. Cancellation names the native question id. A new question opens popup;
close/Escape/outside native blur cancels pending question so it cannot hang unseen.
Requests are labelled as sent and do not imply paired/connected completion.

Source: j2bar panel/settings/bluetooth.rs + app/bluetooth.rs; native panel glass
approximation accepted. Six-row pages, Up/Down/Return, real pairing buttons/field.
CPU: lua tests/bluetooth/controller.lua. Native compiler --check entry.plm passes.
Private native BlueZ fixture/input/visual evidence pending allocated workload;
never test against host radio or real devices without explicit authority.

Root measured popup anchor: follow the instant native property `bt_button_x`
with the bar button center in screen-local logical pixels. Its default `-1`
selects `screen.width - 305` (native prop initializers require a numeric literal).
Any nonnegative value, including zero, is an explicit root allocation. The390px
panel opens at `clamp(button_x - 195, 8, max(8, screen.width - 398)), 36`;
negative fallback preserves the prior default position away from output edges,
and narrow outputs clamp to x8. Root owns the measured center/follow rule; no
controller, timer or shared layout manager is involved.
