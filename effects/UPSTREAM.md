# Source and compatibility provenance

Extracted from `/home/jesse/projects/j2bar`, HEAD
`3a8daa5e077f4085b5262041ea4832373b8754ca` on 2026-11-07.
The checkout contained unrelated README/Rust/golden/performance changes; none of
the selected `hyprland/` files or root license was dirty. `source-manifest.json`
records SHA-256 of every copied file before extraction changes.

`focus/` and `bars/` derive from `hyprwm/hyprland-plugins` v0.56.0,
BSD 3-Clause (`UPSTREAM-LICENSE`). The j2bar source already incorporates local
focus class-filter, combined-modes, render-shrink and bars transformed-pass,
unload-listeners patches, plus its subsequent fixes. Jesse Malinosky's original
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
paths while preserving observable external false/unset writes. Component symbols,
internal diagnostics and the special `j2bar:maximize` config token are retained
for easy algorithm/settings comparison. They do not depend on j2bar's process.
The new main.cpp combines only the four selected components, checks the private
build hash, unwinds partial initialization and exposes a distinct read-only
capability command. The former managed-rules/glass/panel orchestration is omitted;
compositor configuration belongs to Nix.

## Reviewed target

- Nix repository flake.lock `nixpkgs-hyprland`:
  `8ce4ef6cb6f871616146b9fe26d2a5ae594e94fe`;
  NAR `sha256-xB8mKMOx1IA9vTDNLmJZ6n4wCMq/cuWBBOzGCRnqxrU=`.
- Hyprland 0.56.2, actual source commit
  `efb50993780079460b0cbed1363e2166a2de1d9f`.
- Examined existing development output:
  `/nix/store/cbzp6ml69cd8c2v12qkrhsdp62bhp81k-hyprland-0.56.2-dev`.
  Its src/version.h declares aquamarine 0.15.0, hyprutils 0.14.1,
  hyprgraphics 0.5.1, hyprcursor 0.1.13, hyprlang 0.6.8.
- Expected API hash from those headers:
  `efb50993780079460b0cbed1363e2166a2de1d9f_aq_0.15_hu_0.14_hg_0.5_hc_0.1_hlg_0.6`.

Nix asserts the reviewed version and C++ asserts the exact source commit.
`Abi.hpp` privately computes the complete Hyprland API hash from the build
headers before initializing any hook; host mismatch fails loading. This also
avoids weak-symbol interposition in upstream's inline client-hash helper.
The API hash does not encode every local patch or compiler option: the plugin
must be built from the same compositor derivation used by the candidate session.
Pass that derivation explicitly as `hyprland` to package.nix.

`/home/jesse/nix/hosts/home/gaming.nix` adds tearing-first-frame.patch to the
physical compositor. That existing patch stays owned by Nix, with no copy or
activation here. The standalone flake is the unpatched exact upstream baseline;
it is not a claim that a separately patched desktop build has been validated.

Private internals include CWindow::sendWindowSize (focus render-only shrink),
CWindowFadeout::create (sheet close snapshot), transformer/render-pass behavior,
window decoration/rules and animation state. Successful compilation does not
prove hook installation, rendering parity or unload safety. Review and private
compositor validation are required before widening this pin.

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
