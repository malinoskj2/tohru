# Native capture investigation and open gates

No screenshot command, exported image, clipboard image or completed selection UI
is claimed by this source note. These paths must remain disabled until their
entry point and lifetime contracts are implemented and checked privately.

Pinned Pleamar dc902feb source:

- `src/platform/capture.rs:1–27,55–85`: `capture.freeze(output)` returns a named
  in-memory texture; `capture.pixel` samples a point. `FROZEN` retains the last
  frame per output until another freeze replaces it or the process exits. No
  public release command exists. Clearing the scene's text reference cannot
  prove frame deletion or full cancellation cleanup. Release gap decision pending.
- `src/platform/mod.rs:272–315`: public queries route files/capture/services;
  no public output-rectangle query is exposed. Clipboard command accepts text
  only; capture exports neither a PNG nor a selected image. PNG/image clipboard
  gap decision remains pending; do not call an external screenshot tool or add
  an exporter under an unrelated security approval.
- `src/language/compiler.rs:4995–5033`: `screens: each` supplies screen name,
  width, height, index and count (up to8 copies). It does not supply the output's
  global logical origin. Named surfaces' own cursor is local to that surface.
- `src/render.rs:1108–1148,1401–1406,2737–2781`: native render owns screen facts
  and keeps monitor rectangles/cursor state internally; `cursor.x/y` exposed to
  a scene is relative to the selected surface, not global desktop coordinates.
  Removing a sheet updates count, without a complete topology record to Luau.

Retained source `src/app/screenshot/{mod.rs,overlay.rs,stitch.rs}` in read-only
j2bar freezes every output before showing overlays, selects across global logical
output areas, intersects/stitches the region at actual scales, cancels on Escape
or a region below2×2, and releases overlay surfaces/textures before save/copy.
Visual overlay is a frozen frame with65% black outside the region,2px
Catppuccin-primary outline and14px bold dimension badge at pointer+12,+14.

Source evidence indicates another topology boundary is needed for full
cross-output selection: portable global output origins and stale/hotplug handling.
Native single-output local selection is an available partial alternative; it is
not silently accepted as a replacement for retained cross-output selection.
Ask the coordinator/user before a new topology workaround or that reduction.
No Hyprland polling/IPC dependency is introduced here. Freeze/selection UI source
can proceed only with an honest supplied-topology contract and pending cleanup
semantics; no partial UI is advertised as the finished screenshot command.
