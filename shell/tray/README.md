# System tray

The bar's StatusNotifierItem (SNI) system tray and DBusMenu popup. It runs in the bar process and uses Pleamar's built-in `tray` service, which serves `org.kde.StatusNotifierWatcher` when none is running (or registers as a `StatusNotifierHost` with an existing watcher) and reads application menus over `com.canonical.dbusmenu`.

## Files

| File | Contents |
|---|---|
| `state.plm` | `tray` / `tray.*` permissions, `tray_items` (up to 24) and `tray_menu_rows` (up to 16) models, popup facts and `tray_*` events |
| `view.plm` | Inline bar icons (up to 6, shrinking on narrow outputs), vertical separator, attention indicator and overflow button (`⋯`) |
| `popup.plm` | Adaptive-glass DBusMenu popup with submenu navigation (`‹ Back`) and the overflow list popup |
| `controller.luau` | Watches `tray`, filters `Passive` items, handles left/middle/right click, scroll, DBusMenu tree traversal and fallback to `ContextMenu` |

## Interactions

- **Left click**: calls `tray.activate`. For apps that only expose a menu (when `Activate` fails and `menu` is true), opens the DBusMenu popup automatically.
- **Right click**: opens the app's DBusMenu in an adaptive-glass popup under the icon, or falls back to `tray.context`.
- **Middle click**: calls `tray.secondary`.
- **Scroll**: forwards wheel notches to `tray.scroll`.
- **Overflow / CLI**: `tohru panel toggle tray` or clicking `⋯` opens the full tray list popup.
