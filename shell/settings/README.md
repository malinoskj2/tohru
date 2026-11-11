# Settings

The Settings window. It runs as its own process, `tohru-settings`, started on demand by `bin/tohru-settings-run` with `TOHRU_SHELL=1`, so it has its own `tohru_settings` state.

## Files

| File | Contents |
|---|---|
| `settings.plm` | The scene: a window titled "Tohru Settings" with app ID `dev.tohru.Settings`, asking for 760×640 px. Permission: `say:tohru`. The process quits when `settings_open` becomes false |
| `state.plm` | Navigation facts, the search text, the category and row models (up to 15 rows) and the `settings_*` events |
| `window.plm` | The layout: a 220 px sidebar, search, page heading and rows, laid out at whatever size the window is given (rows between 300 and 760 px wide) |
| `corner-size.plm`, `corner-size.luau` | The corner size field on the Appearance page |
| `controller.luau` | Pages, rows, search, history and toggling settings |
| `settings.luau` | Entry logic: starts both modules and tells the bar about each change |

## Opening and closing

`tohru settings open` sends `emit settings_open_request` to a running Settings, which reloads both settings files and shows the current values. If none is running, it starts `tohru-settings-run` from the same directory as `tohru` (or `TOHRU_SETTINGS_RUN`). When `TOHRU_MANAGED_CONFIG` is unset or empty, it first asks the bar for the managed file it loaded (`get managed_config`) and passes that on, so a keybind without the variable still sees the Nix-managed values.

`tohru settings close` (which sends `quit`), Escape, and closing the window all end the process.

## Pages

| Page | Rows |
|---|---|
| Bar | Mooncoil mark, System information button, About button (switches), Outputs (shows `bar.preferred_output`, not editable) |
| Appearance | Screen corners (switch), Corner size (a field; Enter applies 1 to 100), Palette (activating cycles through the eight palettes) |
| System | The shell and its Pleamar version |

Each row says where its value comes from: saved in `config.toml`, Nix managed, or a temporary runtime override of a Nix-managed value.

## How changes reach the bar

Settings writes through its own `tohru_settings`. After each accepted change it sends the bar `emit config_reload`, so the bar re-reads the files. A change to a Nix-managed key is never written, only held in Settings' memory, so Settings also sends the bar the matching `setting_*` event (`setting_mooncoil`, `setting_info_button`, `setting_about_button`, `setting_corners`, `setting_corner_size`, `setting_palette`) for every override it holds. If the bar does not take a request, the window shows the error.

Values that `tohru config set` saves show up in an open window the next time it is opened. Overrides of Nix-managed values made with `tohru config set` live only in the bar, and Settings does not show them.

## Keyboard

Ctrl+F focuses search, which matches row titles, details and keys without regard to case. Tab switches between the sidebar and the rows, Up and Down move, Return (or Space in the rows) activates, Alt+Left and Alt+Right go back and forward through up to 64 visited pages, and Escape closes.

## Tests

```sh
lua tests/settings-ui/process.lua
lua tests/settings-ui/corner-size.lua
cargo test --manifest-path tests/rust/Cargo.toml --test settings-ui
```

Run them from the repository root; the Lua tests need Lua 5.4. `tests/native-settings.sh --runtime <pleamar> --settings` (see `tests/WINDOW-RUNNER.md`) opens, edits and closes Settings on a private Sway session.

## Known limitations

- `appearance.palette` is saved and reported, but no scene draws with it yet, so changing it has no visible effect.
- `bar.preferred_output` can only be changed with `tohru config set` or the config file.
- `tests/settings-ui/contrast.py` still expects the old fixed row width and fails.
