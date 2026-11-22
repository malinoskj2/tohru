# Voice streaming and measurement

Voice runs inside the bar's existing service, not a separate systemd unit. `voice.luau`
starts `tohru-voice` only when enabled. Pleamar's opt-in `{stderr="inherit"}` passes
its diagnostics to the bar's stderr; stdout remains newline-delimited JSON. With
Home Manager's systemd services, stderr reaches `tohru-bar.service`'s journal by
systemd's normal stream inheritance. Foreground execution prints the same events
on stderr. No transcript, response, audio or provider credentials are logged.

```sh
journalctl --user -u tohru-bar.service -f -o cat
journalctl --user -u tohru-bar.service --since '10 minutes ago' -o cat | rg 'tohru-voice:'
TOHRU_VOICE_LOG=off tohru-voice ...  # disable metadata diagnostics
```

The default metadata volume is per utterance, sentence and lifecycle transition,
not per audio frame or provider token. Errors carry stage and exception class;
the detailed human error goes through the private UI protocol. A `session` ID
separates process restarts; monotonic nanoseconds order stages within a boot.
`turn_id` links VAD/ASR finalization and the committed shell turn; `generation`
links the speech stream, synthesis and playback. `speech_stream_start` records the
command-reader timestamp after the assistant's start event, not provider compute
start or SQLite commit duration. `first_provider_text` records arrival at the voice
command reader. Speculative text gets neither event until its whole draft passes
tool-marker validation and a successful database commit.

`asr_finalize_start.last_speech_ns` is the last VAD-positive **application window**,
not physical microphone speech end. `asr_finalize_end`, `turn`,
`speech_stream_start`, `first_provider_text`, `synthesis_start`, `synthesis_ready`
and `sentence_first_write` locate endpointing, ASR finalization, provider delivery,
synthesis and playback submission separately. ASR streaming work overlaps capture;
finalization timing alone is not total ASR CPU time. Input buffering/device latency
and provider transport are included in their surrounding application stages.

`sentence_first_write` means a pacat pipe write/flush completed. The requested
60 ms Pulse buffer is a configuration request, not a measurement of actual device
latency. `inter_sentence_application_gap` excludes waveform padding and device
buffering. `interruption_detected` marks the application's threshold decision;
`audio_cancel_requested` and `player_stop_requested` mark cancellation/kill requests,
not acoustic silence. `spoken_so_far` remains a pacing-based estimate for follow-up
context. None of these events measures a loudspeaker or physical room echo.

## Pipeline and safety

Regular voice turns stream provider deltas through a separate controller listener.
Only completed sentences leave the text buffer before answer completion. Fenced code,
inline code and Markdown links are held until their delimiters balance; an unfinished
fence at completion becomes a chat cue. Final fragments flush once. Text is bounded
at 16 KiB per answer; exceeding that limit closes speech with a chat cue. This is a
resource ceiling, not a summary algorithm. Normal answers use the existing provider
voice instruction (one to three concise sentences, essential qualifications and tool
outcomes preserved); chat retains the provider's complete text. No summarization call
is added and the old 600-character truncation is removed.

Speculative calls remain silent, including a promoted draft while it can still
produce `<<needs-tools>>`. A completed draft's asynchronous commit remains cancellable;
failed commits are not spoken and cancelled successful commits are rolled back.
The ordinary tools-enabled provider path remains responsible for tool execution.

One synthesis producer owns the engine protocol per answer. A semaphore admits
one next utterance, queued **or** in inference, beside the currently playing clip.
Sentence chunks are synthesized once, avoiding a second split inside the engine.
A final fragment can share the preceding sentence if already buffered when the
next synthesis starts. Speech cancellation invalidates text commands and callbacks,
stops the player, and prevents obsolete generated audio from reaching playback.
Inference already running finishes/discards its result before the next job uses the
engine. Kokoro waits/headers/clip sizes are bounded, protocol errors unload the worker,
and engine replacement/retirement serializes with inference. EOF and SIGTERM stop
capture/playback and unload the engine before exit.

Kokoro exports substantial near-silent padding. Playback trims only samples below
-80 dBFS outside the first/last sound and retains an 80 ms guard at both ends.
Viseme times shift with the retained samples. This removes synthetic padding while
preserving quiet consonants and a pause between phrases; it is not voice activity
recognition or a measured acoustic onset.

## Microphone and transcript

Settings → System → Microphone cycles Pulse/PipeWire source names. Empty means
automatic (prefer the echo-cancel-named source, otherwise the actual default).
`assistant.voice_source` also works through `tohru config set`; Home Manager exposes
`programs.tohru.assistant.voice.source`. `TOHRU_VOICE_SOURCE` takes precedence.
Changing the source restarts capture/service and invalidates old process callbacks.
Settings' metadata refreshes when opened or a source changes. `tohru-voice --devices`
queries metadata without loading models or opening the microphone.

A source name containing `echo-cancel` is only a routing preference. Native Pulse's
`module-echo-cancel.c` driver or explicit `aec.method=webrtc` properties report module
provenance; other metadata is **unknown**, including PipeWire nodes whose exported
properties do not identify their AEC implementation. Even module-reported AEC does
not verify effectiveness or the playback reference route. The conservative adaptive
echo threshold remains active for unknown sources. The existing Home Manager AEC
module is unchanged; no default audio device is changed.

The dragon shows at most 240 characters of a wake-accepted live transcript, never
unrelated pre-wake chatter. Final text lingers five seconds; conversation end,
voice disable and process failure clear it. The display does not alter wake,
follow-up, speculative draft, backchannel or conversation timeout rules.

## Reproduce

From the repository root, preserve a baseline before changes:

```sh
mkdir -p .tmp/voice-baseline
cp bin/tohru-voice .tmp/voice-baseline/tohru-voice
python3 tools/voice-benchmark.py --module .tmp/voice-baseline/tohru-voice > .tmp/before.json
python3 tools/voice-benchmark.py > .tmp/after.json
python3 tools/voice-benchmark.py --sentences 12 --repetitions 3 > .tmp/long.json
python3 tools/voice-benchmark.py --interrupt-ms 40 > .tmp/interruption.json
```

For this work's original baseline, use revision
`364545c53998d86a7817852a8b4e70d402a5685a:bin/tohru-voice` with `git show`.
The benchmark's provider always delivers identical text at 120 ms intervals;
synthetic TTS takes 60 ms per chunk and emits 100 ms of PCM. It drives the production
Speaker and parser, but does not measure capture, ASR or a live provider. `--real-tts`
uses real espeak-ng instead. `--pulse-sink NAME` uses actual pacat rather than a fake
player; select an explicit private null sink for unattended tests, not a host output.

For real Kokoro, first inspect `/etc/resource-scheduler/mode` and the shared
`$MODEL_ROOT/{catalog.json,readiness.json,README.md}`. Use the packaged voice Python
environment (numpy, onnxruntime, misaki and its English data); `python3` alone may
lack it. Do not download weights or create another model copy. Example when the
installed scheduler uses gpu-run:

```sh
gpu-run --vram 1GiB --name 'voice CPU Kokoro benchmark' -- \
  /path/to/voice-python/bin/python3 tools/voice-benchmark.py \
  --module .tmp/voice-baseline/tohru-voice \
  --kokoro-folder "$MODEL_ROOT/by-name/voice-kokoro" --pulse-sink PRIVATE_SINK \
  --repetitions 5 > .tmp/kokoro-before.json
# Repeat with --module bin/tohru-voice, then repeat both with --warm.
```

On Slurm use resource-run instead, with the site's CPU/RAM admission settings.
The model command must remain foreground within its reservation through unload.
Fresh engine means a new model worker each repetition; it does **not** mean cold
OS page cache. `--warm` preloads and reuses a worker outside the timed interval.
The harness timestamps entry into the application write call (before child write/flush);
the service event timestamps write/flush completion. Neither is acoustic onset.
Samples report first application write, completion, synthesis times, raw synthesized waveform
padding, pacing-based gaps, and (when interrupted) hush-return/player-reap times and
writes after hush. For 5/8 repetitions nearest-rank p95 is the sample maximum;
it is not a reliable population p95. Increase repetitions for tail-latency claims.

For live end-to-end checks, run the same fixed prompts (short answer, long answer,
interruption, unfinished request resumed while thinking, tool action, follow-up)
with the same source/sink/models/provider, capture metadata logs, and repeat warm
and fresh-worker conditions. Check full chat outcomes and what was spoken, not just
timestamps. To measure physical output, record a calibrated electrical/acoustic
loopback using a separate recorder and align the captured signal to a known trigger.
Pulse monitor capture is only a virtual-server observation; pipe writes, requested
latency, waveform thresholds and source names cannot establish acoustic onset,
physical interruption latency, ASR accuracy or echo cancellation effectiveness.
