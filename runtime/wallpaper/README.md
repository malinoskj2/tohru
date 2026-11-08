# Scoped wallpaper media catalog

`mod.rs` belongs to the existing tohru-shell library; `probe.rs` is its thin
`tohru-wallpaper-probe` binary. `native.rs` installs as the pinned runtime's
`src/tohru_wallpaper/mod.rs` with `presets.rs`/`raster.rs` beside it;
`catalog.patch` adds the read query dispatch. The separately granted fixed preset
mutation is documented in PRESETS.md.
This is a wallpaper-only operation, not an arbitrary filesystem/command service.

S shared hooks (W does not modify shared files):

1. Add `#[path = "../runtime/wallpaper/mod.rs"] pub mod wallpaper;` in root lib.
2. In existing root package, add bin `tohru-wallpaper-probe` at
   `runtime/wallpaper/probe.rs`; add existing runtime-selected `anyhow=1.0.103`
   and `serde_json=1.0.151` dependencies. No new package/crate. Root existing
   thiserror2.0.20 supplies typed catalog errors.
3. During native preparation, copy the root module/tests/probe with their paths,
   install `native.rs` as `src/tohru_wallpaper/mod.rs` with the two sibling modules
   (replace the earlier single-file installation), add `mod tohru_wallpaper;` and
   apply `catalog.patch`. Native already has libc/serde_json; no decoder or
   playback dependencies are needed for this catalog.
4. Build/install probe **next to the actual Pleamar executable**, including Nix
   runtime output. The query resolves `current_exe().parent()/tohru-wallpaper-probe`,
   never PATH or a caller-supplied executable/environment override.
5. Scene declares `permissions { services: "wallpaper" }`; Luau calls
   `sys.ask("wallpaper.catalog", explicitly_configured_or_chosen_absolute_folder)`.
   The native read-query permission checks the base service `wallpaper` (an exact `wallpaper.catalog` permission alone does not grant sys.ask). Result is a sorted list
   of canonical absolute file paths; failures throw, empty folders return `[]`.
   Picker and shuffle/rotation consume this list; root commits an assignment map
   once through the existing TOML/Nix/runtime owner.

The catalog lists regular media files immediately inside the selected folder,
without recursion or opening media contents. Supported retained images are
png/jpg/jpeg/webp; videos mp4/webm/mkv/mov/gif. Symlink entries and special files
are skipped. An explicitly chosen symlink folder is canonicalized; no old j2bar
state is read. A changed file canonicalizing outside that selected canonical
folder is rejected. Unsupported path encoding is an error, never lossy aliasing.

Bounds are4096 scanned entries (including ignored names),4096 bytes per path,
and1MiB response; JSON worst-case escapes are accounted for before encoding.
Overflow fails the whole request instead of publishing a truncated catalog.
The native wrapper concurrently drains nonblocking stdout/stderr, limits stdout
to1MiB/stderr4096, checks a5s deadline and kills/reaps its fixed probe on every
exit path. No detached filesystem thread or uncounted helper remains.
The probe itself never spawns descendants; parent-death protection and a fork/exec parent check also cancel it when its requesting process/thread exits. Kernel-uninterruptible filesystem
I/O can delay kill/reap despite the deadline; no hard kernel shutdown bound is
claimed. Tests exercise responsive local filesystems/processes, not hung mounts.

CPU evidence uses the actual module and native adapter through an ephemeral
harness in tree-root `.tmp/wallpaper-cpu/` (shared files remain S-owned): catalog
validity/scope/overflow/encoding tests, real child timeout/reap/output limits,
and actual sibling-layout query→probe success/failure. Native permission dispatch,
packaging, UI picker and installed runtime calls remain integration gates.

`image-reset.patch` fixes reviewed Q-W1 in native `Texts::image`: the fallback
frame is keyed by both image index and live source identity (base name + declared
size), rather than index alone. Different/cold/missing paths are blank until
ready, including a coalesced detach/reassign/attach. Empty sources release that
last-frame reference; same-source numeric versions retain native reload behavior.
Picture sweeping follows the keyed reference. This applies to live images
throughout the runtime; it adds no renderer, polling, slot tombstones or cache.
GPU textures/atlas entries still follow existing native cache reclamation, so
clearing a reference does not promise immediate deallocation of every cached
pixel. Test `tests/wallpaper/image-reset.py --source PINNED_RUNTIME_SOURCE` compiles
actual patched image/live-name/sweep methods with narrow type/I/O fixtures and
checks cold/missing/empty/same-version and last-reference sweep behavior. The
patch also installs an actual native text unit regression; full native compilation
and private GPU old-output/new-cold-path pixel verification remain required.

## Explicit assignment identities

`sys.ask("wallpaper.identities", {absolute_media_path, ...})` returns canonical
identities in request order for at most32 explicitly assigned/chosen paths. It
uses the same fixed-sibling probe,5s deadline, bounded transport and read-only
`wallpaper` permission; it never lists unrelated directories or opens media
contents. Regular target files are required, symlink aliases canonicalize, and a
canonical identity may lack a media extension even though the chosen alias has
one. Identities are comparison keys, not a silent rewrite of accepted paths.
This lets retained randomization compare existing alias assignments against the
canonical catalog without adding a general filesystem/command API. Missing files
fail honestly, preserving the old all-or-nothing proposal. S installs updated
catalog.patch/native/probe/domain files; no new dependencies/common modules.

`python3 tests/wallpaper/check_cpu.py` runs actual scoped catalog/identity and
native transport tests, Clippy, builds the real probe plus an explicit adapter
fixture beside it, and exercises actual query→probe catalog/alias/missing-file
results. These checks use local metadata; no display, decoder or native role is
started. Rust1.85 is declared by this fixture, but the installed toolchain is
newer, so an actual MSRV toolchain check remains distinct.

## Bounded native raster worker (source checkpoint)

Apply `raster-limits.patch` **after capture/image-lifecycle.patch**, with the
previous order image-reset → fit → capture native/layout-link/image-lifecycle.
Install raster.rs as native `src/tohru_wallpaper/raster.rs`; text.rs declares that
private module directly. Native needs direct thiserror2.0.20 (already requested
for video); foundation dependencies/MSRV stay unchanged. Native cfg(test) needs
`tests/wallpaper/raster.rs` at its corresponding tests path.

The actual opened descriptor is checked regular and ≤64MiB; Linux O_NONBLOCK
avoids FIFO-open blocking if a path changes. Ordinary symlink aliases are followed
as before, then fstat validates the opened inode. A take(64MiB+1) read rejects
concurrent growth and captures immutable bytes. Header and decode both use that
same byte slice with the existing image0.25.10 reader: ≤32Mi source pixels,
≤16384 per side, decoder allocation limit256MiB. Worker pre-sizing reads are also
bounded and are not trusted as decode authorization. Static/live cache metadata
comes from decoded bytes, so a changed path cannot retain the previous file's
source dimensions. SVG uses the same bounded input read; no new SVG decoder is
added, and retained wallpaper formats stay PNG/JPEG/WebP (video GIF stays libmpv).

Native named-file animations use the same bounded input/header and library
limits; retained source frames are bounded to4096 frames /256MiB before existing
output decimation. Over-budget animations follow the native failure/static
fallback path. The byte input, decoder allocation and retained-frame bounds are
separate; conversion/resize buffers, decoder internals and aggregate RSS/VRAM are
not claimed to fit one256MiB process cap. Filesystem/codec calls may still block;
no kernel I/O or decoder execution deadline is claimed.

Source-only19-patch application plus source/legacy/metadata guards pass via
`tests/capture/patch-stack.py`. New `tests/wallpaper/raster.rs` cases cover actual
read growth, geometry/allocation policy and malformed decode, but no Rust/native
compile or image decode has run. Source tests do not close actual special-file,
replacement/growth, codec allocation, animated-budget and pixel/runtime gates.

## Fixed built-in presets

See PRESETS.md for the new explicit wallpaper.preset operation, closed domain,
fixed cache bounds/atomicity, source-only evidence, required namespace/test copy
changes, production permissions and root callback metadata. Add preset.rs to the
foundation module copies and its unit test to foundation tests/wallpaper. Native
adapter/cache tests are pending; no PNG/media/cache generation has run. Updated
native-adapter check_cpu.py now needs native Rust1.88/image PNG and is not a claim
that those dependencies enter foundation Rust1.85.
