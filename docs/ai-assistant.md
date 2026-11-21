# AI assistant: agreed design

Status: decided, not implemented. Mockups (not in Git): `.tmp/ai-widget-ideas/` in this worktree, with `index.md` describing all ten directions and the services they need.

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
- A JSON history store with search.
- A bar streaming/ready state readable while the popup is closed.

## Voice

Always on, using the default PipeWire source, and following changes of the default.

- **Configurable:** users can disable voice in their config, through a setting such as `assistant.voice = false`, in `tohru config set` and in the Home Manager module. When it is disabled, none of the voice infrastructure runs: no voice service or unit, no microphone stream, and no VAD, recognition, Smart Turn or TTS models loaded. It must cost nothing in CPU, RAM or VRAM, and the bar shows no mic dot. The chat popup and the bar entry keep working. Toggling the setting at runtime starts or stops the voice service completely (stopping releases the mic and frees every model), with no restart of the shell. The Home Manager option leaves the voice unit uninstalled when disabled.

- **Gate:** Silero VAD (`$MODEL_ROOT/by-name/silero-vad`, CPU). Nothing else runs without speech.
- **Recognition:** sherpa-onnx streaming Zipformer int8 (`voice-asr`, CPU), only on VAD speech.
- **Wake word:** deterministic. The transcript must start with "tohru" (plus close spellings such as "toru"), otherwise it is discarded. Nothing before the wake word is stored or sent anywhere.
- **End of turn:** Smart Turn v3.2 (`voice-turn`, CPU, `intra_op_num_threads=2`, `OPENBLAS_NUM_THREADS=1`) at a 200 ms pause, plus a silence fallback. No LLM end-of-turn check: too slow and too costly through the CLIs.
- **Conversation mode:** after the wake word, follow-ups need no wake word until about a minute without speech. Barge-in interrupts the dragon.
- **Echo:** the dragon must not hear itself. Use PipeWire echo cancellation, and require barge-in speech to be louder than playback.
- **Pausing:** pause while another app is recording (calls) and while the session is locked.
- **Reference:** n33tc0de-voices `master`: commits `24f54a6`, `521f324`, `bb5cfac` and `docs/voice-turn-detection-2026-11-10.md` (thresholds, timings and benchmarks). Its Qwen3-TTS GPU voice is not reused.
- **Answers:** go to the provider selected in the popup. Voice conversations are saved in the same history, marked as voice, and open in the popup.

## Dragon overlay

- When an answer is ready, the mascot (`shell/brand/mascot.png`) appears floating at the bottom-right of the desktop and speaks it.
- Clicking it opens the popup on that conversation.
- It is pre-rendered for now: the mascot modelled and rendered in Blender as sprite atlases (idle float loop, several jaw-open levels, blink). Frames are chosen from the TTS audio envelope for lip-sync. No real-time 3D.

## Voice of the dragon

- Cute, quirky, cartoony, younger-sounding and original, not a clone of an existing character.
- CPU-only at runtime, running only while speaking.
- Being chosen on the `dragon-voice` card (`.tmp/voices/` there). Kokoro-82M with a fitted custom voicepack and light DSP is the leading approach.

## Cost budget

At idle, only the mic and VAD run, at about 1 % of a core. Recognition runs only on speech. The desktop uses no GPU for any of this.
