# Same-process Settings checkpoint

Root named native window `settings` lives in the existing `tohru` process and
uses the existing process-owned settings singleton. No second settings process,
configuration daemon or custom IPC is started. `tohru settings open|close` targets
real root events `settings_open_request`/`settings_close`.

Initial source-mapped dimensions: 800x560 window,220px sidebar,30px navigation
rows, source15/13/11px typography. This is a native functional candidate, not
accepted1:1 layout/glass/font/liquid motion. Current categories are Bar, Appearance
and System. Mooncoil and screen-corner toggles update root surfaces immediately.
Corner size is displayed, with edits available through `tohru config set
appearance.corner_size 1..100`. Nix-managed rows explicitly describe temporary process
edits; unmanaged edits persist through the existing writable TOML owner.
Deprecated preferred_output is not offered as an active placement control.
Corner surfaces use W's reviewed shader/native layout; GPU pixels/click-through,
scale/hotplug and actual toggle resource lifecycle remain native validation gates.

Search matches setting title/detail/key literally, with15 result slots and256
query bytes processed. Back/Forward retains at most64 routes and truncates stale
forward routes when navigating after Back. Ctrl+F focuses native search; Tab
switches sidebar/results; Up/Down navigates; Enter/Space activates selected
editable results; Alt+Left/Right navigates history; Escape closes. Opening Settings
closes root dropdowns through direct feature lifecycle calls. Keyboard rules are
inactive while a sound/calendar popup owns the root keyboard.

Root settings reload republishes the same owner snapshot to this window. Native
persistence warnings/errors remain distinct from observation; this window does
not save managed edits or import old shell state. Controller CPU fixtures execute
actual source for search/history, failure, managed temporary and unmanaged paths.
Native window focus/WM close/reopen, physical keyboard, native controls and
rendered sizing still require private runtime validation. No acceptance inferred
from scene compiler or mocked native boundary.

Native Wi-Fi/Bluetooth/About and information pages, input/per-app/device sound
controls and full retained category coverage remain subsequent implementation.
No omitted gaming/Assistant/DND/admin controls are reintroduced.
