# The voice service

`tohru-voice` is a Rust program ([`voice/`](../voice)) built on sherpa-onnx and
ONNX Runtime. It owns wake, voice activity, recognition, Smart Turn, Kokoro and
eSpeak NG speech, playback and the voice profiles. The assistant provider,
history, quick voice and Discord helper remain in `bin/tohru-assistant`. The Luau
controller ([`shell/assistant/voice.luau`](../shell/assistant/voice.luau)) keeps
visibility and reply policy.

## Build

On x86-64 Linux:

```sh
mkdir -p .tmp
nix build .#tohru-voice-native --out-link .tmp/native-voice
.tmp/native-voice/bin/tohru-voice-native --help
```

The Tohru package installs this program as `tohru-voice` when `withVoice` is
true, the default. The Nix package pins sherpa's native libraries and supplies
ONNX Runtime, eSpeak NG and the PulseAudio utilities. Building does not restart
the bar or activate the package.

Cargo builds need Rust 1.89 or later. For a manual build, set
`SHERPA_ONNX_LIB_DIR` to compatible static sherpa libraries, which avoids the
upstream crate's automatic native-library download:

```sh
SHERPA_ONNX_LIB_DIR=/path/to/sherpa-onnx/lib cargo build --release --manifest-path voice/Cargo.toml
```

At run time, set `ORT_DYLIB_PATH` to a compatible ONNX Runtime shared library
(the tested version is 1.27.1) and put `parec`, `pacat`, `pactl` and `espeak-ng`
on `PATH`. A library from a newer glibc environment cannot be loaded into an
older manually compiled executable. Install the binary as `tohru-voice` on the
bar's `PATH`.

## Voice profiles

The low profile runs on the CPU and is always loaded. With
`assistant.voice_profiles.high.enabled`, the service loads the high profile while
Tohru is on screen and GPU memory use is under `vram_threshold_percent`:

- It resolves `tts_model` (and the optional `asr_model`) in `$MODEL_ROOT`'s
  catalog and readiness files. Nothing is downloaded.
- It reserves the GPU with `gpu-run --vram`, using `vram_mib` or an estimate
  from the catalog's file sizes, and waits at most `queue_timeout_seconds` in the
  queue.
- Inside the reservation, `high.python` runs `tohru-voice-qwen3-worker`, which
  loads faster-qwen3-tts (and faster-whisper for `asr_model`) from local files.
- The service and worker talk over a private Unix socket. Each frame is a 4-byte
  big-endian header length, a JSON header and an optional body of float32
  samples. Speech streams in chunks and can be cancelled between chunks.
- After Tohru hides, the worker unloads after `hidden_unload_seconds` (5 by
  default), which releases the reservation. Showing her again, or a voice turn
  that shows her, loads it again; an answer that starts within 3 s of the reload
  waits for it, so it still speaks in the high voice.

A sentence the worker cannot speak falls back to the low profile. If the worker
breaks, every answer returns to the low profile and the selector tries again
the next time Tohru appears. `{"event":"profile"}` reports each change.

## Verify without microphone access

Nothing downloads speech models. With the shared catalog, a recognition check
is:

```sh
printf '%s\n' '{"op":"recognize","audio":"/absolute/path/to/input.wav"}' |
  gpu-run --vram 1GiB --name 'native voice verification' -- \
  .tmp/native-voice/bin/tohru-voice-native --evaluate \
  --asr-model "$MODEL_ROOT/by-name/voice-asr"
```

The input must be a mono 16 kHz WAV. The response includes the final transcript
and changed partial transcripts. `--evaluate` never opens a microphone or a
playback sink. It accepts newline-delimited JSON operations:

| Operation | Input | Output |
| --- | --- | --- |
| `phonemes` | `text` | English phoneme string |
| `synthesize` | `text`, `output` WAV path | Waveform metadata and a float32 WAV |
| `recognize` | `audio` WAV path | Final and partial transcripts |
| `vad` | `audio` WAV path | Stateful 512-sample window probabilities |
| `turn` | `audio` WAV path | Smart Turn probability after production preprocessing |
| `features` | `audio`, `output` raw float32 path | Smart Turn input tensor |
| `policy` | `text` | Wake, continuation, and speech-text decisions |
| `stream` | `chunks` array | Incremental speech sentence extraction |

Pass `--vad-model`, `--turn-model`, and `--tts-model` for operations that need
those artifacts. The existing settings flags select the Tide voicepack, speed,
pitch, wake mode, and model lifetime. `--replay input.wav` runs the real VAD,
ASR, and turn state machine without a microphone. Replay emits protocol events
and does not call an assistant provider.

The contract runner `tools/voice-native-contract.py` runs the packaged service
with real models and fake audio utilities inside a reservation. It checks stale
generations, submitted PCM, the session lock, cancellation and EOF cleanup. Its
`--ort-library` argument must name the native library supplied by the Nix
package. `--new-contracts` also checks admitted background tasks, canonical
request metadata, bounded diagnostics and foreground release before task speech.

## Limits

The tests use synthetic speech and fake devices. Physical microphone behavior,
room echo, compositor presentation, audible interruption and acoustic latency
remain unverified. KWS mode is implemented but has no compatible shared-catalog
artifact available in the tested environment. Other platforms are unverified.

Each synthesis operation is cancellable through ORT run options. The speaker
overlaps playback with at most one next synthesized clip. After Kokoro unloads,
the frontend's embedded static dictionaries and tagger remain resident.

The eSpeak fallback is not byte-stable across launches in the tested
environment; sample counts match.
