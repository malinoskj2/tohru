#!/usr/bin/env python3
import ctypes
import json
import math
import os
import queue
import re
import select
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

LOW_DEVICES = ("cpu",)
LOW_PRECISIONS = ("int8", "float32")
HIGH_DEVICES = ("cuda",)
HIGH_PRECISIONS = ("bfloat16", "float16", "float32")
ALIAS = re.compile(r"[a-z0-9][a-z0-9._-]{0,127}")
SPEAKER = re.compile(r"[A-Za-z0-9_]{1,64}")
# Peak working memory beyond the weights: CUDA context once, then each model's
# caches, CUDA graphs and activations at the worker's fixed sizes.
CUDA_CONTEXT_MIB = 512
WORKING_MIB = {"tts": 1536, "asr": 1024}
WEIGHT_MARGIN = 1.10
WORKER_RATE_LIMIT = 48_000 * 60
WORKER_TIMEOUT = 60


class ConfigError(ValueError):
    pass


class WorkerReplyError(RuntimeError):
    """The worker is healthy but could not answer this one request."""


class ModelsNotReady(RuntimeError):
    def __init__(self, problems):
        self.problems = problems
        super().__init__("; ".join(f"{alias}: {why}" for alias, why in problems))


def _choice(raw, key, allowed, default):
    value = raw.get(key.rsplit(".", 1)[-1], default)
    if value not in allowed:
        raise ConfigError(f"assistant.voice_profiles.{key} must be one of {', '.join(allowed)}, not {value!r}")
    return value


def _integer(raw, key, low, high, default):
    value = raw.get(key.rsplit(".", 1)[-1], default)
    if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
        raise ConfigError(f"assistant.voice_profiles.{key} must be a whole number from {low} to {high}, not {value!r}")
    return value


def parse_config(raw):
    """The profile settings the shell passes as JSON, validated the same way as the settings file."""
    if not isinstance(raw, dict):
        raise ConfigError("voice profiles must be a JSON object")
    low = raw.get("low") or {}
    high = raw.get("high") or {}
    if not isinstance(low, dict) or not isinstance(high, dict):
        raise ConfigError("assistant.voice_profiles.low and .high must be tables")
    root = raw.get("model_root", "")
    if not isinstance(root, str) or root and not root.startswith("/"):
        raise ConfigError(f"assistant.voice_profiles.model_root must be empty ($MODEL_ROOT) or an absolute path, not {root!r}")
    config = {
        "model_root": root,
        "threshold": _integer(raw, "vram_threshold_percent", 1, 100, 50),
        "unload_seconds": _integer(raw, "hidden_unload_seconds", 0, 86400, 120),
        "queue_seconds": _integer(raw, "queue_timeout_seconds", 1, 3600, 60),
        "low": {"device": _choice(low, "low.device", LOW_DEVICES, "cpu"),
                "precision": _choice(low, "low.precision", LOW_PRECISIONS, "int8")},
    }
    enabled = high.get("enabled", False)
    if not isinstance(enabled, bool):
        raise ConfigError("assistant.voice_profiles.high.enabled must be true or false")
    tts = high.get("tts_model", "voice-tts")
    asr = high.get("asr_model", "")
    speaker = high.get("tts_speaker", "Serena")
    python = high.get("python", "")
    if not isinstance(tts, str) or not ALIAS.fullmatch(tts) or ".." in tts:
        raise ConfigError(f"assistant.voice_profiles.high.tts_model must be a $MODEL_ROOT alias such as voice-tts, not {tts!r}")
    if not isinstance(asr, str) or asr and (not ALIAS.fullmatch(asr) or ".." in asr):
        raise ConfigError(f"assistant.voice_profiles.high.asr_model must be empty or a $MODEL_ROOT alias, not {asr!r}")
    if not isinstance(speaker, str) or not SPEAKER.fullmatch(speaker):
        raise ConfigError(f"assistant.voice_profiles.high.tts_speaker must be a speaker name such as Serena, not {speaker!r}")
    if not isinstance(python, str) or python and not python.startswith("/"):
        raise ConfigError(f"assistant.voice_profiles.high.python must be an absolute interpreter path, not {python!r}")
    if enabled and not python:
        raise ConfigError("assistant.voice_profiles.high.python is not set: name the interpreter with torch (CUDA) and faster-qwen3-tts")
    config["high"] = {
        "enabled": enabled,
        "device": _choice(high, "high.device", HIGH_DEVICES, "cuda"),
        "precision": _choice(high, "high.precision", HIGH_PRECISIONS, "bfloat16"),
        "python": python,
        "tts_model": tts,
        "tts_speaker": speaker,
        "asr_model": asr,
        "vram_mib": _integer(high, "high.vram_mib", 0, 262_144, 0),
    }
    return config


def read_json(path):
    with open(path, "rb") as stream:
        data = stream.read(64 * 1024 * 1024 + 1)
    if len(data) > 64 * 1024 * 1024:
        raise ValueError(f"{path} exceeds 64 MiB")
    return json.loads(data)


def resolve_models(high, model_root=None):
    """Each high-profile alias's ready folder and catalog size, from $MODEL_ROOT's catalog and readiness.

    Nothing is downloaded or copied; a missing or unready alias is reported by name."""
    root = model_root if model_root is not None else os.environ.get("MODEL_ROOT", "")
    roles = [("tts", high["tts_model"])] + ([("asr", high["asr_model"])] if high["asr_model"] else [])
    if not root:
        raise ModelsNotReady([(alias, "MODEL_ROOT is not set") for _, alias in roles])
    try:
        catalog = read_json(Path(root) / "catalog.json").get("entries", {})
        readiness = read_json(Path(root) / "readiness.json").get("entries", {})
    except (OSError, ValueError, AttributeError) as error:
        raise ModelsNotReady([(alias, f"cannot read the model catalog: {error}") for _, alias in roles]) from None
    resolved, problems = {}, []
    for role, alias in roles:
        entry, ready = catalog.get(alias), readiness.get(alias)
        if not isinstance(entry, dict):
            problems.append((alias, "not in $MODEL_ROOT/catalog.json; add it to the host catalog and run local-models-sync"))
            continue
        if not entry.get("enabled", True):
            problems.append((alias, "disabled in the catalog"))
            continue
        status = ready.get("status") if isinstance(ready, dict) else None
        files = ready.get("files", []) if isinstance(ready, dict) else []
        if status != "ready" or any(isinstance(f, dict) and f.get("status") != "ready" for f in files):
            problems.append((alias, f"readiness is {status or 'missing'}; run systemctl --user start local-models-sync.service on the host"))
            continue
        folder = Path(ready.get("path") or Path(root) / "by-name" / alias)
        absent = [f["target"] for f in entry.get("files", []) if isinstance(f, dict) and f.get("target")
                  and not (folder / f["target"]).exists()]
        if not folder.is_dir() or absent:
            problems.append((alias, f"files missing under {folder}: {', '.join(absent[:3]) or 'the folder'}"))
            continue
        size = sum(int(f.get("size") or 0) for f in entry.get("files", []) if isinstance(f, dict))
        resolved[role] = {"alias": alias, "path": str(folder), "bytes": size}
    if problems:
        raise ModelsNotReady(problems)
    return resolved


def estimate_vram_mib(models, precision):
    """A conservative peak: weights (stored in 16 bits; doubled for float32) with a margin, each model's working
    memory and one CUDA context, rounded up to 512 MiB."""
    factor = 2.0 if precision == "float32" else 1.0
    total = CUDA_CONTEXT_MIB
    for role, model in models.items():
        total += model["bytes"] / 2**20 * factor * WEIGHT_MARGIN + WORKING_MIB.get(role, 1024)
    return int(math.ceil(total / 512) * 512)


def vram_usage(status):
    """Committed VRAM as a percentage of the GPU, from `gpu-status --json`.

    Measured use, with each running reservation charged at its full budget even before its model has
    allocated it; the scheduler's desktop headroom is a reserve, not use, so it is left out."""
    memory = status.get("memory") if isinstance(status, dict) else None
    if not isinstance(memory, dict) or status.get("telemetry_error"):
        raise RuntimeError(f"GPU telemetry unavailable: {status.get('telemetry_error') if isinstance(status, dict) else status!r}")
    total, used = int(memory["total_mib"]), int(memory["used_mib"])
    attributed = int(memory.get("attributed_mib") or 0)
    reserved = sum(int(job.get("vram_mib") or 0) for job in status.get("jobs", [])
                   if job.get("state") == "running" and job.get("mode") != "exclusive")
    if any(job.get("state") == "running" and job.get("mode") == "exclusive" for job in status.get("jobs", [])):
        return 100.0, total, total
    committed = max(used, used - attributed + reserved)
    if total <= 0:
        raise RuntimeError("GPU telemetry reports no memory")
    return 100.0 * committed / total, committed, total


def gpu_status(runner=subprocess.run):
    if not shutil.which("gpu-status"):
        raise RuntimeError("gpu-status is not installed, so the shared GPU scheduler is unavailable")
    result = runner(["gpu-status", "--json"], capture_output=True, text=True, timeout=10)
    if result.returncode != 0:
        raise RuntimeError(f"gpu-status failed: {(result.stderr or '').strip()[:200]}")
    return json.loads(result.stdout)


def _die_with_parent():
    try:
        ctypes.CDLL(None, use_errno=True).prctl(1, signal.SIGTERM)  # PR_SET_PDEATHSIG
    except (OSError, AttributeError):
        pass


def worker_read(worker, size=None, timeout=WORKER_TIMEOUT):
    deadline = time.monotonic() + timeout
    data = bytearray()
    while size is None or len(data) < size:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([worker.stdout], [], [], remaining)[0]:
            raise TimeoutError("the high-VRAM voice worker timed out")
        chunk = os.read(worker.stdout.fileno(), 1 if size is None else min(65536, size - len(data)))
        if not chunk:
            raise RuntimeError("the high-VRAM voice worker exited")
        data.extend(chunk)
        if size is None:
            if chunk == b"\n":
                return bytes(data)
            if len(data) > 65536:
                raise RuntimeError("the high-VRAM voice worker header is too large")
    return bytes(data)


class HighWorker:
    """The high-VRAM models in one child under `gpu-run`: the reservation lives exactly as long as the child.

    `start` returns at once; the queue wait and the model load happen on a thread, and `ready` only turns
    true once the worker has loaded everything, so callers keep using the low profile until then."""

    def __init__(self, config, models, vram_mib, log, launcher=subprocess.Popen, script=None):
        self.config, self.models, self.vram_mib, self.log = config, models, vram_mib, log
        self.launcher = launcher
        self.script = script or os.path.realpath(__file__)
        self.process = None
        self.ready = False
        self.asr = False
        self.state = "idle"
        self.rate = 24_000
        self.lock = threading.RLock()
        self.cancelled = False

    def command(self):
        high = self.config["high"]
        spec = {"device": high["device"], "precision": high["precision"], "speaker": high["tts_speaker"],
                "models": {role: model["path"] for role, model in self.models.items()}}
        return ["gpu-run", "--vram", f"{self.vram_mib}MiB", "--name", "Tohru voice (high VRAM)",
                "--timeout", str(self.config["queue_seconds"]), "--",
                high["python"], self.script, "--worker", json.dumps(spec)]

    def start(self, on_ready=None, on_failed=None):
        with self.lock:
            if self.process is not None:
                return
            self.cancelled = False
            self.state = "queued"
            environment = {k: v for k, v in os.environ.items() if k != "GPU_LEASE_ID"}
            environment.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1", TOHRU_VOICE_ENV_READY="1")
            self.process = self.launcher(self.command(), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                         stderr=subprocess.PIPE, bufsize=0, env=environment,
                                         preexec_fn=_die_with_parent, start_new_session=False)
        threading.Thread(target=self._watch_stderr, args=(self.process,), daemon=True).start()
        threading.Thread(target=self._await_ready, args=(self.process, on_ready, on_failed), daemon=True).start()

    def _watch_stderr(self, process):
        for raw in iter(process.stderr.readline, b""):
            line = raw.decode(errors="replace").strip()
            if line.startswith("gpu-run: waiting:"):
                self.log(f"voice profile high: reservation queued ({line[len('gpu-run: waiting:'):].strip()})")
            elif line.startswith("gpu-run: granted"):
                with self.lock:
                    if self.process is process:
                        self.state = "loading"
                self.log(f"voice profile high: reservation granted for {self.vram_mib} MiB; loading models")
            elif line:
                self.log(f"voice profile high: {line[:300]}")
        process.stderr.close()

    def _await_ready(self, process, on_ready, on_failed):
        try:
            header = json.loads(worker_read(process, timeout=self.config["queue_seconds"] + 300))
            if "error" in header:
                raise RuntimeError(header["error"])
            if not header.get("ready"):
                raise RuntimeError("the worker did not report ready")
            with self.lock:
                if self.process is not process or self.cancelled:
                    return
                self.rate = int(header.get("rate") or 24_000)
                self.asr = bool(header.get("asr"))
                self.ready = True
                self.state = "ready"
            if on_ready:
                on_ready(header)
        except Exception as error:
            with self.lock:
                mine = self.process is process and not self.cancelled
            code = process.poll()
            self.stop()
            if mine and on_failed:
                reason = str(error)
                if code is not None and code != 0 and "exited" in reason:
                    reason = f"gpu-run or the worker exited with status {code} (no reservation, or the load failed)"
                on_failed(reason)

    def stop(self):
        with self.lock:
            process, self.process = self.process, None
            self.ready = False
            self.cancelled = True
            self.state = "idle"
        if process is not None:
            try:
                process.stdin.close()
            except OSError:
                pass
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        if process is not None:
            try:
                process.stdout.close()
            except OSError:
                pass
        return process is not None

    def request(self, message, payload=b"", timeout=WORKER_TIMEOUT):
        with self.lock:
            process = self.process
            if not self.ready or process is None:
                raise RuntimeError("the high-VRAM voice profile is not loaded")
            try:
                process.stdin.write((json.dumps(message) + "\n").encode() + payload)
                process.stdin.flush()
                header = json.loads(worker_read(process, timeout=timeout))
                if "error" not in header:
                    size = int(header.get("bytes") or 0)
                    if not 0 <= size <= WORKER_RATE_LIMIT * 4:
                        raise ValueError("invalid reply size")
                    return header, worker_read(process, size, timeout=timeout) if size else b""
            except (OSError, ValueError, TimeoutError, RuntimeError) as error:
                self.ready = False
                raise RuntimeError(f"the high-VRAM voice worker failed: {error}") from None
        raise WorkerReplyError(str(header["error"]))


class Selector:
    """Picks the voice profile each time Tohru appears; the low profile is always loaded and always the fallback.

    Shown: below the VRAM threshold, the high profile's reservation is requested and its models load in the
    background. Hidden for `hidden_unload_seconds`: the worker exits and the reservation is released.
    Every decision runs on one long-lived thread, which is also the thread that spawns `gpu-run`, so the
    parent-death signal only fires when the voice service itself goes away."""

    def __init__(self, config, log, notify=None, status=gpu_status, resolve=resolve_models,
                 worker=HighWorker, timer=threading.Timer, inline=False):
        self.config, self.log, self.notify = config, log, notify or (lambda active, reason: None)
        self.status, self.resolve, self.make_worker, self.make_timer = status, resolve, worker, timer
        self.worker = None
        self.unload_timer = None
        self.visible = False
        self.hidden_serial = 0
        self.inline = inline
        self.events = queue.Queue()
        self.thread = None
        if not inline:
            self.thread = threading.Thread(target=self._loop, name="voice-profiles", daemon=True)
            self.thread.start()

    @property
    def enabled(self):
        return self.config["high"]["enabled"]

    def active(self):
        return "high" if self.high() is not None else "low"

    def high(self):
        worker = self.worker
        return worker if worker is not None and worker.ready else None

    def _post(self, *event):
        if self.inline:
            self._handle(event)
        else:
            self.events.put(event)

    def _loop(self):
        while True:
            event = self.events.get()
            try:
                self._handle(event)
            except Exception as error:
                self.log(f"voice profile: {type(error).__name__}: {error}; using low")
            if event[0] == "shutdown":
                return

    def presence(self, visible):
        self._post("presence", bool(visible))

    def failed(self, reason, worker=None):
        self._post("failed", reason, worker)

    def shutdown(self, timeout=15):
        self._post("shutdown")
        if self.thread is not None and self.thread is not threading.current_thread():
            self.thread.join(timeout)

    def _handle(self, event):
        kind = event[0]
        if kind == "presence":
            self._presence(event[1])
        elif kind == "hidden":
            if not self.visible and event[1] == self.hidden_serial:
                self._unload(f"Tohru has been hidden for {self.config['unload_seconds']} s")
        elif kind == "ready":
            if self.worker is event[1]:
                self.log("voice profile: high loaded; new answers use it")
                self.notify("high", "loaded")
        elif kind == "failed":
            reason, worker = event[1], event[2]
            if worker is not None and self.worker is not worker:
                return
            current, self.worker = self.worker, None
            if current is not None:
                current.stop()
            self.log(f"voice profile: falling back to low; {reason}")
            self.notify("low", reason)
        elif kind == "shutdown":
            self.visible = False
            self._cancel_timer()
            self._unload("the voice service stopped")

    def _cancel_timer(self):
        self.hidden_serial += 1
        if self.unload_timer is not None:
            self.unload_timer.cancel()
            self.unload_timer = None

    def _presence(self, visible):
        self.visible = visible
        self._cancel_timer()
        if not self.enabled:
            return
        if not visible:
            if self.worker is None:
                return
            if self.config["unload_seconds"] == 0:
                self._unload("Tohru is hidden")
            else:
                serial = self.hidden_serial
                self.unload_timer = self.make_timer(self.config["unload_seconds"], lambda: self._post("hidden", serial))
                self.unload_timer.daemon = True
                self.unload_timer.start()
            return
        if self.worker is None:
            self._consider()

    def _consider(self):
        threshold = self.config["threshold"]
        try:
            percent, committed, total = vram_usage(self.status())
        except Exception as error:
            self.log(f"voice profile: staying on low; cannot read GPU use from the scheduler ({error})")
            return False
        if percent >= threshold:
            self.log(f"voice profile: staying on low; GPU VRAM {percent:.0f}% used ({committed}/{total} MiB) is at or above {threshold}%")
            return False
        try:
            models = self.resolve(self.config["high"], self.config.get("model_root") or None)
        except ModelsNotReady as error:
            aliases = ", ".join(alias for alias, _ in error.problems)
            self.log(f"voice profile: staying on low; high-VRAM models not ready ({aliases}): {error}")
            self.notify("low", f"models not ready: {aliases}")
            return False
        if not shutil.which("gpu-run"):
            self.log("voice profile: staying on low; gpu-run is not installed, so no GPU reservation can be held")
            return False
        budget = self.config["high"]["vram_mib"] or estimate_vram_mib(models, self.config["high"]["precision"])
        worker = self.worker = self.make_worker(self.config, models, budget, self.log)
        self.log(f"voice profile: GPU VRAM {percent:.0f}% used ({committed}/{total} MiB) is under {threshold}%; "
                 f"requesting a {budget} MiB reservation for {', '.join(m['alias'] for m in models.values())}; "
                 "answering with low until it is ready")
        try:
            worker.start(on_ready=lambda _: self._post("ready", worker),
                         on_failed=lambda reason: self._post("failed", reason, worker))
        except OSError as error:
            self.worker = None
            worker.stop()
            self.log(f"voice profile: falling back to low; cannot start gpu-run: {error}")
            self.notify("low", str(error))
            return False
        return True

    def _unload(self, reason):
        worker, self.worker = self.worker, None
        if worker is not None and worker.stop():
            self.log(f"voice profile: high unloaded and its GPU reservation released ({reason}); using low")
            self.notify("low", reason)


def worker_main(spec_json):
    """Runs inside the gpu-run reservation, in the high profile's interpreter: load, report ready, serve.

    Replies go out on a private copy of stdout; fd 1 itself points at stderr, so anything the model libraries
    print, from Python or native code, lands in the log instead of corrupting the reply stream."""
    sys.stdout.flush()
    out = os.fdopen(os.dup(1), "wb")
    os.dup2(2, 1)
    sys.stdout = sys.stderr

    def reply(header, body=b""):
        out.write((json.dumps(header) + "\n").encode() + body)
        out.flush()

    try:
        spec = json.loads(spec_json)
        import numpy as np
        import torch
        if not torch.cuda.is_available():
            raise RuntimeError("torch reports no CUDA device in the high-VRAM interpreter")
        dtype = {"bfloat16": torch.bfloat16, "float16": torch.float16, "float32": torch.float32}[spec["precision"]]
        try:
            from faster_qwen3_tts import FasterQwen3TTS
        except ImportError:
            raise RuntimeError("faster-qwen3-tts is not installed in the high-VRAM interpreter; nothing was installed") from None
        tts = FasterQwen3TTS.from_pretrained(spec["models"]["tts"], device=spec["device"], dtype=dtype,
                                             attn_implementation="sdpa", max_seq_len=1024, local_files_only=True)
        tts.warmup(prefill_len=128)
        asr = None
        if spec["models"].get("asr"):
            try:
                from faster_whisper import WhisperModel
            except ImportError:
                raise RuntimeError("faster-whisper is not installed in the high-VRAM interpreter; nothing was installed") from None
            asr = WhisperModel(spec["models"]["asr"], device=spec["device"],
                               compute_type={"bfloat16": "bfloat16", "float16": "float16", "float32": "float32"}[spec["precision"]],
                               local_files_only=True)
    except Exception as error:
        reply({"error": f"{type(error).__name__}: {error}"[:400]})
        return 3
    rate = 24_000
    reply({"ready": True, "rate": rate, "asr": asr is not None})
    stdin = sys.stdin.buffer
    for line in iter(stdin.readline, b""):
        try:
            message = json.loads(line)
            if message.get("op") == "tts":
                chunks, rate = [], 24_000
                for audio, sample_rate, _timing in tts.generate_custom_voice_streaming(
                        text=str(message["text"])[:2000], speaker=spec["speaker"], language="English",
                        chunk_size=4, max_new_tokens=900):
                    chunks.append(np.asarray(audio, dtype=np.float32).reshape(-1))
                    rate = int(sample_rate)
                audio = np.concatenate(chunks) if chunks else np.zeros(0, dtype=np.float32)
                body = audio.astype("<f4").tobytes()
                reply({"rate": rate, "bytes": len(body)}, body)
            elif message.get("op") == "asr" and asr is not None:
                size = int(message["bytes"])
                if not 0 <= size <= 16_000 * 4 * 60:
                    raise ValueError("audio too long")
                audio = np.frombuffer(stdin.read(size), dtype="<f4")
                segments, _ = asr.transcribe(audio, language="en", beam_size=1, vad_filter=False,
                                             condition_on_previous_text=False)
                reply({"text": " ".join(segment.text.strip() for segment in segments).strip()})
            else:
                reply({"error": "unsupported request"})
        except Exception as error:
            reply({"error": f"{type(error).__name__}: {error}"[:300]})
    del tts, asr
    torch.cuda.empty_cache()
    return 0


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--worker":
        sys.exit(worker_main(sys.argv[2]))
    print("usage: tohru_voice_profiles.py --worker SPEC", file=sys.stderr)
    sys.exit(2)
