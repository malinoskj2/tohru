# Launcher

The application launcher. It runs as its own process, `tohru-launcher`, started by `bin/tohru-launcher-run`. It does not set `TOHRU_SHELL`, so it has no Tohru Luau globals.

`tohru launcher toggle`, `open` and `close` send `emit launcher_toggle`, `launcher_open` and `launcher_close`.

## Files

| File | Contents |
|---|---|
| `entry.plm` | The scene: surfaces, the deck, tiles, the action menu and key rules. Permissions: `apps`, `apps.launch`, `files`, `files.write`, `window` |
| `tile.wgsl` | An icon's surge: turbulence, scale and squash, and a glow in the icon's own colour |
| `reveal.wgsl` | The opening's blur and ripple over the sidebar and the stage |
| `entry.luau` | Wires the modules below together |
| `rail.luau` | A link to `../bar/rail.luau`, the workspace ribbon's driver, which Luau can only require from the scene's own folder |
| `controller.luau` | Catalog, categories and their counts, selection, scrolling, the sidebar's numeral, the stage's headings, the action menu, launching and saving |
| `search.luau` | j2bar's search ranking: which text matched, its weight, the forgiving second pass and the launch boost |
| `fuzzy.luau` | frizbee 0.13.0's scoring, which j2bar matches with: its prefilter (with up to a typo in four letters), Smith-Waterman with its chunked gap propagation, and the greedy fallback for texts over 1024 bytes |
| `usage.luau` | Pins, launch counts and recent launches, loaded from and saved to `launcher.json` |
| `geometry.luau` | Tile positions: rows of 82 px under 26 px section headings, Home's applications under a heading of their own |
| `output.luau` | Chooses the output to open on, and empties the backdrop's outputs as closing starts |

## Surfaces

- A 1×1 px surface per output (up to 8) that is never shown. It gives the scene the outputs' names.
- `launcher_backdrop`, an empty full-screen overlay layer with the namespace `tohru-launcher-backdrop` (from patch 0005), on its own outputs text (`launcher_backdrop_outputs`). A Pleamar surface whose `open:` turns false is only drawn empty, so emptying that text is what unmaps it the moment the launcher starts closing.
- `launcher_layer`, a full-screen overlay layer with the default `pleamar` namespace. It draws j2bar's faint scrim (`#0e0816` at 14 %, 6 % over a light desktop, gone as closing starts) and the deck, takes the keyboard while visible, stays mapped through the closing animation, and is then removed.

Both cover the whole output, the bar's strip included (`reserve: -1`), as j2bar's do, and open on the focused output, or the first one if that is unknown, staying there while open unless that output goes away.

The backdrop's namespace lets the optional Hyprland plugin tilt (16°), scale (0.86) and blur the windows behind the launcher; since it goes as closing starts, the windows spring back while the deck sinks. The launcher works the same without the plugin, over a sharp desktop.

## Look

The deck is concept 03, *Frosted Lens Deck*: two densities of glass side by side, 176 + 600 px wide and 524 px tall on an output that fits it.

- **Glass.** Both halves are the panels' Pleamar glass (`ink_glass`, as the network and sound popups'), with no bending at the edge. The 176 px sidebar is frosted thick (frost 100 %, tint 74 %); the stage beside it is clearer (frost 50 %, tint 44 %). A 0.75 px hairline divides them, and the deck has the panels' 20 px corners, inset hairline and shadow. The shared `water.wgsl`, unfolded with its waterline below the deck, drifts its palette light over both at twice its size.
- **Light and dark.** The launcher layer says `bright: panel_bright` (patch 0023), so `../panel-ink.plm` turns the deck's glass and ink light over a bright desktop and dark over a dark one, with j2bar's 55 %/45 % hysteresis, like the bar's popups. The layer keeps the last share while closed, so a launcher reopened over the same desktop opens in the same ink.
- **Sidebar.** The tabs Home, Create, Code, Media, Web, Office and System, 30 px rows with their counts, on the workspace ribbon turned upright: `../bar/ribbon.wgsl` driven by `../bar/rail-springs.plm`, `rail-rules.plm` and `rail.luau` as rail 0 with 32 px slots. Its bead takes the ribbon's palette spread from the first tab to the last, and waves, stretches and swells as the workspace bead does; on a light desktop it is darkened and runs on an ink track. It dims while searching. At the foot, a 62 px numeral at weight 200 counts the tab's applications or the search's results (`24`, `14`, `00`), over `Home · apps`, `Search results` or `No matches`.
- **Stage.** A 40 px search field (magnifier, `Search applications…`, `Tab ↕`; while typing a caret in the bead's colour and a `× Esc` button that clears it), then the grid: 80 × 78 px tiles with 40 px icons and their names, in rows of 82 px. Home's sections are `Pinned` and `Applications`; another tab's heading is its name; a search's is `All tabs · N matches`. A search with no results says `No applications match "…"` with a hint, centred on the stage. A scroll thumb shows when the grid overflows.
- **Selection.** The selected tile gets a soft plate (10 % ink, a 0.75 px rim) and the 4 px glowing mark under it; both slide from tile to tile. An icon surges under the pointer, or under the keyboard while the pointer is off the tiles, and each icon surges once as the deck opens. Tabs light under the pointer; the action menu is the same glass, with a plate under its row.
- **Motion.** Opening, the deck scales from 94 % and rises 12 px on a spring (stiffness 170, damping ratio 0.5), out of a 12 px blur under a ripple, while the windows behind recede. The sidebar's frost settles first, over 180 ms; the stage starts as frosted and clears behind it over 320 ms from 140 ms. Closing runs the spring back; the windows are given back as it starts, and the layer is removed once the spring is at rest.
- Text is SF Pro Text.

## The bar while the launcher is open

As under j2bar, the bar rises above the launcher and tints sky while it is open. The launcher tells the bar (`tohru`) its state with `emit launcher_bar N`: 0 closed, 1 open, 2 open with its layer mapped (on keyboard focus, or once the reveal has started), 3 closing. The bar tints sky (j2bar's 5 % to 20 %, fading over 350 ms) in states 1 and 2, and puts its surface on the overlay level, above the launcher's layers, in states 2 and 3. State 0 follows the card's layer being removed.

- A launcher that starts, or restarts, says 0. It answers `emit launcher_report 0` with its current state.
- While its state is not 0 the bar asks for a report every second; if `tohru-launcher` is not running, the bar takes 0. A launcher that dies open leaves the bar within a second.
- A bar that starts asks for a report a second later, once its surface exists, so it rises over a launcher that is already open.
- The bar tints and rises on whichever output it is on, as j2bar's did, even when the launcher opens on another.

## Behaviour

- The stage is 3 to 7 columns of 80 px tiles, depending on the output's width, and up to 440 px of grid tall, depending on its height, with tabs Home, Create, Code, Media, Web, Office and System. Categories come from each desktop entry's `Categories`; anything that matches no other tab goes under System.
- Home shows pinned applications first, then every other application.
- An application whose icon the theme does not have shows j2bar's window glyph, Lucide's `app-window` (`shell/service-assets/app-window.svg`), at 62 % white.
- Typing searches every tab, ranked as j2bar ranks. The query, lowercased but otherwise as typed, is scored with frizbee's fuzzy match: its letters in order, consecutive letters and the starts of words counting for more. A match in the shown name outranks one in any other of its names (patch 0019 passes every `Name[…]`), which outranks the best of the generic name (weighted 2), keywords (0.8), categories (0.3), ID (1.5) and `Exec` as written (1); a name that starts with the query outranks one that does not. The comment is not searched. When nothing matches, the query is matched again forgiving a missing or wrong letter in four. Each launch adds 2.5, at most 12.5. Ties keep catalog order, and at most 50 results show.
- Right-click, the Menu key or Shift+F10 opens a menu with Open, the entry's desktop actions, and Pin or Unpin.
- Launching calls Pleamar's `apps.launch` with the entry's `Exec`. The launch is counted and the launcher closes once Pleamar accepts the request; a failure shows beside the stage's heading.
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
lua tests/bar/rail.lua
cargo test --locked --manifest-path tests/rust/Cargo.toml --test launcher-deck
python3 tests/bar/panel-ink.py --runtime <pleamar>
python3 tests/launcher/lifecycle.py --cpu
python3 tests/launcher/namespace.py --runtime <pleamar>
pleamar --check shell/launcher/entry.plm
```

Run them from the repository root; the Lua tests need Lua 5.4. `namespace.py` and `panel-ink.py` need the patched runtime. `launcher-deck` validates every shader the scene declares against Pleamar's eight-value ABI, and checks that the rules which must run (the rail's and the ink's) are inside the launcher layer: the scene's own 1 px surface is never shown, and loose rules belong to its copies. `python3 tests/bar/native.py --launcher-only --runtime <pleamar>` and `--lifecycle-only` run the launcher on a private Sway session with its own catalog; both passed on 2026-11-12. They need Pillow, `setsid`, `wayland-scanner` and `pkg-config`. When running the launcher outside the Nix package, `setsid` and `sh` must be on `PATH` for launching to work.

## Known limitations

- Search lowercases ASCII letters only, where j2bar lowercases all of Unicode, and scores a non-ASCII query byte by byte, where frizbee scores it by codepoint. Keyword and category lists keep their escapes.
- Scores are frizbee's on an AVX-512 CPU. frizbee's gap propagation depends on its SIMD width, so j2bar on a CPU without AVX-512 can score some long or gappy matches slightly differently.
- What the catalog contains, and how an entry is started, is up to Pleamar's `apps` service. Patch 0019 has it leave out entries whose `TryExec` program is missing or whose `OnlyShowIn`/`NotShowIn` exclude this desktop, as j2bar did. Pleamar also chooses and scales icons itself, so some come from another file of the theme than j2bar's.
- The search field's caret is drawn at the end of the text: Home and End move Pleamar's own (hidden) caret but not the drawn one, and a selection made with Ctrl+A is not drawn.
- The very first opening after the launcher starts is dark until the first capture of the desktop arrives, about a frame later; after that the deck opens in the last ink it had.
- The brightness share is measured over what is behind the deck after the Hyprland plugin has shaded the windows, so a light desktop full of dark windows reads dark.
- The turbulence an icon flows through is a hash-based gradient noise, not j2bar's `feTurbulence` table, so its shapes differ while the strength and pace match.
- The bar is lowered back once the deck's layer is removed, about 1 s after closing starts rather than j2bar's half second; nothing is drawn on that layer by then.
