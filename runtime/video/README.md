# Integrated video source and verification state

This is the approved in-process libmpv0.41 render-API boundary on video-only
Wayland/EGL per-output background surfaces. It uses the existing native package,
EGL6/SCTK0.21/Wayland0.31 and the safe wayland-egl0.32.11 window wrapper. There is
no mpvpaper process, standalone service, command socket, custom decoder, CPU
screenshot loop, or second general shell renderer. Earlier GStreamer/DMA-BUF
investigation is superseded: NVIDIA CUDA/GL output cannot be assumed to export
hardware DMA-BUF; using libmpv's own GL render API avoids native Vulkan texture
interop changes.

Current evidence is **source/compile-only**. Actual pinned EGL/MPV/wake modules
pass an allocated offline Cargo API check and Clippy with jobs1. Policy/startup,
selected actual report/query methods and MPV wrapper behavior pass cheap cached
Rust fixtures; MPV tests supply fake SDK symbols. Full native actor/Wayland/root
assembly, linked library/driver/codec behavior, private pixels and shutdown/output
measurements remain gates. No video role/decoder/EGL context has been started.

## Ownership and entry points

S owns common manifests, native preparation, root settings/CLI/role bindings,
Nix/dependency pins and verified provider hooks. W owns these files and tests.

1. Root foundation exposes `#[path = "../runtime/video/mod.rs"] pub mod video;`.
   Copy that module and tests/wallpaper/video.rs preserving paths. This uses only
   std and existing thiserror2.0.20; foundation Rust1.85/dependencies stay intact.
2. Install native.rs as **native `src/tohru_video/mod.rs`**, and
   egl.rs/mpv.rs/wake.rs/startup.rs/wayland.rs beside it. Add `mod tohru_video;` in
   native src/lib.rs, gated to Linux. Native direct dependencies: existing anyhow1.0.103/libc0.2.189,
   EGL6/SCTK0.21.1/Wayland0.31.15/protocols0.32.13, thiserror2.0.20,
   wayland-egl0.32.11 and libmpv0.41 dev/link/runtime closure. Backend dependencies
   do not enter foundation. S owns exact full transitive lock preservation.
   Native cfg(test) also needs tests/wallpaper/{mpv,video_startup}.rs copied to
   native tests/wallpaper/ so the installed module test paths remain valid.
3. Apply **wallpaper/catalog.patch before video/status.patch**. The latter adds
   only readonly `video.status` dispatch. A scene needs base `video` read
   permission; no public video publishing/command service is added.
4. Root's existing accepted-settings/projection owner calls
   `tohru_video::publish(snapshot, to_render.clone())`. Snapshot contains only the
   accepted video output→path projection, wallpaper enabled flag, strictly
   increasing **video observation** revision, verified logind and actual dedicated
   locker `Protection`, and verified visible-workspace occupancy. Filter provider
   occupancy to relevant assigned outputs (max32); missing/lost/unsupported
   observations are Unknown. Never substitute root-local lock.held for another
   role's actual protected state, or false for an unavailable provider. Update on
   provider changes/recovery/loss as well as config acceptance.
5. Root declares outgoing `wallpaper_video_changed` (own wallpaper/state.plm).
   Actor sends the existing native ExternalSignal; controller's optional second
   argument is a status getter, e.g. `function() return sys.ask("video.status") end`.
   Instantiate it in production and keep root permission base `video`. Actual
   query acknowledges the one coalesced pending notification. Static/private
   entries omit the callback safely. Source fields wallpaper_video_status and
   wallpaper_video_error are independent of assignment diagnostics; root UI shows
   actual per-output error/protection/pause information rather than claiming
   accepted config proves rendering. Callback reads immediately on initialization
   and exposes refresh_video for root adapters/reload recovery.
6. S forwards `tohru_video::shutdown()` during orderly root teardown before
   platform destruction and reports errors. This stops/joins the owner and its
   bounded startup watcher and retires every owned backend. Keep role authorization
   at the root binding; other roles/plugins must not submit accepted snapshots.

## State, bounds and failure behavior

Snapshot validates max32 assignments/occupancy records, literal native output
names, absolute UTF-8 paths≤4096 and retained video extensions mp4/webm/mkv/mov/gif.
Disabled/unassigned/disconnected outputs stop their core/surface. Connected players
start paused, muted, audio/subtitles/config/scripts/input/reference/paired-file
loading disabled, loop-file inf, cover panscan1 and hardware decode automatic.
Both actual protection providers Clear and known Empty occupancy are required for
continuous playback. Unknown/protected/occupied states pause; status preserves
actual source Unknown/Clear/Protected and requested/observed pause separately.
libmpv may predecode its initial paused frame, as the retained paused-player
backend does; presenting/resizing that frozen frame does not enable playback.

Root updates replace one immutable Arc snapshot; steady frames do not clone its
path map. One nonblocking coalesced wake and one pending renderer notice avoid
update/frame queues. Each player has at most one pending pause command; a safety
pause superseding pending unpause retires instead of growing a command queue.
Acknowledgment deadline500ms, file startup10s, layer configure2s and registry
bootstrap2s. Fresh private connection cancellation is allowed only before GL;
Ready removes the cancellation socket. The video connection uses explicit
WAYLAND_DISPLAY/XDG_RUNTIME_DIR and SDK from_socket; it never consumes or mutates
inherited WAYLAND_SOCKET that could belong to primary rendering. FD-only contexts
without an explicit named endpoint fail honestly (existing secondary native
capture connections also need an available endpoint after primary consumption).
Each retry has one owned/joined SDK bootstrap watcher; no watcher is detached.
A responsive connection/bootstrap failure retries with bounded backoff. While no
video assignment is enabled, the owner parks without opening a connection.
Disabling/removing the last accepted video drops the complete private backend
graph/connection before parking, while keeping the root-facing owner available
for a later accepted enable/assignment snapshot.
Snapshot updates wake the owner but cannot bypass a pending failure deadline.

One native frame callback per output paces commits, EGL swap interval0 avoids
waiting for an occluded surface inside a blocking swap, SDK frame timestamps
control target time. Wayland owns output/layer add/update/remove/close and empty
input regions; keyboard none/reserve-1/background avoid shell input/reservation.
Viewport uses upright physical mode/logical ratios, including fractional scale
and90/270 rotation. For this all-anchor full-output role native configure gives
logical extent when xdg-output metadata is absent. Without viewporter, actual
native integer buffer scale is used. Frames are capped32Mi pixels. libmpv forward
packet-cache budget32MiB/back-cache8MiB use native options; the SDK documents
small packet-limit overruns and separate decoder buffers, so these are not a hard
process/RSS/VRAM cap. Aggregate media memory/decoder concurrency remain measured
gates; no custom packet/frame copying decoder is introduced.

Path replacement/removal/hotplug retires old player before reuse. Responsive
failures back off100ms..10s; retries clear only after30s healthy ownership, not
merely successful async load/configure enqueue. Root revision is rechecked before
unpause/presentation. Report separates running/paused/pausing/failure/unknown and
retained handles; shutdown remains queryable as stopping until join completes.
Render context frees before final core termination, callback boxes outlive core
threads, then EGL surface/window/viewport/layer/context/display/connection follow.
If original GL cannot be rebound or foreign destruction fails, dependent objects
are retained rather than freed unsafely, best-effort terminal pause is explicitly
unacknowledged, status reports cleanup unproven/retained cores, and new video reuse
is blocked. Fatal cleanup sends the existing native ToRender::Quit to terminate
the containing owned root role, so OS teardown can reclaim retained dependencies.
This binding must never be installed as a publisher in the locker or other roles.
A failed core is not reported
as still Playing; unavailable pause is omitted, not false.

Native kernel/filesystem connection operations or opaque EGL/codec/driver calls
can delay shutdown; no hard kernel/driver termination deadline or immediate GPU
memory deallocation is claimed. False cleanup_unproven means no unproven teardown has been observed; while the
owner is running, it does not mean teardown has occurred. A normal Stopped report
is owning API evidence, not a GPU-fence/VRAM measurement. Responsive normal cleanup,
error paths and actual hardware behavior still need private runtime evidence.

## Reproduction and precise gates

- `python3 tests/wallpaper/check_cpu.py`: scoped catalog/probe and pure video policy.
- `python3 tests/wallpaper/check_video_source.py`: cached dependency rustc/Clippy,
  no Cargo graph.6 policy +3 startup +9 report/query/supervisor +7 fake-SDK MPV cases
  (report harness also repeats policy/startup). Explicit controller holder and
  renderer enum fixtures do not exercise native protocol/actor startup. The two
  newest inactive-report/re-enable supervisor cases and latest supervisor changes
  are source-only and have not been executed; the earlier report harness had7
  passing cases. No current9-case pass is claimed.
- `python3 tests/wallpaper/check_video.py --headers /absolute/mpv0.41/include`:
  compile-only SDK ABI/layout/signatures plus fake-SDK actual wrapper tests.
- `python3 tests/wallpaper/check_egl.py`: **allocated heavy slot only**, actual
  EGL/SCTK/Wayland API fixture, offline cargo check/Clippy jobs1/owned target;
  explicit viewport type fixture. Completed0/0; slot released, no native execution.
- `python3 tests/wallpaper/check_native_video.py`: **new allocated slot only**,
  prepared full actor check/Clippy with exact existing direct pins. Not yet run;
  SysValue/ToRender remain explicit framework type fixtures. SDK's production
  transitive lock/full native root/feature-mode assembly is S's separate check.

All logs/argv/exit/source hashes live in worktree-root .tmp/check-results; no
check success infers a runtime result. Actual gates: linked ABI/backend options,
codec errors/GIF/loop/cover, hardware decoding, paused first frame and visibility
revision races, actual dedicated locker authority and provider loss, physical
scale/rotation, surfaces/input/hotplug/assignment churn, process/thread/fd/decoder/
GL/Wayland cleanup and GPU memory/pacing. Only coordinator-allocated private
sandbox displays/buses/GPU tests may run; no live wallpaper/service takeover.

## Attribution and package license

Local SDK0.41 public client/render headers are ISC; preserve
MPV-CLIENT-LICENSE with the package/these bindings. The client header explicitly
says core is GPLv2+ by default unless built with gpl=false; installed AppStream
metadata reports GPL-2.0-or-later AND LGPL-2.0-or-later. These header notices do not
relicense the core/FFmpeg dependencies. S must select/preserve the actual library
build's licenses/notices/source attribution with its exact compatible pin, rather
than assume the wrapper notice covers it. Offline derivation flags were not
available (known .drv missing); no substitute/fetch/build was started to obtain
those flags. No claim about actual core configuration or redistribution terms is
made from version alone.

Retained behavior references, read-only j2bar:
app/wallpaper.rs627–640 and wallpaper/video.rs259–297,319–380,497–700. Those use an
external mpvpaper; this implementation retains assignment/protection/occupancy/
paused-loop-cover/retirement behavior within the existing native process instead.
Pinned Pleamar source is dc902feb/0.7.27. Existing DMA-BUF/thumbnail internals are
not impersonated or exposed as a general video image submission API.
