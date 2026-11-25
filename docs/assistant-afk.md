# Desktop idle dragon scenes

AFK is optional and default-off. Eleven finite native scenes reuse the existing
single desktop actor: screen-window camera approach, corner nap, glass
inspection, upside-down peek, real-window perch, tail chase, sleepy drift,
distant flight, genuine window hide/peek, wing grooming and waiting at the lower
edge. The separate `quiet_rest` is stationary and optional with reduced motion.
No microphone, browser simulation or 3D worktree asset is involved at runtime.

Every maintained desktop idle episode gets at most one seeded chance (50% by
default) and one scene. Eligibility precedes selection; failed rolls, unavailable
poses and interruption consume the episode. Alternatives exclude the previous
successful scene. Visible timelines normally last 11–14 seconds; nap is bounded
at 22.6 seconds. Hidden preparation has separate five-second deadlines; every
actor lease has a 55-second absolute limit.

## Configuration

Use native Settings, `tohru config set assistant.KEY VALUE`, writable TOML or
Home Manager `programs.tohru.settings.assistant`. Include `afk` in the existing
optional service-component selection to install `tohru-afk.service`. The bar
controls it through systemd when enabled; no host setting was activated during
implementation.

| Key under `[assistant]` | Default | Values |
| --- | --- | --- |
| `afk_enabled` | `false` | Boolean |
| `afk_idle_seconds` | `300` | Integer 1–86400 |
| `afk_probability_percent` | `50` | Integer 0–100 |
| `afk_seed` | `""` | Empty or decimal text 0–2147483646 |
| `afk_scene` | `"auto"` | Auto or a scene ID |
| `afk_scenes` | `"all"` | All, none, or comma-separated IDs |
| `afk_quiet_rest` | `false` | Small rest with reduced motion |
| `afk_idle_source` | `"passive"` | Passive or callbacks |

IDs: `screen_window`, `corner_nap`, `glass_inspection`,
`upside_down_peek`, `window_perch`, `tail_chase`, `sleepy_drift`,
`distant_flight`, `hide_peek`, `wing_grooming`, `waiting_for_you`,
and the separate `quiet_rest`. Forced selection still applies chance and every
safety gate; explicitly set probability to 100 for preview.
`tohru afk status --json` and `tohru doctor` expose source state and the last
observed output's scene-availability reasons.

Managed Settings edits retain temporary runtime semantics. Validated source
settings reach the helper through a private atomic session file. Policy-only
edits do not restart an idle owner. Source/threshold changes refresh the helper;
in explicitly Tohru-owned mode only, `try-restart` refreshes an already active
`tohru-idle`. It never starts a lock owner or touches an external manager.

## Real inactivity and lifecycle

The pinned Pleamar 0.7.27 source has no maintained native idle-observer binding.
The helper uses compositor `ext_idle_notifier_v1` notifications through
maintained swayidle, never chat timestamps, elapsed-time guesses or raw input.

* `idle.owner="external"`: the passive client has only timeout/resume commands.
  `-C /dev/null` prevents loading a user's lock/suspend config. It registers no
  lock, unlock, before-sleep or suspend handlers and takes no sleep-delay owner.
* `idle.owner="tohru"`: the existing idle process captures generation,
  manager PID/start time and threshold and receives the callbacks. No second
  observer is started.
* `idle.owner="disabled"`: AFK observes nothing.
* `afk_idle_source="callbacks"`: use
  `tohru_afk_idle.py callbacks --manager-pid PID` for the current timeout and
  generation/PID-bound commands. PID must be the live maintained external idle
  manager; refresh after manager/bridge restarts. Unknown/dead identities are
  inactive. Configure a real maintained listener, never a wall-clock timer.

Protocol inhibition is delegated to the maintained compositor mechanism, as
[the swayidle maintainer describes](https://github.com/swaywm/swayidle/issues/194).
Logind signals invalidate safety before a bounded active/unlocked observation.
Unknown protocol, session, output scale or fullscreen state suppresses affected
scenes. Broad scenes require fullscreen explicitly false. Voice/listening,
pending conversation, Tohru text entry, DND, input, lock, config/source/output
changes preempt AFK. Other applications' text-field focus is not inspected.

The helper keeps its observer/monitor in the foreground through cleanup.
Systemd `KillMode=mixed` lets it invalidate callbacks before terminating its
children, then removes stragglers. [Swayidle also invokes resume commands on
shutdown](https://github.com/swaywm/swayidle/releases); those are not input.
Stopped/dead callbacks are rejected and persistent rearm waits for source
liveness. Source, native lease, pose and configuration generations are fenced.

Private `afk-episode.json` preserves a cryptographically chosen default seed,
episode number and consumed idle flag across helper/VM restarts. Explicit seed
plus episode replays the chance/selection streams. A resume missed while the
source is down is treated conservatively: no new gag is guessed until an actual
maintained resume is observed. Late callbacks cannot resurrect a cancelled
presentation. Real logind/suspend and external service acceptance still require
a real login session.

## Art, resolution and loading

The separate `afk.webp` has 8×45 cells of 240px (1920×10800), leaving the
production body atlas intact. Its first 12 rows hold canonical flight frames and
authored nap, perch, tail/contact keys and a stretch; normal rendering is capped
at 236 physical pixels, so 240px cells are native. The next 12 rows hold six
generated 16-frame in-between sequences: a side-to-side body turn, side-to-front
arrivals from each direction, gliding down into the curled nap, landing on a
ledge with the tail dropping and banking into a climb. The last 20 rows hold the
hide-and-peek edge clips: 48 frames each for the head-first and paw-first peeks
and 64 for the pull-out; a final row holds the horned wave and hover poses. Adjacent frames crossfade, so a sequence stays fluid at
any playback rate; the denser edge clips step frame to frame instead, so quick
moves stay crisp instead of doubled.
Continuous native WGSL moves contact paws, wings, horn rubbing, tail and breath;
these are not static poses translated around a path. Quiet rest freezes frame,
root, zoom, bob, bank, squash and effects while ordinary voice lip/blink ownership
continues normally.

Giant scenes use dedicated 4×4 grids of **1254×1254 cells**, never enlarged
atlas cells: `afk-closeup-motion.webp` is a generated two-wingbeat hover loop and
`afk-bust-motion.webp` a generated bust loop (glances, blinks, a widening smile,
a wing flutter) whose first frame is the canonical bust. Each grid is 96 MiB
decoded; the glass and upside-down scenes bind both, and AFK art binds only
while a scene plays. The canonical 1254px PNGs remain the sources. Actual
dimensions, alpha-occupied bounds, conservative head annotations, hashes, prompts
and registration are in `tools/dragon/art/afk/`. A requested 3840px generation
returned 1254px: no 4K detail is claimed. Registered head detail (including horns)
is conservatively 580×610px for full body and 1020×1000px for bust; transparent
padding is not counted as character detail.

Camera uses one full-body source through approach/gaze/retreat. Upside-down
peek uses the same two sources as glass: she climbs out of view with the full
body, then the bust hangs from the top edge rotated half a turn, its chest crop
kept above the output. Glass stages
both needed sources before approach, then transitions from full-body flight to
head/paws using eye registration (1.9245×, 4.257°); the mouth agrees within six
source pixels. The body hides before its physical canvas exceeds 1254px; the
bust's intentional chest crop is framed below the output edge before it appears.
The small temporary stylized breath sheen writes no desktop content.

Flights follow eased arcs with wingbeats, banking toward the direction of travel
and a flare on arrival; the camera approach first swings out into the room and
then swoops in, with perspective scaling. A change of facing plays the
generated body turn, or the generated arrival into a front hover, rather than
flipping in one frame; rising arcs pitch her nose-up through the generated
bank frames. Corner nap glides down and curls through generated frames and
uncurls through them in reverse; the perch lands and takes off through them;
in hide-and-peek she peeks head first around the window's right edge and grips
it, glances along it and ducks back; at the left edge her paw pokes out and
grips first, her head follows, and she pulls herself out past the edge with a
little overshoot into a wave. Every
scene ends with a flight back to her resting spot at the bottom-right, turning to
face the front as she arrives, where the resting dragon takes over. While the
bust is shown, her bob only presses into the output edge so its crop never lifts
away from it. On a window perch she sits on the back of the top edge and her tail
drops behind the window.

| Real output case | Camera physical canvas | Glass and upside-down physical canvas |
| --- | --- | --- |
| 1920×1080, scale 1 | 1242; available | 1080; available |
| 1920×1080, scale 1.25 (1536×864 logical) | 1242; available | 1080; available |
| Left 1080×1920 rotated, scale 1.25 (864×1536 logical) | 2208 required; disabled | 993.6; available |
| 3840×2160, scale 2 (1920×1080 logical) | 2484 required; disabled | 2160 required; disabled |

At 1080p these head budgets render around 574×604px (camera) and 878×861px
(glass). Portrait glass head width is around 808px, 75% of the short edge.
These are registered-head estimates, not opaque face segmentation. Normal scenes
remain available when giant assets are inadequate; no tiny substitute is shown
for a disabled closeup. Window scenes additionally require verified visible
edges and real compositor occlusion; moving/closing invalidates stale geometry.

Supplemental declarations use `0,0`: pinned `text.rs::image_px/own_px` reads
native raster dimensions. GPU group layers use physical surface dimensions and
transformed bounds. Native PNGs and unscaled crops verify presentation; thumbnails
are not resolution evidence. The runtime exposes no GPU-ready/free acknowledgement:
a conservative three-second warm barrier precedes travel, and all live bindings
clear after retreat/cancel/disable; renderer caches may remain.

Supplemental RGBA estimates are 79.1 MiB normal, 96 MiB camera, or 192 MiB for
the two grids the glass and upside-down scenes need. Add existing voice textures (roughly 57 MiB at scale 1,
89 MiB at 1.25, 228 MiB at 2), flight texture, surface buffers and conservatively
four physical shader layers (31.6 MiB at 1080p; 126.6 MiB at 4K). Decoder
temporaries, mips, fonts, drivers and caches are additional. These are estimates,
not measured VRAM deltas; native captures reserve 2 GiB through the scheduler.

## Evidence and limits

Lua tests cover deterministic chance/no-repeat, gates, persistence sequences,
source/control ordering, cold readiness, priority and stale callbacks. Python
tests cover real private sockets/children including shutdown resume, effective
managed config and retained art registration. Native recordings use the actual
AFK/actor/render modules and real compositor windows. Forced scenes are labelled
separately from real private-seat swayidle timeout and injected-keyboard resume.
No microphone or host display is used.

Recordings, original PNGs, unscaled crops, traces and manifests are in the
worktree `.tmp/`. Selected intermediate frames and timing traces do not certify
every frame: continuous motion remains for human review. This is an illustrated
puppet with discrete drawn posture handoffs and modest facing angles, not 3D
skeletal acting. Rust/Nix builds, rebuilt native Settings, real systemd/logind
lock/suspend and non-Hyprland acceptance were not run when admission/session
capabilities were unavailable. The task report identifies the exact source
revision, recordings, gallery and remaining acceptance gates.

## Generated in-betweens

`tools/dragon/afk-motion/` generates the in-betweens with the shared `h3-fl2va`
first/last-frame video model from `$MODEL_ROOT`, inside one exclusive GPU
reservation: `generate.py prepare`, then `generate.py run`. Each sequence
animates between two canonical keys on a uniform background (`sequences.json`
holds the prompts and seeds). `matte.py` extracts the character, picks 16
frames at equal steps of motion (loops at equal time) and puts the exact
canonical keys back at both ends; the 1254px grids are first upscaled with the
shared `realesrgan-x4plus-anime`. `collect.py` writes the strips and grids with
`tools/dragon/art/afk/motion/provenance.json`, and `afk-pack.py` packs the
atlas.

The hide-and-peek edge clips are generated around a flat gray stand-in for the
window, so the model draws the occlusion itself: her hidden body is simply
absent and the fingers gripping the edge overlap its front. The matte removes
both backdrops, keeps that occlusion, and crops a fixed cell whose stand-in edge
the runtime places on the real window edge, drawing the clip in front of the
window, mirrored for the head-first peek at the right edge. One model-drawn peek,
`tools/dragon/art/afk/edge-peek-source.png`, joins the head-first, paw-first
and pull-out clips. The peek's two tall horns stay on her through the pull-out
and the wave: its end and its overshoot guide are horned versions of the
canonical wave and reach keys (`horned-56.png`, `horned-24.png`, image
edits that only replace the swept crest), and the wave holds that horned pose.
It settles first into a horned copy of the front hover (`horned-8.png`), so the
paws move without her head changing, and only then do the horns fade into the
crest of the shared hover her turn home starts on.
