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
