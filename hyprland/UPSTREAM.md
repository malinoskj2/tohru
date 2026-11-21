# Source and compatibility provenance

Extracted from `/home/jesse/projects/j2bar`, HEAD
`3a8daa5e077f4085b5262041ea4832373b8754ca` on 2026-11-07.
The checkout contained unrelated README/Rust/golden/performance changes; none of
the selected `hyprland/` files or root license was dirty. `source-manifest.json`
records SHA-256 of every copied file before extraction changes.

`focus/` and `bars/` derive from `hyprwm/hyprland-plugins` v0.56.0,
BSD 3-Clause (`UPSTREAM-LICENSE`). The j2bar source already incorporates local
focus class-filter, combined-modes, render-shrink and bars transformed-pass,
unload-listeners patches, plus its subsequent fixes. j2bar's original
code and local modifications retain the root MIT `LICENSE`. Preserve both notices
in source and binary distributions. No hyprglass source or GLASS-LICENSE is
included because no code from that component is extracted.

Component files remain byte-for-byte source copies except the intentional
changes recorded with original and extracted SHA-256 in source-manifest.json:
`bars/main.cpp` gains a null-state guard for failed initialization; `focus/main.cpp`
fixes accepted Q-E-1 by retaining active geometry, opacity and animation config
through both focus animation legs and restoring them before releasing client
sizing on unload. Only owned end callbacks are removed; foreign replacements and
unaffected windows are preserved. Mapped tiled windows use current layout targets,
while floating/fullscreen and unmapped clients restore saved geometry. Selected
modes, target values, curves and speeds remain unchanged. `sheet.cpp` fixes
accepted Q-E-2 by preserving the parent's prior optional noFocus set-property
slot, including explicit false, and restoring it through existing show/cleanup
paths while preserving observable external false/unset writes. Component symbols
and internal diagnostics are retained for easy algorithm/settings comparison;
the special maximize token is now `tohru:maximize`. They do not depend on j2bar's process.
The new main.cpp combines only the four selected components, checks the private
build hash, unwinds partial initialization and exposes a distinct read-only
capability command. The former managed-rules/glass/panel orchestration is omitted;
compositor configuration belongs to Nix.

## Reviewed target

- Root `hyprland-nixpkgs` and `hyprland/flake.nix` `nixpkgs`:
  nixos-unstable `2786a2a72088ecf32aea5b3ec52099e6a7cff78f`;
  NAR `sha256-f/VVvxf5RsQgWVhUtefoca5ktVg0DA2bnkMOcmSnozo=`.
- Hyprland 0.57.2, actual source commit
  `11cbc6c577b84ffb1edf4df3fc5b6128c3ae6947`.
- Examined existing development output:
  `/nix/store/k3c186wq7f8z4bb0qc3d8nwyphx2ngpc-hyprland-0.57.2-dev`.
  Its src/version.h declares aquamarine 0.16.0, hyprutils 0.14.4,
  hyprgraphics 0.6.0, hyprcursor 0.1.13, hyprlang 0.6.8.
- Expected API hash from those headers:
  `11cbc6c577b84ffb1edf4df3fc5b6128c3ae6947_aq_0.16_hu_0.14_hg_0.6_hc_0.1_hlg_0.6`.

Nix asserts the reviewed version and C++ asserts the exact source commit.
`Abi.hpp` privately computes the complete Hyprland API hash from the build
headers before initializing any hook; host mismatch fails loading. This also
avoids weak-symbol interposition in upstream's inline client-hash helper.
The API hash does not encode every local patch or compiler option: the plugin
must be built from the same compositor derivation used by the candidate session.
Pass that derivation explicitly as `hyprland` to package.nix.

Hyprland 0.57 includes upstream's first-frame tearing fix (b04e606), which
`/home/jesse/nix/hosts/home/gaming.nix` carried as tearing-first-frame.patch
for 0.56.2. The standalone flake is the unpatched exact upstream baseline; any
locally patched desktop build must still build the plugin from that derivation.

Private internals include Layout::CWindowTarget::sendWindowSize (focus render-only
shrink; CWindow::sendWindowSize only forwards to it in 0.57), the four-argument
CWindowFadeout::create (sheet close snapshot), transformer/render-pass behavior,
window decoration/rules and animation state. Successful compilation does not
prove hook installation, rendering parity or unload safety. Review and private
compositor validation are required before widening this pin.

## Hyprland 0.57 port

`bars/` takes hyprland-plugins' API-chase commits 81516ad, a9eaa52, 2b41ad8,
722f15a, 64d6f51 and 428bfcd, not its later feature commits. Where those commits
changed behaviour, the 0.56 behaviour is kept: X11 windows that suggest no border
get no bar, rule updates call `updateDecorations`, a touch drag still floats,
resizes, pins and moves the focused window, and a bar drag starts only when the
pointer's window accepts it, as 0.56's `changeMouseBindMode` did. Focus keeps
its 0.56 timing instead of upstream's deferred focus.

`focus/` hooks `Layout::CWindowTarget::sendWindowSize`, which layout updates
call directly. Focus reasons are a bitmask in 0.57, so click-down and click-up
both count as mouse focus.

`sheet.cpp` and `recede.cpp` use 0.57's transformer interface: each reports its
drawn extents and draws the cropped input canvas into a work buffer that it
returns with its monitor-local box. `LogicalFramebuffer.hpp` is gone because 0.57
hands transformers a canvas in logical orientation. Recession clears the blur
matte to opaque so it keeps the frame's box. The sheet's commit listener is on
the window's private XDG surface, reached through `CWaylandBackend`.

## Cleanup ownership limits of the pinned API

The animation API has no end-callback getter or owner/generation handle. Focus
tracks the lifetime of a token captured by each installed end callback, including
its outward leg, so teardown clears only callbacks it still owns. Pre-existing
callback takeover on the variables being animated remains the original effect's
behavior; unrelated windows and later replacement callbacks are not cleared.
Restoration warps suppress end callbacks so a foreign replacement is preserved
without being invoked during unload. Real callbacks/layout/damage behavior still
needs the private-compositor checks.

COverridableVar exposes optional priority values but has no write generation or
owner. Sheet cleanup can observe another writer's false or unset and preserves
those writes. An external write of true (or false/unset followed by true) is
indistinguishable from the sheet's true: cleanup restores the saved slot in that
case. This is a target-API limitation; no private slot access, interception of
other setters or new protocol was added. A destroyed parent needs no restoration;
all surviving-parent sheet removal paths continue through showParent.
