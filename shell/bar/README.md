# Native bar contents

Private entry: `pleamar --scene shell/bar/entry.plm --answer-as tohru-bar-contents --stall 0`.
Use only a private display/bus and an allocated GPU slot during development.

Root integration (S): include `bar/state.plm`, then `bar/contents.plm` at the
bar's origin; call `require("bar/controller")()` once from root logic. Declare
read permissions `window`, `apps`, `media`, `clock`. Forward the existing
process-owned `bar.mooncoil` setting to `fact.bar_mooncoil`; do not create a
second settings owner. Compose `calendar/state.plm` + `calendar/popup.plm` and its controller in
this same root scene (see ../calendar/README.md), preserving the native click
serial. Only `session_toggle` needs routing to its independently owned role. Accepted output policy is now native `screens: all`: the bar appears on every
output. Root owns this layout and the 31px reservation; no preferred/focused-first
selection adapter or reconnect-selection policy is retained.

Source mapping: j2bar `bar.rs` / `bar/providers.rs` (100px inset, 14px app icon,
360px centered title limit, 180px playing-only media, 12-hour default clock),
`sources/media.rs` (title/artist label), `assets/mooncoil.svg` (MIT, notice here).
Pinned services: window title/class + apps id/StartupWMClass/icon; native MPRIS
selection, no commands or controls; clock numeric fields on minute changes.
The native apps service itself checks catalog signatures every two seconds;
the service content adds no polling/frame handlers. Exact rails publish with
accepted cancellable native60Hz timers only while moving, cancel at rest/error.

Checks: `lua tests/bar/controllers.lua`; `pleamar --check shell/bar/entry.plm`;
exclusive private GPU integration `python3 tests/bar/native.py --runtime PATH`
after sourcing `runtime/dev-env.sh` and supplying driver libraries as documented
in runtime/README.md. The runner owns and stops its compositor/apps/private bus.

This is usable native service content, not glass/font/layout/physical-output or
performance acceptance. Exact workspace and final root service assembly use shared S binding/patch hooks.
Native desktop discovery/launch/Identity
limitations are accepted; this feature does not implement a custom
catalog. The mark's original asset is retained, its exact glass/glow still awaits
visual integration. The clock button's event requires root routing.
