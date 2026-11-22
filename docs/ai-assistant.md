# AI assistant: agreed design

Status: implemented. Mockups (not in Git): `.tmp/ai-widget-ideas/` in this worktree, with `index.md` describing all ten directions and the services they need. Code: `bin/tohru-assistant` (chats, history, memory, export), `bin/tohru-voice` (voice), `shell/assistant/` (popup, bar entry, overlay, controllers) and [the assistant README](../shell/assistant/README.md).

## Chat popup: direction 01, Pocket

- A 380 px panel docked down the right edge of the screen, opened from the bar or with `tohru assistant open|toggle|show [ID]`.
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
- A chat runner using each CLI's headless mode in a scratch directory. Chat turns run with full tool access and no approval prompts; memory extraction runs with no tools. Resume one CLI session per conversation; don't spawn fresh each turn.
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

  With neither model nor effort pinned, **tool-free voice drafts** use
  `gpt-6-luna` / low for Codex. Optional semantic voice drafts instead use
  `gpt-6.2-sol` / low: Luna failed two of six contextual turn decisions in the
  evaluation. Typed chat, tool-enabled turns and tool fallback keep the defaults
  above. Provider model/effort settings remain the fallback for all scenarios;
  specific voice or semantic settings take precedence. Claude defaults are unchanged because its quota
  prevented a comparison. `providers` exposes `voice_draft_model_id`,
  `voice_draft_effort`, `semantic_model_id` and `semantic_effort`; each local start
  event reports the actual model and effort. The popup provider label still names
  the regular model. See [measured comparison](voice-text-models.md).

  **Settings → Assistant** lets you choose a provider to edit, enter a model ID,
  and cycle effort separately for regular replies/tools, tool-free spoken replies,
  and turn decisions/draft answers. Choosing a provider here does not switch the
  active assistant. Press Enter to save a model ID, Escape to cancel, or save an
  empty value to restore inheritance. Nix-managed edits remain temporary runtime
  overrides, just like other settings. Changes affect subsequent requests and
  discard unused speculative drafts; already active replies finish normally.

  Equivalent config (replace `codex` with `claude` or another local provider):

  ```toml
  [assistant.models.codex]
  model = "gpt-6.2-sol"
  effort = "medium"
  [assistant.voice_models.codex]
  model = "gpt-6-luna"
  effort = "low"
  [assistant.semantic_models.codex]
  model = "gpt-6.2-sol"
  effort = "low"
  ```

  Each scenario field overrides its corresponding provider field; empty means
  inherit. With no provider or scenario override, the built-in scenario default
  applies. Existing provider-only configurations keep their previous behavior.
  CLI example: `tohru config set assistant.voice_models.codex.model gpt-6-luna`.
  Home Manager exposes `programs.tohru.assistant.models`, `.voiceModels` and
  `.semanticModels`, each keyed by provider with `model` and `effort` fields.
  These settings configure local CLIs, not Orca bot model selection, and do not
  enable semantic detection or choose a different TTS engine.

- **Persona:** `shell/assistant/persona.md`: an ancient dragon who is young at heart, never childish, helpful and accurate first. Voice answers add a rule for one to three short spoken sentences with no Markdown or links, offering more in the popup; typed chats get a lighter touch. claude gets it through `--append-system-prompt` and codex through `-c developer_instructions=`; gemini, agy and opencode have no system-prompt flag, so it is prepended to the first turn only and never shown in history. `assistant.persona = false` turns it off, `assistant.persona_file` replaces it. Claude Code runs with `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, so Tohru's database stays her only memory.
- **Memory:** after a conversation ends (popup closed, new chat, five idle minutes, or a voice conversation timing out), one headless call to the same provider extracts facts, preferences, a summary and relationship notes as strict JSON. Anything malformed is rejected whole. Answers get a memory block of at most 600 tokens. "Forget that" (and "forget about X") in a chat is handled locally. `tohru assistant memory list|forget ID|clear` and the popup's Memory view (from History) list and drop memories. `assistant.memory = false` spawns no extraction and sends no block.
- **Discord:** optional default-off bot integration with channel or fixed-user DM destinations, bounded REST reads and independent posting controls. Opt-in DM reception is owned by the running shell controller, defers while busy, and uses a separate local conversation without speech. Automatic replies obey the desktop acknowledgement policy. The Orca bot backend does not support reception or receive local tools/credentials. See [setup, security boundaries and verification](discord.md).
- **Orca:** when `assistant.orca` is `auto` (the default) or `true`, each chat checks for the Orca CLI shim (`$ORCA_USER_DATA_PATH/linux-orca-cli-shim/orca` or `~/.config/orca/linux-orca-cli-shim/orca`) or `orca-ide` on `PATH` — never bare `orca`, which on Linux is usually the GNOME screen reader. When found, it probes `<cli> status --json` (2 s timeout, cached for 10 s), exports `ORCA_CLI_COMMAND` and `ORCA_USER_DATA_PATH` to the provider, and adds a short Orca section to her prompt noting whether Orca is running, that she is not inside an Orca terminal or worktree, that the user may or may not be talking about Orca, and to load version-matched guides via `<cli> skills get orca-cli`, `orchestration`, `orca-computer-use` or `<cli> skills list` before running commands. Terminal/worktree variables (`ORCA_WORKTREE_ID`, `ORCA_TERMINAL_HANDLE`, `ORCA_TAB_ID`, `ORCA_PANE_KEY`, `ORCA_AGENT_*`) and Claude's Orca status hooks (`~/.orca/agent-hooks/claude-settings.json`) are deliberately omitted and stripped if inherited. Because every provider can run `<cli> skills get`, all providers have skill parity without modifying global agent configs; users who also want Orca's discovery stubs installed into interactive CLI profiles can opt in explicitly with `<cli> skills install --skill orca-cli --skill orchestration --skill orca-computer-use --agent claude,codex,antigravity,gemini,opencode`. `assistant.orca = false` disables detection, environment variables and prompt context.
- **Export and import:** `tohru assistant export [FILE]` writes versioned JSON (`format: tohru-assistant-export`, `schema: 1`) with history, memories, summaries, relationship, the assistant settings and the persona, mode 0600, to stdout without a file. `tohru assistant import FILE` merges by default, newer `updated` winning; `--replace --yes` replaces; `--only history|memory|relationship` (repeatable) limits it. The whole file is validated before the database is touched.
- **Proactive alerts:** `tohru assistant alert [--chat ID] [--speak] [--title TITLE] MESSAGE` lights up the bar dragon's unread dot (pointing at `ID` when given), posts a desktop notification through `tohru-notifications`, and optionally speaks the message via `tohru-voice` when `--speak` is set and the session is neither locked nor in Do Not Disturb.
- **Opt-in Orca bot backend:** Default `assistant.backend = "local"` makes zero assumptions about Orca. Users running Orca can opt in (`assistant.backend = "orca"`, `assistant.orca_bot = "Tohru"`, or `programs.tohru.assistant.orcaBot.enable = true` in Home Manager) after creating the `Tohru` bot in Orca's Bots panel and running `tohru assistant orca init --bot Tohru` (which seeds `SOUL.md` and migrates existing SQLite memories into `~/.orca/bots/tohru/MEMORY.md`). Popup and voice then share the same bot chats, memory, persona, mailbox, schedules and abilities as Orca's Bots panel. See [assistant-bot-abilities.md](assistant-bot-abilities.md).

## Voice

When enabled, captures from `assistant.voice_source`, an `echo-cancel`-named source when available, or the default source. Source names are routing preferences, not verification of AEC. See [streaming, source metadata and reproducible measurements](voice-pipeline.md).

- **Configurable:** users can disable voice in their config, through a setting such as `assistant.voice = false`, in `tohru config set` and in the Home Manager module. When it is disabled, none of the voice infrastructure runs: no voice service or unit, no microphone stream, and no VAD, recognition, Smart Turn or TTS models loaded. It must cost nothing in CPU, RAM or VRAM, and the bar shows no mic dot. The chat popup and the bar entry keep working. Toggling the setting at runtime starts or stops the voice service completely (stopping releases the mic and frees every model), with no restart of the shell. The Home Manager option leaves the voice unit uninstalled when disabled.

- **Models:** none ship and nothing is downloaded; each is an explicit path in the config. See [Voice models](#voice-models).
- **Gate:** Silero VAD (`assistant.vad_model`, CPU). Nothing else runs without speech.
- **Recognition:** sherpa-onnx streaming Zipformer int8 (`assistant.asr_model`, CPU), only on VAD speech.
- **Wake word:** deterministic. The transcript must start with "tohru" (plus close spellings such as "toru"), otherwise it is discarded. Nothing before the wake word is stored or sent anywhere.
- **End of turn:** Smart Turn v3.2 (`assistant.turn_model`, optional, CPU, `intra_op_num_threads=2`, `OPENBLAS_NUM_THREADS=1`) checks at a 220 ms pause. Complete-looking phrases finish after 650 ms of silence (480 ms when Smart Turn confirms completion or a speculative draft is ready). Unfinished phrases (trailing connectors or fillers such as "and", "but", "so", "because", "um", "is", "the", or a trailing comma) and incomplete Smart Turn scores hold for 1050 ms so thinking pauses are not cut off; utterances still cap at 30 s. No LLM end-of-turn check: too slow and too costly through the CLIs.
- **Speculative drafts:** after 220 ms of silence on a complete-looking partial of at least 4 words, `tohru-voice` emits a `draft` event so `tohru-assistant` silently starts a tool-less provider call (`claude` with `--tools "" --strict-mcp-config --fork-session`, `codex` with `exec fork`, `mcp_servers={}` and `sandbox_mode="read-only"`) without touching the popup, TTS, SQLite history or memory. Because drafts run without tools, a draft-only instruction tells the model to reply with `<<needs-tools>>` if answering requires any tool, command, file access or action. On a matching final transcript with a plain answer, the draft is committed (`tohru-assistant commit`) and its buffered text speaks immediately with no second provider call. If the draft replies `<<needs-tools>>` or errors, it is discarded without committing or advancing the session and the turn runs normally with full tools; the marker is never stored or spoken. Resumed speech or a changed transcript cancels the draft (at most two drafts per utterance; never more than one in flight). Drafts are disabled for `gemini`, `agy` and `opencode` because they have no session-fork flag to isolate a resumed session cleanly. Discarded drafts still consume the provider call started before cancellation.
- **Conversation mode:** after the wake word, follow-ups need no wake word until about a minute without speech.
- **Echo:** the dragon must not hear itself. By default (when `programs.tohru.assistant.voice.echoCancel` is enabled in Home Manager), a PipeWire `libpipewire-module-echo-cancel` (`aec/libspa-aec-webrtc`) drop-in provides `tohru-echo-cancel-source` and `tohru-echo-cancel-sink` at session priority 0 so the session's default devices stay unchanged. `tohru-voice` routes TTS to any `echo-cancel` sink (overridable with `TOHRU_VOICE_SINK`) and captures from any `echo-cancel` source (overridable with `TOHRU_VOICE_SOURCE`). With AEC module provenance reported by source metadata, barge-in triggers on a flat 0.065 mic RMS threshold plus Silero speech for 200 ms. With unknown AEC status, `tohru-voice` tracks a smoothed, bounded acoustic coupling (`mic RMS / playback RMS`, clamped to 0.05–2.0) while she speaks and requires `1.6×` coupling × recent playback. In both modes, the microphone is ignored for 0.4 s after playback stops.
- **Interruptions and continuations:** when barge-in triggers while she is speaking, playback fades out over 100 ms and pauses immediately (mouth returns to rest, dragon moves to listening) while the recogniser receives ~1 s of pre-roll. If real words appear, playback is hushed, `barge` carries what she had already spoken so her next reply addresses the user directly without repeating herself, and the speech becomes the next turn. If nothing but noise, a cough or a lone backchannel ("mm", "mhm", "uh huh", "yeah", "right", "okay") arrives within ~1 s, playback resumes smoothly (25 ms fade-in) where it paused without restarting the sentence. If the user resumes speaking while she is still thinking (before first audio), it is treated as a continuation: the pending answer is cancelled and rolled back from history (`tohru-assistant rollback`), and the previous fragment and new speech are joined into one turn with no canned "busy" message.
- **Pausing:** pause while the session is locked.
- **Reference:** n33tc0de-voices `master`: commits `24f54a6`, `521f324`, `bb5cfac` and `docs/voice-turn-detection-2026-11-10.md` (thresholds, timings and benchmarks). Its Qwen3-TTS GPU voice is not reused.
- **Streaming speech:** regular committed turns release complete sentences before provider completion, with bounded synthesis lookahead. Speculative drafts stay silent until validated and saved. Final fragments flush once; cancellation invalidates obsolete text and audio. The full provider text remains in chat; speech uses the concise voice instructions rather than a 600-character cut.
- **Transcript and settings:** wake-accepted recognized text appears above the dragon; final text clears after five seconds. Settings → System exposes microphone selection and module-reported/unknown AEC metadata.
- **Answers:** go to the provider selected in the popup. Voice conversations are saved in the same history, marked as voice, and open in the popup.

### Voice models

| Part | Setting | Required | Model |
|---|---|---|---|
| Gate | `assistant.vad_model` | yes | Silero VAD v6.2.3 `.onnx` (2.3 MB, MIT) |
| Recognition | `assistant.asr_model` | yes | `csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21`, int8 encoder and joiner (190 MB, Apache-2.0) |
| End of turn | `assistant.turn_model` | no; silence alone without it | `pipecat-ai/smart-turn-v3`, `smart-turn-v3.2-cpu.onnx` (8.7 MB, BSD-2-Clause) |
| Speech | `assistant.tts_model` | no; espeak-ng without it | `onnx-community/Kokoro-82M-v1.0-ONNX`, fp32 `onnx/model.onnx` as `model.onnx` plus `tokenizer.json` (326 MB, Apache-2.0) |
| Wake spotter | `assistant.kws_model` | only for `voice_wake = "kws"` | `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01` (5.3 MB of int8 files, Apache-2.0) |

Revisions, file names, sizes, hashes and an example config are in [configuration.md](configuration.md#voice-models). A required path that is unset or wrong keeps the service from loading anything; the popup names each missing setting and the files it expects.

### Wake word

`assistant.voice_wake` chooses how "Tohru" is heard. `asr` (the default) runs the recogniser on every VAD speech segment and drops it as soon as the partial transcript cannot start with the wake word. `kws` runs a sherpa-onnx keyword spotter on the segment first (`shell/assistant/voice/wake-keywords.txt`: TOHRU, TORU, TARU, TERU, TORO, TO RU, TOO RU; score 2.0, threshold 0.05); only when it fires does the recogniser decode the buffered segment, pre-roll included, and the transcript still has to start with the wake word. Follow-ups in a conversation skip the spotter.

Measured on 2026-11-21 with `sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01` (int8; [k2-fsa/sherpa-onnx release `kws-models`](https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2), SHA-256 `f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a`; files `encoder/decoder/joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx`, `tokens.txt`) against the recogniser (`csukuangfj/sherpa-onnx-streaming-zipformer-en-2023-06-21`). Test set: 70 wake phrases ("Tohru, …" and "Tohru." alone) spoken by Kokoro in 13 stock voices plus tide; 343 s of long speech without the wake word (four passages in four voices plus two LibriSpeech-style clips shipped with the spotter); 48 confusable sentences (Toru, to do, true, taro, Toronto, two rooms, Tora, torus, …).

| | `asr` | `kws` |
|---|---|---|
| CPU per second of non-Tohru speech (one core; VAD alone is 0.0016) | 0.031 | 0.006 |
| Recall | 91.4 % (64/70) | 70.0 % (49/70): the spotter missed 17, the transcript rejected 4 |
| False triggers on long speech | 0 | 0 spotter hits |
| Spotter hits on confusables | — | 7/48 (Toru, Toronto, Tora) |
| False accepts after the transcript check | 3/48: "Taro is…" twice, "Toru Watanabe…" | 0/48 at this setting; "Toru Watanabe…" at score 1.0 |

The spotter is about five times cheaper but fails the bar for the default (recall ≥ 95 % and no confirmed false accepts), so the default stays `asr`; the fp32 spotter and other thresholds (score 1.0–3.0, threshold 0.02–0.25) did no better than 73 %. Neither mode clears the bar: the recogniser hears Kokoro's "Tohru" as TORU, TERU, TARU, TORO or TORY, and the names Toru and Taro sound the same as the wake word. "Tory" stays out of the accepted forms as a real word. All of this is synthetic speech; recall on real voices is unmeasured.

## Dragon overlay

- As built: `shell/assistant/dragon.plm` is a 210×230 overlay layer (`tohru-dragon`) at the bottom right.
  - She shows once a voice conversation opens, stays while it lasts, and goes five seconds after her last word once it has ended.
  - Clicking her opens the popup on that conversation.
- **Sheets:** pre-rendered 2D, in `shell/assistant/dragon/`; their layout and provenance are in its README.
  - `body.webp` retains the legacy breath, flutter and expressive moves, and adds 20 redrawn airborne body poses with registered 17-frame transitions. Their feet, pelvis, tail, wings and facing change together.
  - Each new resting view has its own eyes and mouth patches in `faces.webp`; `travel.webp` keeps lip sync registered through the new body turns. No front-facing eye patch is forced onto a turned face.
  - `glass.wgsl` adds the shimmer of light through the glass.
  - The sheets are set once the voice service is ready, and drawn unseen for three seconds so they are loaded before she first shows. With voice off nothing is loaded.
- **Animator:** `shell/assistant/puppet.luau` runs at 24 fps, and only while she is shown.
  - Idle: she blinks every 3–6 s, holds varied floating rests for roughly 12–25 s, excludes her last three selections, and occasionally drifts into a more extended alignment.
  - Listening: an attentive forward hover while follow-up speech is heard (`hearing` events), with quiet variation during longer waits.
  - Thinking: a cheek paw with loosely tucked feet and hanging tail, from the turn until her answer starts; long waits can change to another quiet posture.
  - Talking: she explains, and changes pose by sentence across her talking poses. Explicit positive excitement gives an airborne wing lift; punctuation alone does not celebrate an error, laughter gives the laugh, lists give the scroll, questions give a head tilt, step cues ("first", "second", "one thing") point up, caveats ("it depends", "either", "maybe") shrug, agreement ("yes", "right", "exactly") gives a soft nod, recalling ("let's see", paths) glances aside, summaries ("overall", "in short") open both palms, contrasts ("on the other hand", "however") gesture right, tips ("by the way", "handy") lean in, sincere remarks ("honestly", "recommend") rest a paw on her chest, light topic shifts ("good news", "speaking of") lift one wing, and warm words give happy; uncued sentences vary across general conversational poses without immediate repeats.
  - Pose changes wait for a dip in her voice (five quiet mouth events) and at least 2.5–4 s since the last change.
  - After speech: settle into a quiet airborne rest. Interruption preserves listening/thinking or recovers to neutral.
  - Drawn moves go through neutral, except explain↔cheer. Poses without drawn in-betweens swap under a squash from their registered hub poses (`neutral`, `explain`, `listening`, `thinking`, `pleased`, `scroll`, or `happy`).
- **Lip-sync:** `tohru-voice` maps each sentence's phonemes to eight mouths (rest, M/B/P, AA, EH, EE, OH, OO, F/V).
  - espeak-ng's IPA (`--ipa`) is spread over the voiced part of the clip.
  - Kokoro's misaki phonemes are placed by the model's own durations when the ONNX export returns them, and spread by weight otherwise.
  - Mouths shorter than two frames merge into a neighbour, except a closed M/B/P.
  - Each `mouth` event carries the `viseme` 40 ms ahead of the audio. Without one, the mouth falls back to the envelope (rest, EE, EH, AA by level).
  - The jaw drops and the body squashes slightly with the level.
- **Reduced motion:** Settings → Appearance → Reduce dragon motion, or `assistant.reduced_motion = true`, keeps the body still and disables bobbing, shimmer and flourishes while preserving blinking and lip sync. It applies live, including temporary Nix-managed overrides, without restarting the voice service.
- Source art, provenance, decoded memory costs and reproducible packing/native verification are documented in `shell/assistant/dragon/README.md` and `tools/dragon/README.md`.

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
