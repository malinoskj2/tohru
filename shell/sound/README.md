# Native sound controls

Root scene `tohru` uses one existing native `sys.watch("audio")` whole-report
subscription and the supported output/input/app commands. Each report fills the
plain audio facts and bounded native models, reconciles the selected ID/index,
and publishes CLI status/OSD once. It does not also declare `service audio` or
start an additional observer, high-rate `audio.level` stream or manual poll.
Native HUBS reuses the same owner/name/watch subscription across VM reload and
replays its last report. The native audio backend still collects its normal
input/app fields while closed; this view adds no extra closed-panel stream.

When migrating from an older declarative-audio scene, restart the ordinary shell
process once: native HUBS distinguishes the old `service:<alias>` tag from `watch`
and retains a superseded hub until that process exits. A VM/scene reload alone
cannot stop the old hub. No live desktop activation or takeover is performed here.

The bar sound icon opens a native xdg popup. Native placement anchors its 300px
panel at the sound button center minus150px, 6px below the31px bar; native slide
constraints keep it onscreen. Sound header uses j2bar15px/600 type,6px padding,
40px header and30px row; SVG speaker paths are from j2bar's OSD speaker with its
MIT notice retained. Mocha text/sky palette follows the source. This is a native
glass/interaction candidate using the accepted native effects approximation;
no claim of1:1 graphics/motion or glass pixel parity.
The More control exposes native output selection, microphone level/mute and
per-stream application level/mute in a paged view.

Public CLI, reaching only an already running root scene:

```sh
tohru panel toggle sound
tohru panel open sound
tohru panel close
tohru audio volume up
tohru audio volume down
tohru audio mute toggle
tohru audio status --json
```

Volume keys move5%, preserve mute, and native service clamps writes to0..1.
Slider writes are bounded0..1, coalesce at most one pending50ms timer, flush the
last value on release/close, and never poll. Observations stay authoritative;
status distinguishes awaiting_report from last_report, missing from zero/unmuted,
and command errors from successful native calls. The status is a last observation,
not a promise of live server health; pinned native audio has no disconnect status.
No volume key claims asynchronous system completion merely from CLI acceptance.

Tab selects mute/slider; arrows adjust5%, Return/Space on mute toggles, Escape
closes. Parent asks exclusive keyboard only while panel is open and releases it
on close. Native pointer-open popup grabs use a recent real pointer serial;
outside click then follows compositor popup_done. CLI-open without recent pointer
serial has no native outside grab and may require Escape. This native behavior
is accepted by AUTONOMOUS_DECISIONS.md; outside-click parity is not claimed.
No layer catcher, serial workaround or native patch added.

User accepted native every-output bars, superseding preferred single-output
policy. Root now uses screens:all, which also permits native root popup compilation;
no singleton-each compiler fix. bar.preferred_output is deprecated/inert legacy
configuration; preserve old TOML instead of deleting it. No fallback selector.

The accepted native OSD approximation uses44×200 glass/spring/velocity/fill and
1600ms hold. It reports output/microphone levels, mute and native device names;
custom randomized Slosh and pixel parity are not claimed.

CPU tests in tests/sound/logic.rs execute the real controller using mlua at the
native-call/timer boundary; GUI/native service/input tests require an exclusive
private compositor/GPU slot and private PulseAudio null sink, never live audio.

B clock/calendar parts are integrated in the same root scene; no standalone
calendar process is started, so a real clock click retains root popup serial.
Root panel-close closes both panels and opening either closes the other.

Follow-up: native false-open transitions and toggle/explicit closes flush/cancel
pending writes and reset dragging idempotently. Logic reload cancels unissued
pending drag work and clears presentation drag state. Command failures now render
in a growing error area, separately from awaiting native observations.
User accepted the pinned synchronous Pulse I/O limitation: a stalled accepting
server can block root logic indefinitely. The50ms timer bounds coalescing only;
no command deadline/cancellation, backend replacement or workaround was added.


## Expanded native controls

Output devices are actionable using the observed native sink ID. Microphone
level/mute use the actual native default input. Input devices are deliberately
read-only: pinned `audio.default(id)` checks sink IDs before source IDs, and common
IDs collide. The accepted choice is an honest unavailable input selector, with no
name guessing, custom observer/backend or Pleamar patch. No native default input
report disables microphone commands rather than targeting a fallback implicitly.

Applications remain native per-stream reports, capped64 (devices32 each), four
rows per page. Names/titles, level, mute and playing come from the native service;
there is no aggregate app model or alternate process/audio catalog. UI events
carry native IDs, which must still exist in the current bounded report before a
command is issued. Explicit commands for removed stream IDs fail visibly. Keyboard selection
preserves its native ID through reordering, moves its outline/page with that ID,
and clamps/reselects the current row when the identity disappears. Whole-report
updates refresh same-count name/title/volume/mute/playing changes too. Native request
acceptance remains distinct from subsequent state observation. Device/app status
is included in schema2 `audio status --json` for CLI ID discovery, with text fields
bounded256 UTF-8 bytes and explicit capacities/selection limitation.

Default output and microphone sliders share one50ms coalescing slot; switching
kind flushes the previous value, close/release flushes the final value, and VM
replacement cancels unissued work and clears both drag flags. App buttons change
5% and toggle mute using explicit native commands; all writes clamp0..1. No peak
meter, overdrive, balance or game policy is added. The existing native audio
backend already reports input/app data even while closed; this view does not add
an `audio.level` observer or a recurring query/poll.

Expanded keyboard: Tab cycles Outputs/Input/Apps, Up/Down selects rows (or changes
microphone level), Left/Right adjusts the default output/input or selected app,
Enter/Space selects an output or toggles microphone/app mute, PageUp/PageDown
pages the bounded list, Escape closes. Model changes invalidate stale ID actions.

```sh
tohru audio input volume up
tohru audio input mute toggle
tohru audio output select 0
tohru audio app 7 volume down
tohru audio app 7 mute toggle
```

These are fixed, validated native requests. Input selection is rejected before
IPC; output/app IDs must be canonical unsigned32-bit integers. Mic SVGs retain
Lucide's license under LICENSE.LUCIDE. Controller/CLI/compiler checks cover source
and API mappings only; private Pulse input/apps/hotplug/input/visual evidence
remains pending an exclusive native workload allocation.

## Whole-report sound/OSD presentation boundary

The volume vessel consumes the sound controller's already-published native audio
report; it does not create a second watch, issue commands, or poll. A same-ID
device rename refreshes a visible vessel's label without reopening it or extending
the hold. Hidden metadata updates do not show an OSD. Explicit hide, panel-open
suppression and disappearing devices clear pointer-hover presentation state so a
later observed level/mute transition gets its finite hold. A queued enter after
hide is ignored. Native commands alone never display an optimistic new level.

`tests/sound/osd-integration.lua` executes both production controllers against a
plain model/fact boundary, one native-watch fixture and cancellable timer fixture.
It verifies native report ordering, rename/mute/device/disappearance, hover/hide,
command acceptance/error and reload baseline behavior. These are CPU source
checks, not real Pulse/input/compositor/pixel/resource measurements. Native audio
I/O can still block root logic indefinitely, as accepted.
