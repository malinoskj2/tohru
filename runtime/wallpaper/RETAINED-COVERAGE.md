# W retained-source coverage checkpoint

This is a source audit, not a runtime parity declaration. Task26 in
PLEAMAR_PLAN.md requires 1:1 wallpaper behavior. Existing committed source covers
per-output image/video assignment, bounded catalog/picker/randomization, four
static fits, capture selection/export/release/clipboard and native corner overlays.
Actual native build, linked renderer, outputs/pixels and measured cleanup gates
remain open. Review closure of source does not close those gates.

## Rotation: native facility exists; narrow source module supplied

Read-only j2bar preferences.rs342–367 defines Never/Hourly/Daily/Login. The root
shell schedules initial Login after1s (app/mod.rs235–245), hourly3600s or daily86400s,
and tick1987–1998 reschedules after execution, even on failure. Accepted settings
app/settings.rs125–137 reset interval deadlines; later Login settings schedule no
initial timer. Folder changes can trigger rotation independently in settings
acceptance (settings.rs153–154).

Pinned native logic_luau.rs1002–1017 already provides owned monotonic
`after`/`every`/`cancel` timers, documented in10-luau-logic.md33–35. No native patch,
worker thread, polling/frame listener or daemon is required. W
shell/wallpaper/rotation.luau uses one `after` at a time, rejects stale generations,
reschedules after the root action returns, and handles reentrant settings changes
or stop. Constructor `(rotate, notify, initial_change)` schedules initial policy;
`configure(change)` is called after **accepted config** changes, not video
observation updates. `stop()` cancels the owned timer. Canonical policies are
`never|hourly|daily|login`, default never. Root action/provider/persistence remains
S-owned; W does not invent a primary-output selection or alternate state file.

Production Lua with native timer/action boundaries mocked passes login one-shot,
interval units/deadline reset, failed action next-interval retry, invalid policy
preservation, stale callbacks, stop and synchronous settings/result reentry.
Evidence .tmp/check-results/wallpaper-rotation.txt. No real timer/service/media ran.
Root integration waits for accepted typed setting and complete rotate action.

## Additional source gaps requiring narrow boundary direction

1. **Six built-in preset backgrounds.** j2bar preferences.rs277–308 and
   wallpaper/presets.rs114–165 define Spectrum/Dusk/Tide/Ember/Moss/Solid. Presets
   generate ordinary640×400 PNG assignments, with exact byte-rounded two-segment
   RGB interpolation from t=.85*y+.15*x (break at.55). Solid uses selected palette
   base. Operation::Preset persists its generated path and selected preset;
   Rotate uses chosen-folder media different from current, or advances presets
   when no folder (app/wallpaper.rs83–124). Current W catalog/randomization/picker
   do not implement those preset actions or the no-folder rotation fallback.
   Existing native gradients/shaders can paint colors, but generated PNG paths
   preserve current assignment/fit/cache behavior with fewer model changes and
   match the source's raster sampling. A small fixed-preset operation in the
   existing native package, reusing its PNG encoder and a bounded fresh-owned
   cache, is proposed; no generic filesystem writer, second decoder, old state
   import or wallpaper daemon. New native mutation/permission/cache/publication
   hooks need coordinator direction before coding. No preset media was generated.
2. **Retained static input bounds.** j2bar wallpaper.rs22,232–249,329–356 checks
   regular file/type,64MiB input and32Mi source pixels, with16,384-side/256MiB
   decoder allocation limits. W catalog/identities are deliberately metadata-only;
   they currently validate regular supported paths but not those image bounds.
   Pinned native text.rs1250–1265 rasterize_image uses std::fs::read before
   image::load_from_memory and then reduces storage; source-side texture sizing
   is not an input-read/decode-allocation cap. No image declaration/schema exposes
   those limits (ImageSource variants are File/Pixels/Icon/Live). The image crate's
   own defaults do not establish the retained explicit bounds. A preflight-only
   metadata check would leave replacement/growth between checks and decode; exact
   parity requires bounded reading/actual decode limits at the existing worker.
   The narrow worker extension must preserve other native image users and use
   existing image library APIs, not a custom decoder. Coordinator direction is
   needed before changing that native I/O boundary. No large file/decode was run.

Remaining integration/runtime gates include S root-only accepted publication and
actual dedicated-locker/logind/occupancy provenance, fit patch after image-reset
and corrected capture lifecycle, complete actor native typing/linking and library
license closure, actual media errors/loop/GIF/hardware decoding, mixed-scale/
rotation/hotplug/replace pixels, clipboard ownership and normal/error shutdown
process/thread/fd/decoder/texture cleanup. Old mutable data/live owners remain
untouched. Explicit32 assignments and literal native screen-selector restrictions
are accepted bounded limits, not hidden unlimited/parity claims.
