# Fixed built-in preset operation: source checkpoint

This implements only the six retained built-ins, using the existing native PNG
encoder and ordinary accepted wallpaper assignments. No caller-supplied filename,
directory, pixel content, RGB color, image format, decoder, command, daemon or
separate renderer is exposed. No PNG/media/cache operation has been executed at
this checkpoint; Rust/native/codec/filesystem behavior remains unverified.

## API and permissions

`sys.ask("wallpaper.preset", preset, palette)` accepts exactly two text arguments.
Preset: `spectrum|dusk|tide|ember|moss|solid`. Palette:
`mocha|macchiato|rose_pine|nord|gruvbox|tokyo_night|dracula|everforest`.
Both parse to closed domain enums before I/O. It returns:
`{path = ABSOLUTE_PNG, durable = boolean, cached = boolean}`. Failures throw;
durability is a separate actual result, not inferred from messages. The path is
an ordinary static assignment, subject to existing native image/fit/cache rules.

Apply presets.patch after raster-limits.patch (itself after corrected capture
image-lifecycle). Platform dispatch adds only the named operation. Native Luau
query_mutates includes both existing capture writes and this one preset mutation;
base `wallpaper` remains read-only. Explicit `wallpaper.preset` or deliberately
`wallpaper.*` grants it. No permission flag broadens folder/identity reads into
arbitrary writes. The standalone private picker declares the exact permission;
S owns its production root permission. Source search found platform::query is
entered by logic_luau only in the pinned native source; privileged S root binding
calls must maintain their existing root-role provenance.

## Fixed raster and bounded cache

Pure domain preset.rs retains source j2bar RGB triples, eight palette bases,
640×400 dimensions, two-segment interpolation t=.85*y+.15*x with break.55, opaque
alpha and byte rounding. Solid uses selected palette base. Native presets.rs
uses existing image0.25.10 PngEncoder. J2BAR-LICENSE preserves the source MIT notice.

Cache base is trusted owned process XDG_CACHE_HOME, else HOME/.cache. Both must
resolve to an absolute valid path; the cache lives only in fixed leaf
`tohru-wallpaper-presets-v1`. Parent must belong to effective UID and not be group/
other writable; private leaf must be same UID/mode0700. Existing directory is
opened without following its leaf symlink. All mutation after parent acquisition
uses held directory descriptors and fixed names with O_NOFOLLOW/O_NONBLOCK.

Six presets ×eight palettes means at most48 produced final filenames plus one
fixed `.pending-v1.png`; no timestamp/color-key cache accumulation, scan, pruning
of final assets or unrelated-file deletion. Each PNG writer/reader is capped2MiB
(≤96MiB produced final asset bytes plus one≤2MiB temporary); raw generation is
fixed1,024,000 RGBA bytes. External/unrelated contents are not included in this
produced-file bound and are left alone. Existing final content must exactly match
expected encoder bytes and private regular-file ownership, otherwise error;
files/symlinks are never overwritten. Pinned encoder/algorithm changes requiring
different bytes must use a new explicit cache version, not overwrite old assets.

A nonblocking cross-process directory flock rejects concurrent mutation. After
acquiring it, the one owned fixed stale temporary may be reclaimed (private
regular/≤2MiB check); no active conforming writer can be using it. New temporary
uses create-exclusive0600; scoped Drop unlinks only that owned name. Complete data
is synced then linked to final fixed name without replacement. Parent/leaf/data
sync outcomes determine durable; a post-publication directory-sync failure returns
a real path with durable=false. Directory pathname identity is rechecked against
held inode before returning. Same-UID external changes after the check remain
ordinary mutable-filesystem behavior; actual decoder reads are independently
bounded by raster.rs. No hard filesystem/encoder deadline or aggregate RSS bound
is claimed. No helper process or background cache owner is added.

## W picker and retained rotation

Picker model adds six preset choices; `commit(assignments, optional_preset)` must
accept the ordinary whole assignment map and selected preset metadata together.
Existing one-argument callers remain syntactically compatible, but S must consume
the second argument for persistence parity. After accepted commit, optional sixth
factory callback receives the actual native result for notifications; durability
false also produces an explicit picker warning. Preparation success alone is not
settings acceptance. Failed acceptance leaves a bounded reusable cache asset,
with the old assignment/config unchanged.

Public `picker.preset(output, name)` and `picker.rotate(output)` require explicit
root-owned output selection, never guessing a primary output. Rotate with a
folder selects a shuffled file different from current when possible. Without a
folder it advances the selected preset; absent selected metadata defaults to
Spectrum then advances to Dusk, matching retained source. Solid cycles to Spectrum.
Folder submission with changed nonempty folder proposes initial file rotation in
`save_folder(folder, optional_assignments)` so S can accept both in one write;
empty/non-media folder fails before acceptance. Empty folder string clears that
preference and enables later preset fallback.

Root timer source remains default never. Initial rotation factory receives the
accepted initial change policy; configure is for accepted configuration changes,
stop for teardown. Its injected action calls picker.rotate(actual_owned_output)
and must propagate its false result as an error to the timer notification callback.
On a Solid-selected palette change, S should generate with the proposed new palette
and accept palette/updated ordinary assignment together; native query accepts an
explicit palette for that preparation. S owns atomic config/palette/root hooks.

The source picker content now extends to≈590 logical pixels; standalone window
is588×640. S must accommodate that content in its root Settings container. No new
scene compiler/layout/pixel check is claimed.

## Exact S installation hooks and gates

- Foundation copies runtime/wallpaper/{mod,probe,preset}.rs; its public preset
  domain uses std/thiserror only and retains foundation Rust1.85 dependencies.
  Copy tests/wallpaper/{catalog,preset}.rs to the corresponding foundation tests.
- Native adapter now has sibling presets.rs. Install native.rs as
  **src/tohru_wallpaper/mod.rs** and presets.rs/raster.rs beside it. Replace the
  previous single-file src/tohru_wallpaper.rs installation; do not leave both.
  Existing `mod tohru_wallpaper;` declaration needs no name change. text.rs uses
  the same directory's raster module. Copy tests/wallpaper/{presets,raster}.rs to
  native tests/wallpaper/ for cfg(test) paths.
- Native uses its existing image/png/libc graph plus direct thiserror2.0.20;
  no new encoder/dependency enters foundation. Updated check_cpu.py covers the
  native adapter graph at native Rust1.88 and sets future test media artifacts
  to tree-root .tmp. It has NOT been run; native cache tests require explicit
  absolute TOHRU_TEST_ARTIFACT_DIR inside the owned tree-root .tmp.
- S owns typed optional wallpaper.preset, existing accepted palette →canonical
  palette mapping, typed wallpaper.change/default never, root permission,
  commit/save_folder optional proposal metadata, native/foundation copy paths,
  prepare/Nix patch ordering and MIT license installation. No common files were
  edited by W.

Source evidence: full20-patch application against immutable c32a63a with current
W-owned overrides, original lifecycle failure replay preserved; source guards on
closed names/descriptor flags/non-overwrite/syncs and48 fixed key combinations;
17 production picker Lua cases with native query/settings fixtures. Updated
capture permission fixture includes exact preset grant/base denial. All new Rust
pixel/cache/lock/symlink/stale-temporary/encoder-cap tests are source-only and
rustfmt formatted, not compiled/run. New PLM choices have not been compiled.
Remaining gates: actual native type/link/permission tests, generated PNG sample
bytes vs retained source, POSIX lock/race/replacement/recovery/sync failures,
private UI/fit pixels and timer/root atomicity. No live state/desktop/service or
old j2bar data was touched.
