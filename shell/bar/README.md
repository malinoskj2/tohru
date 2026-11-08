# Native bar contents

Private entry: `pleamar --scene shell/bar/entry.plm --answer-as tohru-bar-contents --stall 0`.
Use only a private display/bus and an allocated GPU slot during development.

Root integration (S): include `bar/state.plm`, then `bar/contents.plm` at the
bar's origin; call `require("bar/controller")()` once from root logic. Declare
read permissions `window`, `apps`, `media`, `clock`. Forward the existing
process-owned `bar.mooncoil` setting to `fact.bar_mooncoil`; do not create a
second settings owner. Route `calendar_toggle` and `session_toggle` to their
named scenes. This part does not select outputs: root owns one selected output,
preferred/focused/first fallback and reconnect, and reserves the 31px surface.

Source mapping: j2bar `bar.rs` / `bar/providers.rs` (100px inset, 14px app icon,
360px centered title limit, 180px playing-only media, 12-hour default clock),
`sources/media.rs` (title/artist label), `assets/mooncoil.svg` (MIT, notice here).
Pinned services: window title/class + apps id/StartupWMClass/icon; native MPRIS
selection, no commands or controls; clock numeric fields on minute changes.
The native apps service itself checks catalog signatures every two seconds;
this feature adds no polling/timers/frame handlers.

Checks: `lua tests/bar/controllers.lua`; `pleamar --check shell/bar/entry.plm`;
exclusive private GPU integration `python3 tests/bar/native.py --runtime PATH`
after sourcing `runtime/dev-env.sh` and supplying driver libraries as documented
in runtime/README.md. The runner owns and stops its compositor/apps/private bus.

This is usable native service content, not glass/font/layout/physical-output or
performance acceptance. Exact workspace simulation/frame lifetime and final
sound/network/battery assembly remain separate. Native desktop discovery/launch
limitations are pending user decision; this feature does not implement a custom
catalog. The mark's original asset is retained, its exact glass/glow still awaits
visual integration. The clock button's event requires root routing.
