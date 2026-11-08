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

Native layout hooks for S final measured root assembly (no layout manager):

- `bar_title_cap` instant prop defaults360, clamped0..360. Centered group width is
  min(cap, measured text +20px icon lead), text width cap-lead. Below21px the icon
  is omitted as in source Title.layout;0 hides the group rather than width0's
  native unconstrained-text behavior. Source cap360/14px icon/6px gap retained.
- `bar_media_right_inset` instant prop defaults408; label's right edge is
  screen.width-inset. `bar_media_cap` defaults180, clamped0..180;0 hides it. Native
  tight text ensures actual intrinsic clipped measure/right anchor, not an empty
  fixed180px box for short tracks. `bar_media_measure.width` is available to root.

Use native `follow` on these props from root measured cluster allocation; no Lua
screen-size mirror/setting/backend required. For output width W, center C=W/2,
occupied left endpoint L and right-cluster start R (including current media), a
nonoverlapping centered title bound is min(360,max(0,2*min(C-L,R-C)-2*gap)).
Root decides graceful source widget allocation under width pressure, including
media cap/anchor and service buttons, rather than B guessing final cluster size.
Current default private title fixtures retain360/340 text limits and placement.
This corrects the exposed fixed-layout overlap possibility without claiming final
root narrow-output layout/visual acceptance; S owns those measured formulas.

Workspace cells are native advertised-only: `ws_advertised.1..10` updates from the
native list, absent cells have a native dim treatment/no hit target, and Lua also
rejects stale/unadvertised activation. Exact5..10 physics/scalars stay unchanged.
Pinned generic ext-workspace can only activate advertised handles; it has no
create-workspace command. Coordinator accepted this native UI behavior instead
of a Sway IPC/create adapter; no compositor detection/poll or backend fork. Native
Hyprland focus semantics remain unchanged when native actions are invoked; the
bar does not infer extra capabilities from environment variables or row counts.
