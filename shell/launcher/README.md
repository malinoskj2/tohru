# Launcher

The application launcher. It runs as its own process, `tohru-launcher`, started by `bin/tohru-launcher-run`. It does not set `TOHRU_SHELL`, so it has no Tohru Luau globals.

`tohru launcher toggle`, `open` and `close` send `emit launcher_toggle`, `launcher_open` and `launcher_close`.

## Files

| File | Contents |
|---|---|
| `entry.plm` | The scene: surfaces, the card, tiles, the action menu and key rules. Permissions: `apps`, `apps.launch`, `files`, `files.write`, `window` |
| `shapes.wgsl` | j2bar's capsule (shadows, tint, inner light and rim), raised fills, the lit segment and the search field's Fill track |
| `tile.wgsl` | An icon's surge: turbulence, scale and squash, and a glow in the icon's own colour |
| `reveal.wgsl` | The opening's blur and ripple over the field, tabs and section titles |
| `track.luau` | j2bar's Fill track: the liquid's spring to the caret and the waves each change of the text's length throws |
| `slice.wgsl` | Three columns of the waving track and liquid; `track.luau` hands it the columns while waves run |
| `entry.luau` | Wires the modules below together |
| `controller.luau` | Catalog, categories, selection, scrolling, the action menu, launching and saving |
| `search.luau` | j2bar's search ranking: which text matched, its weight, the forgiving second pass and the launch boost |
| `fuzzy.luau` | frizbee 0.13.0's scoring, which j2bar matches with: its prefilter (with up to a typo in four letters), Smith-Waterman with its chunked gap propagation, and the greedy fallback for texts over 1024 bytes |
| `usage.luau` | Pins, launch counts and recent launches, loaded from and saved to `launcher.json` |
| `geometry.luau` | Tile positions, including the gap between the Pinned and Applications sections |
| `output.luau` | Chooses the output to open on, and empties the backdrop's outputs as closing starts |

## Surfaces

- A 1×1 px surface per output (up to 8) that is never shown. It gives the scene the outputs' names.
- `launcher_backdrop`, an empty full-screen overlay layer with the namespace `tohru-launcher-backdrop` (from patch 0005), on its own outputs text (`launcher_backdrop_outputs`). A Pleamar surface whose `open:` turns false is only drawn empty, so emptying that text is what unmaps it the moment the launcher starts closing.
- `launcher_layer`, a full-screen overlay layer with the default `pleamar` namespace. It draws j2bar's faint scrim (`#0e0816` at 14 %, gone as closing starts) and the card, takes the keyboard while visible, stays mapped through the closing animation, and is then removed.

Both cover the whole output, the bar's strip included (`reserve: -1`), as j2bar's do, and open on the focused output, or the first one if that is unknown, staying there while open unless that output goes away.

The backdrop's namespace lets the optional Hyprland plugin tilt, shade and blur the windows behind the launcher; since it goes as closing starts, the windows spring back while the card sinks, as under j2bar. The launcher works the same without the plugin, over a sharp desktop.

## Look

The launcher is drawn to match j2bar's: the numbers and equations are j2bar's `src/launcher/view.rs`, `src/kit/` and `src/shaders/`.

- The card is not Pleamar glass. Like j2bar's capsule it is a dark tint (`rgba(22,16,34)` at 30 % to 16 % down the card) with an inner light, a 160° rim and two drop shadows, drawn by `shapes.wgsl`, over whatever the compositor shows behind it. What blurs the desktop is the Hyprland plugin's recession, as under j2bar.
- 8 px padding, a 44 px search field (raised fill, magnifier, `Search apps`, a sky caret, the Fill track), tabs at their label widths with the lit segment springing under the active one, and 72 px cells with 44 px icons and no labels. Home's sections are `PINNED` and `APPS`. An empty grid says why in the middle. A scroll thumb shows when the grid overflows.
- An icon surges under the pointer, or under the keyboard while the pointer is off the tiles, and each icon surges once as the card opens, 0.2 s plus 1 s per 1500 px from the card's centre. Tabs and the clear button light under the pointer; the menu's raised pill follows the pointer, or Up and Down.
- Opening, the card rises 16 px out of a 12 px blur, scaling from 94 % and fading in by halfway on a spring (stiffness 170, damping ratio 0.5), under a ripple that dies as it settles. Closing runs the same spring back to 0.
- Text is SF Pro Text, as j2bar's Pango rendered it.
- The search field's Fill track is j2bar's: `track.luau` springs the liquid to the caret and, on each change of the text's length, throws a wave (crest riding ahead of the liquid, steep front, long back and trough, chop, and the wash it leaves; at most ten), stepping j2bar's equations at 240 Hz on a timer that stops once everything is at rest. While waves run the track is drawn as 3-column `slice.wgsl` slices fed from the `launcher_track` model.

## The bar while the launcher is open

As under j2bar, the bar rises above the launcher and tints sky while it is open. The launcher tells the bar (`tohru`) its state with `emit launcher_bar N`: 0 closed, 1 open, 2 open with its layer mapped (on keyboard focus, or once the reveal has started), 3 closing. The bar tints sky (j2bar's 5 % to 20 %, fading over 350 ms) in states 1 and 2, and puts its surface on the overlay level, above the launcher's layers, in states 2 and 3. State 0 follows the card's layer being removed.

- A launcher that starts, or restarts, says 0. It answers `emit launcher_report 0` with its current state.
- While its state is not 0 the bar asks for a report every second; if `tohru-launcher` is not running, the bar takes 0. A launcher that dies open leaves the bar within a second.
- A bar that starts asks for a report a second later, once its surface exists, so it rises over a launcher that is already open.
- The bar tints and rises on whichever output it is on, as j2bar's did, even when the launcher opens on another.

## Behaviour

- The card is 3 to 8 columns of 72 px tiles, depending on the output's width, with tabs Home, Create, Code, Media, Web, Office and System. Categories come from each desktop entry's `Categories`; anything that matches no other tab goes under System.
- Home shows pinned applications first, then every other application.
- An application whose icon the theme does not have shows j2bar's window glyph, Lucide's `app-window` (`shell/service-assets/app-window.svg`), at 62 % white.
- Typing searches every tab, ranked as j2bar ranks. The query, lowercased but otherwise as typed, is scored with frizbee's fuzzy match: its letters in order, consecutive letters and the starts of words counting for more. A match in the shown name outranks one in any other of its names (patch 0017 passes every `Name[…]`), which outranks the best of the generic name (weighted 2), keywords (0.8), categories (0.3), ID (1.5) and `Exec` as written (1); a name that starts with the query outranks one that does not. The comment is not searched. When nothing matches, the query is matched again forgiving a missing or wrong letter in four. Each launch adds 2.5, at most 12.5. Ties keep catalog order, and at most 50 results show.
- Right-click, the Menu key or Shift+F10 opens a menu with Open, the entry's desktop actions, and Pin or Unpin.
- Launching calls Pleamar's `apps.launch` with the entry's `Exec`. The launch is counted and the launcher closes once Pleamar accepts the request; a failure shows in the card.
- The launcher closes when it loses keyboard focus after having had it.

| Key | Action |
|---|---|
| Arrows | Move the selection: Left and Right one tile (patch 0016 lets the rules have them before the search field's caret), Up and Down to the nearest tile in the row above or below |
| Return | Launch the selection, or the first result |
| Tab, Shift+Tab | Next or previous tab; clears the search |
| Page Up, Page Down | Scroll a page |
| Menu, Shift+F10 | Open the action menu for the selection |
| Escape | Close the action menu, else clear the search, else close |

## Saved state

`launcher.json` is written through Pleamar's `files` service, under `$XDG_DATA_HOME/pleamar/entry/`. It holds up to 64 pins, launch counts for up to 2000 applications and the six most recent launches. Nothing is imported from j2bar. Launch counts raise search results a little; recent launches are recorded but nothing uses them yet.

## Tests

```sh
lua tests/launcher/controllers.lua
lua tests/launcher/search.lua
lua tests/launcher/geometry.lua
lua tests/launcher/output.lua
lua tests/launcher/entry.lua
lua tests/launcher/track.lua
python3 tests/launcher/lifecycle.py --cpu
python3 tests/launcher/namespace.py --runtime <pleamar>
pleamar --check shell/launcher/entry.plm
```

Run them from the repository root; the Lua tests need Lua 5.4. `namespace.py` needs the patched runtime. `python3 tests/bar/native.py --launcher-only --runtime <pleamar>` and `--lifecycle-only` run the launcher on a private Sway session with its own catalog; both passed on 2026-11-12. They need Pillow, `setsid`, `wayland-scanner` and `pkg-config`. When running the launcher outside the Nix package, `setsid` and `sh` must be on `PATH` for launching to work.

## Known limitations

- Search lowercases ASCII letters only, where j2bar lowercases all of Unicode, and scores a non-ASCII query byte by byte, where frizbee scores it by codepoint. Keyword and category lists keep their escapes.
- Scores are frizbee's on an AVX-512 CPU. frizbee's gap propagation depends on its SIMD width, so j2bar on a CPU without AVX-512 can score some long or gappy matches slightly differently.
- What the catalog contains, and how an entry is started, is up to Pleamar's `apps` service. Patch 0017 has it leave out entries whose `TryExec` program is missing or whose `OnlyShowIn`/`NotShowIn` exclude this desktop, as j2bar did. Pleamar also chooses and scales icons itself, so some come from another file of the theme than j2bar's.
- The search field's caret is drawn at the end of the text: Home and End move Pleamar's own (hidden) caret but not the drawn one, and a selection made with Ctrl+A is not drawn.
- The Fill track's glows stay straight while waves bend the track and liquid, and lie under the track rather than over it; j2bar's follow the bend.
- The turbulence an icon flows through is a hash-based gradient noise, not j2bar's `feTurbulence` table, so its shapes differ while the strength and pace match.
- The bar is lowered back once the card's layer is removed, about 1 s after closing starts rather than j2bar's half second; nothing is drawn on that layer by then.
