# Wallpaper popup

The existing compact tank, expanding library, thumbnails, live-preview/revert,
playback scrub and meniscus transitions are retained. `controller.luau` calls
`tohru-wallpaper`; disabled/unavailable ownership is shown as an actionable error.

The helper now owns a standalone opt-in mpvpaper lifecycle instead of connecting
to a separate desktop's player sockets. Configure `[wallpaper]` in Tohru TOML and
start its generated user service; see [standalone setup](../../docs/standalone.md#wallpaper-ownership).

| Command | Behavior |
|---|---|
| `serve` | Foreground supervisor; connected outputs come from standard Wayland observations |
| `scan`, `status` | JSON output/library/assignment/playback and owner state, without implicit startup |
| `thumbs FILE...` | Existing FFmpeg 320×200 and 64×40 preview cache, one JSON line per file |
| `snapshot OUTPUT` | Current owned file/frame for the meniscus |
| `load OUTPUT FILE [SECONDS]` | Nonpersistent preview; revert restores prior file/pause/position |
| `apply OUTPUT FILE` | Apply successfully, then atomically persist assignment |
| `pause OUTPUT yes\|no`, `seek OUTPUT SECONDS` | Owned IPC playback controls |
| `stop OUTPUT` | Stop only that owned child and clear assignment |

Default library: `~/Pictures/Wallpapers`. State:
`$XDG_STATE_HOME/tohru/wallpaper/assignments.json`. Cache:
`$XDG_CACHE_HOME/tohru/wallpaper`. Sockets are private under the session's
XDG_RUNTIME_DIR. File/directory/executable/options are normal managed/native
settings; existing assignment files can be selected explicitly. No Hyprland
monitor query, personal scripts, service names or home-directory links are used.

`tohru_wallpaper.py` owns process/socket policy and atomic state;
`tohru-wallpaper-media` preserves the original thumbnail/snapshot pipeline.
A wallpaper not already owned by Tohru is never killed or adopted. Hotplug polling
restores assignments; temporary discovery failure preserves currently running
children. Shutdown terminates/reaps owned children and removes their sockets.
Tests distinguish local child/socket lifecycle from unperformed live mpvpaper
rendering and physical display checks.
