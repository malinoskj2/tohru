"""Linux /proc sampler retained from prototype history; CPU/memory only.

Source: 89fee46efc9ade2313e67c8ebb3ff016149e6f51:prototypes/benchmark_gpu.py
The sample function is byte-identical to that source, extracted without changes.
"""
from pathlib import Path
import time


def sample(pid):
    proc = Path('/proc') / str(pid)
    fields = (proc / 'stat').read_text().split(') ', 1)[1].split()
    memory = {}
    for line in (proc / 'smaps_rollup').read_text().splitlines()[1:]:
        key, _, value = line.partition(':')
        if value.strip():
            memory[key] = int(value.split()[0])
    switches = dict(voluntary_ctxt_switches=0, nonvoluntary_ctxt_switches=0)
    for thread in (proc / 'task').iterdir():
        try:
            for line in (thread / 'status').read_text().splitlines():
                key, _, value = line.partition(':')
                if key in switches:
                    switches[key] += int(value)
        except FileNotFoundError:
            pass
    return dict(wall=time.monotonic(), ticks=int(fields[11]) + int(fields[12]),
                rss_kib=memory['Rss'], pss_kib=memory['Pss'], **switches)
