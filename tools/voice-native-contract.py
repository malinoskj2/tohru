#!/usr/bin/env python3
"""Exercise the native voice service and real models through fake audio utilities.

This proves process/protocol behavior and submitted PCM, not physical speaker output.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import queue
import signal
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
MODEL = Path(os.environ.get("MODEL_ROOT", "/missing")) / "by-name"
FAKE = r'''#!/usr/bin/env python3
import json,os,sys,time
from pathlib import Path
name=Path(sys.argv[0]).name
root=Path(os.environ["FAKE_AUDIO_ROOT"])
with (root/"children.jsonl").open("a") as f: f.write(json.dumps(dict(pid=os.getpid(),name=name,argv=sys.argv[1:]))+"\n")
if name=="pactl":
 if "get-default-source" in sys.argv: print("test-source")
 else: print(json.dumps([dict(name="test-source",driver="test",properties={})]))
elif name=="parec":
 while True:
  try: sys.stdout.buffer.write(bytes(1024));sys.stdout.buffer.flush()
  except BrokenPipeError: break
  time.sleep(.032)
elif name=="pacat":
 with (root/"submitted.pcm").open("ab",buffering=0) as f:
  while True:
   data=os.read(0,1600)
   if not data: break
   f.write(data)
'''


class Helper:
    def __init__(self, argv, directory, environment):
        self.directory = directory
        directory.mkdir(parents=True, exist_ok=True)
        for artifact in ("submitted.pcm","complete.pcm","children.jsonl","events.json"):
            (directory/artifact).unlink(missing_ok=True)
        fake = directory / "bin"
        fake.mkdir(exist_ok=True)
        for name in ("pactl", "parec", "pacat"):
            path = fake / name
            path.write_text(FAKE)
            path.chmod(0o755)
        env = dict(os.environ, **environment, PATH=str(fake) + ":" + os.environ["PATH"],
                   FAKE_AUDIO_ROOT=str(directory), TOHRU_VOICE_ENV_READY="1", TOHRU_VOICE_LOG="info",
                   TOHRU_VOICE_SOURCE="test-source", TOHRU_VOICE_LOCK_FILE=str(directory / "lock"))
        self.stderr = (directory / "stderr.log").open("w")
        self.start = time.monotonic()
        self.process = subprocess.Popen(argv, cwd=ROOT, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, text=True, bufsize=1)
        self.queue = queue.Queue()
        self.events = []
        def read():
            for line in self.process.stdout:
                try: self.queue.put(json.loads(line))
                except ValueError: self.queue.put(dict(event="invalid_json", line=line))
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        try:
            self.wait(lambda e: e.get("event") == "ready")
            self.startup_ms = (time.monotonic() - self.start) * 1000
            self.wait(lambda e: e.get("event") == "listening" and e.get("on"))
        except BaseException:
            self.close()
            raise

    def wait(self, predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try: event = self.queue.get(timeout=min(0.1, max(0, deadline-time.monotonic())))
            except queue.Empty:
                if self.process.poll() is not None: raise RuntimeError(f"helper exited {self.process.returncode}")
                continue
            self.events.append(event)
            if event.get("event") in ("error", "invalid_json"): raise AssertionError(event)
            if predicate(event): return event
        raise TimeoutError("helper event deadline")

    def send(self, **message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try: self.process.wait(10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
                raise AssertionError("helper did not shut down on EOF")
        self.reader.join(2)
        self.stderr.close()
        (self.directory / "events.json").write_text(json.dumps(self.events, indent=2))


def pss_tree(pid):
    pids = [pid]
    total = 0
    for ident in pids:
        try:
            children = Path(f"/proc/{ident}/task/{ident}/children").read_text().split()
            pids.extend(map(int, children))
            for line in Path(f"/proc/{ident}/smaps_rollup").read_text().splitlines():
                if line.startswith("Pss:"): total += int(line.split()[1])
        except (FileNotFoundError, PermissionError): pass
    return dict(pss_kib=total, processes=len(pids))


def exercise(argv, directory, environment, check_extended=False):
    helper = Helper(argv, directory, environment)
    try:
        if check_extended:
            helper.send(cmd="diagnostic",stage="assistant_origin",request="local:request",canonical_request="orca:canonical",generation=9)
            helper.send(cmd="diagnostic",stage="task_update",task="task:fixture",sequence=7,kind="progress",request="r"*700,canonical_request="orca:canonical",reason="matched")
            helper.send(cmd="diagnostic",stage="task_update_dropped",task="PRIVATE secret",sequence=2147483647,kind="PRIVATE",reason="PRIVATE",text="PRIVATE")
            helper.send(cmd="diagnostic",stage="waiting_config",enabled=False)
            helper.send(cmd="task_accepted",request="orca:hidden-accepted",generation=9)
        helper.send(cmd="speech_start", generation=9, conversation="isolated",**(dict(background=True,request="orca:canonical") if check_extended else {}))
        helper.send(cmd="speech_delta", generation=8, text="THIS STALE TEXT MUST NOT PLAY. ")
        first = time.monotonic()
        helper.send(cmd="speech_end", generation=9, text="Hello, I'm here. What do you need?")
        sentence = helper.wait(lambda e: e.get("event") == "sentence")
        first_sentence_ms = (time.monotonic()-first)*1000
        process_to_first_sentence_ms = (time.monotonic()-helper.start)*1000
        assert "STALE" not in sentence["text"]
        completed=helper.wait(lambda e: e.get("event") == "speaking" and not e.get("on"))
        assert completed.get("generation")==9,"completion lost the controller generation"
        pcm = directory / "submitted.pcm"
        complete = pcm.read_bytes()
        assert len(complete)>2400 and len(complete)%2==0
        (directory / "complete.pcm").write_bytes(complete)
        memory = pss_tree(helper.process.pid)
        with (directory/"lock").open("w") as lock_file:
            fcntl.flock(lock_file,fcntl.LOCK_EX)
            helper.wait(lambda e:e.get("event")=="paused" and e.get("reason")=="locked",timeout=6)
            size=pcm.stat().st_size
            helper.send(cmd="speech_start",generation=99,conversation="isolated")
            helper.send(cmd="speech_end",generation=99,text="Must stay silent while locked.")
            time.sleep(.15)
            assert pcm.stat().st_size==size,"speech submitted while locked"
            fcntl.flock(lock_file,fcntl.LOCK_UN)
            helper.wait(lambda e:e.get("event")=="paused" and e.get("reason")=="",timeout=6)
            helper.wait(lambda e:e.get("event")=="listening" and e.get("on"),timeout=6)
        helper.send(cmd="speech_start", generation=10, conversation="isolated")
        helper.send(cmd="speech_end", generation=10, text="This is a longer answer with several words to interrupt. The second sentence must be cancelled too.")
        helper.wait(lambda e: e.get("event") == "sentence")
        cancel = time.monotonic()
        helper.send(cmd="hush")
        interrupted=helper.wait(lambda e: e.get("event") == "speaking" and not e.get("on"))
        assert interrupted.get("generation")==10,"interruption lost the controller generation"
        cancel_event_ms = (time.monotonic()-cancel)*1000
        time.sleep(.15)
        size = pcm.stat().st_size
        helper.send(cmd="speech_delta", generation=10, text="LATE STALE TEXT. ")
        helper.send(cmd="speech_end", generation=10)
        time.sleep(.2)
        assert pcm.stat().st_size==size, "stale audio written after cancellation"
        helper.send(cmd="end")
        helper.wait(lambda e: e.get("event") == "conversation" and not e.get("on"))
        helper.close()
        assert helper.process.returncode==0
        diagnostics=[json.loads(line.split("tohru-voice: ",1)[1]) for line in (directory/"stderr.log").read_text().splitlines() if line.startswith("tohru-voice: {")]
        assert any(event.get("stage")=="synthesis_ready" and event.get("generation")==9 for event in diagnostics),"synthesis lost the controller generation"
        if check_extended:
            by_stage={event["stage"]:event for event in diagnostics}
            assert by_stage["assistant_origin"]["canonical_request"]=="orca:canonical"
            assert any(event.get("stage")=="speech_stream_start" and event.get("background") is True and event.get("request")=="orca:canonical" for event in diagnostics)
            assert by_stage["task_update"]["sequence"]==7 and by_stage["task_update"]["kind"]=="progress"
            assert len(by_stage["task_update"]["request"])==512
            assert "task" not in by_stage["task_update_dropped"] and "sequence" not in by_stage["task_update_dropped"]
            assert by_stage["waiting_config"]["enabled"] is False
            assert by_stage["foreground_released"]["request"]=="orca:hidden-accepted"
            assert by_stage["foreground_released"]["generation"]==9
            assert "PRIVATE" not in json.dumps(diagnostics)
        return dict(startup_ready_ms=helper.startup_ms, first_sentence_ms=first_sentence_ms,process_to_first_sentence_ms=process_to_first_sentence_ms,
                    cancel_event_ms=cancel_event_ms, complete_bytes=len(complete), memory=memory,
                    result="passed", audible_proof=False)
    finally:
        if helper.process.poll() is None: helper.close()


def main(args):
    if not os.environ.get("GPU_LEASE_ID"): raise SystemExit("Run inside gpu-run")
    out=Path(args.output).resolve()
    out.mkdir(parents=True,exist_ok=True)
    native=Path(args.native).resolve()
    # Bypass only Nix's PATH wrapper so the disposable utilities take priority.
    # The ELF and native runtime are the same packaged artifacts being tested.
    wrapped=native.with_name(".tohru-voice-native-wrapped")
    if wrapped.exists(): native=wrapped
    common=["--tts-engine",args.engine,"--vad-model",str(MODEL/"silero-vad/silero-vad-v6.2.3.onnx"),"--asr-model",str(MODEL/"voice-asr"),
            "--turn-model",str(MODEL/"voice-turn/smart-turn-v3.2-cpu.onnx"),"--tts-model",str(MODEL/"voice-kokoro")]
    results={}
    results["native"]=exercise([str(native),*common,"--assets",str(ROOT/"shell/assistant/voice")],out/"native",dict(ORT_DYLIB_PATH=args.ort_library),args.new_contracts)
    results["classification"]="protocol and cancellation assertions passed; timings are one-run diagnostic observations"
    (out/"contract.json").write_text(json.dumps(results,indent=2)+"\n")
    print(json.dumps(results,indent=2))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native",required=True)
    parser.add_argument("--ort-library",required=True)
    parser.add_argument("--engine",choices=("kokoro","espeak-ng"),default="kokoro")
    parser.add_argument("--new-contracts",action="store_true",help="Verify admitted background speech and bounded task diagnostics")
    parser.add_argument("--output",default=".tmp/docs/rust-voice/contract")
    main(parser.parse_args())
