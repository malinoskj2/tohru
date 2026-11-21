# Wallpaper

The bar's wallpaper button and the wallpaper popup (Aperture Tank). It runs in the bar process. It does not play anything itself. The owner's `~/nix` units still own the wallpaper: `wallpaper@MON.service` runs mpvpaper on each output, and `$XDG_STATE_HOME/wallpaper/assignments.json` records what each output plays. The popup reads both and writes them the way the old `wallpaper-select` did.

## Files

| File | Contents |
|---|---|
| `state.plm` | Facts, texts and models (`wall_items`, `wall_mons`), fold and water springs, the four sweep slots and `wall_*` events |
| `view.plm` | The bar button and the popup's keys |
| `popup.plm` | The `268 × 204 px` compact tank, which unfolds into the `336 px` library. The tank reuses `../sound/tank.wgsl` with the index numeral, and the unfolded glass reuses `../sound/fill.wgsl` |
| `library.plm` | Monitor tabs (rail 6), the playback scrub (rail 7), Apply to Both and Randomize All, the 2 × 3 tile grid and the pager |
| `desktop.plm` | The meniscus layers (one per output taking a wallpaper, between mpvpaper and the windows) and the "Live Preview" pill |
| `meniscus.wgsl` | The clear water surface that falls from the bar, with the old picture under it |
| `scrim.wgsl` | The tiles' bottom scrim |
| `controller.luau` | Folding, selection, preview and revert, apply, random, the command queue and polling. Returns `{open, close, toggle}` |
| `wallpaper.svg`, `pause.svg`, `play.svg`, `shuffle.svg` | Lucide icons (ISC; see `LICENSE.LUCIDE`) |

`bin/tohru-wallpaper` does the work outside the UI thread. The controller calls it with `run` and `spawn`:

| Command | Does |
|---|---|
| `scan` | Lists the wallpaper directory, outputs, assignments and what each mpvpaper plays, as JSON |
| `status` | What each mpvpaper plays: path, position, length, pause |
| `thumbs FILE...` | For each file, makes a `320 × 200` thumbnail, a `64 × 40` blur source and its metadata, cached under `$XDG_CACHE_HOME/tohru/wallpaper/` by path, size and time. It prints one JSON line per file |
| `snapshot MON` | A still of the frame on screen, which the meniscus draws as the old picture |
| `load MON FILE [START]` | Plays a file on an output without recording it (preview and revert) |
| `apply MON FILE` | Records the file in `assignments.json` under its `flock`, then plays it |
| `seek MON SECONDS`, `pause MON yes\|no` | Playback control |

It accepts only files in the wallpaper directory with a video or image extension. It needs jq, socat, ffmpeg, flock and coreutils; the Nix package wraps it with them. `hyprctl` comes from the session, so it always matches the running compositor. `TOHRU_WALLPAPER_DIR` overrides the directory (default `~/.wallpapers/video`).

## How it is wired

`shell/bar.plm` includes the four scenes and allows `run: "tohru-wallpaper"`. `bin/tohru-run` puts its own `bin/` on `PATH`. `shell/bar.luau` calls `require("wallpaper/controller")()` and keeps one popup open at a time. The button takes the second extra slot in `shell/services.plm`, between the bell and Info.

| Event | Effect |
|---|---|
| `wall_open_request`, `wall_toggle` | Open or toggle. `tohru panel open wallpaper` and `tohru panel toggle wallpaper` send them |
| `wall_fold_toggle` | Fold or unfold (Tab) |
| `wall_tab <i>`, `wall_chip <i>` | Pick an output, or All after the last one |
| `wall_hover <i>`, `wall_leave <i>` | Preview a tile on the selected outputs, then revert 220 ms after the pointer leaves |
| `wall_choose <i>`, `wall_enter` | Apply the tile, or the previewed one |
| `wall_escape` | Revert a preview, otherwise close |
| `wall_apply_all`, `wall_random`, `wall_random_all` | Apply to both or all, randomize the selected output, randomize every output |
| `wall_pause`, `wall_scrub_*` | Pause or play, and seek |

### Preview, apply and the meniscus

Hovering a tile loads it on the output with `load`, which leaves `assignments.json` alone. The pill at the foot of that output names it. The first preview on an output saves what was playing and where. Leaving, Escape, folding or closing loads that again at the same position and pause state. If the bar stops during a preview, its next start reloads the recorded assignment.

Choosing a tile snapshots the frame on screen, maps a layer over mpvpaper that shows that still, then applies the file. Only after `apply` succeeds does the water surface fall from the bar, uncovering the new wallpaper above it. When a preview was already showing, the still is skipped and only the waterline falls over a light veil. A failed apply drops the layer, shows the error under the popup and rescans.

Randomize uses `wallpaper-randomize`'s policy: videos only, each different from the output's previous wallpaper and, where possible, from the others'.

## Relation to the `~/nix` units

- A choice is recorded exactly as `wallpaper-select` recorded one: in `assignments.json`, with the file loaded over mpvpaper's IPC socket. It survives an mpvpaper restart, a monitor unplug and a bar restart.
- `wallpaper-outputs` calls `wallpaper-randomize` for every output each time the session starts. A choice therefore lasts until the next login, as `wallpaper-select`'s did. `MIGRATION.md` gives the optional `~/nix` change that keeps it.
- Stills play through mpvpaper like videos, because the session has no other still mechanism (`force_default_wallpaper = 0`, no hyprpaper).
- `wallpaper-autopause` sets the pause again on its next event, which can override the popup's pause button.

## Tests

`tests/wallpaper/controller.lua` drives the controller with mocked `run`, `spawn`, timers and models. It covers folding, per-output selection and the All tab, preview, swap and revert, Escape, Enter and choosing, Apply to Both, Randomize All, a file that went missing before apply, a missing current assignment, reverting on fold and close, and healing after a restart. `tests/wallpaper/shaders.rs` validates `meniscus.wgsl` and `scrim.wgsl` with naga, and checks that the reused sound shaders still validate.
