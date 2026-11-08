# Native text notification role

Private entry: `pleamar --scene shell/notifications/notifications.plm --answer-as tohru-notifications`.
One dedicated native process owns org.freedesktop.Notifications, independently of
ordinary bar UI restart. Never import/start this owner from the root bar scene.
Production singleton launch/cutover belongs to S/Nix; no live takeover is enabled.
Native name replacement/queue policy is retained, so development must use private
buses/endpoints. No old notification daemon/policy implementation is ported.

The user accepted native internal expiration history (50 entries), without any
history/unread/DND UI or history subscription. Presentation is transient title/body
text in four plain banners, six seconds maximum, removed earlier on native close.
No app/content filters, buttons, inline reply, hover pause or countdown. Exactly
one fixed native sine tone (880Hz/80ms) is requested for each observed new/changed
notification, with the same parameters for every app/urgency. No custom sound
assets or per-app policy. Native sound backend owns playback/device availability.

UI work inspects at most64 most recent native active entries, keeps only bounded
fingerprints and at most4 timers/models; title/body bounds preserve UTF-8. Native
process-owned last-ID scene fact prevents replay across logic reload, and begins
fresh with a new process. Identical same-second replacements may be coalesced by
the native service and do not necessarily retrigger a tone; no event/backend patch.
Native active protocol bookkeeping/history remains native even after UI timeout.
There is no user-facing retained notification state.

Checks: `pleamar --check shell/notifications/notifications.plm` and
`lua tests/session/notifications.lua` execute scene parser and actual interaction
logic with native boundaries controlled. Real private Notify/replace/close/name
ownership/reload checks and captured presentation remain required; placeholder
tone parameters are checked, not claimed audible on physical hardware.

Power warning native events are `power_warning_clear`, `power_warning_low`,
`power_warning_critical`, `power_warning_empty`, each with one finite normalized
charge0..1. Fixed title/percentage text shares the same placeholder tone; repeated
severity is suppressed across logic reload, clear removes only the power banner.
No battery-triggered session/power action. B owns transition/hysteresis decisions;
S owns the exact native say permission/route, with no arbitrary text event.

Capture feedback uses the existing native **submit** event on text binding
`capture_result`, with a single command per result (native `emit` accepts only a
number). S injects this fixed-target callback; no new service/backend or second
text-write/event exchange is needed:

```lua
sys.call("say", "tohru-notifications", "submit capture_result " .. json.encode({
    status = status,
    message = message,
}))
```

Root permission is exactly `"say:tohru-notifications"`. Status is one of `saved`,
`copied`, `saved_copied`, `partial`, `error`; titles are fixed by this owner.
S/W classifies actual native export results, including independent save/copy
errors and durability warnings; request/spawn acceptance is not export success.
Message is plain valid UTF-8, at most512 codepoints/2048 bytes; NUL, DEL and other
C0 controls are rejected except tab/LF/CR. JSON payload maximum3072 bytes, checked
before decoding; the entire say command stays below native4096-byte limit.
Only status/message keys are accepted; malformed/oversized/unknown results leave
the current banner and tone unchanged. The sender must bound/clean backend/path
text before encoding, and handle native say delivery errors honestly.

Latest capture feedback replaces its own banner in the existing four-banner
budget and requests the same placeholder tone once. Native notification snapshots
and power-warning clear do not remove it. It lasts at most six seconds and stores
only its validated message/status/expiry in native scene text `capture_notice`;
logic reload restores remaining time without tone replay or timeout reset.
Expiry uses pinned Luau's Linux `os.clock()`/CLOCK_MONOTONIC, and is stored in text
JSON to avoid epoch/precision loss through32-bit scene facts. Eviction/expiry clears
that transient value, so reload does not resurrect removed results; process
restart starts empty. This adds no notification history/unread state. Existing
native application notifications and native50-entry history remain unchanged.

`lua tests/session/notifications.lua` checks actual handler/schema/bounds,
replacement, snapshot/power coexistence, timeout, eviction and reload behavior.
JSON codec, monotonic clock, timers and audio are controlled native boundaries in
that CPU test; it does not establish native IPC delivery, real export, native
JSON-parser behavior, renderer visibility or audio playback. No native/GPU run
was used for this slice.
