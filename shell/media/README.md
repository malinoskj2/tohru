# Media

The media popup (Prism Edge), opened from the bar's song label. It runs in the bar process and uses Pleamar's `media` service, which talks to MPRIS players on the session bus.

## Files

| File | Contents |
|---|---|
| `state.plm` | Popup facts and texts, up to 8 players in `media_players`, the `media_*` events, icons, the fold and dim springs, and the position clock |
| `popup.plm` | The `316 × 166 px` compact popup: the artwork strip with its play/pause badge, the player chip, remaining time, title, artist, the elapsed numeral, previous/play-pause/next, the progress rail and the light edge. It unfolds into a header, a row per player (at most five shown) and a volume rail |
| `lens.wgsl` | The group shader over the artwork: the strip as a chamfered lens, its mean colour bled into the frosted pane, blurred and greyed while paused, empty glass with nothing playing. With `b.w = 1` it draws a player row's colour tab from that row's artwork |
| `controller.luau` | Turns `media` reports into facts and the player list, keeps the position clock, and turns popup actions into `media.*` calls. Returns `{observe, open, close, toggle}` |
| `pause.svg`, `previous.svg`, `next.svg`, `music.svg` | Lucide icons (ISC; see `LICENSE.LUCIDE`) |

The play, speaker and muted icons come from `shell/service-assets/`. The label and its `root_media` zone are in `shell/services.plm`.

## How it is wired

`shell/bar.plm` includes `media/state.plm` and `media/popup.plm`, with the `media` and `media.*` permissions. The bar keeps its one `media` watch in `bar/controller.luau`, which passes each report on to `media.observe`; `shell/bar.luau` closes the other popups when this one opens, so one popup is open at a time. The popup is centred under the label, kept 8 px from the output's edges, and takes the keyboard while open.

The progress bead is rail 5 of `bar/rail-state.plm`, drawn by the workspace ribbon's `rail_shader`: a jump (a seek, another song or player) glides it with the ribbon's wave and a drag stretches it as the ribbon's bead does. The elapsed time is a scene clock that runs while the popup is open and the player plays; each report reseeds it.

| Event | Effect |
|---|---|
| `media_open`, `media_close`, `media_toggle` | Open, close or toggle. `tohru panel open media` and `tohru panel toggle media` send the first and last |
| `media_fold_toggle` | Fold or unfold (the chip or the chevron) |
| `media_play`, `media_next`, `media_previous`, `media_step <s>` | Play/pause, skip, or move by seconds in the reported player |
| `media_seek_press/drag/release <fraction>` | Drag the bead; the numeral and `2:40 / 4:03` follow, and the player seeks once, on release |
| `media_choose <row>`, `media_row_play <row>` | Make a row's player the reported one, and also play/pause it |
| `media_volume_press/drag/release <0..1>`, `media_volume_step <±1>` | Set the reported player's own volume; scrolling over the popup steps it by 5 % |

Folded, `Space`/`Return` play or pause, `Left`/`Right` move 5 s, and `Up`/`Down` change the volume. Unfolded, `Up`/`Down` move between the header, the player rows and the volume, `Space`/`Return` play/pause from the header or choose a row, and `Left`/`Right` move 5 s on the header or change the volume on its row. `Page Up`/`Page Down` skip, `Tab` folds, and `Escape` or a click outside closes.

## Volume

The volume rail sets the player's MPRIS `Volume` through `media.volume` (runtime patch `0024-media-players-volume`), at most once every 50 ms while dragged. Many players, browsers among them, have no `Volume` property. For those the rail and the light edge show `–` dimmed and do nothing; use the sound popup's app rows, which set the app's PulseAudio stream volume, instead.

## Running and tests

```sh
lua tests/media/controller.lua
cargo test --manifest-path tests/rust/Cargo.toml --test media-lens-shader
pleamar --check shell/bar.plm
python3 tests/media/mpris-player.py --name spotify --identity Spotify --title "Midnight City" \
    --artist M83 --art cover.png --length 243 --position 134 --playing --volume 0.72
```

Run them from the repository root; the Lua test needs Lua 5.4. `tests/media/mpris-player.py` is a fake MPRIS player with a song on a clock, seeking and, with `--volume`, a volume of its own; start several to try the unfolded list.

## Known limitations

- Only local artwork (`file://` art URLs) is painted. Players that give an `http` URL, as Spotify does, get the empty lens and a palette-coloured row tab.
- At most five players are listed; the rest are chosen with `playerctl` or from the player.
- The position is the player's report carried on by the scene's clock, so a player that changes speed without reporting it drifts until its next report.
