# Configuration

## Files

Tohru reads two TOML files:

- The writable file, `$XDG_CONFIG_HOME/tohru/config.toml`, or `~/.config/tohru/config.toml` when `XDG_CONFIG_HOME` is unset. Settings and `tohru config set` write here.
- An optional read-only managed file, named by the `TOHRU_MANAGED_CONFIG` environment variable. The Home Manager module generates it from `programs.tohru.settings`. It must not be the same file as the writable one.

Only the bar and Settings processes read settings. Nothing is imported from j2bar.

## Keys

| Key | Type | Default | Effect |
|---|---|---|---|
| `bar.preferred_output` | string, up to 256 characters | `""` | Output name (such as `DP-1`) for the bar, its popups and the volume OSD. Empty means every output. With an output set, the workspace switcher counts only that output's workspaces |
| `bar.mooncoil` | boolean | `false` | Show the Mooncoil mark at the left of the bar |
| `appearance.palette` | string | `"mocha"` | One of `mocha`, `macchiato`, `rose_pine`, `nord`, `gruvbox`, `tokyo_night`, `dracula`, `everforest` |
| `appearance.corners` | boolean | `true` | Draw rounded screen corners. Corners show on every output |
| `appearance.corner_size` | integer, 1 to 100 | `32` | Size of the screen corners |

Example `config.toml`:

```toml
[bar]
preferred_output = "DP-1"
mooncoil = false

[appearance]
palette = "nord"
corners = true
corner_size = 32
```

The same values in Home Manager:

```nix
programs.tohru.settings = {
  bar = { preferred_output = "DP-1"; mooncoil = false; };
  appearance = { palette = "nord"; corners = true; corner_size = 32; };
};
```

## Precedence

From lowest to highest: built-in defaults, the writable file, the managed file, runtime overrides.

- Editing a key that the managed file does not set (in Settings or with `tohru config set`) applies at once and is written to the writable file.
- Editing a key that the managed file sets applies at once, survives `tohru config reload`, but is never written and ends when the process exits. Settings marks these rows as Nix managed.
- Settings is its own process, so its runtime overrides live there. It forwards them to the bar, which keeps its copy until it exits.

## Changing settings at runtime

```sh
tohru config set appearance.palette nord
tohru config set appearance.corners false
tohru config set appearance.corner_size 24
tohru config set bar.mooncoil true
tohru config set bar.preferred_output DP-1
tohru config set bar.preferred_output ''    # every output
tohru config reload                         # re-read both files
tohru status --json                         # current values and where each came from
tohru settings-error                        # the last settings error, if any
```

These commands talk to the running bar. They report that the bar accepted the request, not that it succeeded; check `tohru status --json` or `tohru settings-error` afterwards. Changes made with the CLI reach an open Settings window the next time it is opened.

The Settings window has Bar, Appearance and System pages. It shows `bar.preferred_output` but does not offer a control for it.
