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
bound accepted by the coordinator, not unrestricted output capacity.
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
Scoped native-folder catalog, capture export/image clipboard and frozen release
are now authorized and implemented as source slices; see their owning modules.

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

## Settings picker integration

Include `picker-state.plm` globally and `picker-view.plm` in the existing Settings
content group. The drawing is540×526 logical pixels; S owns positioning/window
layout/category and hides the generic settings rows while this page is active.
The part directly includes input fields, preserving their global text bindings.
No production picker window is added. `picker-entry.*` are private development
entry points with transient fixture settings and no wallpaper surfaces.

Instantiate `picker.luau(wallpaper, read, commit, save_folder, connected_outputs)`:

- `wallpaper` is the existing controller object, including validate/randomize.
- `read()` returns fresh `{assignments, folder}` from the one root owner. The
  picker copies assignments before any proposed change, even if read shares data.
- `commit(map)` atomically accepts/persists the complete assignment setting once,
  republishes static/video projections and propagates failures. S also retains
  durability/provenance warnings in the existing root settings presentation.
- `save_folder(path)` updates `wallpaper.folder` through the same owner; string
  default empty, bounded4096 bytes. Native catalog succeeds before persistence.
  Empty clears the folder, and no old mutable state or implicit folder is read.
- `connected_outputs()` returns native records `{name,routable}`; the readonly
  `capture.outputs` query supplies these after `layout-link.patch`, requiring only
  base `capture`. Assigned disconnected outputs are merged, retained and editable.

Emit `wallpaper_picker_open_request` from the Settings page. Set
`wallpaper_request_output` first for an explicit initial target; otherwise the
previous/first sorted output is selected honestly, not advertised as focused.
Root category/window close hides `wallpaper_picker_open`; root modal policy sets
`wallpaper_picker_keyboard=false` during capture/other exclusive root flows and
guards the generic Settings arrow/Escape actions while the picker is active.
Physical input/focus and root modal precedence remain integration tests.

The model publishes at most8 files (native max8), backed by at most4096 catalog
paths. Filename filtering/paging/output cycling/keyboard up-down and Enter,
clear, randomize selected output and randomize saved assignments are wired.
Native text rows retain the source picker's filename/current selection behavior;
no preview decoders are created. Current identical literal choice is a no-op.
Accepted assignment capacity remains32; connected plus offline choices are at
most64, and adding a33rd assignment fails before commit. Displayed filenames are
native text, never command lines or tab/newline-delimited transport.

CLI randomization emits `wallpaper_randomize` with `wallpaper_randomize_outputs`
empty for retained implicit selection or a JSON array of explicit names. The
factory's `randomize(targets)` is also available to root adapters. Randomization
refreshes the chosen catalog, Fisher-Yates shuffles it, queries canonical keys of
at most32 existing assigned paths through `wallpaper.identities`, then uses the
reviewed preference policy and commits its whole proposal once. Invalid requests,
missing identities/catalog failures and persistence errors preserve accepted
assignments. No generic filesystem/command wrapper or extra settings owner.

`lua tests/wallpaper/picker.lua` executes11 production picker cases; existing
controller fixtures also pass. Actual native `--check picker-entry.plm` accepts
17properties/101instructions/21rules/18zones; `--luau-types` generates names only.
Root persistence/provenance/native CLI/keyboard/modal/window/installed-probe/GPU
and video rendering checks remain distinct gates.

## Retained static fit projection

`controller.publish(enabled, assignments, fit)` accepts the optional canonical
user fit `fill|fit|centre|tile`; missing/invalid internal fit defaults to `fill`.
Accepted Fill maps to native cover, Fit maps to contain. Four exclusive native
image operations share one image handle per static output; video projection is
unchanged. Native `runtime/wallpaper/fit.patch` must be applied **after
image-reset.patch** before compiling this view. Centre/Tile do not exist in the
old installed native compiler. Source checkpoint awaits independent review and
allocated native compiler/pixel validation; Lua tests are not native proof.
S owns accepted fit schema/default/settings control and third-argument publication.
See runtime/wallpaper/FIT-CAPABILITIES.md for exact changes and remaining gates.
