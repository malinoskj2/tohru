# Notifications

The notification daemon, run as its own process, `tohru-notifications`, by `bin/tohru-notifications-run`. Pleamar's `notifications` service owns `org.freedesktop.Notifications` on the session bus, so no other notification daemon may run. The bar never starts it.

Its toasts are the Capillary Bevel design: glass cards in the top-right corner, just under the bar, newest at the top. `notifications.position` (see [docs/configuration.md](../../docs/configuration.md)) can put them back in the bottom-right corner, newest at the bottom.

## Files

| File | Contents |
|---|---|
| `notifications.plm` | A `404 × 620 px` overlay surface anchored to the top right of every output (`screens: all`), or the bottom right, holding one popup of the same size, so the glass gets the `bright:` reading that `shell/panel-ink.plm` turns into light or dark glass. Up to three `374 px` toasts, each its own copy of `Toast` with springs for arriving, moving along the stack and the capillary. Permissions: `notifications`, `notifications.*`, `sound`, `files`, `files.write` and `say:tohru` |
| `pill.plm` | The `294 × 46 px` Do Not Disturb corner pill, with a still ribbon rail (`shell/bar/ribbon.wgsl`) |
| `bevel.wgsl` | The capillary and the critical refraction, confined to the rim |
| `notifications.luau` | Turns the service's notification list into toasts, keeps their time, plays the tone, and keeps unread and Do Not Disturb for the bar's bell |

The glass is the sound popup's: a lens `body`, `shell/sound/fill.wgsl` for the drifting palette light, and the 0.6 px hairline at 26 % (16 % on light glass).

## Toasts

- **Card.** The app icon (`app_icon`, or a bell when there is none) and the app name; the arrival time as a `26 px` Ultralight numeral at the top right; the summary in bold on one line; the body in up to three lines; and the image (`image-data` or `image-path`) as a `46 px` rounded thumbnail at the right. Summaries are cut at 256 characters and bodies at 768. Hovering shows a close button beside the numeral.
- **Expiry.** A thin light line runs from the top-left corner down the left edge, round the corner and along the bottom bevel. What is lit is the time left, so its head drains down the left edge and then along the bottom to the bottom-right corner, coloured from the workspace ribbon's five stops by position and time. The time is the server's: Pleamar reports each notification's `timeout` and `left` (patch `0025-notification-hold`), and the logic redraws the line ten times a second while one is counting, smoothed by a spring. Hovering a toast stops its line and its expiry on the server (`notifications.hold`); leaving lets it run on from where it stopped. A notification that does not expire (sent with a timeout of 0) has no line.
- **Actions.** Up to three actions sit in a flush segmented footer, `30 px` tall, the first at full ink and the rest at 66 %. The `default` action is not a button: clicking the card invokes it. Invoking an action or closing a toast closes the notification (reason 2).
- **Critical.** An urgency-2 notification refracts rose into peach along its whole bevel, its numeral is `36 px` in the same rose, `· CRITICAL` follows the app name, and it has no line, because the server never expires it.
- **Stack.** At most three show, `9 px` apart, the older two at 93 % and 84 % opacity. At the top right the newest is at the top, `6 px` under the bar's glass, the gap the bar's popups open at (the surface sits below the bar's 31 px reserve), and older ones go below it; at the bottom right the newest is at the bottom and older ones go above it. A fourth pushes the oldest out of view; it stays on the server. A replacement (`replaces_id`) changes in place and keeps its position.
- **Arriving and leaving.** At the top right a toast drops `28 px` from under the bar with a critically damped spring and rises back under it as it fades out; at the bottom right it slides in `28 px` from the right and fades out the same way. Each toast keeps one of four model slots for its life, so the springs follow the toast and never another one's content.
- **Bar popups.** The bar's popups (sound, network, Bluetooth, calendar, media, wallpaper, info and about) open under the bar at the top right too, and the toasts are on the overlay level above them. While any of them is open the toasts and the pill fade out, their zones stop taking the pointer, and after the fade the glass popup closes, so nothing covers the popup's controls; their lines stop as if hovered, so they come back with the time they had. A notification that arrives meanwhile plays its tone and waits the same way. The bar sends `notices_cover` when the first popup opens and the last closes. Hiding was chosen over shifting the toasts left: the calendar is 446 px wide, so a shifted stack would sit in the middle of the screen, and the daemon would need each popup's geometry from the bar. At the bottom right the popups do not reach the toasts, which stay.

A toast leaves when the server's list no longer has it: expired (reason 1), closed here (2) or by the application (3).

Each new or changed notification plays the same short tone (an 880 Hz sine for 80 ms), whatever its application or urgency. There is no history: a notification that expires is gone.

`fact.last_id` remembers the highest notification ID seen, so a logic reload does not show old notifications again. It starts from zero in a new process.

## Unread and Do Not Disturb

The daemon keeps two states for the bar's bell (`shell/bar/bell.plm`), which shows a `4 px` lavender mark with a soft glow while something is unread and a struck-through bell at 40 % under Do Not Disturb:

- Unread is set by a new notification that is not low urgency; a replacement keeps whether it was seen. Only `notices_seen` clears it. It is saved to `notices.json` in the daemon's Pleamar files folder.
- Do Not Disturb is not saved. While it is on, notifications show no toast and play no tone, but still count as unread. Turning it on removes the toasts showing and shows the corner pill, "Do Not Disturb · On / Right-click bell to resume", for 2.6 s, in the toasts' corner (dropping from under the bar at the top right); toasts showing then sit below it at the top right, or above it at the bottom right. Turning it off removes the pill. Power warnings bypass it.

| Event | Effect |
|---|---|
| `notices_seen` | Clears unread. `tohru notifications seen` sends it |
| `notices_dnd <0\|1\|2>` | Do Not Disturb off, on or toggle. `tohru notifications dnd off\|on\|toggle` sends it |
| `notices_report` | Reports the state again |
| `notices_position <0\|1>` | Top right or bottom right, from the bar's `notifications.position` |
| `notices_cover <0\|1>` | From the bar: 1 while one of its popups is open |

After each change, and on `notices_report`, the daemon sends `emit notices_state <n>` to `tohru`: 1 for unread, 2 for Do Not Disturb, 3 for both. When it starts it also sends `emit notices_ask`, and the bar answers with `notices_position` and `notices_cover`; without the bar the toasts stay at the top right.

The toasts send `toast_enter`, `toast_leave`, `toast_close` and `toast_open` with their slot (0 to 3), and `toast_action` with `slot * 4 + button`, buttons from 1.

## Power warnings

The bar's `info/power.luau` sends these events with the battery charge from 0 to 1:

| Event | Toast |
|---|---|
| `power_warning_low` | Battery low, a normal toast that expires after 6 seconds |
| `power_warning_critical` | Battery critical, a critical toast that stays until closed |
| `power_warning_empty` | Battery nearly empty, a critical toast that stays until closed |
| `power_warning_clear` | Removes the power toast |

The numeral is the percentage remaining and the body says it again; it plays the same tone. A repeated warning at the same level is ignored, until `power_warning_clear`. These toasts are the daemon's own, not the server's, so hovering one holds its local timer.

## Running and tests

```sh
pleamar --check shell/notifications/notifications.plm
lua tests/session/notifications.lua
cargo test --manifest-path tests/rust/Cargo.toml --test notifications-bevel-shader
```

Run them from the repository root; the Lua test needs Lua 5.4. It covers stacking order and slots in both corners, the capillary's time, holding and resuming it under the pointer and under a bar popup, critical toasts, actions, power warnings, unread, and Do Not Disturb with its pill. The Rust test validates `bevel.wgsl` against Pleamar's shader ABI and checks the order in which the line drains.

To try it, stop any other notification daemon, start `bin/tohru-notifications-run` with a runtime that has patch 0025 and send one with `notify-send`, for example `notify-send -a Signal -i photo.png -A reply=Reply -A read="Mark Read" "Liam Vance" "Move the review to 17:30?"`. `tests/session/native.sh --runtime <pleamar> --notifications` runs it on a private session bus with a null audio sink; it has not been re-run since the repository was restructured.

## Known limitations

- A replacement is recognised by a change in title, body or timestamp. One that keeps all three, such as an identical replacement sent within the timestamp's resolution, shows no new toast and plays no tone.
- How many lines a body takes is estimated in the logic from SF Pro Text's character widths, rounded up, so the card is sized before it is drawn. A body in a different font can be cut a line early or leave a short gap.
- The popup's `bright:` reading is shared by every output and is taken over the whole `404 × 620 px` popup, so every toast is light or dark together.
- The top-right placement assumes the bar's 31 px reserve: the surface sits below it, and the first toast is drawn 6 px lower. Without the bar the toasts start 6 px from the top of the screen.
- `notices_cover` is one state for the bar on every output, so a popup open on one monitor hides the toasts on all of them.
- Pleamar's `--say` socket path must fit in a Unix socket name (108 bytes), so `PLEAMAR_SOCKETS` must be short.
