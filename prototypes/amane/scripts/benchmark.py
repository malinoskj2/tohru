#!/usr/bin/env python3
"""Release process + isolated compositor accounting, with five-second warm-up."""
import argparse, json, os, pathlib, statistics, subprocess, time
parser=argparse.ArgumentParser()
parser.add_argument('--compositor-pid',type=int,required=True)
args=parser.parse_args()
root=pathlib.Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
out=root/'.tmp'
hz=os.sysconf('SC_CLK_TCK')

def sample(pid):
    proc=pathlib.Path('/proc')/str(pid)
    fields=(proc/'stat').read_text().split(') ',1)[1].split()
    memory={}
    for line in (proc/'smaps_rollup').read_text().splitlines()[1:]:
        if ':' in line:
            k,v=line.split(':',1)
            memory[k]=int(v.split()[0])
    switches={'voluntary_ctxt_switches':0,'nonvoluntary_ctxt_switches':0}
    for thread in (proc/'task').iterdir():
        try:
            for line in (thread/'status').read_text().splitlines():
                k,_,v=line.partition(':')
                if k in switches: switches[k]+=int(v)
        except FileNotFoundError: pass
    return dict(pid=pid,ticks=int(fields[11])+int(fields[12]),rss_kib=memory['Rss'],
                pss_kib=memory['Pss'],threads=int(fields[17]),**switches)

reports=[]
for mode in ('idle','active'):
    frames=out/f'{mode}-views.ns'
    env={**os.environ,'AMANE_FRAME_LOG':str(frames)}
    with (out/f'benchmark-{mode}.log').open('w') as log:
        app=subprocess.Popen([root/'prototypes/amane/target/release/workspace-amane','--mode',mode],env=env,stdout=log,stderr=log)
        try:
            time.sleep(5)
            raw=[]
            for i in range(16):
                raw.append(dict(monotonic=time.monotonic(),epoch_ns=time.time_ns(),app=sample(app.pid),compositor=sample(args.compositor_pid)))
                if i<15: time.sleep(1)
            first,last=raw[0],raw[-1]
            elapsed=last['monotonic']-first['monotonic']
            result=dict(mode=mode,elapsed_seconds=elapsed,clock_ticks_per_second=hz)
            for who in ('app','compositor'):
                a,b=first[who],last[who]
                result[who]=dict(cpu_one_core_percent=(b['ticks']-a['ticks'])/hz/elapsed*100,
                    rss_kib=b['rss_kib'],pss_kib=b['pss_kib'],threads=b['threads'],
                    voluntary_switches=b['voluntary_ctxt_switches']-a['voluntary_ctxt_switches'],
                    involuntary_switches=b['nonvoluntary_ctxt_switches']-a['nonvoluntary_ctxt_switches'])
            # Entries are view invocation offsets from the first view; measure middle 15 seconds.
            stamps=[int(v)/1e9 for v in frames.read_text().splitlines()]
            selected=[v for v in stamps if 5<=v<=20]
            intervals=[b-a for a,b in zip(selected,selected[1:])]
            result['view_invocations_5_to_20_seconds']=len(selected)
            if intervals:
                result['view_interval_ms_median']=statistics.median(intervals)*1000
                result['view_interval_ms_p95']=sorted(intervals)[int(.95*(len(intervals)-1))]*1000
            (out/f'benchmark-{mode}-raw.json').write_text(json.dumps(raw,indent=2))
            reports.append(result)
        finally:
            app.terminate(); app.wait()
(out/'benchmark-summary.json').write_text(json.dumps(reports,indent=2))
print(json.dumps(reports,indent=2))
