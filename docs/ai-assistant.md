# AI assistant: agreed design

Status: implemented, apart from the Blender sprite atlases (the overlay uses `mascot.png` with a drawn mouth until they exist). Mockups (not in Git): `.tmp/ai-widget-ideas/` in this worktree, with `index.md` describing all ten directions and the services they need. Code: `bin/tohru-assistant` (chats, history, memory, export), `bin/tohru-voice` (voice), `shell/assistant/` (popup, bar entry, overlay, controllers) and [the assistant README](../shell/assistant/README.md).

## Chat popup: direction 01, Pocket

- A 280 px popup, the size of the sound and Bluetooth popups, opened from the bar.
- Header: the current provider's name with the fold chevron, like Bluetooth's device name. Unfolding it lists the installed CLIs (`claude`, `codex`, `gemini`, `agy`, `opencode`) with their model, or **Sign in…** for a signed-out one, which opens its CLI login in a terminal. Missing binaries never appear. Each provider keeps a fixed palette colour by position.
- Clock and + buttons: history (with search) and new chat. The field is always at the foot. Answers stream with a caret in the provider's colour, with Stop.

## Bar entry: direction 09's behaviour

- The entry is the tinted dragon mark (`shell/brand/dragon.png`).
- While a provider answers with the popup closed, a soft glow in the provider's colour breathes behind the mark.
- When an answer is ready and unread, a 4 px mark in that colour sits on the dragon until opened.
- Right-click: provider menu (with **Sign in…** inline), New chat, History.
- While the microphone is listening, the bar shows a small mic dot.

## Services

As listed under "Services needed" in `.tmp/ai-widget-ideas/index.md`:

- Provider discovery on `PATH`.
- Per-CLI auth probes that never prompt.
- A chat runner using each CLI's headless mode in a scratch directory with no tools or file access. Resume one CLI session per conversation; don't spawn fresh each turn.
- A history store with search (SQLite, below).
- A bar streaming/ready state readable while the popup is closed.

### As built

- **Store:** `$XDG_DATA_HOME/tohru/assistant.sqlite` (directory 0700, file 0600), WAL, numbered migrations in `PRAGMA user_version`, FTS5 over turns and memories. Tables: `conversations`, `turns`, `memories` (facts and preferences), `summaries` and `relationship`. A database from a newer Tohru is refused rather than rewritten.
- **Models:** `assistant.provider` picks the CLI; empty means the first installed and signed-in one of claude, codex, gemini, agy, opencode. `assistant.models.<cli>.model` and `.effort` pass straight to the CLI, and the popup header shows the active one ("Opus 5.5 · medium"). A model the CLI rejects ends the turn with the CLI's own message; nothing retries with another model.

  | CLI | Default | How it is passed | Checked |
  |---|---|---|---|
  | claude | `claude-opus-5-5`, medium | `--model`, `--effort` | yes; a bad model returns "There's an issue with the selected model" |
  | codex | `gpt-6.2-sol`, medium | `-m`, `-c model_reasoning_effort=` | yes; a bad model fails the turn |
  | agy | `gemini-4-neon`, high | `--model`, `--effort` | yes (`agy models` also lists `gemini-4-neon-high`) |
  | gemini | `gemini-4-neon`, high | `-m` plus a `tohru-chat` alias in the scratch `.gemini/settings.json` setting `thinkingConfig.thinkingLevel` | no: gemini CLI 0.47 (nixpkgs, marked as replaced by Antigravity) is not installed here; it has no effort flag, and "Gemini Neon" is not in its built-in model list |
  | opencode | its own configured model | `--model provider/model#variant` | flags only; no default is forced because opencode's providers are per user |

- **Persona:** `shell/assistant/persona.md`: an ancient dragon who is young at heart, never childish, helpful and accurate first. Voice answers add a rule for one to three short spoken sentences with no Markdown or links, offering more in the popup; typed chats get a lighter touch. claude gets it through `--append-system-prompt` and codex through `-c developer_instructions=`; gemini, agy and opencode have no system-prompt flag, so it is prepended to the first turn only and never shown in history. `assistant.persona = false` turns it off, `assistant.persona_file` replaces it. Claude Code runs with `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`: its own memory instructions otherwise make the tool-less model print tool calls as text.
- **Memory:** after a conversation ends (popup closed, new chat, five idle minutes, or a voice conversation timing out), one headless call to the same provider extracts facts, preferences, a summary and relationship notes as strict JSON. Anything malformed is rejected whole. Answers get a memory block of at most 600 tokens. "Forget that" (and "forget about X") in a chat is handled locally. `tohru assistant memory list|forget ID|clear` and the popup's Memory view (from History) list and drop memories. `assistant.memory = false` spawns no extraction and sends no block.
- **Export and import:** `tohru assistant export [FILE]` writes versioned JSON (`format: tohru-assistant-export`, `schema: 1`) with history, memories, summaries, relationship, the assistant settings and the persona, mode 0600, to stdout without a file. `tohru assistant import FILE` merges by default, newer `updated` winning; `--replace --yes` replaces; `--only history|memory|relationship` (repeatable) limits it. The whole file is validated before the database is touched.

## Voice

Always on, using the default PipeWire source, and following changes of the default.

- **Configurable:** users can disable voice in their config, through a setting such as `assistant.voice = false`, in `tohru config set` and in the Home Manager module. When it is disabled, none of the voice infrastructure runs: no voice service or unit, no microphone stream, and no VAD, recognition, Smart Turn or TTS models loaded. It must cost nothing in CPU, RAM or VRAM, and the bar shows no mic dot. The chat popup and the bar entry keep working. Toggling the setting at runtime starts or stops the voice service completely (stopping releases the mic and frees every model), with no restart of the shell. The Home Manager option leaves the voice unit uninstalled when disabled.

- **Gate:** Silero VAD (`$MODEL_ROOT/by-name/silero-vad`, CPU). Nothing else runs without speech.
- **Recognition:** sherpa-onnx streaming Zipformer int8 (`voice-asr`, CPU), only on VAD speech.
- **Wake word:** deterministic. The transcript must start with "tohru" (plus close spellings such as "toru"), otherwise it is discarded. Nothing before the wake word is stored or sent anywhere.
- **End of turn:** Smart Turn v3.2 (`voice-turn`, CPU, `intra_op_num_threads=2`, `OPENBLAS_NUM_THREADS=1`) at a 200 ms pause, plus a silence fallback. No LLM end-of-turn check: too slow and too costly through the CLIs.
- **Conversation mode:** after the wake word, follow-ups need no wake word until about a minute without speech. Barge-in interrupts the dragon.
- **Echo:** the dragon must not hear itself. As built there is no echo-cancelling source: barge-in speech must be 1.6× louder than the dragon's own recent playback, and the microphone is ignored for 0.4 s after she stops. Without it, an end-to-end test over the null sink had her answer her own last words.
- **Pausing:** pause while another app is recording (calls) and while the session is locked.
- **Reference:** n33tc0de-voices `master`: commits `24f54a6`, `521f324`, `bb5cfac` and `docs/voice-turn-detection-2026-11-10.md` (thresholds, timings and benchmarks). Its Qwen3-TTS GPU voice is not reused.
- **Answers:** go to the provider selected in the popup. Voice conversations are saved in the same history, marked as voice, and open in the popup.

### Wake word

`assistant.voice_wake` chooses how "Tohru" is heard. `asr` (the default) runs the recogniser on every VAD speech segment and drops it as soon as the partial transcript cannot start with the wake word. `kws` runs a sherpa-onnx keyword spotter on the segment first (`shell/assistant/voice/wake-keywords.txt`: TOHRU, TORU, TARU, TERU, TORO, TO RU, TOO RU; score 2.0, threshold 0.05); only when it fires does the recogniser decode the buffered segment, pre-roll included, and the transcript still has to start with the wake word. Follow-ups in a conversation skip the spotter.

Measured on 2026-11-21 with `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01` (int8; [k2-fsa/sherpa-onnx release `kws-models`](https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2), SHA-256 `f170013b…6561a`; files `encoder/decoder/joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx`, `tokens.txt`) against `voice-asr` (`csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21`). Test set: 70 wake phrases ("Tohru, …" and "Tohru." alone) spoken by Kokoro in 13 stock voices plus tide; 343 s of long speech without the wake word (four passages in four voices plus two LibriSpeech-style clips shipped with the spotter); 48 confusable sentences (Toru, to do, true, taro, Toronto, two rooms, Tora, torus, …).

| | `asr` | `kws` |
|---|---|---|
| CPU per second of non-Tohru speech (one core; VAD alone is 0.0016) | 0.031 | 0.006 |
| Recall | 91.4 % (64/70) | 70.0 % (49/70): the spotter missed 17, the transcript rejected 4 |
| False triggers on long speech | 0 | 0 spotter hits |
| Spotter hits on confusables | — | 7/48 (Toru, Toronto, Tora) |
| False accepts after the transcript check | 3/48: "Taro is…" twice, "Toru Watanabe…" | 0/48 at this setting; "Toru Watanabe…" at score 1.0 |

The spotter is about five times cheaper but fails the bar for the default (recall ≥ 95 % and no confirmed false accepts), so the default stays `asr`; the fp32 spotter and other thresholds (score 1.0–3.0, threshold 0.02–0.25) did no better than 73 %. Neither mode clears the bar: the recogniser hears Kokoro's "Tohru" as TORU, TERU, TARU, TORO or TORY, and the names Toru and Taro sound the same as the wake word. "Tory" stays out of the accepted forms as a real word. All of this is synthetic speech; recall on real voices is unmeasured.

## Dragon overlay

- As built: `shell/assistant/dragon.plm`, a 210×230 overlay layer (`tohru-dragon`) at the bottom right, shown while she speaks and for five seconds after. Its mouth follows the TTS envelope (`ai_mouth`, 30 levels a second, smoothed over 70 ms). Sprite interface for the Blender atlases: `ai_sprite` is a sheet of `ai_sprite_columns × ai_sprite_rows` cells, cells 0 to 4 being jaw levels closed to open, then blink frames; set `ai_sprite_frames` above 1 and the drawn placeholder mouth disappears.
- When an answer is ready, the mascot (`shell/brand/mascot.png`) appears floating at the bottom-right of the desktop and speaks it.
- Clicking it opens the popup on that conversation.
- It is pre-rendered for now: the mascot modelled and rendered in Blender as sprite atlases (idle float loop, several jaw-open levels, blink). Frames are chosen from the TTS audio envelope for lip-sync. No real-time 3D.

## Voice of the dragon

- Cute, quirky, cartoony, younger-sounding and original, not a clone of an existing character.
- Locked: **tide**, a fitted Kokoro-82M v1.0 voicepack shipped as `shell/assistant/voice/tohru-tide.bin` (provenance in its README), at speed 1.0 with no DSP. Kokoro runs fp32 ONNX on onnxruntime's CPU provider with 4 threads, misaki G2P (espeak-ng fallback for unknown words), in a child process so it can be freed. espeak-ng stays as the fallback engine.
- Loading: by default Kokoro loads when the voice service starts and stays loaded while voice is on; `assistant.tts_load_on_conversation = true` loads it when a conversation starts and frees it when it ends. First audio about 0.7 s after the answer arrives, real-time factor 0.10.

## Cost budget

At idle, only the mic and VAD run, at about 1 % of a core. Recognition runs only on speech. The desktop uses no GPU for any of this.

Measured on 2026-11-21 in the sandbox (Python 3.14, onnxruntime CPU):

| State | CPU (one core) | RAM |
|---|---|---|
| Voice off | 0 (no process, no microphone stream) | 0 |
| On, silence | 0.8 % | 334 MB service + 671 MB Kokoro = 1.0 GB; 334 MB with `tts_load_on_conversation` |
| Speech without the wake word | 3.1 % (`asr`), 0.6 % (`kws`) | as above |
| A spoken exchange (wake, answer, Kokoro speaking), averaged over 20 s | 18 % | 1.13 GB peak |
