# Same-process Settings checkpoint

Root named native window `settings` lives in the existing `tohru` process and
uses the existing process-owned settings singleton. No second settings process,
configuration daemon or custom IPC is started. `tohru settings open|close` targets
real root events `settings_open_request`/`settings_close`.

Initial source-mapped dimensions: 800x700 window,220px sidebar,30px navigation
rows, source15/13/11px typography. This is a native functional candidate, not
accepted1:1 layout/glass/font/liquid motion. Current categories are Bar, Appearance, System and Wallpaper. Mooncoil, screen-corner and wallpaper-enabled toggles update root publication.
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
inactive while any sound/calendar/network/Bluetooth/info/About popup owns the
root keyboard.

Root settings reload republishes the same owner snapshot to this window. Native
persistence warnings/errors remain distinct from observation; this window does
not save managed edits or import old shell state. Controller CPU fixtures execute
actual source for search/history, failure, managed temporary and unmanaged paths.
Native window focus/WM close/reopen, physical keyboard, native controls and
rendered sizing still require private runtime validation. No acceptance inferred
from scene compiler or mocked native boundary.

Native Wi-Fi/Bluetooth/About and information popups are now composed in the same
root and reachable through measured bar controls or fixed public CLI panel routes.
They do not add another Settings owner/window. Native protocol/input/rendering
acceptance, embedded Settings category coverage and input/per-app/device sound
controls remain subsequent gates.
No omitted gaming/Assistant/DND/admin controls are reintroduced.


Wallpaper uses W's actual540×526 picker at244,104 in this same window. Its folder,
assignment map and enabled flag belong to the one Rust owner, including Nix
process overrides and durability warnings. Folder loading happens only on explicit
picker open/refresh/change, with native scoped catalog and output queries. No
second preferences reader is introduced. Assignment failures leave the accepted
map/routes unchanged; offline assignments survive. Video entries are retained
with an honest pending-renderer notice until that separately owned renderer is
integrated. Native decoding/pixels are not inferred from successful persistence.

`tohru wallpaper open` enters this route, `wallpaper randomize` uses retained
implicit output selection, and `config set wallpaper.enabled true|false` changes
the existing flag. Mouse/category/history navigation remains in the same window;
Ctrl+F targets picker search on this page, Up/Down belong to file selection, and
Escape returns to Appearance. Closing the window hides the picker. Native input,
geometry/hotplug and rendered catalog evidence remain runtime gates.
