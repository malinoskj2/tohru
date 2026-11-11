# Nix

The flake only exports `x86_64-linux`.

## Flake outputs

| Output | What it is |
|---|---|
| `packages.x86_64-linux.tohru` (also `default`) | The shell: scenes under `share/tohru/shell`, the `tohru` CLI and every launch wrapper, all pointed at the patched runtime |
| `packages.x86_64-linux.pleamar-runtime` | The patched Pleamar 0.7.27 runtime on its own |
| `packages.x86_64-linux.tohru-hyprland` | The Hyprland plugin, built against Hyprland 0.56.2 from the `hyprland-nixpkgs` input |
| `apps.x86_64-linux.default` (also `shell`) | Runs `tohru-run`, the bar |
| `apps.x86_64-linux.tohru` | Runs the `tohru` CLI |
| `overlays.default` | Adds `tohru-hyprland`, built against the Hyprland in the package set it is applied to |
| `homeManagerModules.tohru` (also `default`) | The Home Manager module below |

There is no NixOS module.

```sh
nix run github:malinoskj2/tohru                              # the bar
nix run github:malinoskj2/tohru#tohru -- status --json       # the CLI
nix build github:malinoskj2/tohru#pleamar-runtime
```

The package contains these programs:

| Program | Starts |
|---|---|
| `tohru-run` | The bar, with its popups, OSD and corners |
| `tohru-session-run` | The session menu |
| `tohru-launcher-run` | The launcher |
| `tohru-notifications-run` | The notification daemon |
| `tohru-settings-run` | The Settings window (normally started by `tohru settings open`) |
| `tohru` | The CLI |

Without flakes, the root `default.nix` takes an explicit package set: `import ./default.nix { pkgs = import <your pinned nixpkgs> { }; }`.

## Home Manager module

```nix
{ inputs, ... }:
{
  imports = [ inputs.tohru.homeManagerModules.default ];
  programs.tohru = {
    enable = true;
    systemd.enable = true;
    settings.bar.preferred_output = "DP-1";
  };
}
```

With a standalone Home Manager flake, pass `inputs` through `extraSpecialArgs`, or import `tohru.homeManagerModules.default` directly in `modules`.

### Options

| Option | Default | Effect |
|---|---|---|
| `programs.tohru.enable` | `false` | Installs the package and sets `TOHRU_MANAGED_CONFIG` in `home.sessionVariables` |
| `programs.tohru.package` | Built from the consumer's `pkgs` | The Tohru package to install and run |
| `programs.tohru.settings` | `{ }` | Settings written to a read-only TOML file in the store. See [configuration.md](configuration.md) |
| `programs.tohru.systemd.enable` | `false` | Adds the user services below |
| `programs.tohru.systemd.target` | `"graphical-session.target"` | The user target that starts and stops those services |
| `programs.tohru.hyprlandPlugin.package` | Plugin built against your session's Hyprland | The plugin package. Nothing loads it automatically |

Everything is inert unless `enable` is true; `systemd.enable` alone adds no services.

### Services

With `systemd.enable`, the module defines four user services:

| Unit | Runs |
|---|---|
| `tohru-bar` | `tohru-run` |
| `tohru-session` | `tohru-session-run` |
| `tohru-launcher` | `tohru-launcher-run` |
| `tohru-notifications` | `tohru-notifications-run` |

Each is `WantedBy`, `PartOf` and `After` the configured target, restarts on failure after 2 seconds, and only starts when `WAYLAND_DISPLAY` is in the user manager's environment. Each gets `TOHRU_MANAGED_CONFIG` in its unit environment. `tohru-launcher` uses `KillMode=process`, so restarting it does not kill applications it launched. Settings is not a service; it starts on demand.

The services inherit the systemd user manager's environment. Your compositor session must import `WAYLAND_DISPLAY`, plus the `PATH` and `XDG_DATA_DIRS` the launcher uses to find and start applications, before the target starts. UWSM does this, or run this from your compositor's startup:

```sh
systemctl --user import-environment WAYLAND_DISPLAY PATH XDG_DATA_DIRS
```

If your Hyprland setup provides its own target, such as `hyprland-session.target`, set `systemd.target` to it.

`tohru-notifications` must be the only notification daemon. Disable mako, dunst, swaync or similar first. These units are new and have not been tested on a real session.

`TOHRU_MANAGED_CONFIG` reaches other programs, such as the Settings window started from a keybind, only through `home.sessionVariables`, which applies to sessions started from a login shell.

## Hyprland plugin

`tohru-hyprland` is optional. The shell works without it. It must be built against the exact Hyprland your session runs: the package asserts Hyprland 0.56.2, and the plugin refuses to load if its build headers do not match the running compositor.

The default for `programs.tohru.hyprlandPlugin.package` uses, in order: `wayland.windowManager.hyprland.package`, the NixOS `programs.hyprland.package` when Home Manager runs as a NixOS module, then `pkgs.hyprland`. Evaluating it fails if that Hyprland is not 0.56.2. If your compositor carries local patches, build against that exact derivation:

```nix
programs.tohru.hyprlandPlugin.package =
  pkgs.callPackage "${inputs.tohru}/hyprland/package.nix" { hyprland = myHyprland; };
```

Load it from your Hyprland Lua configuration:

```lua
hl.plugin.load("/nix/store/...-tohru-hyprland-0.1.0/lib/libtohru-hyprland.so")
```

With Home Manager, the path is `"${config.programs.tohru.hyprlandPlugin.package}/lib/libtohru-hyprland.so"`. To try it in a running session instead, use `hyprctl plugin load <path>`; `hyprctl desktop-effects-capabilities` then reports its features.

The plugin reads the `plugin:hyprfocus`, `plugin:hyprbars`, `plugin:hyprsheet` and `plugin:hyprrecede` settings. `hyprland/examples/effects.lua` has j2bar's defaults for all four. They are source defaults, not a verified live configuration. Do not load it next to hyprland-plugins' hyprfocus or hyprbars, or j2bar's plugin: they share hooks and config keys.

Known problems: its animation settings only apply after a compositor reload, and its visual, unload and damage checks have not been done. See `hyprland/README.md` and `hyprland/VALIDATION.md`.

`hyprland/` also has its own standalone flake (`nix build path:./hyprland`).

## Pinning

- `nixpkgs` is pinned to `4fe25e12eb9f4c744966dd3de5a05246cf098601`. It builds the `packages` outputs. You can make it follow yours with `inputs.tohru.inputs.nixpkgs.follows = "nixpkgs";`.
- `hyprland-nixpkgs` is pinned to `8ce4ef6cb6f871616146b9fe26d2a5ae594e94fe`, the package set with Hyprland 0.56.2. It is only used for `packages.x86_64-linux.tohru-hyprland`. Do not make it follow a nixpkgs that has a different Hyprland.
- The Home Manager module builds `programs.tohru.package` and the plugin from the consumer's `pkgs`, not from the flake's pins. To use the flake's own build instead, set `programs.tohru.package = inputs.tohru.packages.x86_64-linux.default;`.
- Pleamar is pinned separately in `runtime/pleamar/pin.json`: version 0.7.27, revision `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`, with the archive hash. `runtime/pleamar/prepare.sh` repeats the revision and hash rather than reading `pin.json`. Changing the pin means updating both, rebasing every patch in `runtime/pleamar/patches/` and updating `runtime/pleamar/Cargo.lock`.

`nix/tests/home-manager.nix` evaluates the module against a Home Manager input you supply and checks the options and units above.
