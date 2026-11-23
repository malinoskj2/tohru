# Notifications

Users see incoming toasts, open Notification Center, mark items seen, clear them
and toggle Do Not Disturb without a second desktop notification owner.

## Sub-features

- `notifications-arrival`: real private-bus sender produces a toast and unseen dot.
- `notifications-center`: open/close/toggle and seen/clear.
- `notifications-dnd`: CLI and bell menu change Do Not Disturb.

## How to get to it (user POV)

- Receive a desktop notification; left-click the bar bell for Notification Center.
- Right-click the bell for Do Not Disturb.
- Run `tohru notifications open|close|toggle|seen|clear` or
  `tohru notifications dnd on|off|toggle`.
- Run `tohru panel open notifications` or `tohru panel toggle notifications`.

## Driving it with the native session harness

Preconditions: an owned private D-Bus notification daemon, private compositor and
private Pulse null sink, patched runtime, `gdbus`, `pactl`, `pulseaudio` and the
suite's loader setup. Read `tests/session/native.sh`/`native.py` and its arguments.

- **Existing harness.** Start `tests/session/native.sh --runtime "$verify_runtime"`
  with `--notifications --pulseaudio "$(command -v pulseaudio)" --gdbus
  "$(command -v gdbus)" --pactl "$(command -v pactl)"` under an exclusive GPU
  lease. Its private sender must be an actual D-Bus client; do not claim host
  notification integration from fixture state alone.
- **Proof.** Retain sender call, daemon response ID, visible toast/center, unread
  transition and DND state. Each CLI alias and bell input entry requires its own
  observation; a successful command alone is insufficient.

## Gotchas

- This seed did not execute the notification harness. `drive.py` starts only the
  bar; its private bus has no notification owner. Those paths remain blocked there.
- The session harness uses recorded fake power commands; those records cannot
  establish real lock or suspend behavior. Never substitute live power commands.
- The driver must not claim notification UX from a calendar screenshot or a fact
  injected into a scene. No host D-Bus owner is stopped or replaced.
