# Frozen region selection

Production integration includes `state.plm`, `presentation.plm`, `view.plm` into
one existing root scene and instantiates `controller.luau` once with an options
callback and a result-notification callback. `entry.plm`/`entry.luau` are private
development entry points, not another production role/settings owner.

The options callback returns `{save, copy, directory, filename_pattern}` from the
existing settings owner. Save/copy default true; directory/pattern default empty.
The transaction snapshots these options, queries native layout/destination, then
freezes every output **before** publishing any overlay route or opening it.
Partial freeze failure releases all tokens already obtained. Up to32 outputs are
supported; names that cannot pass literal native screen selectors fail explicitly.
Global logical coordinates preserve cross-output selection, negative origins and
mixed scale/rotation. Native export validates actual frozen geometry again.

Emit `capture_open_request` for `tohru screenshot region`. Escape, a click/smaller
than2×2 logical selection, failed export, topology change and completion all close
routes and release the transaction's exact version tokens. Native right-click
prototype quit behavior is consumed. Rendering follows source j2bar screenshot
constants:65% black dim, clipped bright selection,2px Catppuccin outline,14px bold
badge,6/4 padding,12/14 pointer offset and3px badge radius. Source behavior comes
from j2bar `src/app/screenshot/{overlay,mod,files}.rs`; native typography and
renderer are retained, no copied GPU renderer.

`capture_status` reports phase/error/actual native result; `capture_error` remains
available for the root UI/CLI. Native result independently reports saved/copied,
actual numbered path and durability/error details. The optional notify callback
receives `(message, error)`; failure never erases successful save/copy evidence.
S/L own connecting it to the existing notification role and placeholder sound.

Apply `runtime/capture/output-events.patch` after native and image-lifecycle
patches. Existing Wayland output add/update/remove dispatch forwards a narrow
internal event only when the scene subscribes to `capture_output_changed`.
The controller queries the existing capture link and cancels only when metadata
changed/became unavailable, avoiding timer polling and false cancellation from
normal overlay creation. Source fixtures check dispatch placement before early
returns; real protocol ordering still needs private display validation.

An open request during an active transaction currently reports busy and preserves
the current frozen selection. The original replaces active selection; this
provisional policy has been sent to the coordinator because native close/rescreen
is asynchronous and has no presentation acknowledgment. Even cancel→immediate
reopen needs actual private pixel validation: no guessed delay is claimed as a
close acknowledgment. Native capture exclusion is requested where supported but
is not assumed for correctness. Logic reload retires preserved text tokens;
compile/reload-reset cleanup and actual late in-flight GPU references remain
explicit runtime gates.

Reproduce CPU-only entry/compiler/Lua checks:

```
python3 tests/capture/check_ui.py --pleamar /absolute/pleamar --lua /absolute/lua
python3 tests/capture/native-source.py --source /absolute/pinned/runtime/source
```

`--check` accepts322 properties/544 instructions/129 rules/32 zones. Ten production
Lua transaction cases pass with explicit native transport fixtures. Type generation
confirms the scene's exposed names, not a separate Luau static analysis. These
commands do not create a display, capture a desktop or claim clipboard/GPU results.
