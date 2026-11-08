# Native static wallpaper projection

`entry.plm` and its Luau controller are a private-display development entry.
No wallpaper is enabled or routed on startup. Do not start it beside the live
wallpaper owner. Production integration adds these parts/controller to the root
process, with settings controlled by its existing one owner.

S integration request:

- `wallpaper.enabled`: boolean, default false.
- `wallpaper.assignments`: output-name → absolute local image/video path map,
  default empty, retaining offline outputs. These are fresh Tohru values; never
  read j2bar's assignment state. Both keys need managed/runtime provenance and
  normal unmanaged TOML persistence through the shared owner.
- Include `wallpaper/state.plm`, `wallpaper/view.plm` and install
  `local wallpaper = require("wallpaper/controller")(assign)` before initial
  settings publication. Call `wallpaper.publish(enabled, assignments)` on every
  accepted snapshot/reload. `assign(output, path_or_nil)` changes one map member
  atomically through that owner and republishes; it must propagate failures.
- `wallpaper_assign` reads `wallpaper_request_output`/`wallpaper_request_path`;
  `wallpaper_clear` reads the output. Requests report errors in
  `wallpaper_error`; accepted configuration never proves decoding/presentation.
- `publish` returns the video's output→path projection for the approved integrated
  renderer. The image decoder receives only retained png/jpg/jpeg/webp formats.
  Existing video formats mp4/webm/mkv/mov/gif remain reserved for the video path.
  Do not advertise working video until that renderer and lifecycle are integrated.

The static route is a named native background surface per assignment slot:
`size: full, full`, `reserve: -1`, `keyboard: none`, no zones. Dynamic `screens:`
text owns connect/remove/reconnect, and a 0×0 native image with `fit: cover` keeps
source aspect ratio and centred cropping. The native renderer owns texture
loading/caching/release and scale/transform; no screenshot readback, subprocess,
frame callback or polling is introduced. Removing an assignment detaches its
route and clears its path; changing a path reuses that output's slot. The whole
snapshot is validated first, so invalid input preserves previous routes.

This source slice has a **32 assignment capacity**, rejects overflow before any
publication, and never silently truncates. This is an explicit implementation
bound awaiting coordinator disposition, not accepted unrestricted output parity.
The pinned compiler does not expand these declaration/surface repeats into a
usable scene, so the 32 small declarations are explicit. Unassigned outputs
create no surfaces, and disconnected assignments remain routed by name for
native reconnect handling. The static controller requires native-routable output
names (no commas or surrounding whitespace), since native `screens:` treats
commas as separators.

Checks: `lua tests/wallpaper/controller.lua` runs the production controller;
`pleamar --check shell/wallpaper/entry.plm` checks actual native declarations.
These establish source/controller behavior only. Root TOML persistence, native
CLI/UI assignment, GPU image rendering/errors, resource reclamation, output
hotplug/scale/rotation, picker/randomization, video, and performance remain gates.
Native arbitrary-folder listing, capture export/image clipboard and frozen-frame
release are pending decisions, with no workaround here.

`controller.randomize(assignments, outputs, shuffled_catalog, canonical)` returns
an all-or-nothing proposed assignment map; the shared owner must commit the map
once. It preserves j2bar's preference order: different and unused, different,
then the first catalog item. Empty output selection rerolls currently assigned
videos and images in the catalog, retaining other images. Duplicate requested
outputs are processed once. The caller supplies shuffled file paths and a
canonical identity function from its approved file boundary, so symlink aliases
cannot defeat the previous/other-output checks. No filesystem probing, shell
command or implicit folder substitution is added. Folder discovery, picker UI
and root persistence/event wiring remain pending; this policy is not advertised
as an integrated randomize command yet.
