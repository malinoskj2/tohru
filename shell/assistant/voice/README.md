# Voicepacks

`tohru-tide.bin` is Tohru's default Kokoro voice, "tide": 510 rows of 256 little-endian float32 values (522,240 bytes), the style row for an utterance of N phoneme tokens being row N. The voice service reads it with `numpy.fromfile`; nothing is unpickled at runtime.

## Provenance

- It is derived from the stock Kokoro-82M v1.0 voicepacks (Apache-2.0, hexgrad/Kokoro-82M) as a softmax mix weighted toward `af_heart` 0.49, `af_bella` 0.39 and `af_nova` 0.06 (with `bf_alice`, `af_jessica` and `af_sarah` below 0.02), plus a regularised offset fitted by gradient descent through Kokoro (300 steps).
- The fit target was an original voice designed from this text prompt with Qwen3-TTS VoiceDesign (Apache-2.0): "A mellow, bright young adult female voice, gentle but impish, smooth and melodic with a teasing lilt and relaxed confident pacing, like a wise old spirit in a youthful body. Clear American English."
- It is not a clone of any real person or character; no recording of anyone was used.
- It was converted from `fit3EN_tide_s11.pt` (SHA-256 `3a3ff4058368778dcae3951ab3d38d49ac6f7130a7c5fef2314258d431f415d7`) with `torch.load(..., weights_only=True)`. The `.bin` has SHA-256 `ef8baa2c855c47ade11eeaf5348f83de489f09f18d980dd2b68dcc573b026e70`.
- Measured for the design (dragon-voice card): WER 0.07 and F0 about 324 Hz, spoken at speed 1.0 with no DSP.

It is distributed under Apache-2.0, like the packs it is derived from.
