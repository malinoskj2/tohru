# Integrated video boundary investigation (no runtime implementation yet)

The approved scope is an in-process renderer using an established decoder/playback
backend, integrated with the shell's assignment/output/settings/lifetime owners.
It does not authorize spawning mpvpaper as the replacement owner, a separate
wallpaper daemon/repository, implementing codecs, or CPU screenshot/readback loops.

Source anchors at pinned Pleamar dc902feb:

- `src/platform/mod.rs` exposes no playback service (service/query/command dispatch).
- `src/dmabuf.rs:1–10,46–105` already imports BGRA/RGBA/NV12 decoder-style DMA-BUF
  planes into Vulkan/wgpu and contains `YuvToRgb` integration; use this native
  path before introducing another rendering backend.
- `src/platform/thumbnails.rs:78–118` exposes a `Frame` with optional on-card
  buffer; its `OnCard` private read/reader/skipped coordination is owned by native
  thumbnail capture, not a general decoder submission API. Do not impersonate a
  thumbnail or reuse its global frame cache as an undocumented playback API.
- `src/scene.rs` `PieceContent::Dmabuf`/`DmabufPiece`, consumed in `src/render.rs`
  and `Gpu::copy_dmabuf`, carries external buffers for compositor/window pieces;
  not exposed as a scene image/ordinary shell playback API.

A practical candidate is a minimal GStreamer decode pipeline feeding hardware
DMA-BUF frames through a narrow explicit image-source boundary, reusing native
import/conversion. This would need one focused existing Rust package, thin native
binding installation, dependencies/pin through S, bounded frame ownership and
native frame wakeup, and independent review. This is investigation, not a chosen
or verified backend: availability of hardware-backed export, correct format,
modifier, synchronization and buffer lifetimes must be established before code
commits promise the renderer. libmpv render API is another established backend;
its OpenGL rendering requires an interoperable texture boundary for the existing
Vulkan path, rather than a second general shell renderer.

Retained j2bar lifecycle anchors (`/home/jesse/projects/j2bar`, read-only):

- `src/app/wallpaper.rs:627–640`: only connected assigned outputs reach video;
  unlocked requires both logind false and trusted shell lock authority.
- `src/wallpaper/video.rs:259–297`: a snapshot reports visible-workspace occupancy;
  unknown occupancy is paused, as are locked/unknown-lock states.
- `video.rs:319–380`: player starts paused, muted, looping, cover crop, hardware
  decode automatic; never decode before visibility/unlocked state is confirmed.
- `video.rs:497–504,543–700`: stop/reap on removal/reassignment/shutdown, recheck
  observation revision before unpause, fail closed on disconnected observations
  and unacknowledged pause, bounded retry rather than a restart spin.
- Current source uses mpvpaper; the target implementation must integrate ownership
  here rather than copy that external-process ownership architecture unchanged.

W currently holds no decoder/thread/backend processes, display/bus/GPU/build slot
or live wallpaper ownership. Actual backend compile, frame rendering, decoder
pause/reassignment/hotplug/shutdown and memory/process/resource evidence remain
unverified. Root/shared native changes remain S-owned, with precise requested
patch boundaries required before simultaneous integration.
