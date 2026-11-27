# Configuration

## Files

Tohru reads two TOML files:

- The writable file, `$XDG_CONFIG_HOME/tohru/config.toml`, or `~/.config/tohru/config.toml` when `XDG_CONFIG_HOME` is unset. Settings and `tohru config set` write here.
- An optional read-only managed file, named by the `TOHRU_MANAGED_CONFIG` environment variable. The Home Manager module generates it from `programs.tohru.settings`. It must not be the same file as the writable one. An empty value means no managed file. When `tohru settings open` starts Settings without the variable set, it gives Settings the managed file the running bar loaded.

Only the bar and Settings processes read settings; the bar forwards `notifications.position` to `tohru-notifications`. Nothing is imported from j2bar.

## Keys

Optional desktop-idle dragon settings and their current implementation status
are documented in [Desktop idle dragon scenes](assistant-afk.md).

| Key | Type | Default | Effect |
|---|---|---|---|
| `bar.preferred_output` | string, up to 256 characters | `""` | Output name (such as `DP-1`) for the bar, its popups and the volume OSD. Empty means every output. With an output set, workspace monitor metadata filters its ribbon; unknown metadata remains visible |
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
| `assistant.backend` | string | `"local"` | Which backend runs chats: `local` (default, local CLIs + SQLite, zero Orca dependency) or opt-in `orca` (routes desktop popup and voice through an Orca bot). See [assistant-bot-abilities.md](assistant-bot-abilities.md) |
| `assistant.orca_bot` | string, 1 to 64 of `A-Z a-z 0-9 _ . -` and space | `"Tohru"` | The Orca bot name used when `assistant.backend = "orca"` |
| `assistant.orca_voice_chat` | string | `"home"` | Home shares the public GUI, desktop text and voice history. Opt-in `quick` answers spoken turns first with a fast responder and hands work to Home, without creating another chat. `foreground` retains the earlier separate-chat route; `new` starts a fresh session. See [quick voice](#quick-voice). |
| `assistant.orca_offline` | string | `"error"` | What happens when `assistant.backend = "orca"` and Orca isn't running: `error` (shows an offline note so state never splits) or `local` (falls back to local CLIs) |
| `assistant.orca_needs_you_minutes` | integer, 0 to 1440 | `5` | How often, in minutes, the bar asks Orca which of the bot's items are in Needs you when `assistant.backend = "orca"`; `0` turns the watcher off. Each item that newly needs you wakes the bot once in its Home Chat to say what it is and what it asks, on the desktop and aloud (see [ai-assistant.md](ai-assistant.md)) |
| `assistant.quick_voice_model` | string, `claude-` then up to 121 of `A-Z a-z 0-9 . _ : / @ # + -` | `"claude-sonnet-5-5"` | The Claude model the `quick` voice responder answers with. It is separate from `assistant.models.claude.model`, which Home and local chats keep using. An invalid value falls back to the default |
| `assistant.quick_voice_effort` | string: `minimal`, `low`, `medium`, `high`, `xhigh` or `max` | `"low"` | The quick voice responder's effort |
| `assistant.quick_voice_cache` | boolean | `false` | Whether the quick voice responder caches its rarely changing records (items, worktrees, workers) for five minutes. A warm call costs less and a cold one more, so it pays off only for questions in quick succession |
| `assistant.quick_voice_transport` | string: `api` or `cli` | `"api"` | How the quick voice responder reaches Claude. `api` streams from the Messages API over a connection opened while the turn settles, so speech starts with the first words. It needs an Anthropic API key and falls back to the Claude CLI when none is set or the call fails before any text. `cli` always runs the Claude CLI |
| `assistant.anthropic_key_file` | string: absolute path, or empty | `""` | A file holding the Anthropic API key for `quick_voice_transport = "api"`: the bare key, or a dotenv `ANTHROPIC_API_KEY=` line. `ANTHROPIC_API_KEY` in the environment takes precedence. Empty means `$XDG_CONFIG_HOME/tohru/anthropic-key` (default `~/.config/tohru/anthropic-key`) in the same format, if it exists; keep it mode `600`, since Tohru warns, naming the path and mode, when group or others can read it. Tohru reads the key only to send it to the API and never logs or stores it |
| `assistant.models.<cli>.model` | string, up to 128 of `A-Z a-z 0-9 . _ : / @ # + -` | `""` | The model for `claude`, `codex`, `gemini`, `agy` or `opencode`, passed to that CLI as is. Empty means Tohru's default for it: `claude-opus-5-5`, `gpt-6.2-sol`, `gemini-4-neon`, `gemini-4-neon`, and opencode's own configured model. A model the CLI rejects shows its error; nothing falls back |
| `assistant.models.<cli>.effort` | string | `""` | Reasoning effort: `minimal`, `low`, `medium`, `high`, `xhigh` or `max`, mapped to what each CLI accepts. Empty means `medium` for claude and codex, `high` for gemini and agy, and opencode's default |
| `assistant.voice_models.<cli>.model` / `.effort` | same model/effort formats | `""` | Overrides for tool-free spoken drafts; provider settings and scenario defaults supply unset values |
| `assistant.semantic_models.<cli>.model` / `.effort` | same model/effort formats | `""` | Overrides for optional semantic turn decisions and their draft answers; does not enable semantic detection |
| `assistant.persona` | boolean | `true` | Speak as Tohru (`shell/assistant/persona.md`). False sends prompts without the persona |
| `assistant.persona_file` | absolute path or `""` | `""` | A Markdown file used in place of the shipped persona |
| `assistant.memory` | boolean | `true` | Learn facts and preferences after a chat ends and give answers a short memory block. False runs no extraction and sends no memory; history is still kept |
| `assistant.discord.*` | booleans/strings/integer | off/empty; timeout 60 s | Optional bot integration: `enabled`, `read`, `post`, `token_file`, `guild_id`, `channels`, `allowed_user_id`, `desktop_first`, `desktop_timeout_seconds` (10–300). An explicit user ID is required. Desktop-first optionally requires a failed/unacknowledged desktop attempt before every post. See [Discord setup and boundaries](discord.md) |
| `assistant.orca` | string (`auto`, `true` or `false`) | `"auto"` | Give provider CLIs Orca CLI context (`ORCA_CLI_COMMAND`, `ORCA_USER_DATA_PATH` and a short prompt section pointing at `<cli> skills get`). `auto` detects Orca when installed (shim under `ORCA_USER_DATA_PATH` or `~/.config/orca/linux-orca-cli-shim/orca`, else `orca-ide` on `PATH`, never bare `orca`); `true` always includes Orca guidance; `false` disables detection, env and prompt context |
| `assistant.voice` | boolean | `true` | Run the voice service: wake word, conversation mode and the speaking dragon. False stops it, frees the microphone and every model, and hides the mic dot; the chat popup keeps working. Applies at once |
| `assistant.sheet_b_turn` | boolean | `false` | Play the Sheet-B model's turn when she lands from a right glide (profile right to front) and when a right-click dismisses her (front to profile right). The rest of the dragon is still the illustrated model, so each turn cuts between the two models at its ends. |
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
| `assistant.voice_profiles.vram_threshold_percent` | integer, 1 to 100 | `50` | When Tohru appears, the high-VRAM profile loads only below this GPU VRAM use. See [Voice profiles](#voice-profiles) |
| `assistant.voice_profiles.hidden_unload_seconds` | integer, 0 to 86400 | `5` | How long she stays hidden before the high-VRAM profile unloads and releases its GPU reservation; `0` unloads at once |
| `assistant.voice_profiles.queue_timeout_seconds` | integer, 1 to 3600 | `60` | How long the high-VRAM profile waits in the GPU scheduler's queue before staying on the low profile |
| `assistant.voice_profiles.model_root` | absolute path or `""` | `""` | The shared model root with `catalog.json`, `readiness.json` and `by-name/`; empty reads `$MODEL_ROOT` |
| `assistant.voice_profiles.low.device` | `cpu` | `"cpu"` | The low-VRAM profile, always loaded. Its models are `vad_model`, `asr_model`, `turn_model`, `kws_model` and `tts_model` above |
| `assistant.voice_profiles.low.precision` | `int8` or `float32` | `"int8"` | Which recogniser and keyword-spotter `.onnx` files the low profile prefers when a folder has both |
| `assistant.voice_profiles.high.enabled` | boolean | `false` | Allow the high-VRAM profile. Off, nothing touches the GPU |
| `assistant.voice_profiles.high.device` | `cuda` | `"cuda"` | Where the high-VRAM profile loads |
| `assistant.voice_profiles.high.precision` | `bfloat16`, `float16` or `float32` | `"bfloat16"` | Tensor precision of the high-VRAM models |
| `assistant.voice_profiles.high.python` | absolute path or `""` | `""` | An interpreter with torch (CUDA), numpy and `faster-qwen3-tts` (plus `faster-whisper` when `asr_model` is set). Required when `enabled`; Tohru installs nothing |
| `assistant.voice_profiles.high.tts_model` | `$MODEL_ROOT` alias | `"voice-tts"` | A Qwen3-TTS CustomVoice checkpoint, by its `by-name` alias |
| `assistant.voice_profiles.high.tts_speaker` | 1 to 64 of `A-Z a-z 0-9 _` | `"Serena"` | The Qwen3-TTS built-in speaker |
| `assistant.voice_profiles.high.asr_model` | `$MODEL_ROOT` alias or `""` | `""` | A faster-whisper checkpoint that re-transcribes each finished turn on the GPU. Empty keeps the low profile's streaming recogniser |
| `assistant.voice_profiles.high.vram_mib` | integer, 0 to 262144 | `0` | The `gpu-run --vram` budget in MiB; `0` estimates it from the catalog |

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

## Quick voice

`assistant.orca_voice_chat = "quick"` keeps one public Home conversation. Whether Home is idle or busy, an ephemeral Claude CLI process answers each spoken turn first: small talk, general questions and status questions get a direct answer, a question about something current gets a short web lookup, and requests for work go on to Home. It creates no Orca session and keeps no separate provider conversation. With an Orca whose quick-voice patch predates `--always`, an idle Home answers voice itself, as before.

The responder reads recent native Home history, pending approvals, bot items, worktrees, and verified `orchestration worker-show` receipts. It receives a compact copy: the newest conversation messages within 12,000 characters (`NO_REPLY` and earlier turns' tool rows dropped, long messages shortened), the steps of Home's current turn by tool name and state only (never their input or output), open and recently finished items, active and recent workers, recent worktrees, and the status fields of each receipt. The desktop keeps those receipts in `status-ledger/worker-receipts.sqlite3` under the assistant scratch directory. Every observation carries its Home scope, request, timestamp, and age. Stale or unavailable records do not prove current task status.

While Home is busy, an acknowledgement plays immediately; while it is idle, the answer comes without one. The substantive answer waits until the acknowledgement and owner speech finish. Interruption or a newer owner request suppresses stale speech. Home records the question and answer in its native journal. A separate internal context note informs the persistent Home provider without asking it to answer again. Only an answered exchange leaves a note: a request handed to Home arrives as the owner turn itself, and an interrupted exchange answered nothing. Status answers are one-shot. Requests to do work, change state, make a decision, or follow a task go to Home as actual owner turns.

The responder answers in Tohru's persona from its own knowledge, with Home's history as shared memory, and only rarely adds a remark of its own. For weather, news, prices, opening hours or a fact to check, it says a short line, then runs one Claude CLI call limited to `WebSearch` and `WebFetch` with a 25-second deadline, speaks the result and records both in Home like an answer. Code, landing, edits and anything that changes the computer still go to Home.

Home never holds the voice. Once Home admits a handed-over request, the voice is free for the next spoken turn while the helper keeps following Home's reply in the background. When Home answers, the reply is spoken once, after the owner and any other answer have finished, provided no newer request went to Home and voice was not hushed; it remains in the shared history either way. Home's explicit task updates (`orca bot voice-task`) keep their speech through quick exchanges, so progress and completion of running work are still spoken.

This mode requires the matching Orca quick-voice integration patch, the automatic steering patch, and a signed-in Claude CLI. The responder uses `assistant.quick_voice_model` (default `claude-sonnet-5-5`) at `assistant.quick_voice_effort` (default `low`), whatever model Home uses. It has its own system prompt in place of Claude Code's and no `CLAUDE.md` files. Prompt caching is off unless `assistant.quick_voice_cache = true`, since a one-off call rarely reads its cache back. Sonnet at low effort answered status questions as accurately as Opus in trials, at about half the cost and with a faster first reply. Haiku was cheaper still, but made confident wrong status claims. With an Anthropic API key, the responder streams from the Messages API instead of the CLI (`assistant.quick_voice_transport`), so its first words reach the voice as they arrive. Its model call has a 20-second deadline. Invalid output or an unavailable model hands the original request to Home. A failed or uncertain native admission is never automatically resent.

The default remains `home`. After the matching builds are installed, select `quick` with `tohru config set assistant.orca_voice_chat quick`, or set `programs.tohru.assistant.orcaBot.voiceChat = "quick"` in Home Manager. An older Orca rejects the new protocol instead of creating a fallback chat.

With the Orca backend, a spoken turn reaches Home or the quick responder only after it settles: the voice service sends `settled` once you have not carried on within 352 ms of quiet after the turn. New speech holds it until its words decide, for at most 2.5 seconds. Carrying on cancels the fragment before anything is admitted, and the joined sentence goes as one turn. The helper reads Orca meanwhile, so a final turn waits only for what is left of the window; `settle_wait` in the metrics below records that wait. A pause longer than the window still sends the first part as its own turn, as before.

Desktop text and voice sent to Home select automatic native steering. Provider readiness and pending approvals retain Orca's existing gates. Internal context notes wait for idle and never count as fresh owner authority. They stay out of the visible queue while they wait, so they offer no **Steer**; one Orca holds after a failed send shows there for Send or Delete. No manual **Steer** click is required when the provider can accept the message.

Desktop diagnostics are in `home-metrics.jsonl` under the assistant scratch directory. `owner_wait` measures submission to observation of native owner admission. `side_ack` and `side_response` measure response preparation, not audible playback. Metrics omit message and command contents. No live latency measurements are implied by installing the instrumentation.

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

Echo cancellation is configured at the PipeWire level via the Home Manager option `programs.tohru.assistant.voice.echoCancel` (explicit opt-in, default false), which installs `~/.config/pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf` without changing the default audio devices. `TOHRU_VOICE_SOURCE` and `TOHRU_VOICE_SINK` override the PipeWire capture source and TTS playback sink that `tohru-voice` selects.

## Voice profiles

The voice service has two profiles. The low-VRAM profile is the CPU setup under [Voice models](#voice-models): it loads with the service and stays loaded, so every turn can always be answered. The high-VRAM profile is off by default. With `voice_profiles.high.enabled = true`:

1. Each time Tohru comes onto the screen (the dragon or the chat popup becomes visible), the service reads `gpu-status --json`. Committed VRAM is measured use plus, for every running `gpu-run` reservation, any part of its budget it has not allocated yet; an exclusive reservation counts as full. The scheduler's desktop headroom is a reserve, not use, so it is not counted. At or above `vram_threshold_percent` she keeps the low profile.
2. Below it, the high profile's aliases are resolved through `$MODEL_ROOT/catalog.json` and `readiness.json`. An alias that is absent, disabled, not `ready` or missing files keeps the low profile, and the log names the alias. Nothing is downloaded or copied.
3. The models load in one worker started as `gpu-run --vram BUDGET --name "Tohru voice (high VRAM)" --timeout queue_timeout_seconds -- PYTHON tohru-voice-qwen3-worker --socket ...`, which talks to the service over a private Unix socket. The budget is `vram_mib`, or an estimate: the catalog's file sizes (doubled for `float32`) plus 10 %, 1.5 GiB working memory for speech, 1 GiB for recognition and 512 MiB of CUDA context, rounded up to 512 MiB. A queued reservation waits in the scheduler; nothing bypasses it.
4. Until the worker reports its models loaded, answers use the low profile, except that an answer starting within 3 s of the request waits for it: the worker reloads in about a second, so the first answer after she reappears keeps the high voice. A reservation still queued after that answers with low. Each new answer then speaks through it; a sentence it cannot make, a worker that dies or a reservation that times out falls back to the low profile.
5. When she has been hidden for `hidden_unload_seconds` (5 s by default, so a quick hide and show keeps it loaded), the worker exits and the reservation is released. Showing her again, including a wake-word turn, which shows her, cancels the unload; the next appearance after an unload checks VRAM again. Stopping the voice service also releases it.

Every decision goes to the voice service's log (the bar's journal), each line starting `tohru-voice: voice profile`.

| Profile | Part | Model | Device, precision | VRAM |
|---|---|---|---|---|
| low (default) | gate, recognition, end of turn, speech | Silero VAD, Zipformer streaming, Smart Turn v3.2, Kokoro-82M | CPU, int8 recogniser | none |
| high | speech | `voice-tts`: Qwen3-TTS-12Hz-1.7B-CustomVoice, speaker `Serena` | CUDA, bfloat16 | estimated budget 7 GiB (7168 MiB) |
| high, optional | final-pass recognition | a faster-whisper checkpoint, such as Whisper large-v3-turbo | CUDA, bfloat16 | about +2.5 GiB (9.5 GiB with speech) |

The high profile keeps the low profile's VAD, streaming recognition and turn detection on the CPU, since they run on every 32 ms of audio. No faster-whisper checkpoint larger than `whisper-tiny.en` is in the shared catalog yet, so `asr_model` stays empty until one is added there.

```toml
[assistant.voice_profiles]
vram_threshold_percent = 50

[assistant.voice_profiles.high]
enabled = true
python = "/opt/tohru-voice-gpu/bin/python3"
```

## Standalone lifecycle and fonts

These keys share the native schema, TOML layers and `tohru config set KEY VALUE`.
All new keys honor `TOHRU_MANAGED_CONFIG`. Helpers read managed values from disk;
restart their services after changing ownership, executable/options or environment.

| Key | Default | Effect |
|---|---|---|
| `appearance.font_text` | `"Noto Sans"` | UI body family |
| `appearance.font_display` | `"Noto Sans"` | UI headings family |
| `appearance.font_mono` | `"DejaVu Sans Mono"` | Monospace UI family |
| `shortcuts.mode` | `"compositor"` | `compositor`, `portal` or `disabled`; portal ownership requires explicit service start/setup |
| `shortcuts.launcher_trigger` | `""` | Optional preferred trigger request for portal-owned launcher action |
| `shortcuts.session_trigger` | `""` | Optional preferred trigger request for portal-owned session-menu action |
| `shortcuts.assistant_trigger` | `""` | Optional preferred trigger request for portal-owned assistant action |
| `wallpaper.enabled` | `false` | Explicitly allow Tohru wallpaper ownership |
| `wallpaper.backend` | `"mpvpaper"` | Supported still/video backend |
| `wallpaper.directory` | `""` | Empty uses `~/Pictures/Wallpapers`; otherwise absolute directory |
| `wallpaper.assignments` | `""` | Empty uses `$XDG_STATE_HOME/tohru/wallpaper/assignments.json`; otherwise absolute path |
| `wallpaper.executable` | `"mpvpaper"` | Executable name or absolute path |
| `wallpaper.options` | `"no-audio loop-file=inf"` | libmpv options, excluding socket/config/script overrides |
| `idle.owner` | `"external"` | `external`, `disabled` or `tohru`; never starts a second known idle owner |
| `idle.lock_seconds` | `300` | 0 disables idle lock, otherwise 1–86400 seconds |
| `idle.suspend_seconds` | `0` | 0 disables idle suspend; otherwise greater than lock_seconds, up to 86400 |
| `assistant.voice_python` | `"python3"` | No effect; configured the former Python voice service |
| `assistant.voice_environment` | `""` | No effect; configured the former Python voice service |

Font controls are in Appearance; managed font values are read-only there. Font
changes apply on component restart and include greeter/lock. Settings → Shortcuts
shows read-only compositor ownership/runtime observations and a separate opt-in
portal action owner with desktop-controlled Setup/Configure; no invented bindings. [standalone.md](standalone.md) explains supported protocols, generated
units, safe ownership setup, PipeWire AEC, voice dependencies and greetd templates.

## Precedence

From lowest to highest: built-in defaults, the writable file, the managed file, runtime overrides.

- Editing a key that the managed file does not set (in Settings or with `tohru config set`) applies at once and is written to the writable file.
- Editing a key that the managed file sets applies at once, survives `tohru config reload`, but is never written and ends when the process exits. Settings marks these rows as managed.
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

The Settings window has Bar, Appearance, System and Shortcuts pages. It shows `bar.preferred_output` but does not offer a control for it, and does not show `polkit.agent`. The lock style and the notification position are on the Appearance page; clicking one steps to the next value.

The locker is a separate process that reads the files when it locks, so a runtime override (a change to a Nix-managed `lock.style` made in Settings or with `tohru config set`) does not reach it; change `programs.tohru.lock.style` instead.
