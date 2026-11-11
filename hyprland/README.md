# tohru-hyprland

An optional Hyprland plugin, `libtohru-hyprland.so`, with four window effects ported from j2bar. It does not link to or talk to the shell, and the shell works without it.

| Effect | Config group | Source | What it does |
|---|---|---|---|
| Focus dips | `plugin:hyprfocus` | `focus/` | Animates the window that gains focus: `none`, `flash`, `shrink`, `slide`, or a comma-separated combination. `shrink` is drawn only; the client is not resized |
| Titlebars | `plugin:hyprbars` | `bars/` | Thin titlebars with buttons. Double-click runs `on_double_click` |
| File chooser sheets | `plugin:hyprsheet` | `sheet.cpp` | Attaches matching dialogs (by default `xdg-desktop-portal-gtk`) to their parent window |
| Recession | `plugin:hyprrecede` | `recede.cpp` | Tilts, shrinks, shades and blurs the windows on a monitor while a matching layer is open on it |

`main.cpp` combines them, and registers `hyprctl desktop-effects-capabilities`, which prints the plugin's capabilities and the Hyprland build hash as JSON. `focus/` and `bars/` derive from hyprland-plugins v0.56.0; `UPSTREAM.md` and `source-manifest.json` record where every file came from.

## Compatibility

The plugin is built for Hyprland 0.56.2 and nothing else. On load it compares the build hash of the headers it was compiled against with the running compositor's, and refuses to load if they differ, so it must be built against the exact Hyprland derivation you run, including any local patches. `package.nix` also fails to evaluate for any other Hyprland version.

Do not load it next to hyprland-plugins' hyprfocus or hyprbars, or j2bar's plugin. They use the same hooks and config keys.

## Building

With Nix, from the repository root:

```sh
nix build .#tohru-hyprland        # against the flake's pinned Hyprland 0.56.2
nix build path:./hyprland         # the standalone flake in this folder
nix develop path:./hyprland -c make -C hyprland -j1
```

To build against your own compositor derivation:

```nix
pkgs.callPackage ./hyprland/package.nix { hyprland = myHyprland; }
```

Without Nix you need a C++23 GCC toolchain, `make`, `pkg-config`, and the development files for that exact Hyprland plus pixman, libdrm, pangocairo, libinput, libudev, wayland-server, xkbcommon and Lua.

| Target | Does |
|---|---|
| `make` | Builds `libtohru-hyprland.so` |
| `make check-deps` | Checks that `pkg-config` finds every dependency |
| `make check` | Checks copied sources against `source-manifest.json`, and runs the focus-restoration and sheet tests in `tests/` |
| `make check-abi` | Checks the build-hash comparison against a fake host |
| `make install PREFIX=...` | Installs `lib/libtohru-hyprland.so` and the licenses |

The Nix package runs `make check-abi check-focus check-sheet` as its check phase.

## Loading and configuring

Load it from your Hyprland Lua configuration with `hl.plugin.load("<path>/lib/libtohru-hyprland.so")`, or in a running session with `hyprctl plugin load <path>`.

`examples/effects.lua` sets j2bar's values for all four effects. They are j2bar's source defaults, not a checked live configuration. Things to know when writing your own:

- The plugin registers its animation leaves (`hyprfocusIn`, `hyprfocusOut`, `hyprsheetIn`, `hyprsheetOut`, `hyprrecedeTilt`, `hyprrecedeShade`, `hyprrecedeBlur`) only once it is loaded. At startup that is after your configuration has run, and `hl.animation` on an unknown leaf is an error that stops the rest of the file. The example sets them inside `pcall`, and again shortly after `hyprland.start`.
- `plugin:hyprrecede:namespace` defaults to `^j2bar-launcher-backdrop$`. For Tohru's launcher set it to `^tohru-launcher-backdrop$`. Recession starts when a matching layer opens, so load and configure the plugin before opening the launcher.
- Titlebars are drawn on every window unless a window rule sets `hyprbars:no_bar`. The example keeps them only on Alacritty and mpv with a `negative:` class rule.
- `on_double_click = "tohru:maximize"` toggles maximize. An empty value disables double-click; any other value is run as a command.
- The `class` patterns for focus and sheets, and the recession namespace, are C++ `std::regex` (ECMAScript) patterns. Hyprland window rules use RE2.

## Known limitations

Static checks and the unit tests pass, and the effects have been seen working in a nested Hyprland 0.56.2. Unloading the plugin from a running session, damage handling, scaling and rotation, and performance have not been checked. `VALIDATION.md` lists the remaining checks.
