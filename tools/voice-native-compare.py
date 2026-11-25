#!/usr/bin/env python3
"""Isolated differential tests. Python is a test oracle, never a native runtime dependency.

Run the parent under gpu-run, including CPU model loads. No real devices/providers/history.
"""
import argparse
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import time
import wave

ROOT = Path(__file__).resolve().parents[1]
MODEL = Path(os.environ.get("MODEL_ROOT", "/missing")) / "by-name"
TEXTS = [
    "Hello, I'm here. What do you need?",
    "I read the record yesterday; now record a new message.",
    "The cost is $12.50, or 3.5 percent of £357.",
    "Dr. Smith's API uses HTTP, NASA, and SQL.",
    "Alice’s dogs' toys aren't where Thomas’ book was.",
    "Tohru, compare Hyprland, NixOS, Zipformer and SQLite.",
    "il-lilly ilili. café naïve résumé X Æ A-12",
    "An object, to object, a minute, minute details, the wind, to wind.",
    "AT&T. A B C. 2026-11-23, 12:30, 1/2 and 123rd.",
    "[Misaki](/misˈɑki/) is a grapheme-to-phoneme engine.",
    "", "...", "hello\nworld", "_like _it", "GPU GPUs CPU CPUs",
]
POLICY = TEXTS + ["tohru", "too roo tell me why", "to", "tell me", "hello tohru", "okay",
                  "hey tohru, what's up", "can you check this tohru", "so tohru, what's next", "I had taro for lunch",
                  "I want to row", "can you check this to ru",
                  "I could use a map and", "Wait a moment", "go ahead", "Here is `a. b`. Next.",
                  "```rust\nlet x=1;\n``` Done.", "[link](https://example.com) **bold**", "α" * 450]


def voice_module():
    loader = importlib.machinery.SourceFileLoader("native_oracle", str(ROOT / "bin/tohru-voice"))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def python_worker():
    import numpy as np
    import onnxruntime as ort
    import sherpa_onnx
    v = voice_module()
    g2p = vad = turn = asr = kokoro = None
    print(json.dumps({"event": "evaluation_ready", "python": True}), flush=True)
    try:
        for line in sys.stdin:
            request = json.loads(line)
            started = time.monotonic()
            result = {}
            try:
                op, text = request["op"], request.get("text", "")
                if op == "phonemes":
                    if g2p is None:
                        import types
                        sys.modules.setdefault("torch", types.ModuleType("torch"))
                        shim = types.ModuleType("transformers")
                        shim.BartForConditionalGeneration = None
                        sys.modules.setdefault("transformers", shim)
                        from misaki import en, espeak
                        g2p = en.G2P(trf=False, british=False, fallback=espeak.EspeakFallback(british=False))
                    result["phonemes"] = g2p(text)[0]
                elif op == "policy":
                    result = dict(wake=v.wake_split(text), sentence=v.sentence(text),
                                  backchannel=v.is_backchannel(text), unfinished=v.is_unfinished(text), hold=v.explicit_hold(text),
                                  handoff=v.explicit_handoff(text), speakable=v.speakable(text), chunks=v.sentences(text))
                elif op == "synthesize":
                    if kokoro is None:
                        kokoro = v.Kokoro(dict(model=str(MODEL / "voice-kokoro"), voicepack="tide", speed=100))
                    worker = kokoro.load()
                    worker.stdin.write((json.dumps(dict(text=text, speed=1.0)) + "\n").encode())
                    worker.stdin.flush()
                    header = json.loads(v.worker_read(worker))
                    if "error" in header:
                        raise RuntimeError(header["error"])
                    audio = np.frombuffer(v.worker_read(worker, header["samples"] * 4), dtype="<f4")
                    Path(request["output"]).write_bytes(audio.tobytes())
                    result = dict(samples=len(audio), rate=24000, phonemes=header["phonemes"],
                                  peak=float(np.max(np.abs(audio))) if len(audio) else 0,
                                  timeline=v.kokoro_timeline(np,audio,header["phonemes"],header.get("durations")))
                elif op in ("vad", "turn", "features", "recognize"):
                    with wave.open(request["audio"]) as wav:
                        assert wav.getnchannels() == 1 and wav.getframerate() == 16000 and wav.getsampwidth() == 2
                        audio = np.frombuffer(wav.readframes(wav.getnframes()), dtype="<i2").astype(np.float32) / 32768
                    if op == "vad":
                        if vad is None: vad = v.Silero(MODEL / "silero-vad/silero-vad-v6.2.3.onnx", np, ort)
                        vad.reset()
                        result["probabilities"] = [vad(np.pad(frame, (0, 512 - len(frame)))) for frame in np.array_split(audio, range(512, len(audio), 512))]
                    elif op == "turn":
                        if turn is None: turn = v.SmartTurn(MODEL / "voice-turn/smart-turn-v3.2-cpu.onnx", np, ort)
                        result["probability"] = turn.probability(audio)
                    elif op == "features":
                        if turn is None: turn = v.SmartTurn(MODEL / "voice-turn/smart-turn-v3.2-cpu.onnx", np, ort)
                        padded = np.pad(audio[-128000:], (max(0, 128000 - len(audio)), 0))
                        features = turn.features(padded)
                        Path(request["output"]).write_bytes(features.astype("<f4").tobytes())
                        result["shape"] = list(features.shape)
                    else:
                        if asr is None: asr = v.Recognizer(MODEL / "voice-asr", sherpa_onnx)
                        asr.start()
                        partials = []
                        for start in range(0, len(audio), 512):
                            asr.feed(audio[start:start+512])
                            partial = asr.partial()
                            if not partials or partials[-1] != partial: partials.append(partial)
                        result = dict(text=asr.finish(), partials=partials)
                else:
                    raise ValueError(op)
                result.update(ok=True, id=request.get("id"), elapsed_ms=(time.monotonic() - started) * 1000)
            except Exception as error:
                result = dict(ok=False, id=request.get("id"), error=repr(error))
            print(json.dumps(result), flush=True)
    finally:
        if kokoro: kokoro.unload()


class Worker:
    def __init__(self, argv, log):
        self.log = open(log, "w")
        self.started = time.monotonic()
        self.process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
                                        text=True, bufsize=1, cwd=ROOT)
        self.read()
        self.startup_ms = (time.monotonic() - self.started) * 1000

    def read(self):
        if not select.select([self.process.stdout], [], [], 120)[0]: raise TimeoutError("worker response")
        line = self.process.stdout.readline()
        if not line: raise RuntimeError(f"worker exited: {self.process.poll()}")
        return json.loads(line)

    def call(self, request):
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        return self.read()

    def close(self):
        self.process.stdin.close()
        try: self.process.wait(15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.log.close()


def compare(args):
    if not os.environ.get("GPU_LEASE_ID"):
        raise SystemExit("Run this comparison inside gpu-run; both workers load local models")
    import numpy as np
    out = Path(args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    native = Worker([str(Path(args.native).resolve()), "--evaluate", "--vad-model", str(MODEL / "silero-vad/silero-vad-v6.2.3.onnx"),
                     "--asr-model", str(MODEL / "voice-asr"), "--turn-model", str(MODEL / "voice-turn/smart-turn-v3.2-cpu.onnx"),
                     "--tts-model", str(MODEL / "voice-kokoro")], out / "native.stderr.log")
    python = Worker([sys.executable, __file__, "--python-worker"], out / "python.stderr.log")
    rows = []

    def pair(request):
        request = dict(request, id=len(rows))
        a = python.call(request)
        b = native.call(request)
        row = dict(request=request, python=a, native=b)
        rows.append(row)
        return row

    try:
        for text in POLICY:
            row = pair(dict(op="policy", text=text))
            strip = lambda x: {k: v for k, v in x.items() if k not in ("elapsed_ms", "id")}
            row["same"] = strip(row["python"]) == strip(row["native"])
        for text in TEXTS:
            row = pair(dict(op="phonemes", text=text))
            row["same"] = row["python"].get("phonemes") == row["native"].get("phonemes") and row["python"]["ok"] and row["native"]["ok"]
        for index, text in enumerate(TEXTS[:6]):
            raw = out / f"python-{index}.f32"
            wav = out / f"native-{index}.wav"
            a = python.call(dict(op="synthesize", text=text, output=str(raw)))
            native_raw = out / f"native-{index}.f32"
            b = native.call(dict(op="synthesize", text=text, output=str(wav), raw_output=str(native_raw)))
            ba = np.fromfile(native_raw, dtype="<f4") if b["ok"] else np.zeros(0)
            aa = np.fromfile(raw, dtype="<f4") if a["ok"] else np.zeros(0)
            same_length = len(aa) == len(ba)
            error = float(np.max(np.abs(aa - ba))) if same_length and len(aa) else None
            at,bt=a.get("timeline",[]),b.get("timeline",[])
            timeline_same=len(at)==len(bt) and all(x[2]==y[2] and abs(x[0]-y[0])<=1e-6 and abs(x[1]-y[1])<=1e-6 for x,y in zip(at,bt))
            row = dict(request=dict(op="synthesize", text=text), python=a, native=b,
                       same=bool(a["ok"] and b["ok"] and a["phonemes"] == b["phonemes"] and same_length and error is not None and error <= 1e-5 and timeline_same), max_abs_error=error,timeline_same=timeline_same)
            rows.append(row)
            if a["ok"]:
                # One matched PCM fixture is used by both recognizers. It is synthetic speech, not a room recording.
                pywav = out / f"python-{index}.wav"
                with wave.open(str(pywav), "wb") as w:
                    w.setparams((1, 2, 24000, 0, "NONE", "not compressed"))
                    w.writeframes((np.clip(aa, -1, 1) * 32767).astype("<i2").tobytes())
                fixture = out / f"input-{index}.wav"
                subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-i", str(pywav), "-ar", "16000", "-ac", "1", str(fixture)], check=True)
                for op in ("recognize", "vad", "turn"):
                    row = pair(dict(op=op, audio=str(fixture)))
                    if op == "recognize": row["same"] = row["python"].get("text") == row["native"].get("text") and row["python"].get("partials") == row["native"].get("partials")
                    elif op == "vad": row["max_abs_error"] = float(np.max(np.abs(np.array(row["python"]["probabilities"]) - row["native"]["probabilities"]))); row["same"] = row["max_abs_error"] <= 1e-5
                    else: row["max_abs_error"] = abs(row["python"]["probability"] - row["native"]["probability"]); row["same"] = row["max_abs_error"] <= 1e-5
                feature_files = [out / f"features-{index}-{side}.f32" for side in ("python", "native")]
                a = python.call(dict(op="features", audio=str(fixture), output=str(feature_files[0])))
                b = native.call(dict(op="features", audio=str(fixture), output=str(feature_files[1])))
                delta = float(np.max(np.abs(np.fromfile(feature_files[0], dtype="<f4") - np.fromfile(feature_files[1], dtype="<f4"))))
                rows.append(dict(request=dict(op="features", audio=str(fixture)), python=a, native=b, same=delta <= 1e-5, max_abs_error=delta))
        report = dict(kind="differential correctness; timings are diagnostic, not a speed claim", startup_ms=dict(python=python.startup_ms, native=native.startup_ms), rows=rows)
        (out / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
        failures = [r for r in rows if not r.get("same")]
        print(json.dumps(dict(cases=len(rows), passed=len(rows)-len(failures), failed=len(failures), report=str(out / "comparison.json"), failures=[r["request"] for r in failures]), indent=2))
        return bool(failures)
    finally:
        native.close()
        python.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python-worker", action="store_true")
    parser.add_argument("--native", default="voice/target/release/tohru-voice-native")
    parser.add_argument("--output", default=".tmp/docs/rust-voice/comparison")
    args = parser.parse_args()
    if args.python_worker: python_worker()
    else: sys.exit(compare(args))
