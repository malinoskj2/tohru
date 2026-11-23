# Native voice dependencies

The Rust worker uses these implementations. Exact Cargo versions and checksums
are recorded in `Cargo.lock`. The Nix native-library archive has a fixed hash in
`nix/voice-native.nix`.

| Component | Version | License and source |
| --- | --- | --- |
| Rust ONNX Runtime wrapper | ort and ort-sys 2.0.0-rc.10 | MIT or Apache-2.0, https://github.com/pykeio/ort |
| Native ONNX Runtime | Nix-pinned package | MIT, https://github.com/microsoft/onnxruntime |
| Streaming recognition C API | sherpa-onnx 2.0.3 | Apache-2.0, https://github.com/k2-fsa/sherpa-onnx |
| English frontend | misaki-rs 0.11.0 | MIT, https://github.com/MicheleYin/misaki-rs |
| English frontend lexicons | Misaki 0.9.4 resources carried by misaki-rs | Apache-2.0, https://github.com/hexgrad/misaki |
| Embedded English tagger | spaCy en_core_web_sm 3.8.0, converted by misaki-rs | MIT, https://github.com/explosion/spacy-models |
| Unknown-word phonemes and fallback speech | installed eSpeak NG executable | GPL-3.0-or-later, https://github.com/espeak-ng/espeak-ng |

The worker invokes the installed eSpeak executable. It does not enable
misaki-rs's `espeak` feature or statically link eSpeak into the Rust binary.
The upstream sherpa archive contains static native dependencies, including
ONNX Runtime, Kaldi components, OpenFST, SentencePiece, utf8proc, and KissFFT.
Their notices remain applicable to distribution of the resulting binary.
The retained Apache license and Misaki Rust notice are in `licenses/`.

No ASR, VAD, Smart Turn, or Kokoro weights are bundled or downloaded by the
application. Their existing licenses and pinned sources are listed in
`docs/configuration.md`. Tide's provenance and Apache license are described in
`shell/assistant/voice/README.md`.

The embedded tagger has 1,570,514 float32 parameters. Conversion of the installed
English model produces the same bytes as misaki-rs's tagger, with SHA-256
`0f884fe5841b110c7457a3dd46c78236ad91dc6d028d4ebc37bb9f4c93c22bf3`.
This preserves the current tagger rather than selecting a new model.
