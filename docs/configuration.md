# Configuration

## Files

Tohru reads two TOML files:

- The writable file, `$XDG_CONFIG_HOME/tohru/config.toml`, or `~/.config/tohru/config.toml` when `XDG_CONFIG_HOME` is unset. Settings and `tohru config set` write here.
- An optional read-only managed file, named by the `TOHRU_MANAGED_CONFIG` environment variable. The Home Manager module generates it from `programs.tohru.settings`. It must not be the same file as the writable one. An empty value means no managed file. When `tohru settings open` starts Settings without the variable set, it gives Settings the managed file the running bar loaded.

Only the bar and Settings processes read settings; the bar forwards `notifications.position` to `tohru-notifications`. Nothing is imported from j2bar.

## Keys

| Key | Type | Default | Effect |
|---|---|---|---|
| `bar.preferred_output` | string, up to 256 characters | `""` | Output name (such as `DP-1`) for the bar, its popups and the volume OSD. Empty means every output. With an output set, the workspace switcher counts only that output's workspaces |
| `bar.mooncoil` | boolean | `false` | Show the Mooncoil mark at the left of the bar |
| `bar.info_button` | boolean | `false` | Show the System information button in the bar's tray |
| `bar.about_button` | boolean | `false` | Show the About Tohru button in the bar's tray |
| `appearance.palette` | string | `"mocha"` | One of `mocha`, `macchiato`, `rose_pine`, `nord`, `gruvbox`, `tokyo_night`, `dracula`, `everforest` |
| `appearance.corners` | boolean | `true` | Draw rounded screen corners. Corners show on every output |
| `appearance.corner_size` | integer, 1 to 100 | `32` | Size of the screen corners |
| `lock.style` | string | `"flooded-monolith"` | The lock screen's look: `still-glass`, `ribbon-key`, `flooded-monolith`, `drifting-frost-veil`, `unfolding-glass-capsule`, `edge-glass-drawer`, `horizon-meniscus`, `refractive-typographic-lens`, `corner-anchor` or `bar-drop-curtain`. An unknown value logs a warning and locks with `flooded-monolith`. `tohru-lock` reads it each time it locks; `TOHRU_LOCK_STYLE` in the locker's environment overrides it |
| `notifications.position` | string | `"top-right"` | Where notification toasts stack: `top-right`, just under the bar with the newest nearest it, or `bottom-right`, with the newest at the bottom edge. Any other value is an error. The bar sends it to `tohru-notifications`, which moves its toasts at once |
| `polkit.agent` | boolean | `false` | Let `tohru-polkit` become the session's polkit authentication agent. Read when `tohru-polkit` starts; another agent such as hyprpolkitagent must not be running. See [MIGRATION.md](../MIGRATION.md#polkit-agent) |
| `assistant.provider` | string | `""` | The CLI answers go to: `claude`, `codex`, `gemini`, `agy` or `opencode`. Empty, or one that is missing or signed out, means the first installed one that is signed in. The popup's provider menu sets it |

Example `config.toml`:

```toml
[bar]
preferred_output = "DP-1"
mooncoil = false

[appearance]
palette = "nord"
corners = true
corner_size = 32

[lock]
style = "horizon-meniscus"

[notifications]
position = "bottom-right"
```

The same values in Home Manager:

```nix
programs.tohru.settings = {
  bar = { preferred_output = "DP-1"; mooncoil = false; };
  appearance = { palette = "nord"; corners = true; corner_size = 32; };
};
programs.tohru.lock.style = "horizon-meniscus";   # the same as settings.lock.style
programs.tohru.notifications.position = "bottom-right";   # the same as settings.notifications.position
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
tohru config set lock.style corner-anchor
tohru config set notifications.position bottom-right
tohru config set bar.info_button true
tohru config set bar.preferred_output DP-1
tohru config set bar.preferred_output ''    # every output
tohru config set polkit.agent true          # takes effect when tohru-polkit restarts
tohru config reload                         # re-read both files
tohru status --json                         # current values and where each came from
tohru settings-error                        # the last settings error, if any
```

These commands talk to the running bar. They report that the bar accepted the request, not that it succeeded; check `tohru status --json` or `tohru settings-error` afterwards. Changes made with the CLI reach an open Settings window the next time it is opened.

The Settings window has Bar, Appearance and System pages. It shows `bar.preferred_output` but does not offer a control for it, and does not show `polkit.agent`. The lock style and the notification position are on the Appearance page; clicking one steps to the next value.

The locker is a separate process that reads the files when it locks, so a runtime override (a change to a Nix-managed `lock.style` made in Settings or with `tohru config set`) does not reach it; change `programs.tohru.lock.style` instead.
