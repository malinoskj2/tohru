# Evidence and private compositor gates

## Checked in the extraction tree

- Source provenance/scope: `make check` checks SHA-256 for every copied file,
  allowing only the recorded partial-init null guard in bar teardown.
- Compilation: GCC 16.2.0, existing stdenv and exact reviewed Hyprland 0.56.2
  dev output, Lua 5.5.0; `make -j1` succeeded. One inherited warning about
  deprecated `HyprlandAPI::addConfigKeyword` in the legacy-config branch.
- Packaging expressions parsed with `nix-instantiate --parse`; standalone
  flake resolves offline at the recorded nixpkgs pin. Nix package derivation
  evaluates and its dry-run initially requested only this plugin, no compositor
  or dependency build.
- The shared object has the required `pluginInit`, `pluginExit`,
  `pluginAPIVersion` exports, four selected component init functions, debug
  information/build ID. No extracted glass/panel/managed-rule component.

Development diagnostics live at absolute host paths:

- `/home/jesse/orca/workspaces/tohru/pleamar-effects/.tmp/effects/compile.log`
- `/home/jesse/orca/workspaces/tohru/pleamar-effects/.tmp/effects/compile.sh`
- `/home/jesse/orca/workspaces/tohru/pleamar-effects/.tmp/effects/build-environment.json`
- `/home/jesse/orca/workspaces/tohru/pleamar-effects/.tmp/effects/nix-build-dry-run.log`

These development paths are not standalone package runtime dependencies. The
progress log records later packaging/ABI checks and final commits. No screenshot,
recording, benchmark, live loading, Nix activation or desktop changes were made.
No claim of visual/performance acceptance follows from these static checks.

## Remaining private compositor validation

Request the coordinator's exclusive GPU test slot first. Use a private sandbox
display and D-Bus, exact candidate compositor/plugin derivations, fixed stimulus
and the same reference configuration. Verify the compositor actually presents on
the hardware adapter before recording any visual/performance conclusion. Store
all media under the active tree's `.tmp/` and report absolute host paths.

1. Start the disposable compositor without any old duplicate plugin/hooks;
   load the extracted library, check the distinct capability query, then apply
   examples/effects.lua or the resolved reference-selected configuration.
2. Focus keyboard/mouse repeatedly, including interrupted dips, new-window
   mapping, floating/fullscreen, chooser exclusions and invalid class patterns.
   Confirm render-only shrink sends no smaller client configure and returns to
   exact layout without extra settled frames.
3. Show titlebars only on configured applications; double-click toggles
   maximized state while retaining client fullscreen semantics. Verify transformed
   nested-pass drawing, hidden/grouped windows and fresh config reload.
4. Open/close/cancel/kill GTK portal choosers with and without resolvable parents,
   move either window between workspaces/outputs and destroy parent/chooser.
   Confirm parent restoration, close snapshots, damage and resize behavior.
5. Map the verified optional launcher layer on each output, rapidly toggle,
   destroy the launcher client and remove outputs. Multiple matching layers on
   one output must restore only after the last closes. Other outputs remain
   unchanged; new/moved windows follow the current output recession.
6. Exercise all eight Wayland transforms, scale and output hotplug with animated
   asymmetric client content, fullscreen windows and backdrop blur/shade. Compare
   matched captures against the reference without loading old/new hooks together.
7. Unload during active focus/sheet/recession animations, with hidden/grouped,
   closing and unmapped clients; continue interacting and reload the extracted
   plugin. Verify no residual callbacks/decorations/transformers/dangling passes
   and no crash. Deliberately mismatched build must refuse before hooks start.
8. Run the shell/launcher with plugin absent and on Sway. Optional effects
   absence must not block launcher functionality or create hard Hyprland deps.

The old j2bar `tests/hyprland/` scripts are reference recipes, not passing tests
for this package: several invoke the old bar/glass/managed-rule API and write
media outside this tree. Do not run them unchanged. Port fixture-driven checks
only after the candidate adapter and reference have been verified. Performance
requires the plan's matched trials/presentation evidence and separate accounting;
this extraction alone has none of that evidence.
