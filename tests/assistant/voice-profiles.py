#!/usr/bin/env python3
"""Voice profile selection: settings, $MODEL_ROOT readiness, VRAM checks, the gpu-run lifecycle and fallbacks."""
import importlib.machinery
import importlib.util
import json
import os
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "bin"))
import tohru_voice_profiles as profiles  # noqa: E402

loader = importlib.machinery.SourceFileLoader("tohru_voice", str(ROOT / "bin/tohru-voice"))
spec = importlib.util.spec_from_loader("tohru_voice", loader)
voice_module = importlib.util.module_from_spec(spec)
loader.exec_module(voice_module)

try:
    import numpy
except ImportError:
    numpy = None

HIGH = {"enabled": True, "python": "/gpu/bin/python3"}


def config(**high):
    return profiles.parse_config({"high": {**HIGH, **high}, "hidden_unload_seconds": 30})


def status(used=2000, total=32000, jobs=(), attributed=0, error=None):
    return {"memory": {"total_mib": total, "used_mib": used, "headroom_mib": 4096, "attributed_mib": attributed},
            "telemetry_error": error, "jobs": list(jobs)}


def model_root(base, entries):
    """entries: alias -> (status, {target: bytes}, write_files)."""
    catalog, readiness = {}, {}
    for alias, (state, files, write) in entries.items():
        folder = Path(base) / "by-name" / alias
        catalog[alias] = {"enabled": True, "files": [{"target": t, "size": n} for t, n in files.items()]}
        readiness[alias] = {"path": str(folder), "status": state,
                            "files": [{"target": t, "status": state} for t in files]}
        if write:
            for target in files:
                (folder / target).parent.mkdir(parents=True, exist_ok=True)
                (folder / target).write_bytes(b"x")
    Path(base, "catalog.json").write_text(json.dumps({"entries": catalog}))
    Path(base, "readiness.json").write_text(json.dumps({"entries": readiness}))
    return base


class ConfigTests(unittest.TestCase):
    def test_defaults_keep_the_cpu_profile_and_leave_high_off(self):
        parsed = profiles.parse_config({})
        self.assertEqual(parsed["low"], {"device": "cpu", "precision": "int8"})
        self.assertFalse(parsed["high"]["enabled"])
        self.assertEqual((parsed["high"]["device"], parsed["high"]["precision"]), ("cuda", "bfloat16"))
        self.assertEqual((parsed["high"]["tts_model"], parsed["high"]["asr_model"]), ("voice-tts", ""))
        self.assertEqual((parsed["threshold"], parsed["unload_seconds"], parsed["queue_seconds"]), (50, 120, 60))

    def test_errors_name_the_setting(self):
        cases = [
            ({"vram_threshold_percent": 0}, "assistant.voice_profiles.vram_threshold_percent"),
            ({"vram_threshold_percent": True}, "vram_threshold_percent"),
            ({"low": {"device": "cuda"}}, "assistant.voice_profiles.low.device"),
            ({"low": {"precision": "bfloat16"}}, "assistant.voice_profiles.low.precision"),
            ({"high": {"precision": "int4"}}, "assistant.voice_profiles.high.precision"),
            ({"high": {"tts_model": "/mnt/models/voice-tts"}}, "high.tts_model must be a $MODEL_ROOT alias"),
            ({"high": {"asr_model": "../x"}}, "high.asr_model"),
            ({"high": {"tts_speaker": "two words"}}, "high.tts_speaker"),
            ({"high": {"enabled": True}}, "high.python is not set"),
            ({"high": {"enabled": "yes"}}, "high.enabled must be true or false"),
            ({"model_root": "models"}, "model_root"),
            ([], "JSON object"),
        ]
        for raw, message in cases:
            with self.subTest(raw=raw), self.assertRaises(profiles.ConfigError) as caught:
                profiles.parse_config(raw)
            self.assertIn(message, str(caught.exception))

    def test_invalid_settings_keep_low_only_with_a_visible_warning(self):
        warnings = []
        selector, precision = voice_module.voice_profiles('{"high": {"enabled": true}}', warnings)
        self.assertIsNone(selector)
        self.assertEqual(precision, "int8")
        self.assertIn("high.python is not set", warnings[0])
        self.assertIn("low-VRAM profile only", warnings[0])
        self.assertEqual(voice_module.voice_profiles('{"low": {"precision": "float32"}}', [])[1], "float32")

    def test_low_precision_picks_the_transducer_files(self):
        with tempfile.TemporaryDirectory() as folder:
            for name in ("tokens.txt", "encoder.onnx", "encoder.int8.onnx", "decoder.onnx", "joiner.int8.onnx"):
                Path(folder, name).write_text("x")
            self.assertTrue(voice_module.transducer(Path(folder))["encoder"].endswith("encoder.int8.onnx"))
            full = voice_module.transducer(Path(folder), "float32")
            self.assertTrue(full["encoder"].endswith("encoder.onnx"))
            self.assertTrue(full["joiner"].endswith("joiner.int8.onnx"), "an int8-only file is still used")


class ModelRootTests(unittest.TestCase):
    def test_ready_alias_resolves_to_its_folder_and_catalog_size(self):
        with tempfile.TemporaryDirectory() as base:
            model_root(base, {"voice-tts": ("ready", {"model.safetensors": 3833402552, "speech_tokenizer/model.safetensors": 682293092}, True)})
            resolved = profiles.resolve_models(config()["high"], base)
            self.assertEqual(resolved["tts"]["path"], str(Path(base) / "by-name/voice-tts"))
            self.assertEqual(resolved["tts"]["bytes"], 4515695644)
            self.assertNotIn("asr", resolved)

    def test_missing_unready_and_absent_aliases_are_reported_by_name(self):
        with tempfile.TemporaryDirectory() as base:
            model_root(base, {"voice-tts": ("missing", {"model.safetensors": 1}, False),
                              "whisper-x": ("ready", {"model.bin": 1}, False)})
            with self.assertRaises(profiles.ModelsNotReady) as caught:
                profiles.resolve_models(config(asr_model="whisper-x")["high"], base)
            names = [alias for alias, _ in caught.exception.problems]
            self.assertEqual(names, ["voice-tts", "whisper-x"])
            self.assertIn("local-models-sync", str(caught.exception))
            self.assertIn("files missing", str(caught.exception))
            with self.assertRaises(profiles.ModelsNotReady) as caught:
                profiles.resolve_models(config(tts_model="voice-nope")["high"], base)
            self.assertIn("voice-nope: not in $MODEL_ROOT/catalog.json", str(caught.exception))
        with self.assertRaises(profiles.ModelsNotReady) as caught:
            profiles.resolve_models(config()["high"], "")
        self.assertIn("MODEL_ROOT is not set", str(caught.exception))

    def test_vram_budget_is_conservative_and_rounded(self):
        tts = {"tts": {"alias": "voice-tts", "path": "/m", "bytes": 4520159586}}
        self.assertEqual(profiles.estimate_vram_mib(tts, "bfloat16"), 7168)
        self.assertEqual(profiles.estimate_vram_mib(tts, "float32"), 11776)
        both = {**tts, "asr": {"alias": "whisper", "path": "/w", "bytes": 1_620_000_000}}
        self.assertEqual(profiles.estimate_vram_mib(both, "float16"), 9728)


class UsageTests(unittest.TestCase):
    def test_running_reservations_count_at_their_full_budget(self):
        percent, committed, total = profiles.vram_usage(status(used=4000, jobs=[
            {"state": "running", "mode": "shared", "vram_mib": 12000},
            {"state": "waiting", "mode": "shared", "vram_mib": 20000}], attributed=1000))
        self.assertEqual((committed, total), (15000, 32000))
        self.assertAlmostEqual(percent, 46.875)

    def test_measured_use_wins_when_higher_and_exclusive_means_full(self):
        self.assertEqual(profiles.vram_usage(status(used=20000))[1], 20000)
        self.assertEqual(profiles.vram_usage(status(jobs=[{"state": "running", "mode": "exclusive", "vram_mib": 0}]))[0], 100.0)
        with self.assertRaises(RuntimeError):
            profiles.vram_usage(status(error="nvidia-smi failed"))


class FakeWorker:
    instances = []

    def __init__(self, config, models, vram_mib, log):
        self.config, self.models, self.vram_mib = config, models, vram_mib
        self.ready, self.asr, self.stopped, self.callbacks = False, False, 0, None
        FakeWorker.instances.append(self)

    def start(self, on_ready=None, on_failed=None):
        self.callbacks = (on_ready, on_failed)

    def stop(self):
        was = not self.stopped
        self.stopped += 1
        self.ready = False
        return was


class FakeTimer:
    made = []

    def __init__(self, seconds, callback):
        self.seconds, self.callback, self.cancelled, self.started = seconds, callback, False, False
        FakeTimer.made.append(self)

    def start(self):
        self.started = True

    def cancel(self):
        self.cancelled = True


TTS = {"tts": {"alias": "voice-tts", "path": "/m/voice-tts", "bytes": 4520159586}}


class SelectorTests(unittest.TestCase):
    def setUp(self):
        FakeWorker.instances, FakeTimer.made = [], []
        self.logs, self.notices, self.status = [], [], status()
        self.resolve = lambda high, root=None: dict(TTS)
        self.which = profiles.shutil.which
        profiles.shutil.which = lambda name: f"/bin/{name}"

    def tearDown(self):
        profiles.shutil.which = self.which

    def selector(self, cfg=None, **kw):
        return profiles.Selector(cfg or config(), self.logs.append, lambda a, r: self.notices.append((a, r)),
                                 status=lambda: self.status, resolve=kw.get("resolve", self.resolve),
                                 worker=FakeWorker, timer=FakeTimer, inline=True)

    def test_disabled_high_never_touches_the_gpu(self):
        calls = []
        selector = profiles.Selector(profiles.parse_config({}), self.logs.append, status=lambda: calls.append(1),
                                     worker=FakeWorker, inline=True)
        selector.presence(True)
        self.assertEqual((calls, FakeWorker.instances, selector.active()), ([], [], "low"))

    def test_under_threshold_requests_a_reservation_and_switches_when_loaded(self):
        selector = self.selector()
        selector.presence(True)
        worker = FakeWorker.instances[0]
        self.assertEqual(worker.vram_mib, 7168)
        self.assertEqual(selector.active(), "low", "low answers while the high profile queues or loads")
        self.assertIn("requesting a 7168 MiB reservation for voice-tts", self.logs[-1])
        selector.presence(True)
        self.assertEqual(len(FakeWorker.instances), 1, "showing again never queues a second reservation")
        worker.ready = True
        worker.callbacks[0]({})
        self.assertEqual(selector.active(), "high")
        self.assertIs(selector.high(), worker)
        self.assertEqual(self.notices[-1], ("high", "loaded"))

    def test_at_or_over_threshold_stays_low(self):
        self.status = status(used=16000, total=32000)
        selector = self.selector()
        selector.presence(True)
        self.assertEqual(FakeWorker.instances, [])
        self.assertIn("50% used (16000/32000 MiB) is at or above 50%", self.logs[-1])
        cfg = config()
        cfg["threshold"] = 51
        self.selector(cfg).presence(True)
        self.assertEqual(len(FakeWorker.instances), 1)

    def test_unready_models_and_missing_scheduler_stay_low_and_say_why(self):
        def missing(high, root=None):
            raise profiles.ModelsNotReady([("voice-tts", "readiness is missing")])
        self.selector(resolve=missing).presence(True)
        self.assertIn("high-VRAM models not ready (voice-tts)", self.logs[-1])
        self.assertEqual(self.notices[-1], ("low", "models not ready: voice-tts"))
        self.status = status(error="telemetry socket missing")
        self.selector().presence(True)
        self.assertIn("cannot read GPU use from the scheduler", self.logs[-1])
        self.status = status()
        profiles.shutil.which = lambda name: None
        self.selector().presence(True)
        self.assertIn("gpu-run is not installed", self.logs[-1])
        self.assertEqual(FakeWorker.instances, [])

    def test_explicit_budget_overrides_the_estimate(self):
        self.selector(config(vram_mib=9000)).presence(True)
        self.assertEqual(FakeWorker.instances[0].vram_mib, 9000)

    def test_hidden_unloads_after_the_grace_and_releases_the_reservation(self):
        selector = self.selector()
        selector.presence(True)
        worker = FakeWorker.instances[0]
        selector.presence(False)
        timer = FakeTimer.made[-1]
        self.assertEqual((timer.seconds, timer.started, worker.stopped), (30, True, 0))
        selector.presence(True)
        self.assertTrue(timer.cancelled, "coming back cancels the unload")
        timer.callback()
        self.assertEqual(worker.stopped, 0, "a cancelled timer that still fires is ignored")
        selector.presence(False)
        FakeTimer.made[-1].callback()
        self.assertEqual(worker.stopped, 1)
        self.assertEqual(selector.active(), "low")
        self.assertIn("high unloaded and its GPU reservation released (Tohru has been hidden for 30 s)", self.logs[-1])
        selector.presence(True)
        self.assertEqual(len(FakeWorker.instances), 2, "the next appearance checks VRAM and loads again")

    def test_zero_grace_unloads_at_once(self):
        cfg = config()
        cfg["unload_seconds"] = 0
        selector = self.selector(cfg)
        selector.presence(True)
        selector.presence(False)
        self.assertEqual((FakeWorker.instances[0].stopped, FakeTimer.made), (1, []))

    def test_failed_reservation_or_load_falls_back_to_low(self):
        selector = self.selector()
        selector.presence(True)
        worker = FakeWorker.instances[0]
        worker.callbacks[1]("gpu-run or the worker exited with status 1 (no reservation, or the load failed)")
        self.assertEqual((selector.active(), worker.stopped), ("low", 1))
        self.assertIn("falling back to low; gpu-run or the worker exited", self.logs[-1])
        stale = worker
        selector.presence(False)
        selector.presence(True)
        selector.failed("late failure from the first worker", stale)
        self.assertEqual(FakeWorker.instances[1].stopped, 0, "a stale failure never stops the current worker")

    def test_shutdown_releases_the_reservation(self):
        selector = self.selector()
        selector.presence(True)
        selector.shutdown()
        self.assertEqual(FakeWorker.instances[0].stopped, 1)
        self.assertIn("the voice service stopped", self.logs[-1])

    def test_threaded_selector_never_blocks_the_caller(self):
        gate = threading.Event()
        def slow_status():
            gate.wait(5)
            return status()
        selector = profiles.Selector(config(), self.logs.append, status=slow_status, resolve=self.resolve,
                                     worker=FakeWorker, timer=FakeTimer)
        started = time.monotonic()
        selector.presence(True)
        self.assertLess(time.monotonic() - started, 0.5)
        gate.set()
        selector.shutdown()
        self.assertEqual(FakeWorker.instances[0].stopped, 1)


FAKE_GPU_RUN = textwrap.dedent("""\
    #!{python}
    import os, sys, time
    args = sys.argv[1:]
    mode = os.environ.get("FAKE_GPU_MODE", "grant")
    with open(os.environ["FAKE_GPU_ARGS"], "w") as out:
        out.write(repr(args))
    if mode == "queue-timeout":
        print("gpu-run: waiting: behind Video render in the queue", file=sys.stderr, flush=True)
        time.sleep(0.2)
        print("gpu-run: error: Timed out waiting for a GPU reservation", file=sys.stderr, flush=True)
        sys.exit(1)
    print("gpu-run: granted shared reservation for Tohru voice (high VRAM)", file=sys.stderr, flush=True)
    command = args[args.index("--") + 1:]
    os.execv(sys.executable, [sys.executable, os.environ["FAKE_WORKER"], *command[2:]])
""")

FAKE_WORKER = textwrap.dedent("""\
    import json, os, struct, sys
    with open(os.environ["FAKE_WORKER_PID"], "w") as out:
        out.write(str(os.getpid()))
    out = sys.stdout.buffer
    if os.environ.get("FAKE_GPU_MODE") == "load-error":
        out.write(b'{"error": "RuntimeError: torch reports no CUDA device"}\\n'); out.flush(); sys.exit(3)
    out.write((json.dumps({"ready": True, "rate": 24000, "asr": True}) + "\\n").encode()); out.flush()
    for line in iter(sys.stdin.buffer.readline, b""):
        message = json.loads(line)
        if message["op"] == "tts" and message["text"] == "bad":
            out.write(b'{"error": "ValueError: unspeakable"}\\n')
        elif message["op"] == "tts":
            body = struct.pack("<240f", *([0.25] * 240))
            out.write((json.dumps({"rate": 24000, "bytes": len(body)}) + "\\n").encode() + body)
        elif message["op"] == "asr":
            sys.stdin.buffer.read(message["bytes"])
            out.write(b'{"text": "turn on the lights"}\\n')
        out.flush()
""")


class WorkerLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        base = Path(self.dir.name)
        (base / "gpu-run").write_text(FAKE_GPU_RUN.format(python=sys.executable))
        (base / "gpu-run").chmod(0o755)
        (base / "worker.py").write_text(FAKE_WORKER)
        self.saved = {k: os.environ.get(k) for k in ("PATH", "FAKE_GPU_MODE", "FAKE_GPU_ARGS", "FAKE_WORKER", "FAKE_WORKER_PID")}
        os.environ["PATH"] = f"{base}:{os.environ['PATH']}"
        os.environ.update(FAKE_GPU_ARGS=str(base / "args"), FAKE_WORKER=str(base / "worker.py"),
                          FAKE_WORKER_PID=str(base / "pid"))
        self.base, self.logs = base, []

    def tearDown(self):
        for key, value in self.saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        self.dir.cleanup()

    def run_worker(self, mode):
        os.environ["FAKE_GPU_MODE"] = mode
        worker = profiles.HighWorker(config(), TTS, 7168, self.logs.append)
        ready, failed = threading.Event(), []
        worker.start(on_ready=lambda _: ready.set(), on_failed=lambda reason: (failed.append(reason), ready.set()))
        self.assertTrue(ready.wait(10))
        return worker, failed

    def test_reservation_is_requested_held_while_loaded_and_released_on_stop(self):
        worker, failed = self.run_worker("grant")
        self.assertEqual(failed, [])
        args = eval((self.base / "args").read_text())
        self.assertEqual(args[:7], ["--vram", "7168MiB", "--name", "Tohru voice (high VRAM)", "--timeout", "60", "--"])
        self.assertEqual(args[7], "/gpu/bin/python3")
        self.assertEqual(json.loads(args[-1])["models"], {"tts": "/m/voice-tts"})
        self.assertEqual(json.loads(args[-1])["precision"], "bfloat16")
        self.assertTrue(worker.ready and worker.asr)
        header, body = worker.request({"op": "tts", "text": "Hello."})
        self.assertEqual((header["rate"], len(body)), (24000, 960))
        with self.assertRaises(profiles.WorkerReplyError):
            worker.request({"op": "tts", "text": "bad"})
        self.assertTrue(worker.ready, "one failed sentence keeps the profile")
        pid = int((self.base / "pid").read_text())
        self.assertTrue(worker.stop())
        self.assertFalse(worker.ready)
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)
        self.assertTrue(any("reservation granted for 7168 MiB" in line for line in self.logs))

    def test_queue_timeout_reports_and_leaves_nothing_running(self):
        worker, failed = self.run_worker("queue-timeout")
        self.assertIn("exited with status 1", failed[0])
        self.assertFalse(worker.ready)
        self.assertIsNone(worker.process)
        self.assertTrue(any("reservation queued (behind Video render in the queue)" in line for line in self.logs))
        self.assertTrue(any("Timed out waiting" in line for line in self.logs))

    def test_load_error_is_reported(self):
        worker, failed = self.run_worker("load-error")
        self.assertIn("no CUDA device", failed[0])
        self.assertIsNone(worker.process)


class FakeLow:
    name = "kokoro"

    def __init__(self):
        self.spoken = []

    def synthesize(self, text, np):
        self.spoken.append(text)
        yield [0.0], 24000, text, []


class FakeHigh:
    def __init__(self, error=None, text="turn on the lights"):
        self.ready, self.asr, self.error, self.text, self.requests = True, True, error, text, []

    def request(self, message, payload=b"", timeout=None):
        self.requests.append(message)
        if self.error:
            raise self.error
        if message["op"] == "asr":
            return {"text": self.text}, b""
        return {"rate": 24000}, b"\0" * 16


class FakeSelector:
    def __init__(self, worker=None):
        self.worker, self.failures = worker, []

    def high(self):
        return self.worker if self.worker is not None and self.worker.ready else None

    def failed(self, reason, worker=None):
        self.failures.append(reason)
        self.worker = None


class FallbackTests(unittest.TestCase):
    def test_answers_use_low_until_high_is_ready(self):
        low = FakeLow()
        self.assertIs(voice_module.speech_engine(low, None), low)
        self.assertIs(voice_module.speech_engine(low, FakeSelector()), low)
        loading = FakeHigh()
        loading.ready = False
        self.assertIs(voice_module.speech_engine(low, FakeSelector(loading)), low)

    @unittest.skipIf(numpy is None, "numpy is not installed")
    def test_high_speech_falls_back_per_sentence_and_on_worker_failure(self):
        low = FakeLow()
        engine = voice_module.speech_engine(low, FakeSelector(FakeHigh()))
        self.assertEqual(engine.name, "high")
        audio, rate, part, _ = next(engine.synthesize("Hi.", numpy))
        self.assertEqual((rate, part, len(audio), low.spoken), (24000, "Hi.", 4, []))
        one_off = voice_module.speech_engine(low, FakeSelector(FakeHigh(profiles.WorkerReplyError("odd text"))))
        list(one_off.synthesize("Odd.", numpy))
        self.assertEqual(low.spoken, ["Odd."])
        selector = FakeSelector(FakeHigh(RuntimeError("the high-VRAM voice worker failed: exited")))
        broken = voice_module.speech_engine(low, selector)
        list(broken.synthesize("One.", numpy))
        list(broken.synthesize("Two.", numpy))
        self.assertEqual(low.spoken, ["Odd.", "One.", "Two."])
        self.assertEqual(len(selector.failures), 1)
        self.assertIsNone(selector.high())

    @unittest.skipIf(numpy is None, "numpy is not installed")
    def test_final_pass_recognition_replaces_the_streaming_transcript_only_when_it_works(self):
        voice = object.__new__(voice_module.Voice)
        voice.np = numpy
        voice.utterance = [numpy.zeros(512, dtype=numpy.float32)]
        voice.profiles = None
        self.assertEqual(voice.high_transcript("turn on the lice"), "turn on the lice")
        voice.profiles = FakeSelector(FakeHigh())
        self.assertEqual(voice.high_transcript("turn on the lice"), "turn on the lights")
        self.assertEqual(voice.profiles.worker.requests[0], {"op": "asr", "bytes": 2048})
        voice.profiles = FakeSelector(FakeHigh(text=""))
        self.assertEqual(voice.high_transcript("streaming"), "streaming")
        voice.profiles = FakeSelector(FakeHigh(RuntimeError("timed out")))
        self.assertEqual(voice.high_transcript("streaming"), "streaming")
        self.assertEqual(voice.profiles.failures, ["timed out"])
        no_asr = FakeHigh()
        no_asr.asr = False
        voice.profiles = FakeSelector(no_asr)
        self.assertEqual(voice.high_transcript("streaming"), "streaming")
        self.assertEqual(no_asr.requests, [])


if __name__ == "__main__":
    unittest.main()
