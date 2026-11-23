#!/usr/bin/env python3
"""Compare actual VAD/ASR/turn decisions on identical synthetic WAV fixtures."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import wave
import numpy as np
import onnxruntime as ort
import sherpa_onnx

ROOT=Path(__file__).resolve().parents[1]
MODEL=Path(os.environ["MODEL_ROOT"])/"by-name"


def main(args):
    if not os.environ.get("GPU_LEASE_ID"): raise SystemExit("run inside gpu-run")
    out=Path(args.output).resolve();out.mkdir(parents=True,exist_ok=True)
    fixture=runpy.run_path(str(ROOT/"tests/assistant/voice.py"))
    v=fixture["voice_module"]
    vad=v.Silero(MODEL/"silero-vad/silero-vad-v6.2.3.onnx",np,ort)
    asr=v.Recognizer(MODEL/"voice-asr",sherpa_onnx)
    turn=v.SmartTurn(MODEL/"voice-turn/smart-turn-v3.2-cpu.onnx",np,ort)
    rows=[]
    def meaningful(events):
        return [{k:value for k,value in event.items() if k not in ("turn_id","token")} for event in events if event["event"] not in ("ready","replay_done")]
    for path in sorted(Path(args.fixtures).resolve().glob("input-*.wav")):
        events=[];v.emit=lambda event,**fields:events.append(dict(event=event,**fields));v.diagnostic=lambda *a,**k:None
        voice=fixture["make_voice"](asr);voice.np=np;voice.turn=turn;voice.silero=vad;vad.reset()
        with wave.open(str(path)) as wav: audio=np.frombuffer(wav.readframes(wav.getnframes()),dtype="<i2").astype(np.float32)/32768
        for start in range(0,len(audio),512): voice.step(np.pad(audio[start:start+512],(0,max(0,512-len(audio[start:start+512])))))
        for _ in range(200):voice.step(np.zeros(512,dtype=np.float32))
        command=[args.native,"--replay",str(path),"--vad-model",str(MODEL/"silero-vad/silero-vad-v6.2.3.onnx"),"--asr-model",str(MODEL/"voice-asr"),"--turn-model",str(MODEL/"voice-turn/smart-turn-v3.2-cpu.onnx"),"--tts-engine","espeak-ng","--tts-load-on-conversation"]
        with (out/(path.stem+".stderr.log")).open("w") as log:
            result=subprocess.run(command,cwd=ROOT,stdout=subprocess.PIPE,stderr=log,text=True,timeout=60)
        native=[json.loads(line) for line in result.stdout.splitlines()]
        a,b=meaningful(events),meaningful(native)
        rows.append(dict(input=str(path),same=a==b and result.returncode==0,python=a,native=b))
        print(path.name, "same" if rows[-1]["same"] else "DIFFERENT",flush=True)
    (out/"replay.json").write_text(json.dumps(dict(kind="real models; synthetic audio; no physical microphone",rows=rows),indent=2)+"\n")
    return any(not row["same"] for row in rows)


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--native",required=True);parser.add_argument("--fixtures",default=".tmp/docs/rust-voice/comparison");parser.add_argument("--output",default=".tmp/docs/rust-voice/replay");raise SystemExit(main(parser.parse_args()))
