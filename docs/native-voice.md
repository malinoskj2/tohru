# Run the experimental native voice worker

`tohru-voice-native` replaces the acoustic and speech helper with Rust and native
libraries. The assistant provider, history, and Discord helper remain Python.
The existing Luau controller retains visibility and asynchronous reply policy.
The default Tohru package continues to use the Python voice helper.

## Build

On x86-64 Linux, build the isolated helper:

```sh
mkdir -p .tmp
nix build .#tohru-voice-native --out-link .tmp/native-voice
.tmp/native-voice/bin/tohru-voice-native --help
```

Build the complete opt-in package with `nix build .#tohru-native-voice`.
That package installs a native `tohru-voice` entry point and supplies the existing
dragon artwork. Building does not restart the bar or activate the package.

The Nix package pins sherpa's native libraries and supplies ONNX Runtime,
eSpeak NG, and PulseAudio utilities. Cargo builds require Rust 1.89 or later.
For a manual build, set `SHERPA_ONNX_LIB_DIR` to compatible sherpa libraries to
avoid the upstream crate's automatic native-library download. Set
`ORT_DYLIB_PATH` to a compatible native ONNX Runtime library before execution.
The tested Nix runtime is ONNX Runtime 1.27.1. A library from a newer glibc
environment cannot be loaded into an older manually compiled executable.

## Task admission and capture

The desktop sends `task_accepted` only after detaching the matching current voice poller. This clears foreground thinking and old cumulative continuation without resetting the active utterance or starting audio. Visibility and owner speech still gate later playback. A queued completed follow-up is drained after retiring the old request; late acknowledgement and cancellation callbacks cannot take over the new request. `speech_start` with `background=true` retains the same foreground release behavior when actual task speech starts.

## Verify without microphone access

Use the same configured model paths as the Python helper. Nothing downloads
speech models. With the shared catalog, a recognition check is:

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

## Compare against Python

Use the packaged voice Python environment with numpy, onnxruntime, sherpa-onnx,
Misaki, and `en_core_web_sm`. Keep all model loads inside a scheduler reservation:

```sh
gpu-run --vram 2GiB --name 'voice differential verification' -- \
  /path/to/voice-python/bin/python3 tools/voice-native-compare.py \
  --native .tmp/native-voice/bin/tohru-voice-native
```

The comparison saves full results and generated samples under
`.tmp/docs/rust-voice/comparison/`. It checks policy, phonemes, waveform samples,
streaming recognition, VAD, and Smart Turn preprocessing. The contract runner
`tools/voice-native-contract.py` uses the real helpers with fake audio utilities.
It checks stale generations, submitted PCM, cancellation, and EOF cleanup.
Its `--ort-library` argument must name the native library supplied by the Nix
package. Add `--reference /absolute/path/to/bin/tohru-voice --new-contracts`
to compare against a helper that supports admitted background tasks. That check
also verifies canonical request metadata, bounded diagnostics, and foreground
release before task speech. Diagnostic records exclude arbitrary task payloads.
`tools/voice-native-replay.py` compares complete wake and turn event
traces. `tools/voice-native-benchmark.py` repeats the contract runner in
alternating order and records startup, first-sentence, cancellation, and process
tree memory observations. `tools/voice-native-warm.py` records five interleaved
steady-state synthesis trials and checks their waveforms against Python.

## Limits of the current evidence

The initial comparison passed 73 authored cases. Six speech samples produced
bit-identical waveforms and matching streaming transcripts. The contract runner
also observed identical submitted PCM for its complete reply. These results
establish parity on those inputs, not on every English sentence.

The tests use synthetic speech and fake devices. Physical microphone behavior,
room echo, compositor presentation, audible interruption, and acoustic latency
remain unverified. KWS mode is implemented but has no compatible shared-catalog
artifact available in the tested environment. Other platforms are unverified.
Semantic freshness and UI visibility require their existing controller tests in
addition to acoustic replay.

Each synthesis operation remains cancellable through ORT run options. The
speaker overlaps playback with at most one next synthesized clip. After Kokoro
unloads, the native frontend's embedded static dictionaries and tagger remain
resident. Python's separate TTS process can release those too. Memory behavior
for conversation-scoped loading therefore differs even when waveform output
matches.

The eSpeak fallback passes the same process and lock checks. Its output is not
byte-stable across launches in the tested environment, including repeated runs
of the identical reference command. Sample counts matched. The tests do not
establish an eSpeak audio-quality difference.

The repeated timing observations exclude providers and physical audio. They
do not establish a user-perceived speed improvement. Keep the native worker
opt-in until the target microphone, playback, and desktop flow have been tried.
