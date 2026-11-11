# Building and running without Nix

Tohru runs on a patched Pleamar 0.7.27. Stock Pleamar will not run the bar, so you build the runtime from this repository first. This path is less tested than the Nix package.

## Requirements

Build:

- A recent stable Rust toolchain. Both crates use edition 2024, so 1.85 is the minimum; Pleamar's dependencies may need a newer one.
- A C and C++ compiler: Pleamar builds Luau from source.
- `pkg-config` and the development files for fontconfig, wayland, libxkbcommon, linux-pam and libffi.
- `curl`, `git`, `tar` and `sha256sum` for `prepare.sh`.

Run:

- A Wayland compositor with layer-shell. Tohru targets Hyprland.
- A Vulkan loader and libglvnd. Development used an NVIDIA GPU; other GPUs are untested.
- `setsid` (util-linux) and `sh` on `PATH`: Pleamar launches applications through `setsid -f sh -c`.
- For the popups to show anything: PulseAudio or PipeWire's PulseAudio server, NetworkManager, and BlueZ.

## Build the runtime

From the repository root:

```sh
./runtime/pleamar/prepare.sh
cargo build --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --bin pleamar
```

`prepare.sh` downloads Pleamar at revision `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`, checks its SHA-256, applies every patch in `runtime/pleamar/patches/` in name order, copies in the `tohru-shell` library and the Luau bindings, and replaces the lock file. It writes to `.tmp/pleamar-runtime` unless you pass another destination as its only argument, and refuses to overwrite an existing one. Delete the old destination to prepare again.

The binary is `.tmp/pleamar-runtime/target/release/pleamar`. To run the tests that cover Tohru's additions:

```sh
cargo test --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --lib tohru_
cargo test --locked --manifest-path runtime/Cargo.toml
```

Check that a scene parses:

```sh
.tmp/pleamar-runtime/target/release/pleamar --check shell/bar.plm
```

## Run it

Every script in `bin/` uses `TOHRU_RUNTIME` as the Pleamar binary, falling back to `pleamar` on `PATH`. Each wrapper finds the scenes relative to its own location, so run the scripts from the checkout. Add `bin/` to `PATH` rather than symlinking the scripts elsewhere.

```sh
export TOHRU_RUNTIME="$PWD/.tmp/pleamar-runtime/target/release/pleamar"
export PATH="$PWD/bin:$PATH"

tohru-run &                  # bar, popups, OSD, corners
tohru-session-run &          # session menu
tohru-launcher-run &         # launcher
tohru-notifications-run &    # notifications; stop any other notification daemon first

tohru status --json
tohru settings open
```

Each process is independent and can be restarted on its own. `tohru settings open` starts `tohru-settings-run` from the same directory as `tohru` when no Settings window is running.

To point a wrapper at different scene files, set `TOHRU_SCENE` (bar), `TOHRU_SESSION_SCENE`, `TOHRU_LAUNCHER_SCENE`, `TOHRU_NOTIFICATIONS_SCENE` or `TOHRU_SETTINGS_SCENE`.

## Starting with the session

The repository ships no systemd units outside the Home Manager module. To write your own, follow what that module generates (see [nix.md](nix.md#services)): one user service per wrapper, part of your graphical session target, restarting on failure, with `KillMode=process` for the launcher so restarting it leaves launched applications alone. Import `WAYLAND_DISPLAY`, `PATH` and `XDG_DATA_DIRS` into the user manager before the target starts. Starting the wrappers from your compositor's startup configuration also works, without restarts.

## Hyprland plugin

The plugin builds separately with `make`. It needs a C++23 GCC toolchain, `pkg-config`, and the development headers of the exact Hyprland 0.56.2 build you run, plus pixman, libdrm, pangocairo, libinput, libudev, wayland-server, xkbcommon and Lua.

```sh
cd hyprland
make check-deps
make -j1
make install PREFIX="$HOME/.local"
```

Then load `$HOME/.local/lib/libtohru-hyprland.so` from your Hyprland configuration with `hl.plugin.load(...)`, or with `hyprctl plugin load`. See [nix.md](nix.md#hyprland-plugin) for its settings and caveats.
