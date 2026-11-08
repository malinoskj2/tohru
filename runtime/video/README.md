# Integrated video renderer source

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

## Backend choice constraints verified during source investigation

The official [GStreamer NVIDIA H.264 decoder pad contract](https://gstreamer.freedesktop.org/documentation/nvcodec/nvh264dec.html)
lists CUDA/GL memory outputs; direct DMA-BUF export cannot be assumed for the
reference NVIDIA card. Thus the DMA-BUF candidate above is not yet a demonstrated
minimal hardware path. The official [DMA-BUF design](https://gstreamer.freedesktop.org/documentation/additional/design/dmabuf.html)
requires preserving per-buffer memory/plane metadata rather than treating every
sample as a flat RGBA upload. [AppSink](https://gstreamer.freedesktop.org/documentation/rust/stable/latest/docs/gstreamer_app/struct.AppSink.html)
can bound queued buffers and avoid EOS waiting; that does not itself establish
bounded decoder shutdown or cross-API synchronization.

Local installed mpv0.41.0 headers exist at
`/nix/store/sf4alw13ap5rzfhrr116zdq47q396c85-mpv-0.41.0-dev/include/mpv/`.
Its public render API is OpenGL or software; do not infer Vulkan render API support
from mpv's separate Vulkan video-output driver. The pinned Pleamar Cargo.toml
already includes dynamic khronos-egl6 and smithay-client-toolkit0.21. A focused
in-process libmpv renderer on per-output EGL background surfaces may avoid a new
GPU texture-import patch altogether, but requires agreeing the native surface
owner boundary with S: root owner submits assignments/visibility, renderer owns
only video surfaces/contexts/callbacks, and root teardown joins/stops that owner.
This is a proposed boundary, not permission to launch a separate daemon/player
or a tested implementation. Check codec/plugin availability, library ABI and
license packaging before committing concrete backend bindings.

## Approved concrete boundary and initial policy

Coordinator approved in-process libmpv0.41 render API on video-only Wayland/EGL
per-output surfaces. Reuse native EGL6/SCTK0.21/wayland-client0.31; the safe
wayland-egl0.32.11 window wrapper is the one requested additional dependency.
The earlier GStreamer discussion records rejected investigation, not the chosen
implementation. No mpvpaper/player process, custom decoder, screenshot loop or
second general shell renderer.

`mod.rs` is the root foundation's std/thiserror-only video policy. S exposes
`#[path = "../runtime/video/mod.rs"] pub mod video;` and copies its owning tests
under tests/wallpaper/video.rs during preparation. `Snapshot` bounds32 video
assignments/occupancy observations, validates literal output names/video paths,
and keeps actual root revision. Backend native code validates a whole snapshot
before publication. Disabled/disconnected/unassigned outputs stop their owner;
unknown/protected logind or dedicated-locker observations pause, as does unknown
or occupied visible workspace. Both observations Clear and occupancy Empty are
required for Playing. Dedicated locker state must describe its actual protected
role, never a different process-local lock.held. Missing observations are Unknown.

Root supplies accepted video projection and verified observations. Native owner
must coalesce updates, recheck revision before unpause, create paused/muted/looping
cover players, retire removed/reassigned/disconnected players and drop render
contexts before backend cores and EGL/Wayland objects. Failures back off100ms up
to10s. Policy tests execute27 protection/occupancy combinations, assignment
lifecycle/bounds and retry saturation; these do not instantiate a decoder.
`python3 tests/wallpaper/check_cpu.py` includes these4 tests (17 total at this
checkpoint), Clippy and existing scoped metadata/probe checks. No full native
build or GL/decoder role has been allocated or started.
