#!/usr/bin/env python3
"""Repeated mock-device observations; provider/network/acoustic latency are excluded.

The adoption verdict remains inconclusive without profiling and physical end-to-end
measurement. This captures medians/ranges instead of treating a single run as a win.
"""
import argparse
import json
import os
from pathlib import Path
import statistics
import importlib.util
import sys

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("voice_contract",Path(__file__).with_name("voice-native-contract.py"))
contract=importlib.util.module_from_spec(spec);spec.loader.exec_module(contract)


def main(args):
    if not os.environ.get("GPU_LEASE_ID"):raise SystemExit("run inside gpu-run")
    if args.repetitions<5:raise SystemExit("comparisons require at least five interleaved trials")
    out=Path(args.output).resolve();out.mkdir(parents=True,exist_ok=True)
    native=Path(args.native).resolve();wrapped=native.with_name(".tohru-voice-native-wrapped")
    if wrapped.exists():native=wrapped
    common=["--vad-model",str(contract.MODEL/"silero-vad/silero-vad-v6.2.3.onnx"),"--asr-model",str(contract.MODEL/"voice-asr"),"--turn-model",str(contract.MODEL/"voice-turn/smart-turn-v3.2-cpu.onnx"),"--tts-model",str(contract.MODEL/"voice-kokoro")]
    commands=dict(python=[sys.executable,str(ROOT/"bin/tohru-voice"),*common],native=[str(native),*common,"--assets",str(ROOT/"shell/assistant/voice")])
    runs=[]
    for trial in range(args.repetitions):
        for side in (("python","native") if trial%2==0 else ("native","python")):
            directory=out/f"trial-{trial}"/side
            result=contract.exercise(commands[side],directory,{} if side=="python" else dict(ORT_DYLIB_PATH=args.ort_library))
            runs.append(dict(trial=trial,side=side,**result));print(trial,side,"passed",flush=True)
            (out/"runs.json").write_text(json.dumps(runs,indent=2)+"\n")
        assert (out/f"trial-{trial}/python/complete.pcm").read_bytes()==(out/f"trial-{trial}/native/complete.pcm").read_bytes()
    summary={}
    for side in ("python","native"):
        rows=[r for r in runs if r["side"]==side];summary[side]={}
        for key in ("startup_ready_ms","first_sentence_ms","process_to_first_sentence_ms","cancel_event_ms"):
            values=[r[key] for r in rows];summary[side][key]=dict(median=statistics.median(values),minimum=min(values),maximum=max(values))
        values=[r["memory"]["pss_kib"] for r in rows];summary[side]["tree_pss_kib"]=dict(median=statistics.median(values),minimum=min(values),maximum=max(values))
    report=dict(verdict="inconclusive for user-perceived performance; no physical audio or provider and no independent limiter profile",setup="release/Nix build, same model files/threads, process starts with warm filesystem cache, alternating side order",failures=0,completed_runs=len(runs),summary=summary,runs=runs)
    (out/"benchmark.json").write_text(json.dumps(report,indent=2)+"\n");print(json.dumps(dict(verdict=report["verdict"],completed_runs=len(runs),summary=summary),indent=2))


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--native",required=True);parser.add_argument("--ort-library",required=True);parser.add_argument("--repetitions",type=int,default=5);parser.add_argument("--output",default=".tmp/docs/rust-voice/benchmark");main(parser.parse_args())
