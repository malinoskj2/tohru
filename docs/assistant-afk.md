# Desktop idle dragon scenes

Status: episode policy, semantic choreography, configuration and the maintained
idle bridge are implemented; native composition and authored pose supplements
are in progress. The bar does not load AFK yet. Doctor deliberately reports an
enabled AFK setting as unsupported until native acceptance passes.

AFK is optional and default-off. Real desktop idle notifications qualify one
episode, with one seeded 50% chance and at most one finite scene. A failed roll,
unavailable pose, blocked or interrupted scene consumes that episode until
activity resumes. Helper reconnection/configuration does not grant another roll
while idle. Chat timestamps never qualify an episode. Explicit seed + session
episode sequence reproduces the chance and selection; the empty default seed
uses the helper's cryptographically random generation. No immediate scene repeats
occur when eligible alternatives exist.

The eleven semantic timelines last 11–14 seconds, except the 22.6-second corner
nap. Hidden pose preparation is separately bounded. The optional reduced-motion
rest lasts ten seconds without translation/zoom. Forced selection still applies
chance and every safety gate; explicitly set probability to 100 for capture.

## Settings

Use existing TOML, native Settings, `tohru config set assistant.KEY VALUE`, or
Home Manager `programs.tohru.settings.assistant`. Managed UI overrides remain
temporary; lifecycle helpers read declared configuration. Source/threshold edits
require a helper restart; Tohru-owned idle callbacks require its existing owner
to restart after the bridge is ready. Nothing is enabled automatically in this
branch.

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

Scene IDs: `screen_window`, `corner_nap`, `glass_inspection`,
`upside_down_peek`, `window_perch`, `tail_chase`, `sleepy_drift`,
`distant_flight`, `hide_peek`, `wing_grooming`, `waiting_for_you`.
`quiet_rest` is the separate reduced-motion option.

## Idle ownership and lifecycle

The pinned Pleamar 0.7.27 source and Tohru capability patch have no maintained
idle observer binding. No raw input devices are sniffed. Optional systemd
`tohru-afk.service` runs `tohru_afk_idle.py serve` with the compositor's
`ext_idle_notifier_v1` through maintained swayidle.

* `idle.owner="external"`: the passive client has only timeout/resume callbacks.
  `-C /dev/null` prevents loading a user's lock/suspend config. It never requests
  a logind delay inhibitor or registers lock/sleep commands. Existing desktop
  ownership remains intact.
* `idle.owner="tohru"`: the existing `tohru-idle` captures the bridge generation
  and adds callbacks to its one swayidle process. Missing bridge or threshold
  mismatch safely leaves AFK inactive until an explicit owner restart.
* `idle.owner="disabled"`: the bridge exits without observing.
* `afk_idle_source="callbacks"`: external owners can use
  `tohru_afk_idle.py callbacks` to obtain JSON with timeout, idle command and
  resume command for their maintained listener. This starts no passive client.
  Refresh these generation-capturing commands after bridge changes. Do not invoke
  them from timers that merely measure elapsed wall time.

The service uses existing generated-unit/Home Manager component conventions and
is not installed as an automatically enabled component. Systemd owns the helper,
passive observer and logind signal monitor until cleanup. Private per-session
socket registration rejects earlier-generation callbacks. Heartbeats establish
liveness only, never inactivity. Missing protocols, helper failure or unknown
session state disable AFK. Protocol inhibition belongs to the maintained idle
mechanism; logind signals invalidate session safety before a fresh observation.

Native arbitration must additionally preempt on input, lock, session loss,
voice/listening/pending turns, text entry, DND, source/config/output changes.
Fullscreen true or unknown excludes broad scenes. Window scenes require a
verified real target and hide/peek requires compositor occlusion; moving or
closing the target cancels stale geometry.

## Shared actor boundary

The concurrent contextual-flight child owns the single actor, input surface,
output geometry, compositor scene helper and low-level pose arbitration. AFK
must reuse it without writing flight facts or creating another surface. Stable
dependency SHA is pending. Agreed API:

* `actor.acquire(on_ready,on_stop)` returns a lease and eventually a verified
  output-local logical scene with optional real-window target.
* `actor.play(lease,sample)` rejects stale leases and reports preparing/ready.
  Stage the correct pose invisibly and wait before root travel.
* `actor.release(lease)` invalidates callbacks and restores ownership. Voice,
  geometry and activity preempt this lease; AFK releases for its other gates.
* `register_skin(path,rows)` supports an isolated 8-column, at most 12-row
  supplemental atlas; sample `pose="skin"`, `skin_frame` selects its frame.
  `register_closeup(path)` preserves a separate native-resolution source.

Policy is pure; semantic beats explicitly require authored actions. Unsupported
poses cannot be selected. The controller fences asynchronous source/pose
callbacks and pauses scene time until pose readiness. The composition caller
must resolve semantic anchors through the existing geometry adapter.

## Evidence and outstanding acceptance

`tests/assistant/afk.lua` covers replay, one roll, rearm/no-repeat, inhibition,
fullscreen unknown, reduced motion and cancellation. `afk-controller.lua` covers
cold-pose readiness and stale callback teardown. `afk-idle.py` exercises a real
private socket and process lifecycle with a stub client; that is not real desktop
idle evidence. Rust settings tests are added but compilation requires admission
unavailable at initial sandbox inspection.

Still required: stable actor dependency/composition, authored animation
registration, eleven native scene recordings, real idle/resume, chance/no-repeat,
interaction/reduced-motion and moved/closed-window captures, inspected frames,
gallery and preserved evidence manifests. No native, Rust or Nix pass is claimed
from policy/IPC tests.
