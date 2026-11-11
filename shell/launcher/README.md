# Launcher

The application launcher. It runs as its own process, `tohru-launcher`, started by `bin/tohru-launcher-run`. It does not set `TOHRU_SHELL`, so it has no Tohru Luau globals.

`tohru launcher toggle`, `open` and `close` send `emit launcher_toggle`, `launcher_open` and `launcher_close`.

## Files

| File | Contents |
|---|---|
| `entry.plm` | The scene: surfaces, the card, tiles, the action menu and key rules. Permissions: `apps`, `apps.launch`, `files`, `files.write`, `window` |
| `entry.luau` | Wires the modules below together |
| `controller.luau` | Catalog, categories, selection, scrolling, the action menu, launching and saving |
| `search.luau` | The search matcher |
| `usage.luau` | Pins, launch counts and recent launches, loaded from and saved to `launcher.json` |
| `geometry.luau` | Tile positions, including the gap between the Pinned and Applications sections |
| `output.luau` | Chooses the output to open on |

## Surfaces

- A 1×1 px surface per output (up to 8) that is never shown. It gives the scene the outputs' names.
- `launcher_layer`, a full-screen layer on the overlay level, with the layer namespace `tohru-launcher-backdrop` (from patch 0005). It opens on the focused output, or the first one if that is unknown, and stays there while open unless that output goes away. It takes the keyboard while visible, stays mapped through the closing animation, and is then removed.

The namespace lets the optional Hyprland plugin tilt and shade the windows behind the launcher. The launcher works the same without the plugin.

## Behaviour

- The card is 3 to 8 columns of 72 px tiles, depending on the output's width, with tabs Home, Create, Code, Media, Web, Office and System. Categories come from each desktop entry's `Categories`; anything that matches no other tab goes under System.
- Home shows pinned applications first, then every other application.
- Typing searches every tab. A query matches the name, localised name, generic name, ID, `Exec`, keywords, categories and comment. A match at the start beats a match anywhere, which beats the letters appearing in order. Ties keep catalog order, and at most 50 results show.
- Right-click, the Menu key or Shift+F10 opens a menu with Open, the entry's desktop actions, and Pin or Unpin.
- Launching calls Pleamar's `apps.launch` with the entry's `Exec`. The launch is counted and the launcher closes once Pleamar accepts the request; a failure shows in the card.
- The launcher closes when it loses keyboard focus after having had it.

| Key | Action |
|---|---|
| Arrows | Move the selection: Left and Right one tile, Up and Down to the nearest tile in the row above or below |
| Return | Launch the selection, or the first result |
| Tab, Shift+Tab | Next or previous tab; clears the search |
| Page Up, Page Down | Scroll a page |
| Menu, Shift+F10 | Open the action menu for the selection |
| Escape | Close the action menu, else clear the search, else close |

## Saved state

`launcher.json` is written through Pleamar's `files` service, under `$XDG_DATA_HOME/pleamar/entry/`. It holds up to 64 pins, launch counts for up to 2000 applications and the six most recent launches. Nothing is imported from j2bar. Launch counts and recent launches are recorded but nothing is ordered by them yet.

## Tests

```sh
lua tests/launcher/controllers.lua
lua tests/launcher/search.lua
lua tests/launcher/geometry.lua
lua tests/launcher/output.lua
lua tests/launcher/entry.lua
python3 tests/launcher/lifecycle.py --cpu
python3 tests/launcher/namespace.py --runtime <pleamar>
pleamar --check shell/launcher/entry.plm
```

Run them from the repository root; the Lua tests need Lua 5.4. `namespace.py` needs the patched runtime. `python3 tests/bar/native.py --launcher-only --runtime <pleamar>` and `--lifecycle-only` run the launcher on a private Sway session with its own catalog; they have not been re-run since the repository was restructured. When running the launcher outside the Nix package, `setsid` and `sh` must be on `PATH` for launching to work.

## Known limitations

- Search has no typo tolerance and no ranking by use. Case folding covers ASCII letters only.
- What the catalog contains, and how an entry is started, is up to Pleamar's `apps` service.
