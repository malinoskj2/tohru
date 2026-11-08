# Scoped wallpaper media catalog

`mod.rs` belongs to the existing tohru-shell library; `probe.rs` is its thin
`tohru-wallpaper-probe` binary. `native.rs` installs as the pinned runtime's
`src/tohru_wallpaper.rs`; `catalog.patch` adds only the public query dispatch.
This is a wallpaper-only operation, not an arbitrary filesystem/command service.

S shared hooks (W does not modify shared files):

1. Add `#[path = "../runtime/wallpaper/mod.rs"] pub mod wallpaper;` in root lib.
2. In existing root package, add bin `tohru-wallpaper-probe` at
   `runtime/wallpaper/probe.rs`; add existing runtime-selected `anyhow=1.0.103`
   and `serde_json=1.0.151` dependencies. No new package/crate. Root existing
   thiserror2.0.20 supplies typed catalog errors.
3. During native preparation, copy the root module/tests/probe with their paths,
   install `native.rs` as `src/tohru_wallpaper.rs`, add `mod tohru_wallpaper;` and
   apply `catalog.patch`. Native already has libc/serde_json; no decoder or
   playback dependencies are needed for this catalog.
4. Build/install probe **next to the actual Pleamar executable**, including Nix
   runtime output. The query resolves `current_exe().parent()/tohru-wallpaper-probe`,
   never PATH or a caller-supplied executable/environment override.
5. Scene declares `permissions { services: "wallpaper.catalog" }`; Luau calls
   `sys.ask("wallpaper.catalog", explicitly_configured_or_chosen_absolute_folder)`.
   The native permission check remains authoritative. Result is a sorted list
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
The probe itself never spawns descendants. Kernel-uninterruptible filesystem
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
