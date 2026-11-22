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
| `assistant.models.<cli>.model` | string, up to 128 of `A-Z a-z 0-9 . _ : / @ # + -` | `""` | The model for `claude`, `codex`, `gemini`, `agy` or `opencode`, passed to that CLI as is. Empty means Tohru's default for it: `claude-opus-5-5`, `gpt-6.2-sol`, `gemini-4-neon`, `gemini-4-neon`, and opencode's own configured model. A model the CLI rejects shows its error; nothing falls back |
| `assistant.models.<cli>.effort` | string | `""` | Reasoning effort: `minimal`, `low`, `medium`, `high`, `xhigh` or `max`, mapped to what each CLI accepts. Empty means `medium` for claude and codex, `high` for gemini and agy, and opencode's default |
| `assistant.persona` | boolean | `true` | Speak as Tohru (`shell/assistant/persona.md`). False sends prompts without the persona |
| `assistant.persona_file` | absolute path or `""` | `""` | A Markdown file used in place of the shipped persona |
| `assistant.memory` | boolean | `true` | Learn facts and preferences after a chat ends and give answers a short memory block. False runs no extraction and sends no memory; history is still kept |
| `assistant.orca` | string (`auto`, `true` or `false`) | `"auto"` | Give provider CLIs Orca CLI context (`ORCA_CLI_COMMAND`, `ORCA_USER_DATA_PATH` and a short prompt section pointing at `<cli> skills get`). `auto` detects Orca when installed (shim under `ORCA_USER_DATA_PATH` or `~/.config/orca/linux-orca-cli-shim/orca`, else `orca-ide` on `PATH`, never bare `orca`); `true` always includes Orca guidance; `false` disables detection, env and prompt context |
| `assistant.voice` | boolean | `true` | Run the voice service: wake word, conversation mode and the speaking dragon. False stops it, frees the microphone and every model, and hides the mic dot; the chat popup keeps working. Applies at once |
| `assistant.vad_model` | absolute path or `""` | `""` | The Silero VAD `.onnx` file. Required for voice. See [Voice models](#voice-models) |
| `assistant.asr_model` | absolute path or `""` | `""` | The sherpa-onnx streaming Zipformer folder: `tokens.txt`, `encoder*.onnx`, `decoder*.onnx`, `joiner*.onnx` (int8 files are preferred when both are present). Required for voice |
| `assistant.turn_model` | absolute path or `""` | `""` | The Smart Turn v3.2 `.onnx` file. Optional: empty means a turn ends only after 800 ms of silence, with no early end at a 200 ms pause |
| `assistant.voice_source` | string | `""` | Pulse/PipeWire microphone source; empty selects automatically. Settings → System cycles available sources and reports AEC metadata, not verified acoustic cancellation |
| `assistant.voice_wake` | string | `"asr"` | How "Tohru" is heard: `asr` runs the recogniser on all speech; `kws` runs a small keyword spotter first and the recogniser only after it fires. Both confirm the wake word from the transcript. See [ai-assistant.md](ai-assistant.md#wake-word) for the measurements behind the default |
| `assistant.kws_model` | absolute path or `""` | `""` | The sherpa-onnx keyword-spotter folder (same four kinds of file as `asr_model`). Required only with `voice_wake = "kws"` |
| `assistant.tts_engine` | string | `"kokoro"` | The dragon's speech engine: `kokoro` (Kokoro-82M v1.0 on the CPU) or `espeak-ng`. Kokoro falls back to espeak-ng, with an error, when its files are missing |
| `assistant.tts_model` | absolute path or `""` | `""` | Kokoro's folder, holding `model.onnx`, `tokenizer.json` and, for stock voicepacks, `voices/`. Empty means the dragon speaks with espeak-ng and the popup says why |
| `assistant.tts_voicepack` | string or absolute path | `"tide"` | Kokoro's voice: `tide` (Tohru's, shipped in `shell/assistant/voice/`), a stock pack name such as `af_heart` from the model's `voices/`, or a path to a 510×256 float32 `.bin` |
| `assistant.tts_load_on_conversation` | boolean | `false` | False keeps Kokoro loaded while voice is on (about 670 MB, answers start sooner). True loads it when a conversation starts and frees it when the conversation ends |
| `assistant.tts_voice` | string, 1 to 64 of `A-Z a-z 0-9 + - _ .` | `"en-us+f3"` | espeak-ng's voice name |
| `assistant.tts_speed` | integer, 50 to 200 | `100` | espeak-ng's speaking rate, in percent of its normal |
| `assistant.tts_pitch` | integer, −12 to 12 | `3` | espeak-ng's pitch shift in semitones |

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

## Voice models

Tohru ships no model weights, downloads nothing and never looks for them on its own: each path below is empty until you set it. With `assistant.voice` on, the voice service checks the paths before loading anything. If `vad_model` or `asr_model` (or `kws_model` with `voice_wake = "kws"`) is unset or wrong, it loads nothing, opens no microphone, and the assistant popup shows one error naming each setting and the file it expects. The chat keeps working. A missing or wrong `tts_model` falls back to espeak-ng and shows the error; an unset `turn_model` ends turns on silence alone, and a wrong one does the same with an error.

These are the files Tohru was tested with:

| Setting | Model | Source | Files (size, SHA-256) | Licence |
|---|---|---|---|---|
| `vad_model` (file) | Silero VAD v6.2.3 | [snakers4/silero-vad](https://github.com/snakers4/silero-vad) at `5cd7945676eb32225748052e2e6a0580e4686a08`, `src/silero_vad/data/silero_vad.onnx` | `silero_vad.onnx`, any name: 2,327,524 bytes, `1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3` | MIT |
| `asr_model` (folder) | sherpa-onnx streaming Zipformer, English, 2023-06-21 | Hugging Face [`csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21`](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21) at `9a65b6ea94c311ca770c2bf895b30f456a22d703` | `encoder-epoch-99-avg-1.int8.onnx` 187,823,992 bytes, `32c98281c7bd8b63e3e142d007251b37f120572e8fdea9a4f5a79ce22b10ec4f`; `decoder-epoch-99-avg-1.onnx` 2,092,566 bytes, `9da02b77cb08826756ec6a88635f35a40374e4164e7c6359121a9145958a6ceb`; `joiner-epoch-99-avg-1.int8.onnx` 259,335 bytes, `831477d390e59a61f1b6a6f763b9903e6c6366ff6034f1ddba613be82637122f`; `tokens.txt` 5,048 bytes (git blob `b4d1bf82e68de1354f2a3f5fd51094d61d65c77c`) | Apache-2.0 |
| `turn_model` (file, optional) | Smart Turn v3.2, CPU | Hugging Face [`pipecat-ai/smart-turn-v3`](https://huggingface.co/pipecat-ai/smart-turn-v3) at `f766f81d3cfdf7737ac64aad813d91bbfd56bf93` | `smart-turn-v3.2-cpu.onnx` 8,679,182 bytes, `2bb026316b14a660486a75b1733cd3fbab8c2fd0314dc9af7be49f8cca967e4f` | BSD-2-Clause |
| `tts_model` (folder) | Kokoro-82M v1.0, fp32 ONNX | Hugging Face [`onnx-community/Kokoro-82M-v1.0-ONNX`](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX) at `1939ad2a8e416c0acfeecc08a694d14ef25f2231` | `onnx/model.onnx` saved as `model.onnx`: 325,532,232 bytes, `8fbea51ea711f2af382e88c833d9e288c6dc82ce5e98421ea61c058ce21a34cb`; `tokenizer.json` 3,497 bytes; `voices/*.bin` (55 packs, 522,240 bytes each, 28.7 MB in all) only for stock voices such as `af_heart`. Tohru's own `tide` ships with Tohru | Apache-2.0 |
| `kws_model` (folder, only for `voice_wake = "kws"`) | sherpa-onnx KWS Zipformer, GigaSpeech, 3.3M | [k2-fsa/sherpa-onnx release `kws-models`](https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2): 17,626,723 bytes, `f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a` | The unpacked folder; Tohru uses `encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx` (4,807,159 bytes), `decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx` (277,985), `joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx` (163,380) and `tokens.txt` (5,006) | Apache-2.0 |

Upstream folders that also hold fp32 files work as they are: the service takes the int8 encoder, decoder and joiner when there is one. The required files and Smart Turn come to about 527 MB; the stock voicepacks add 28.7 MB and the spotter's int8 files 5.3 MB.

Example, with the files under `~/models` (paths must be absolute):

```toml
[assistant]
voice = true
vad_model = "/home/me/models/silero-vad/silero_vad.onnx"
asr_model = "/home/me/models/sherpa-onnx-streaming-zipformer-en-2023-06-21"
turn_model = "/home/me/models/smart-turn-v3/smart-turn-v3.2-cpu.onnx"
tts_model = "/home/me/models/kokoro-82m"
# Only with voice_wake = "kws":
# voice_wake = "kws"
# kws_model = "/home/me/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
```

`/home/me/models/kokoro-82m` holds `model.onnx`, `tokenizer.json` and, optionally, `voices/`. The same paths with `tohru config set assistant.vad_model /home/me/models/silero-vad/silero_vad.onnx` and so on, or in [Home Manager](nix.md#voice-models).

Echo cancellation is configured at the PipeWire level via the Home Manager option `programs.tohru.assistant.voice.echoCancel` (enabled by default whenever `programs.tohru.assistant.voice.enable` is true), which installs `~/.config/pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf` without changing the default audio devices. `TOHRU_VOICE_SOURCE` and `TOHRU_VOICE_SINK` override the PipeWire capture source and TTS playback sink that `tohru-voice` selects.

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
