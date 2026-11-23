#!/usr/bin/env python3
"""Interleaved steady-state synthesis observations, with waveform equality checks."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import statistics
import sys
import numpy as np

spec=importlib.util.spec_from_file_location("voice_compare",Path(__file__).with_name("voice-native-compare.py"))
compare=importlib.util.module_from_spec(spec);spec.loader.exec_module(compare)


def main(args):
    if not os.environ.get("GPU_LEASE_ID"):raise SystemExit("run inside gpu-run")
    out=Path(args.output).resolve();out.mkdir(parents=True,exist_ok=True)
    native=compare.Worker([args.native,"--evaluate","--tts-model",str(compare.MODEL/"voice-kokoro")],out/"native.stderr.log")
    python=compare.Worker([sys.executable,str(Path(__file__).with_name("voice-native-compare.py")),"--python-worker"],out/"python.stderr.log")
    workers=dict(native=native,python=python);rows=[]
    try:
        for trial in range(6):
            results={}
            for side in (("python","native") if trial%2==0 else ("native","python")):
                request=dict(op="synthesize",text=compare.TEXTS[0],output=str(out/(side+".wav" if side=="native" else side+".f32")))
                if side=="native":request["raw_output"]=str(out/"native.f32")
                result=workers[side].call(request);assert result["ok"],result;results[side]=result
            a=np.fromfile(out/"python.f32",dtype="<f4");b=np.fromfile(out/"native.f32",dtype="<f4")
            assert len(a)==len(b) and len(a)>0 and np.array_equal(a,b),"steady-state waveform difference"
            rows.append(dict(trial=trial,warmup=trial==0,**results))
        summary={side:dict(median=statistics.median(r[side]["elapsed_ms"] for r in rows[1:]),minimum=min(r[side]["elapsed_ms"] for r in rows[1:]),maximum=max(r[side]["elapsed_ms"] for r in rows[1:])) for side in workers}
        report=dict(verdict="diagnostic component timings; user-perceived speed remains inconclusive without a limiter profile and physical audio",repetitions_per_side=5,failures=0,all_waveforms_bit_identical=True,scope="text frontend, inference and output file write; native additionally writes a WAV header and a raw comparison copy",summary_ms=summary,rows=rows)
        (out/"warm.json").write_text(json.dumps(report,indent=2)+"\n");print(json.dumps({k:v for k,v in report.items() if k!="rows"},indent=2))
    finally:native.close();python.close()


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--native",required=True);parser.add_argument("--output",default=".tmp/docs/rust-voice/warm");main(parser.parse_args())
