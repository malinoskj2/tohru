# Building and running without Nix

Tohru runs on a patched Pleamar 0.7.27. Stock Pleamar will not run the bar, so you either download it with a Tohru release or build it from this repository. This path is less tested than the Nix package.

## Requirements

Build:

- Rust 1.90 or newer. Pleamar's locked dependencies need it (cosmic-text needs 1.89, ordered-float 1.90); the `tohru-shell` library on its own builds and passes its tests with 1.85, its declared `rust-version`.
- A C and C++ compiler: Pleamar builds Luau from source.
- `pkg-config` and the development files for fontconfig, wayland, libxkbcommon, linux-pam and libffi.
- `curl`, `git`, `tar`, `base64` and `sha256sum` for `prepare.sh`.

Run:

- Linux with systemd/logind and a Wayland compositor with layer-shell (including Settings). Hyprland IPC and standard-protocol capabilities are described in [standalone.md](standalone.md).
- Bash, Python 3.11+, coreutils and util-linux. Run `tohru doctor` for feature-specific dependencies.
- A Vulkan loader and libglvnd. Development used an NVIDIA GPU; other GPUs are untested.
- `setsid` (util-linux) and `sh` on `PATH`: Pleamar launches applications through `setsid -f sh -c`.
- For the popups to show anything: PulseAudio or PipeWire's PulseAudio server, NetworkManager, and BlueZ.

## Use a release

Instead of building the runtime, you can download it. Each release has `tohru-x86_64-linux.tar.gz`, built on Ubuntu 22.04 by the repository's release workflow. It unpacks to `tohru/`: the patched `pleamar` binary next to the scripts in `bin/`, the scenes in `shell/`, and the licences of everything compiled into the binary in `LICENSE` and `licenses/`.

```sh
curl -LO https://github.com/malinoskj2/tohru/releases/latest/download/tohru-x86_64-linux.tar.gz
curl -LO https://github.com/malinoskj2/tohru/releases/latest/download/SHA256SUMS
sha256sum --check SHA256SUMS
gh attestation verify tohru-x86_64-linux.tar.gz --repo malinoskj2/tohru
tar -xzf tohru-x86_64-linux.tar.gz
export PATH="$PWD/tohru/bin:$PATH"
```

`sha256sum` only shows that the download is intact. `gh attestation verify` (the GitHub CLI) checks the tarball's build-provenance attestation, which shows that this repository's release workflow built it and from which commit.

The binary needs glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 or later), libstdc++, libxkbcommon and linux-pam. It loads libwayland-client, libwayland-egl, libEGL (from libglvnd or Mesa) and the Vulkan loader, `libvulkan.so.1`, while it runs. On Debian and Ubuntu these are `libxkbcommon0 libpam0g libwayland-client0 libwayland-egl1 libegl1 libvulkan1`, plus your GPU's Vulkan driver.

With `tohru/bin` on `PATH` the scripts find `pleamar` there, so continue at [Run it](#run-it) without its `TOHRU_RUNTIME` line. The release has no Hyprland plugin: the plugin loads only into the exact Hyprland build it was compiled against, so build it for yours as described under [Hyprland plugin](#hyprland-plugin).

## Build the runtime

From the repository root:

```sh
./runtime/pleamar/prepare.sh
cargo build --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --bin pleamar
```

`prepare.sh` downloads the Pleamar archive named in `runtime/pleamar/pin.json` (revision `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`), checks its SHA-256 against the pin, applies every patch in `runtime/pleamar/patches/` in name order, copies in the `tohru-shell` library and the Luau bindings, and replaces the lock file. It writes to `.tmp/pleamar-runtime` unless you pass another destination as its only argument, and refuses to overwrite an existing one. Delete the old destination to prepare again.

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

Use `tohru session-units --output ./units --prefix /absolute/path/to/tohru/bin`
to generate the shared service definitions without activating anything. Install,
enable, import environment, restart, inspect journals and disable them as shown in
[standalone.md#services-and-environment](standalone.md#services-and-environment).
Optional notification/polkit/wallpaper/idle ownership is explicit. A generated
on-demand locker uses native compositor-confirmed readiness before suspend.

## Voice

See [the complete standalone voice environment instructions](standalone.md#voice-without-nix) for pinned dependency metadata, explicit environment configuration and offline behavior.

Voice needs the `tohru-voice` program built from `voice/` ([native-voice.md](native-voice.md#build)), with `ORT_DYLIB_PATH` naming an ONNX Runtime library, and `parec`, `pacat`, `pactl` and `espeak-ng` on `PATH`. Without them, set `assistant.voice = false`; the chat popup works either way.

### Voice models

Nothing is downloaded. Fetch the models listed in [configuration.md](configuration.md#voice-models), at the revisions and hashes given there, to any folder, and set their absolute paths:

```sh
tohru config set assistant.vad_model "$HOME/models/silero-vad/silero_vad.onnx"
tohru config set assistant.asr_model "$HOME/models/sherpa-onnx-streaming-zipformer-en-2023-06-21"
tohru config set assistant.turn_model "$HOME/models/smart-turn-v3/smart-turn-v3.2-cpu.onnx"
tohru config set assistant.tts_model "$HOME/models/kokoro-82m"
```

For example, from Hugging Face:

```sh
hf download csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21 --revision 9a65b6ea94c311ca770c2bf895b30f456a22d703 \
  encoder-epoch-99-avg-1.int8.onnx decoder-epoch-99-avg-1.onnx joiner-epoch-99-avg-1.int8.onnx tokens.txt \
  --local-dir ~/models/sherpa-onnx-streaming-zipformer-en-2023-06-21
hf download pipecat-ai/smart-turn-v3 --revision f766f81d3cfdf7737ac64aad813d91bbfd56bf93 smart-turn-v3.2-cpu.onnx --local-dir ~/models/smart-turn-v3
hf download onnx-community/Kokoro-82M-v1.0-ONNX --revision 1939ad2a8e416c0acfeecc08a694d14ef25f2231 onnx/model.onnx tokenizer.json \
  --local-dir ~/models/kokoro-82m && mv ~/models/kokoro-82m/onnx/model.onnx ~/models/kokoro-82m/model.onnx
hf download onnx-community/Kokoro-82M-v1.0-ONNX --revision 1939ad2a8e416c0acfeecc08a694d14ef25f2231 --include 'voices/*' \
  --local-dir ~/models/kokoro-82m   # only for stock voices such as af_heart
mkdir -p ~/models/silero-vad && curl -L -o ~/models/silero-vad/silero_vad.onnx \
  https://raw.githubusercontent.com/snakers4/silero-vad/5cd7945676eb32225748052e2e6a0580e4686a08/src/silero_vad/data/silero_vad.onnx
```

Check each file against the SHA-256 in that table. Until `vad_model` and `asr_model` are valid the assistant popup names what is missing.

## Hyprland plugin

The plugin builds separately with `make`. It needs a C++23 GCC toolchain, `pkg-config`, and the development headers of the exact Hyprland 0.57.2 build you run, plus pixman, libdrm, pangocairo, libinput, libudev, wayland-server, xkbcommon and Lua.

```sh
cd hyprland
make check-deps
make -j1
make install PREFIX="$HOME/.local"
```

Then load `$HOME/.local/lib/libtohru-hyprland.so` from your Hyprland configuration with `hl.plugin.load(...)`, or with `hyprctl plugin load`. See [nix.md](nix.md#hyprland-plugin) for its settings and caveats.

## Discord bot support

Optional [Discord read/post support](discord.md) uses the native `tohru-discord` command and, for the assistant's per-turn broker, Python 3.11+ (including its standard-library SSL module and system CA certificates). Keep `bin/tohru-discord` and `bin/tohru_discord.py` beside `bin/tohru-assistant`; releases and the Nix package include them. Configure it in Tohru's normal TOML settings; no external service or additional Python package is needed.
