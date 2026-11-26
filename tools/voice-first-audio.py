#!/usr/bin/env python3
"""End of user speech to first audible PCM, per reply path and voice profile.

Runs the packaged voice service with real VAD, ASR, Smart Turn and TTS models behind fake
parec/pacat/pactl, and the real tohru-assistant helper with the real Claude CLI behind it.
A fake parec streams spoken WAVs in real time and records when each utterance's last
audible sample would have reached the application; a fake pacat records when the first
audible sample of each playback stream arrives. Their difference is the measured latency.
Pulse device latency (60 ms requested) and loudspeaker/room delay are not included.

Paths:
  stub     a canned reply after --stub-ms: the voice pipeline alone
  normal   local backend, tools-enabled Claude Code turn (Opus, as Home runs)
  draft    local backend speculative draft, committed when the final transcript matches
  quick    Orca quick voice through a fake Orca CLI: settle, side admission, Sonnet answer
"""
import argparse
import json
import os
from pathlib import Path
import queue
import random
import re
import shutil
import statistics
import subprocess
import sys
import threading
import time
import wave

ROOT = Path(__file__).resolve().parents[1]
MODELS = Path(os.environ.get("MODEL_ROOT", "/missing")) / "by-name"

UTTERANCES = [
    "What's the capital of France?",
    "How are you doing today?",
    "Tell me a fun fact about dragons.",
    "What should I cook for dinner tonight?",
    "Can you recommend a good book?",
    "What is two plus two?",
    "What colour is the sky at sunset?",
    "Give me a word that rhymes with orange.",
]
WORK_UTTERANCES = [
    "Check the build for me.",
    "Fix the calendar rail.",
    "Can you look at the failing test?",
    "Merge the voice branch.",
    "Find the slow query in the logs.",
    "Update the docs for the new setting.",
]
TRAILING_UTTERANCES = [
    "I was thinking about dinner and",
    "Can you put the meeting on the",
    "My plan for tomorrow is to",
    "I want to check the build but",
]
WAKE = "Tohru, are you there?"

FAKE_PAREC = r'''#!/usr/bin/env python3
import json, os, sys, time, wave
from pathlib import Path
root = Path(os.environ["FAKE_AUDIO_ROOT"])
spool = root / "spool"
chunk = 512
silence = bytes(chunk * 2)
log = (root / "eou.jsonl").open("a", buffering=1)
next_at = time.monotonic()
pending = None
position = 0
while True:
    if pending is None:
        ready = sorted(spool.glob("*.wav"))
        if ready:
            path = ready[0]
            with wave.open(str(path)) as w:
                data = w.readframes(w.getnframes())
            name = path.stem
            path.unlink()
            samples = memoryview(data).cast("h")
            last = max((i for i in range(len(samples)) if abs(samples[i]) > 328), default=0)
            pending, position, logged = data, 0, False
    if pending is not None:
        block = pending[position * 2:(position + chunk) * 2]
        block = block + bytes(chunk * 2 - len(block))
    else:
        block = silence
    next_at += chunk / 16000
    delay = next_at - time.monotonic()
    if delay > 0:
        time.sleep(delay)
    try:
        sys.stdout.buffer.write(block)
        sys.stdout.buffer.flush()
    except BrokenPipeError:
        break
    if pending is not None:
        end = position + chunk
        if not logged and end > last:
            # The last audible sample was captured (end - last) samples before this block arrived.
            log.write(json.dumps({"utterance": name, "eou_ns": time.monotonic_ns() - int((end - last) / 16000 * 1e9)}) + "\n")
            logged = True
        position = end
        if position * 2 >= len(pending):
            pending = None
'''

FAKE_PACAT = r'''#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
root = Path(os.environ["FAKE_AUDIO_ROOT"])
rate = next((int(a.split("=", 1)[1]) for a in sys.argv if a.startswith("--rate=")), 24000)
start = time.monotonic_ns()
total = 0
onset = None
log = (root / "pcm.jsonl").open("a", buffering=1)
while True:
    data = os.read(0, 4096)
    if not data:
        break
    now = time.monotonic_ns()
    if onset is None:
        samples = memoryview(data[: len(data) // 2 * 2]).cast("h")
        loud = next((i for i in range(len(samples)) if abs(samples[i]) > 328), None)
        if loud is not None:
            onset = now + int(loud / rate * 1e9)
            log.write(json.dumps({"pid": os.getpid(), "start_ns": start, "onset_ns": onset}) + "\n")
    total += len(data)
'''

FAKE_PACTL = r'''#!/usr/bin/env python3
import json, sys
if "get-default-source" in sys.argv: print("test-source")
elif "get-default-sink" in sys.argv: print("test-sink")
else: print(json.dumps([dict(name="test-source", driver="test", properties={})]))
'''

FAKE_ORCA = r'''#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
args = sys.argv[1:]
root = Path(os.environ["FAKE_ORCA_ROOT"])
fixture = json.loads((root / "fixture.json").read_text())
delay = float(os.environ.get("FAKE_ORCA_DELAY_MS", "0")) / 1000
if delay: time.sleep(delay)
def reply(result):
    print(json.dumps({"ok": True, "result": result})); sys.exit(0)
if args[:2] == ["bot", "list"]:
    reply({"bots": [{"name": "Tohru", "paused": False}], "unreachable": []})
if args[:2] == ["bot", "voice-side"]:
    phase = args[args.index("--phase") + 1]
    if phase == "begin":
        reply({"route": "side", "chat": fixture["home"], "busy": False,
               "request": args[args.index("--request") + 1], "history": fixture["history"], "approvals": []})
    reply({"recorded": True})
if args[:3] == ["bot", "item", "list"]:
    reply({"items": fixture["items"]})
if args[:2] == ["orchestration", "worker-list"]:
    reply({"workers": fixture["workers"]})
if args[:2] == ["orchestration", "worker-show"]:
    reply({"dispatch": {"state": "working"}, "task": {"status": "in_progress"}})
if args[:2] == ["worktree", "list"]:
    reply({"worktrees": fixture["worktrees"], "scope": {"covered": ["local"]}})
reply({})
'''


def fixture():
    """A Home snapshot of roughly the size quick voice reads in production."""
    history = []
    topics = ["the calendar rail ink", "the lock screen styles", "Tomoko's voice", "the dragon flight path",
              "the launcher parity", "the sound panel tank", "voice latency research", "the polkit scene"]
    for n, topic in enumerate(topics * 3):
        history.append({"kind": "message", "role": "user", "interaction": {"kind": "owner"},
                        "text": f"[tohru-desktop:voice]\nHow is {topic} going? Anything you need from me on it?"})
        history.append({"kind": "message", "role": "assistant", "interaction": {"kind": "owner"},
                        "text": f"I'm still working on {topic}. The worker finished its first pass and I'm reviewing "
                                f"the result now; I'll tell you when it's ready to look at. Step {n} is done."})
    items = [{"title": f"{t.capitalize()}", "status": "in_progress",
              "links": [{"ref": f"dispatch:ctx_{i:04d}", "state": "working"}]} for i, t in enumerate(topics)]
    workers = [{"dispatchId": f"ctx_{i:04d}", "terminalState": "running", "task": t} for i, t in enumerate(topics)]
    worktrees = [{"branch": f"malinoskj2/{t.replace(' ', '-')}"} for t in topics]
    return {"home": "00000000-0000-4000-8000-000000000001", "history": history, "items": items,
            "workers": workers, "worktrees": worktrees}


def speak(items, native):
    """Kokoro reads the corpus: the streaming recogniser hears it reliably, unlike eSpeak."""
    pending = [(text, path) for text, path in items if not path.exists()]
    if not pending:
        return
    ops = "".join(json.dumps({"op": "synthesize", "text": text, "output": str(path.with_suffix(".raw.wav"))}) + "\n"
                  for text, path in pending)
    subprocess.run([native, "--evaluate", "--tts-model", str(MODELS / "voice-kokoro")], input=ops, text=True,
                   check=True, capture_output=True)
    for _, path in pending:
        raw = path.with_suffix(".raw.wav")
        subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(raw), "-af", "adelay=300",
                        "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", str(path)], check=True)
        raw.unlink()


def find_espeak():
    return shutil.which("espeak-ng") or str(next(Path("/nix/store").glob("*espeak-ng-*/bin/espeak-ng")))


def normalize(text):
    return " ".join(re.sub(r"[^a-z0-9' ]", " ", text.lower()).split())


ACKS = ["Mm, let me look.", "One moment.", "Let me check.", "On it.", "Give me a second.", "Let me see.",
        "Right, one moment.", "Okay, let me look into that."]
ACK_AFTER = 1.5


class Outbox:
    """One reply as the Luau controller sends it: an optional acknowledgement first, with the
    answer held until the acknowledgement finishes and then streamed."""

    def __init__(self, harness, turn):
        self.h, self.turn = harness, turn
        self.lock = threading.Lock()
        self.label = None
        self.acked = False
        self.ack_ended = threading.Event()
        self.started = False
        self.held = ""
        self.closed = False
        self.generation = harness.generation

    def send_start(self, **extra):
        self.h.send(cmd="speech_start", generation=self.generation, conversation="bench",
                    turn_id=self.turn["turn_id"], path=self.label, **extra)
        self.started = True

    def acknowledge(self, text=None, path="ack"):
        with self.lock:
            if self.closed or self.started or self.acked:
                return
            self.acked = True
            self.h.acks[self.generation] = self
            self.h.send(cmd="speech_start", generation=self.generation, conversation="bench",
                        turn_id=self.turn["turn_id"], path=path)
            self.h.send(cmd="speech_end", generation=self.generation, text=text or self.h.pick_ack())

    def ended(self):
        with self.lock:
            self.h.answer_after = time.monotonic_ns()
            self.ack_ended.set()
            if self.held and not self.started:
                self.h.generation += 1
                self.generation = self.h.generation
                self.send_start(background=True)
                self.h.send(cmd="speech_delta", generation=self.generation, text=self.held)
                self.held = ""

    def start(self, label):
        self.label = label

    def pending(self):
        return not self.started and not self.held

    def delta(self, text):
        with self.lock:
            if self.acked and not self.ack_ended.is_set():
                self.held += text
                return
            if not self.started:
                if self.acked:
                    self.h.generation += 1
                    self.generation = self.h.generation
                self.send_start(**({"background": True} if self.acked else {}))
            self.h.send(cmd="speech_delta", generation=self.generation, text=self.held + text)
            self.held = ""

    def end(self, text=None):
        if self.acked:
            self.ack_ended.wait(60)
        with self.lock:
            self.closed = True
            if not self.started:
                if self.acked:
                    self.h.generation += 1
                    self.generation = self.h.generation
                self.send_start(**({"background": True} if self.acked else {}))
            self.h.send(cmd="speech_end", generation=self.generation, text=(self.held + (text or "")) or None)
            self.held = ""
            return self.generation

    def say(self, text, label):
        self.label = label
        if self.acked:
            return self.end(text)
        with self.lock:
            self.closed = True
            self.h.send(cmd="say", text=text, conversation="bench", generation=self.generation,
                        turn_id=self.turn["turn_id"], path=label)
            return self.generation


class Harness:
    def __init__(self, args, out):
        self.args, self.out = args, out
        audio = out / "audio"
        if audio.exists():
            shutil.rmtree(audio)
        (audio / "spool").mkdir(parents=True)
        (audio / "bin").mkdir()
        for name, source in (("parec", FAKE_PAREC), ("pacat", FAKE_PACAT), ("pactl", FAKE_PACTL)):
            path = audio / "bin" / name
            path.write_text(source)
            path.chmod(0o755)
        self.audio = audio
        orca = out / "orca"
        (orca / "linux-orca-cli-shim").mkdir(parents=True, exist_ok=True)
        shim = orca / "linux-orca-cli-shim" / "orca"
        shim.write_text(FAKE_ORCA)
        shim.chmod(0o755)
        (orca / "fixture.json").write_text(json.dumps(fixture()))
        state = out / "assistant"
        if state.exists():
            shutil.rmtree(state)
        for sub in ("data", "scratch", "xdg-config", "xdg-data", "xdg-state"):
            (state / sub).mkdir(parents=True)
        self.helper_env = dict(os.environ, TOHRU_ASSISTANT_DATA=str(state / "data"),
                               TOHRU_ASSISTANT_SCRATCH=str(state / "scratch"),
                               XDG_CONFIG_HOME=str(state / "xdg-config"), XDG_DATA_HOME=str(state / "xdg-data"),
                               XDG_STATE_HOME=str(state / "xdg-state"), ORCA_USER_DATA_PATH=str(orca),
                               FAKE_ORCA_ROOT=str(orca), FAKE_ORCA_DELAY_MS=str(args.orca_ms))
        for key in [k for k in self.helper_env if k.startswith("ORCA_") and k != "ORCA_USER_DATA_PATH"]:
            del self.helper_env[key]
        if args.config:
            (state / "xdg-config" / "tohru").mkdir(parents=True)
            shutil.copy(args.config, state / "xdg-config" / "tohru" / "config.toml")
        self.events = queue.Queue()
        self.log = (out / "events.jsonl").open("w", buffering=1)
        self.conversation = None
        self.generation = 0
        self.helper = None
        self.draft = None
        self.lock = threading.Lock()
        self.acks = {}
        self.acked = []
        self.answer_after = None

    def start_voice(self):
        native = Path(self.args.native).resolve()
        wrapped = native.with_name(".tohru-voice-native-wrapped")
        argv = [str(native)]
        if wrapped.exists():
            text = native.read_text(errors="ignore")
            assets = re.search(r"--assets (\S+)", text)
            worker = re.search(r"--high-worker (\S+)", text)
            argv = [str(wrapped), "--assets", assets.group(1), "--high-worker", worker.group(1)]
        else:
            argv += ["--assets", str(ROOT / "shell/assistant/voice"),
                     "--high-worker", str(ROOT / "bin/tohru-voice-qwen3-worker")]
        argv += ["--vad-model", str(MODELS / "silero-vad/silero-vad-v6.2.3.onnx"),
                 "--asr-model", str(MODELS / "voice-asr"),
                 "--turn-model", str(MODELS / "voice-turn/smart-turn-v3.2-cpu.onnx"),
                 "--tts-model", str(MODELS / "voice-kokoro"), "--source", "test-source"]
        if self.args.profile == "tomoko":
            argv += ["--profiles", json.dumps({"model_root": str(MODELS.parent), "vram_threshold_percent": 95,
                                               "high": {"enabled": True, "python": self.args.python,
                                                        "tts_model": "voice-tomoko", "tts_speaker": "kuroki_tomoko"}})]
        espeak = Path(find_espeak()).parent
        env = dict(os.environ, PATH=f"{self.audio / 'bin'}:{espeak}:{os.environ['PATH']}", FAKE_AUDIO_ROOT=str(self.audio),
                   TOHRU_VOICE_ENV_READY="1", TOHRU_VOICE_LOG="info", TOHRU_VOICE_SOURCE="test-source",
                   TOHRU_VOICE_SINK="test-sink", TOHRU_VOICE_LOCK_FILE=str(self.out / "lock"),
                   ORT_DYLIB_PATH=self.args.ort_library)
        if self.args.profile == "kokoro":
            env.pop("GPU_LEASE_ID", None)
        self.stderr = (self.out / "voice-stderr.log").open("w")
        self.voice = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=self.stderr, text=True, bufsize=1)
        threading.Thread(target=self.read_voice, daemon=True).start()
        self.wait(lambda e: e.get("event") == "ready", 120)
        self.wait(lambda e: e.get("event") == "listening" and e.get("on"), 60)
        self.send(cmd="presence", visible=True)
        if self.args.ack:
            self.send(cmd="prepare", texts=ACKS)
        if self.args.profile == "tomoko":
            self.wait(lambda e: e.get("event") == "profile" and e.get("active") == "high", self.args.load_timeout)
            if self.args.ack:
                self.send(cmd="prepare", texts=ACKS)

    def read_voice(self):
        for line in self.voice.stdout:
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if event.get("event") not in ("mouth",):
                self.log.write(json.dumps({"t": time.monotonic_ns(), **event}) + "\n")
            self.events.put(event)
            if event.get("event") == "speaking" and not event.get("on"):
                ack = self.acks.pop(event.get("generation"), None)
                if ack:
                    ack.ended()
            if event.get("event") == "draft":
                self.on_draft(event)
            elif event.get("event") == "draft_cancel":
                self.cancel_draft()
            elif event.get("event") == "settled" and self.helper and self.helper.get("settle"):
                self.helper["settle"] = False
                try:
                    self.helper["process"].stdin.write("settled\n")
                    self.helper["process"].stdin.flush()
                except (BrokenPipeError, ValueError):
                    pass

    def wait(self, predicate, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                event = self.events.get(timeout=0.1)
            except queue.Empty:
                if self.voice.poll() is not None:
                    raise RuntimeError(f"voice exited {self.voice.returncode}")
                continue
            if predicate(event):
                return event
        raise TimeoutError("voice event deadline")

    def pick_ack(self):
        line = random.choice([a for a in ACKS if a not in self.acked])
        self.acked = [line, *self.acked][:3]
        return line

    def send(self, **message):
        self.voice.stdin.write(json.dumps(message) + "\n")
        self.voice.stdin.flush()

    def utter(self, name, wav):
        shutil.copy(wav, self.audio / "spool" / f"{name}.wav.tmp")
        (self.audio / "spool" / f"{name}.wav.tmp").rename(self.audio / "spool" / f"{name}.wav")

    def helper_args(self, *extra, voice=True):
        args = [str(ROOT / "bin/tohru-assistant"), "chat", "--provider", "claude"]
        if voice:
            args.append("--voice")
        args += ["--persona", "on", "--persona-file", str(ROOT / "shell/assistant/persona.md"),
                 "--memory", "off", "--orca", "false", "--discord-settings", "{}", *extra]
        return args

    def spawn_helper(self, argv, prompt, settle=False):
        process = subprocess.Popen(argv, env=self.helper_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=(self.out / "helper-stderr.log").open("a"), text=True, bufsize=1)
        if settle:
            process.stdin.write(json.dumps(prompt) + "\n")
            process.stdin.flush()
        else:
            process.stdin.write(prompt)
            process.stdin.close()
        return process

    def models(self):
        return ["--model", self.args.model, "--effort", self.args.effort]

    def on_draft(self, event):
        if self.args.path != "draft":
            return
        self.cancel_draft()
        argv = self.helper_args("--draft", *self.models(), "--scenario-model", self.args.draft_model,
                                "--scenario-effort", self.args.draft_effort,
                                *(["--conversation", self.conversation] if self.conversation else []))
        draft = {"prompt": event["text"], "text": "", "done": threading.Event(), "session": "", "failed": False,
                 "conversation": None}
        draft["process"] = self.spawn_helper(argv, event["text"])

        def read():
            for line in draft["process"].stdout:
                try:
                    item = json.loads(line)
                except ValueError:
                    continue
                kind = item.get("event")
                if kind == "start":
                    draft["conversation"] = item.get("conversation")
                elif kind == "delta":
                    draft["text"] += item.get("text", "")
                elif kind == "done":
                    draft["session"] = item.get("session", "")
                elif kind in ("needs_tools", "error", "draft_unsupported"):
                    draft["failed"] = True
            draft["process"].wait()
            draft["done"].set()
        threading.Thread(target=read, daemon=True).start()
        with self.lock:
            self.draft = draft

    def cancel_draft(self):
        with self.lock:
            draft, self.draft = self.draft, None
        if draft and draft["process"].poll() is None:
            draft["process"].send_signal(2)

    def reply(self, turn):
        self.answer_after = None
        self.generation += 1
        out = Outbox(self, turn)
        if self.args.ack:
            if turn.get("slow"):
                out.acknowledge()
            else:
                timer = threading.Timer(ACK_AFTER, out.acknowledge)
                timer.daemon = True
                timer.start()
        try:
            return self.answer(turn, out)
        finally:
            out.closed = True

    def answer(self, turn, out):
        prompt = turn["text"]
        if self.args.path == "stub":
            time.sleep(self.args.stub_ms / 1000)
            out.start("stub")
            first, _, rest = self.args.stub_text.partition(". ")
            out.delta(first + (". " if rest else ""))
            return out.end(rest or None)
        if self.args.path == "draft":
            with self.lock:
                draft, self.draft = self.draft, None
            if draft and normalize(draft["prompt"]) == normalize(prompt):
                draft["done"].wait(120)
                text = draft["text"]
                if not draft["failed"] and "<<needs-tools>>" not in text and text.strip():
                    commit = [str(ROOT / "bin/tohru-assistant"), "commit", "--provider", "claude", "--voice"]
                    cid = draft["conversation"] or self.conversation
                    if cid:
                        commit += ["--conversation", cid]
                    if draft["session"]:
                        commit += ["--session", draft["session"]]
                    result = subprocess.run(commit, env=self.helper_env, text=True, capture_output=True,
                                            input=json.dumps({"prompt": prompt, "answer": text}), timeout=60)
                    if result.returncode == 0:
                        try:
                            self.conversation = json.loads(result.stdout).get("conversation") or cid
                        except ValueError:
                            pass
                        return out.say(text, "draft")
            elif draft:
                draft["process"].send_signal(2)
        if self.args.path == "quick":
            argv = self.helper_args("--backend", "orca", "--orca-bot", "Tohru", "--orca-voice-chat", "quick",
                                    "--orca-offline", "error", "--await-settle", *self.models())
            settle = True
        else:
            argv = self.helper_args(*self.models(), *(["--conversation", self.conversation] if self.conversation else []))
            settle = False
        process = self.spawn_helper(argv, prompt, settle)
        self.helper = {"process": process, "settle": settle}
        label = "normal" if self.args.path in ("normal", "draft") else self.args.path
        generation = self.generation
        try:
            for line in process.stdout:
                try:
                    item = json.loads(line)
                except ValueError:
                    continue
                kind = item.get("event")
                if kind == "start":
                    label = item.get("path") or label
                    if self.args.path != "quick":
                        self.conversation = item.get("conversation") or self.conversation
                    out.start(label)
                elif kind == "voice_notice":
                    out.acknowledge(item.get("text", ""), "notice")
                elif kind == "delta":
                    out.delta(item.get("text", ""))
                elif kind in ("done", "error", "stopped"):
                    if kind == "error":
                        self.log.write(json.dumps({"helper_error": item.get("message", "")[:300]}) + "\n")
                    generation = out.end(item.get("spoken") if kind == "error" or out.pending() else None)
                    break
        finally:
            process.wait()
            self.helper = None
        return generation

    def run(self, wavs, wake_wav):
        self.start_voice()
        results = []
        try:
            awake_until = 0
            for n in range(self.args.turns):
                name, wav = wavs[n % len(wavs)]
                if time.monotonic() > awake_until:
                    self.utter(f"wake-{n:03d}", wake_wav)
                    woke = self.wait(lambda e: e.get("event") in ("wake", "turn"), 30)
                    if woke.get("event") == "turn":
                        self.send(cmd="thinking", on=False)
                    time.sleep(1.0)
                tag = f"{n:03d}-{name}"
                self.utter(tag, wav)
                turn = self.wait(lambda e: e.get("event") == "turn", 40)
                eou = None
                for line in (self.audio / "eou.jsonl").read_text().splitlines():
                    record = json.loads(line)
                    if record["utterance"] == tag:
                        eou = record["eou_ns"]
                turn_at = time.monotonic_ns()
                generation = self.reply(turn)
                try:
                    self.wait(lambda e: e.get("event") == "speaking" and not e.get("on")
                              and e.get("generation") == generation, 120)
                except TimeoutError:
                    pass
                onsets = [json.loads(l)["onset_ns"] for l in (self.audio / "pcm.jsonl").read_text().splitlines()] \
                    if (self.audio / "pcm.jsonl").exists() else []
                after = sorted(o for o in onsets if eou and o > eou)
                answer = [o for o in after if self.answer_after is None or o > self.answer_after]
                results.append({"turn": n, "utterance": name, "text": turn["text"], "eou_ns": eou,
                                "turn_ms": (turn_at - eou) / 1e6 if eou else None,
                                "first_audio_ms": (after[0] - eou) / 1e6 if after else None,
                                "answer_audio_ms": (answer[0] - eou) / 1e6 if answer else None,
                                "load": os.getloadavg()[0]})
                print(json.dumps(results[-1]), flush=True)
                awake_until = time.monotonic() + 45
                time.sleep(self.args.gap)
        finally:
            self.cancel_draft()
            if self.voice.poll() is None:
                self.voice.stdin.close()
                try:
                    self.voice.wait(20)
                except subprocess.TimeoutExpired:
                    self.voice.kill()
            self.stderr.close()
        return results


def percentile(values, p):
    values = sorted(values)
    if not values:
        return None
    rank = min(len(values), max(1, -(-len(values) * p // 100)))
    return values[int(rank) - 1]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--native", required=True, help="tohru-voice-native (Nix wrapper or binary)")
    parser.add_argument("--ort-library", required=True)
    parser.add_argument("--path", choices=("stub", "normal", "draft", "quick"), required=True)
    parser.add_argument("--profile", choices=("kokoro", "tomoko"), default="kokoro")
    parser.add_argument("--python", default="", help="assistant.voice_profiles.high.python for tomoko")
    parser.add_argument("--turns", type=int, default=8)
    parser.add_argument("--gap", type=float, default=2.0)
    parser.add_argument("--stub-ms", type=float, default=0)
    parser.add_argument("--stub-text", default="Sure, here is a short answer. It has two sentences.")
    parser.add_argument("--orca-ms", type=float, default=0, help="Delay added to every fake Orca call")
    parser.add_argument("--model", default="claude-opus-5-5")
    parser.add_argument("--effort", default="medium")
    parser.add_argument("--draft-model", default="claude-sonnet-5-5")
    parser.add_argument("--draft-effort", default="low")
    parser.add_argument("--config", help="config.toml for the isolated helper")
    parser.add_argument("--corpus", choices=("chat", "work", "trailing"), default="chat")
    parser.add_argument("--ack", action="store_true", help="Acknowledge slow turns as the controller does")
    parser.add_argument("--load-timeout", type=float, default=600)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    out = Path(args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    corpus = out / "corpus"
    corpus.mkdir(exist_ok=True)
    texts = {"work": WORK_UTTERANCES, "trailing": TRAILING_UTTERANCES}.get(args.corpus, UTTERANCES)
    wavs = [(f"u{n}", corpus / f"u{n}.wav") for n in range(len(texts))]
    wake = corpus / "wake.wav"
    speak([*zip(texts, (p for _, p in wavs)), (WAKE, wake)], str(Path(args.native).resolve()))
    results = Harness(args, out).run(wavs, wake)
    measured = [r["first_audio_ms"] for r in results if r["first_audio_ms"] is not None]
    turns = [r["turn_ms"] for r in results if r["turn_ms"] is not None]
    answers = [r["answer_audio_ms"] for r in results if r["answer_audio_ms"] is not None]
    report = subprocess.run([str(Path(args.native).resolve()), "--first-audio-report"],
                            stdin=(out / "voice-stderr.log").open(), capture_output=True, text=True)
    try:
        internal = json.loads(report.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        internal = {"error": report.stderr[-500:]}
    summary = {"path": args.path, "ack": args.ack, "profile": args.profile, "turns": len(results), "silent": len(results) - len(measured),
               "first_audio_ms": {"p50": percentile(measured, 50), "p95": percentile(measured, 95),
                                  "mean": statistics.fmean(measured) if measured else None},
               "answer_audio_ms": {"p50": percentile(answers, 50), "p95": percentile(answers, 95)},
               "eou_to_turn_ms": {"p50": percentile(turns, 50), "p95": percentile(turns, 95)},
               "load": statistics.fmean(r["load"] for r in results) if results else None,
               "service_report": internal, "results": results}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: summary[k] for k in ("path", "ack", "profile", "turns", "silent", "first_audio_ms", "answer_audio_ms", "eou_to_turn_ms", "load")}))


if __name__ == "__main__":
    main()
