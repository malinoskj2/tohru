# Tohru

A Linux desktop shell for Hyprland, written as [Pleamar](https://github.com/k4ditano/pleamar) scenes.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Nix flake](https://img.shields.io/badge/Nix-flake-5277C3?logo=nixos&logoColor=white)](flake.nix)
[![Platform: x86_64-linux](https://img.shields.io/badge/platform-x86__64--linux-lightgrey)](#status)
[![Hyprland 0.56.2](https://img.shields.io/badge/Hyprland-0.56.2-58E1FF)](hyprland/README.md)
[![Pleamar 0.7.27](https://img.shields.io/badge/Pleamar-0.7.27-1f6feb)](runtime/pleamar/pin.json)
[![Status: experimental](https://img.shields.io/badge/status-experimental-orange)](#status)

Tohru replaces my previous shell, j2bar. The shell itself is `.plm` scene files, Luau logic and WGSL shaders, run by a patched build of Pleamar 0.7.27. An optional Hyprland plugin adds window effects.

## Quickstart

### With Nix and Home Manager

Try the bar without installing anything:

```sh
nix run github:malinoskj2/tohru
```

To install, add the flake as an input:

```nix
{
  inputs.tohru.url = "github:malinoskj2/tohru";
}
```

Then, in your Home Manager configuration:

```nix
{ inputs, ... }:
{
  imports = [ inputs.tohru.homeManagerModules.default ];

  programs.tohru = {
    enable = true;
    systemd.enable = true;
    settings = {
      bar.preferred_output = "DP-1";
      bar.mooncoil = false;
      appearance.palette = "nord";
      appearance.corners = true;
      appearance.corner_size = 32;
    };
  };
}
```

`systemd.enable` starts the bar, session menu, launcher and notification daemon as user services with `graphical-session.target`. Your session must import `WAYLAND_DISPLAY`, `PATH` and `XDG_DATA_DIRS` into the systemd user manager before that target starts, and no other notification daemon may run. See [docs/nix.md](docs/nix.md) for every option and for loading the Hyprland plugin.

### Without Nix

You need a recent stable Rust (1.85 at minimum), a C and C++ compiler, `pkg-config`, the development files for fontconfig, wayland, libxkbcommon, linux-pam and libffi, plus `curl`, `git`, `tar` and `sha256sum`. At run time you need a Vulkan loader, libglvnd, and `setsid` and `sh` on `PATH`.

```sh
git clone https://github.com/malinoskj2/tohru && cd tohru
./runtime/pleamar/prepare.sh
cargo build --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --bin pleamar

export TOHRU_RUNTIME="$PWD/.tmp/pleamar-runtime/target/release/pleamar"
./bin/tohru-run &
./bin/tohru-session-run &
./bin/tohru-launcher-run &
./bin/tohru-notifications-run &   # only if no other notification daemon runs
```

[docs/manual-install.md](docs/manual-install.md) covers each step, keeping the processes running, and putting `tohru` on your `PATH`.

## Status

**This is experimental. A lot of it is untested, and some of it is known to be broken.** Expect to read the source when something goes wrong.

- It has only just replaced j2bar on one NixOS desktop (two monitors, NVIDIA). The systemd user units and the notification daemon run there; most panels have only had a short check.
- There is no notification bell and no adaptive light/dark glass. Clock and text rendering differ slightly from j2bar, the reference design.
- Popups opened from the CLI, without a pointer click, may need Escape to close.
- The workspace switcher can only switch to workspaces the compositor already reports. It cannot create one.
- The Hyprland plugin registers its animation leaves only once it is loaded, which at startup is after your configuration runs. Set them as shown in `hyprland/examples/effects.lua`. Its unload and damage checks have not been done.
- The Settings window's size and placement are unresolved: it renders 800 px wide whatever size it gets.
- The native test suites have not been re-run since the bar layout changed and the repository was restructured. Several need updating.
- There is no screen locker, idle handling, polkit agent, wallpaper or screenshot tool. Use hyprlock, hypridle, hyprpolkitagent, and grimblast or similar.
- x86_64-linux only. ARM is untested. Only tested with an NVIDIA GPU. Non-NixOS distributions are untested.
- Tohru needs its own patched Pleamar build. Stock Pleamar will not run the bar.

## Using it

The `tohru` command sends requests to the running processes. It reports that a request was accepted, not that the action finished. Commands most people bind to keys:

| Command | Action |
|---|---|
| `tohru audio volume up` / `down` | Change the output volume (shows the OSD) |
| `tohru audio mute toggle` | Mute or unmute the output |
| `tohru session toggle` | Open or close the session menu |
| `tohru launcher toggle` | Open or close the launcher |
| `tohru panel toggle sound` | Toggle a popup: `sound`, `calendar`, `network`, `bluetooth`, `info`, `about` |
| `tohru panel close` | Close the open popup |
| `tohru settings open` | Open Settings, starting it if needed |

For example, in a Hyprland 0.56 Lua configuration:

```lua
hl.bind("SUPER + Space", hl.dsp.exec_cmd("tohru launcher toggle"))
hl.bind("SUPER + Escape", hl.dsp.exec_cmd("tohru session toggle"))
hl.bind("XF86AudioRaiseVolume", hl.dsp.exec_cmd("tohru audio volume up"), { locked = true, repeating = true })
hl.bind("XF86AudioLowerVolume", hl.dsp.exec_cmd("tohru audio volume down"), { locked = true, repeating = true })
hl.bind("XF86AudioMute", hl.dsp.exec_cmd("tohru audio mute toggle"), { locked = true })
```

Run `tohru help` for the full list, including per-app volume, output selection and `tohru config set`.

## Documentation

- [docs/nix.md](docs/nix.md): flake outputs, Home Manager options, the Hyprland plugin, pinning.
- [docs/manual-install.md](docs/manual-install.md): building and running without Nix.
- [docs/configuration.md](docs/configuration.md): the config file, its keys, and how Nix-managed values and runtime edits combine.
- [docs/architecture.md](docs/architecture.md): processes, how they talk, and what the Pleamar patches add.

## Repository layout

| Path | Contents |
|---|---|
| `shell/` | The Pleamar scenes: `bar.plm` (root) and one folder per feature |
| `bin/` | The `tohru` CLI and the launch wrappers |
| `runtime/` | The `tohru-shell` Rust library (settings, system info, workspace rail physics) |
| `runtime/pleamar/` | The Pleamar pin, patches, Luau bindings and `prepare.sh` |
| `hyprland/` | The optional `tohru-hyprland` compositor plugin (C++) |
| `nix/`, `flake.nix`, `default.nix` | Nix packaging and the Home Manager module |
| `tests/`, `tools/` | Development tests and reference tooling |

## License and credits

Tohru is MIT licensed ([LICENSE](LICENSE)).

- [Pleamar](https://github.com/k4ditano/pleamar) is BSD-3-Clause. Its source is fetched at build time and its license and third-party notices are installed with the runtime. `runtime/src/workspace/PLEAMAR-LICENSE` and `tools/reference/fixtures/workspace/PLEAMAR-LICENSE` carry its notice for the workspace rail code, which was first written inside a vendored Pleamar tree.
- The `J2BAR-LICENSE` files (`shell/bar/`, `shell/corners/`, `shell/sound/`, `runtime/src/workspace/`, `tools/reference/fixtures/workspace/`) are j2bar's MIT notice for layouts, assets, shaders and motion code ported from j2bar.
- `shell/sound/LICENSE.LUCIDE` and `shell/service-assets/LICENSE` are the ISC license for icons from, or based on, [Lucide](https://lucide.dev).
- The Hyprland plugin is MIT (`hyprland/LICENSE`). Its `focus/` and `bars/` components derive from [hyprwm/hyprland-plugins](https://github.com/hyprwm/hyprland-plugins) v0.56.0, under BSD-3-Clause (`hyprland/UPSTREAM-LICENSE`). See `hyprland/UPSTREAM.md` for provenance.
