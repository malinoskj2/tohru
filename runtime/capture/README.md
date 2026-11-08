# Frozen region capture boundary

This extends the pinned native capture connection/cache. No second capture,
custom decoder, screenshot command wrapper or daemon is introduced. UI wiring,
full native compilation and private protocol/GPU validation remain gates.

## Shared installation hooks (S owns assembly)

- Root foundation exposes `#[path = "../runtime/capture/mod.rs"] pub mod capture;`.
  Copy runtime/capture/{mod,files,geometry}.rs and tests/capture/{files,geometry}.rs
  preserving paths during preparation. This domain uses existing thiserror/std;
  foundation dependencies and declared Rust1.85 MSRV remain unchanged.
- Apply patches in this order: wallpaper/image-reset.patch, capture/native.patch,
  capture/image-lifecycle.patch. Install native.rs as
  `src/platform/tohru_capture/native.rs`, and clipboard_owner.rs alongside it.
  No root/common file is edited concurrently by W.
- Add `tohru-capture-clipboard` as a bin in the **existing patched native runtime**,
  pointing at a copied clipboard.rs. Native already uses anyhow, image0.25.10,
  wl-clipboard-rs0.9.3 and libc. Build/install it beside the actual native executable.
  Do not add its Wayland/image dependencies to the Rust1.85 foundation package.
- At orderly native runtime teardown call platform::capture's `shutdown` through
  a thin parent-platform hook owned by S; it stops the process-owned clipboard
  manager and joins/reaps its child. The capture module is private, so a parent
  forwarding function is needed. Forced process/thread death also kills the child.
- Scene permissions: read queries need `"capture"`; export needs explicit
  `"capture.region"`, release needs explicit `"capture.release"` (or deliberately
  `"capture.*"`). Native base read permission alone cannot gain saving/copy/release.

## Query contract

- `sys.ask("capture.outputs")` → ready output records: name,x,y,width,height in
  global logical units, native scale and output transform. Refreshes the existing
  capture link, no compositor-specific polling/backend. Incomplete metadata fails
  honestly; maximum32 outputs.
- Existing `capture.freeze(output_name)` → immutable versioned token. Stored
  record keeps upright pixels and freeze-time logical rectangle/mode/transform.
  Layout changes during freeze fail. Buffers are checked before shared-memory
  allocation; retained cache is at most32 outputs/32Mi pixels (128MiB raw bytes).
- `sys.ask("capture.region", tokens, x,y,width,height, absolute_png_path_or_empty,
  copy_boolean)` checks finite whole coordinates, strict token versions, duplicate
  tokens, retained pixel budgets and current native topology before composing.
  BGRA captured pixels become upright RGBA PNG through existing image encoder;
  mixed/fractional scales use rational outward crops and the densest piece's
  placement, desktop gaps stay transparent. Encoding output is byte-capped.
  At least saving or copy must be enabled.
- Result reports `saved`, actual `path`, `durable`, `copied`, `save_error`,
  `clipboard_error`, `width`, `height`. Saving and copying are independent;
  partial success is explicit, never inferred from spawn. Current path contract
  refuses overwrite of files/symlinks; final filename-pattern/numbering UI
  integration remains to be delivered. Directory sync failure reports committed
  save with durability warning rather than a false rollback.
- `sys.ask("capture.release", token)` removes only that version's map ownership.
  Missing tokens are idempotent; a superseded token reports released=false,
  stale=true and preserves the newer frame. Status reports remaining cached bytes
  and external Arc references when available. Release queues a targeted internal
  renderer event; it is not a synchronous GPU-fence/deallocation acknowledgment.

## Capture image and clipboard lifetimes

Native capture images use immutable full-token identities rather than a live-file
version fallback. Native picture textures use the frozen frame's upright shape,
including `image … = from text,0,0`; they are not rendered as a1×1 thumbnail or
packed into the glyph atlas. This reuses native picture allocation/upload and
native frame fitting, bounded by the card's maximum dimension. Explicit release
removes capture cache references and pending picture uploads, drops native GPU
picture ownership even with closed surfaces, and rejects released late worker
or rebuild deliveries. Regular images and live-file numeric versions retain
normal native behavior. No tombstone cache or polling observer is added.
External Arc references/in-flight GPU work may outlive a map removal; report that
limit honestly rather than claiming all bytes vanish at query return.

Clipboard is one foreground helper belonging to this package. A stable
process-owned manager thread (outside reloadable Lua) parents it; otherwise Linux
parent-death protection would attach to a retiring Lua thread. Commands are
serialized/bounded; at most one old/current and one pending server exist during
replacement. PNG bytes move to the owner without a second parent-side copy.
A bounded nonce clipboard offer plus image/png is read back before READY; merely
spawning or prepare_copy is insufficient. The parent bounds stdin/readiness,
reaps failed children, keeps the old child handle until new readiness, uses pidfds
for PID-reuse-safe cancellation and a blocking waiter for automatic reap after
ownership loss. Parent-death setup checks the fork/exec race. No PATH executable
selection, autostart, separate service or live-owner takeover is involved. The compositor may cancel the previous offer when the new offer takes ownership; a failed replacement does not promise restoration of old clipboard contents.

Filesystem I/O and opaque protocol/kernel operations can still delay shutdown
when the OS does not respond; no kernel-uninterruptible-I/O or hardware-driver
hard deadline is claimed. The wrapper's readiness deadline stops responsive
failed helpers. Capture/encoding/clipboard peak bytes include copies/resampling/
encoder and helper storage, not just the128MiB retained cache. Hardware memory,
clipboard manager/interoperability and codec/frame pacing remain measured gates.

## Reproducible CPU checks

```
python3 tests/capture/check_cpu.py --source /absolute/pinned/runtime/source
```

Runs15 actual domain/export/child-owner tests, builds and checks the actual helper,
rejects invalid PNG/arguments before any Wayland operation, and checks2 actual
native permission methods plus4 actual patched cache/reference methods. Native
protocol types in the harness are explicit fixtures: with_link always fails
closed, and cache tests use synthetic frame storage. These are not real freeze,
clipboard selection, topology events or GPU tests. All subprocess statuses are
recorded independently and propagated on failure. Evidence lives in tree-root
`.tmp/check-results/`; no desktop endpoints are supplied to parser tests.
