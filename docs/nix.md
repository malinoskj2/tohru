# Nix

The flake only exports `x86_64-linux`.

## Flake outputs

| Output | What it is |
|---|---|
| `packages.x86_64-linux.tohru` (also `default`) | The shell: scenes under `share/tohru/shell`, the `tohru` CLI and every launch wrapper, all pointed at the patched runtime |
| `packages.x86_64-linux.pleamar-runtime` | The patched Pleamar 0.7.27 runtime on its own |
| `packages.x86_64-linux.tohru-hyprland` | The Hyprland plugin, built against Hyprland 0.57.2 from the `hyprland-nixpkgs` input |
| `apps.x86_64-linux.default` (also `shell`) | Runs `tohru-run`, the bar |
| `apps.x86_64-linux.tohru` | Runs the `tohru` CLI |
| `overlays.default` | Adds `tohru-hyprland`, built against the Hyprland in the package set it is applied to |
| `homeManagerModules.tohru` (also `default`) | The Home Manager module below |
| `nixosModules.tohru-greeter` (also `default`) | NixOS module configuring `greetd` with `tohru-greeter` |

The NixOS module `nixosModules.tohru-greeter` configures the pre-session login screen via `greetd` (see [Login screen (NixOS)](#login-screen-nixos) below).

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
| `tohru-lock` | The lock screen; exits when unlocked. `tohru-lock --running` exits 0 while one runs |
| `tohru-greeter` | The `greetd` login screen; speaks `GREETD_SOCK`, never PAM directly |
| `tohru-polkit-run` | The polkit authentication agent and its dialog (idle unless `polkit.agent` is true) |
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
| `programs.tohru.polkit.enable` | `false` | Sets `polkit.agent = true` in the managed file and, with `systemd.enable`, adds the `tohru-polkit` service. Turn off any other polkit agent first: see [MIGRATION.md](../MIGRATION.md#polkit-agent) |
| `programs.tohru.systemd.target` | `"graphical-session.target"` | The user target that starts and stops those services |
| `programs.tohru.lock.style` | `null` | Sets the managed `lock.style` |
| `programs.tohru.notifications.position` | `null` | Sets the managed `notifications.position` |
| `programs.tohru.assistant.voice.enable` | `true` | False writes `assistant.voice = false` and builds the package without the voice service or its Python dependencies |
| `programs.tohru.assistant.voice.echoCancel` | `false` | Installs `~/.config/pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf` (`libpipewire-module-echo-cancel`, WebRTC AEC) with `tohru-echo-cancel-source` and `tohru-echo-cancel-sink` at priority 0 so default audio devices stay unchanged |
| `programs.tohru.assistant.voice.source` | `null` | Sets `assistant.voice_source`; empty selects automatically |
| `programs.tohru.assistant.voice.wake` | `null` | Sets the managed `assistant.voice_wake` (`kws` or `asr`) |
| `programs.tohru.assistant.voice.vadModel` | `null` | Sets the managed `assistant.vad_model` |
| `programs.tohru.assistant.voice.asrModel` | `null` | Sets the managed `assistant.asr_model` |
| `programs.tohru.assistant.voice.turnModel` | `null` | Sets the managed `assistant.turn_model` |
| `programs.tohru.assistant.voice.kwsModel` | `null` | Sets the managed `assistant.kws_model` |
| `programs.tohru.assistant.tts.model` | `null` | Sets the managed `assistant.tts_model` |
| `programs.tohru.assistant.tts.loadOnConversation` | `null` | Sets the managed `assistant.tts_load_on_conversation` |
| `programs.tohru.assistant.voiceProfiles.vramThresholdPercent`, `.hiddenUnloadSeconds`, `.queueTimeoutSeconds`, `.modelRoot` | `null` | Set the managed `assistant.voice_profiles.*`. See [Voice profiles](configuration.md#voice-profiles) |
| `programs.tohru.assistant.voiceProfiles.low.precision` | `null` | Sets the managed `assistant.voice_profiles.low.precision` |
| `programs.tohru.assistant.voiceProfiles.high.enable`, `.precision`, `.python`, `.ttsModel`, `.ttsSpeaker`, `.asrModel`, `.vramMib` | `null` | Set the managed `assistant.voice_profiles.high.*` (`enable` writes `enabled`) |
| `programs.tohru.assistant.provider` | `null` | Sets the managed `assistant.provider` |
| `programs.tohru.assistant.backend` | `null` | Sets the managed `assistant.backend` (`local` or `orca`) |
| `programs.tohru.assistant.orcaBot.enable` | `false` | Opt-in: sets `assistant.backend = "orca"` so popup and voice route through an Orca bot (inert when false) |
| `programs.tohru.assistant.orcaBot.bot`, `.voiceChat`, `.offline` | `null` | Set the managed `assistant.orca_bot`, `assistant.orca_voice_chat` (`foreground`\|`home`\|`new`) and `assistant.orca_offline` (`error`\|`local`). Without an explicit value, voice uses the shared `home` conversation. |
| `programs.tohru.assistant.orcaBot.needsYouMinutes` | `null` | Set the managed `assistant.orca_needs_you_minutes` (0 to 1440; `0` turns the Needs-you watcher off) |
| `programs.tohru.assistant.models.<cli>.model`, `.effort` | `null` | Set the managed `assistant.models.<cli>.*` for `claude`, `codex`, `gemini`, `agy` or `opencode` |
| `programs.tohru.assistant.persona.enable`, `.file` | `null` | Set the managed `assistant.persona` and `assistant.persona_file` |
| `programs.tohru.assistant.memory.enable` | `null` | Sets the managed `assistant.memory` |
| `programs.tohru.assistant.orca` | `null` | Sets the managed `assistant.orca` (`"auto"`, `"true"`, `"false"`, or `true`/`false`) |
| `programs.tohru.lock.command` | read-only | `tohru-lock` with `TOHRU_MANAGED_CONFIG` set, for hypridle's `lock_cmd` |
| `programs.tohru.hyprlandPlugin.package` | Plugin built against your session's Hyprland | The plugin package. Nothing loads it automatically |

Everything is inert unless `enable` is true; `systemd.enable` alone adds no services.

### Voice

The package takes `withVoice ? true`. With it, `tohru-voice` is installed with a Python carrying numpy, onnxruntime, sherpa-onnx and misaki (with num2words, phonemizer and spaCy's `en_core_web_sm` for its fallback), and `pulseaudio` (`parec`, `pacat`, `pactl`) and `espeak-ng` on its `PATH`; it sets `HF_HUB_OFFLINE=1` and `OPENBLAS_NUM_THREADS=1`. Without it, none of that is in the closure and the bar never starts a voice service.

#### Voice models

No weights are packaged, fetched or looked up: the module only writes the paths you give it, and a path left `null` stays unset. Fetch the files yourself (the sources, revisions and hashes are in [configuration.md](configuration.md#voice-models)) and point the options at them. Strings are used as is, so a path outside the store is not copied into it.

```nix
programs.tohru.assistant = {
  voice.vadModel = "/srv/models/silero-vad/silero_vad.onnx";
  voice.asrModel = "/srv/models/sherpa-onnx-streaming-zipformer-en-2023-06-21";
  voice.turnModel = "/srv/models/smart-turn-v3/smart-turn-v3.2-cpu.onnx";   # optional
  tts.model = "/srv/models/kokoro-82m";   # model.onnx, tokenizer.json, voices/
  # voice.wake = "kws";
  # voice.kwsModel = "/srv/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01";
};
```

`vadModel` and `asrModel` are required for voice; without them the popup shows which are missing and the chat keeps working. Tohru's own voicepack, tide, ships in the package; without `tts.model` the dragon speaks with espeak-ng and the popup shows why.

### Services

With `systemd.enable`, the module defines the selected components plus an on-demand locker. `systemd.components` defaults to bar, session, launcher and notifications. Add idle/wallpaper only after explicit ownership configuration. It reads the same service specification as standalone `tohru session-units`:

| Unit | Runs |
|---|---|
| `tohru-bar` | `tohru-run` |
| `tohru-session` | `tohru-session-run` |
| `tohru-launcher` | `tohru-launcher-run` |
| `tohru-notifications` | `tohru-notifications-run` |
| `tohru-polkit` | `tohru-polkit-run`, with `polkit.enable` or explicit component selection |
| `tohru-wallpaper` | `tohru-wallpaper serve`, optional |
| `tohru-idle` | `tohru-idle`, optional |
| `tohru-shortcuts` | optional app-scoped portal owner; `settings.shortcuts.mode = "portal"`, then explicit Settings Setup |
| `tohru-lock` | on-demand `tohru-lock`, Type=notify; no WantedBy |

Each selected startup service is `WantedBy`, `PartOf` and `After` the configured target, restarts on failure after 2 seconds, and only starts when `WAYLAND_DISPLAY` is in the user manager's environment. Each gets `TOHRU_MANAGED_CONFIG` in its unit environment. `tohru-launcher` uses `KillMode=process`, so restarting it does not kill applications it launched. Settings is not a service; it starts on demand.

The services inherit the systemd user manager's environment. Your compositor session must import `WAYLAND_DISPLAY`, plus the `PATH` and `XDG_DATA_DIRS` the launcher uses to find and start applications, before the target starts. UWSM does this, or run this from your compositor's startup:

```sh
systemctl --user import-environment WAYLAND_DISPLAY PATH XDG_DATA_DIRS
```

If your Hyprland setup provides its own target, such as `hyprland-session.target`, set `systemd.target` to it.

`tohru-notifications` must be the only notification daemon, and `tohru-polkit` the only polkit agent. Disable mako, dunst, swaync or similar first. These units are new and have not been tested on a real session.

Other programs get `TOHRU_MANAGED_CONFIG` through `home.sessionVariables`, which only applies to sessions started from a login shell. `tohru settings open` does not depend on that: when the variable is unset or empty, it asks the running bar which managed file it loaded and starts Settings with the same one.

## Lock screen

```nix
services.hypridle.settings.general.lock_cmd = config.programs.tohru.lock.command;
```

and on NixOS:

```nix
security.pam.services.tohru-lock = { };
```

Without that service the locker uses `pleamar`'s or `hyprlock`'s PAM service if one exists, else `login`'s. On Hyprland keep `misc:allow_session_lock_restore = true`, so that after a crash the plain style can take over the lock. The session menu starts the on-demand locker and waits for native compositor-confirmed readiness before suspend. External hypridle can still use the foreground locker command; optional `settings.idle.owner = "tohru"` and the idle component provide standalone swayidle ownership. Do not enable both owners. `tohru lock` still starts the locker directly.

## Login screen (NixOS)

Import `inputs.tohru.nixosModules.tohru-greeter` (or `default`) in your NixOS configuration:

```nix
{ inputs, ... }:
{
  imports = [ inputs.tohru.nixosModules.tohru-greeter ];

  services.tohru-greeter = {
    enable = true;
    style = "flooded-monolith"; # any of the 10 lock styles, or "plain"
    preferredOutput = "DP-2";
    defaultUser = "alice";
    defaultSession = "hyprland-uwsm";
    monitors = [
      { output = "DP-1"; mode = "1920x1080@144"; position = "0x0"; transform = 1; }
      { output = "DP-2"; mode = "1920x1080@360"; position = "1080x0"; }
    ];
    # Optional static wallpaper (null uses the dusk gradient) and optional per-user avatars:
    # wallpaper = ./greeter-wallpaper.png;
    # avatars.alice = ./avatar.png;
  };
}
```

The module configures `services.greetd` on VT1 (leaving `tty2–tty6` live for `Ctrl+Alt+F2` recovery), hosts `tohru-greeter` in a minimal isolated Hyprland (or Sway) session with a `tuigreet` fallback on compositor error, adds `video`/`render` groups to `greeter`, enables `services.accounts-daemon` so `tohru avatar set FILE` works across `0700` home directories, and installs a scoped Polkit rule for Sleep/Restart/Shut Down. The module installs Noto fonts system-wide; native appearance font settings also apply to the greeter. No SF Pro installation is required. `nix/tests/nixos.nix` checks the NixOS module evaluation.

The Hyprland greeter config is Lua, so `compositor = "hyprland"` needs Hyprland 0.57 or later; the module asserts this. `extraCompositorConfig` is appended verbatim to that config, so for Hyprland it must be Lua (`hl.config({ ... })`, `hl.monitor({ ... })`, `hl.env(...)`), and for Sway it is Sway config syntax. The greeter starts on `hyprland.start` and leaves Hyprland with `hyprctl dispatch 'hl.dsp.exit()'` once it exits.

greetd joins a session's `cmd` with spaces and runs it through `/bin/sh -c`, so `tohru-greeter` sends every `Exec=` token single-quoted.

`checks.x86_64-linux.greeter-hyprland` and `checks.x86_64-linux.greeter-sway` (`nix/tests/greeter.nix`) boot a VM with real greetd and PAM, type a test user's password into the greeter and check that the chosen session starts with the right argv, stays running without a respawn loop, and that the greeter compositor exits. They render in software and set `PLEAMAR_SWAPCHAIN=1`, because Hyprland's llvmpipe renderer can't import Pleamar's own dmabuf frames and disconnects the greeter:

```sh
resource-run --cpu 4 --ram 6GiB --project tohru --name "tohru greeter VM test" --category other \
  --memory-policy strict -- nix build -L --no-link .#checks.x86_64-linux.greeter-hyprland
```

## Hyprland plugin

`tohru-hyprland` is optional. The shell works without it. It must be built against the exact Hyprland your session runs: the package asserts Hyprland 0.57.2, and the plugin refuses to load if its build headers do not match the running compositor.

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

Its animation leaves (`hyprfocusIn`, `hyprsheetIn`, `hyprrecedeTilt` and the rest) exist only once the plugin is loaded. At startup that is after your configuration has run, and `hl.animation` on an unknown leaf raises an error that stops the rest of the file. `hyprland/examples/effects.lua` sets them through `pcall` and again on `hyprland.start`. Its unload and damage checks have not been done. See `hyprland/README.md` and `hyprland/VALIDATION.md`.

`hyprland/` also has its own standalone flake (`nix build path:./hyprland`).

## Pinning

- `nixpkgs` is pinned to `4fe25e12eb9f4c744966dd3de5a05246cf098601`. It builds the `packages` outputs. You can make it follow yours with `inputs.tohru.inputs.nixpkgs.follows = "nixpkgs";`.
- `hyprland-nixpkgs` is pinned to `2786a2a72088ecf32aea5b3ec52099e6a7cff78f`, the package set with Hyprland 0.57.2. It is only used for `packages.x86_64-linux.tohru-hyprland`. Do not make it follow a nixpkgs that has a different Hyprland.
- The Home Manager module builds `programs.tohru.package` and the plugin from the consumer's `pkgs`, not from the flake's pins. To use the flake's own build instead, set `programs.tohru.package = inputs.tohru.packages.x86_64-linux.default;`.
- Pleamar is pinned separately in `runtime/pleamar/pin.json`: version 0.7.27, revision `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`, with the archive hash. Both the Nix package and `runtime/pleamar/prepare.sh` read the archive and hash from `pin.json`, and both apply every `.patch` file in `runtime/pleamar/patches/` in name order. Changing the pin means editing `pin.json`, rebasing those patches and updating `runtime/pleamar/Cargo.lock`.

`nix/tests/home-manager.nix` evaluates the module against a Home Manager input you supply and checks the options and units above.

Standalone services, read-only doctor, greetd templates and PipeWire setup are documented in [standalone.md](standalone.md). Managed `settings` can set all wallpaper, idle, font and voice environment keys without adding Nix-specific runtime paths. `systemd.components` uses the shared `share/tohru/session/services.json`; do not install generated standalone units on top of HM units.

The optional shortcuts component packages dbus-fast independently of voice. Compositor-managed bindings remain read-only; portal registration/configuration is explicitly user-triggered and publishes backend-confirmed triggers. See the standalone shortcut instructions for v1/v2 support and restart/grant semantics.
