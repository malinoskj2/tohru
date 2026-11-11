# Notifications

The notification daemon, run as its own process, `tohru-notifications`, by `bin/tohru-notifications-run`. Pleamar's `notifications` service owns `org.freedesktop.Notifications` on the session bus, so no other notification daemon may run. The bar never starts it.

## Files

| File | Contents |
|---|---|
| `notifications.plm` | A 420×380 px overlay surface at the top right of every output, 42 px from the top, with up to four 396×80 px banners. Permissions: `notifications` and `sound` |
| `notifications.luau` | Turns the service's notification list into banners and plays the tone |

## Behaviour

- Each new or changed notification shows a banner with its title (up to 256 characters) and body (up to 768 characters, three lines). The newest is on top; at most four show.
- A banner disappears after 6 seconds, or earlier when the notification is closed.
- Each new or changed notification plays the same short tone (an 880 Hz sine for 80 ms), whatever its application or urgency.
- There are no actions, buttons, history, unread count or Do Not Disturb.

`fact.last_id` remembers the highest notification ID seen, so a logic reload does not show old notifications again. It starts from zero in a new process.

## Power warnings

The bar's `info/power.luau` sends these events with the battery charge from 0 to 1:

| Event | Banner |
|---|---|
| `power_warning_low` | Battery low |
| `power_warning_critical` | Battery critical |
| `power_warning_empty` | Battery nearly empty |
| `power_warning_clear` | Removes the power banner |

The banner body is the percentage remaining, and it plays the same tone. A repeated warning at the same level is ignored.

## Running and tests

```sh
pleamar --check shell/notifications/notifications.plm
lua tests/session/notifications.lua
```

Run the Lua test from the repository root with Lua 5.4. To try it, stop any other notification daemon, start `bin/tohru-notifications-run` and send one with `notify-send`. `tests/session/native.sh --runtime <pleamar> --notifications` runs it on a private session bus with a null audio sink; it has not been re-run since the repository was restructured.

## Known limitations

A replacement is recognised by a change in title, body or timestamp. One that keeps all three, such as an identical replacement sent within the timestamp's resolution, shows no new banner and plays no tone.
