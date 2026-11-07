#!/usr/bin/env python3
"""Measure all native process threads, plus the isolated compositor separately."""
import argparse
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import time

parser=argparse.ArgumentParser()
parser.add_argument('mode',choices=['idle','active'])
parser.add_argument('--seconds',type=float,default=16)
parser.add_argument('--warmup',type=float,default=5)
parser.add_argument('--compositor-pid',type=int)
args=parser.parse_args()
root=Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
folder=root/'.tmp';folder.mkdir(exist_ok=True)
base=Path(__file__).resolve().parent

def sample(pid):
    proc=Path('/proc')/str(pid)
    stat=(proc/'stat').read_text().split(') ',1)[1].split()
    memory={}
    for line in (proc/'smaps_rollup').read_text().splitlines():
        if ':' in line:
            k,v=line.split(':',1)
            if v.strip().split()[0].isdigit():memory[k]=int(v.strip().split()[0])
    switches={'voluntary_ctxt_switches':0,'nonvoluntary_ctxt_switches':0}
    for task in (proc/'task').iterdir():
        try:
            for line in (task/'status').read_text().splitlines():
                k,_,v=line.partition(':')
                if k in switches:switches[k]+=int(v)
        except FileNotFoundError:pass
    return dict(pid=pid,wall=time.monotonic(),ticks=int(stat[11])+int(stat[12]),rss_kib=memory.get('Rss'),pss_kib=memory.get('Pss'),threads=len(list((proc/'task').iterdir())),**switches)

with (folder/f'benchmark-{args.mode}.log').open('w') as log:
    app=subprocess.Popen([str(base/'run.sh'),args.mode,'--seconds',str(args.seconds+args.warmup+3),'--record','v1'],stdout=log,stderr=subprocess.STDOUT)
    time.sleep(args.warmup)
    pids=[app.pid]+([args.compositor_pid] if args.compositor_pid else [])
    before=[sample(p) for p in pids]
    trace_before=(folder/f'benchmark-{args.mode}.log').read_text()
    time.sleep(args.seconds)
    after=[sample(p) for p in pids]
    trace_after=(folder/f'benchmark-{args.mode}.log').read_text()
    result=dict(mode=args.mode,warmup_s=args.warmup,requested_s=args.seconds,clock_ticks=os.sysconf('SC_CLK_TCK'),before=before,after=after,processes=[])
    for a,b in zip(before,after):
        elapsed=b['wall']-a['wall']
        result['processes'].append(dict(pid=a['pid'],elapsed_s=elapsed,cpu_one_core_percent=(b['ticks']-a['ticks'])/os.sysconf('SC_CLK_TCK')/elapsed*100,rss_kib=b['rss_kib'],pss_kib=b['pss_kib'],voluntary_switches=b['voluntary_ctxt_switches']-a['voluntary_ctxt_switches'],involuntary_switches=b['nonvoluntary_ctxt_switches']-a['nonvoluntary_ctxt_switches'],threads=b['threads']))
    app.wait()
rows=[]
for line in trace_after[len(trace_before):].splitlines():
    if re.match(r'^\d+\.\d+\t',line):rows.append(float(line.split('\t')[0]))
window=rows
deltas=[b-a for a,b in zip(window,window[1:])]
result['render_loop_trace']={'count':len(window),'median_interval_ms':statistics.median(deltas) if deltas else None,'p95_interval_ms':sorted(deltas)[min(len(deltas)-1,int(len(deltas)*0.95))] if deltas else None,'max_interval_ms':max(deltas) if deltas else None,'note':'render-loop records, not presentation feedback or input-to-photon latency'}
(folder/f'benchmark-{args.mode}.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
