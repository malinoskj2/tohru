#!/usr/bin/env python3
"""Sequential, alternating native-process benchmarks; require a hardware adapter."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--amane', type=Path, required=True)
    parser.add_argument('--pleamar', type=Path, required=True)
    parser.add_argument('--compositor-pid', type=int, required=True)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--warmup', type=float, default=5)
    parser.add_argument('--seconds', type=float, default=15)
    args = parser.parse_args()
    root = Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], text=True).strip())
    out = root / '.tmp/gpu-benchmark'
    out.mkdir(parents=True, exist_ok=True)
    hz = os.sysconf('SC_CLK_TCK')
    results = []
    for round_index in range(args.rounds):
        order = ('amane', 'pleamar') if round_index % 2 == 0 else ('pleamar', 'amane')
        for mode in ('idle', 'active'):
            for name in order:
                label = f'{round_index + 1}-{mode}-{name}'
                path = out / f'{label}.log'
                command = ([str(args.amane), '--mode', mode] if name == 'amane'
                           else [str(args.pleamar), mode])
                with path.open('w') as log:
                    app = subprocess.Popen(command, stdout=log, stderr=log)
                    try:
                        time.sleep(args.warmup)
                        if app.poll() is not None:
                            raise RuntimeError(f'{label} exited during warm-up: {path.read_text()}')
                        text = path.read_text()
                        if 'NVIDIA GeForce RTX 5090' not in text or 'llvmpipe' in text:
                            raise RuntimeError(f'{label} did not confirm RTX 5090: {text}')
                        pids = {'app': app.pid, 'compositor': args.compositor_pid}
                        before = {key: sample(pid) for key, pid in pids.items()}
                        time.sleep(args.seconds)
                        if app.poll() is not None:
                            raise RuntimeError(f'{label} exited during measurement')
                        after = {key: sample(pid) for key, pid in pids.items()}
                        result = dict(round=round_index + 1, mode=mode, framework=name,
                                      pid=app.pid, before=before, after=after)
                        for key in pids:
                            a, b = before[key], after[key]
                            elapsed = b['wall'] - a['wall']
                            result[key] = dict(elapsed_seconds=elapsed,
                                cpu_one_core_percent=(b['ticks'] - a['ticks']) / hz / elapsed * 100,
                                rss_mib=b['rss_kib'] / 1024, pss_mib=b['pss_kib'] / 1024,
                                voluntary_switches=b['voluntary_ctxt_switches'] - a['voluntary_ctxt_switches'],
                                involuntary_switches=b['nonvoluntary_ctxt_switches'] - a['nonvoluntary_ctxt_switches'])
                        results.append(result)
                        (out / 'raw.json').write_text(json.dumps(results, indent=2) + '\n')
                        print(label, json.dumps(result['app']), flush=True)
                    finally:
                        if app.poll() is None:
                            app.terminate()
                        try:
                            app.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            app.kill()
                            app.wait()
                time.sleep(1)
    summary = []
    for name in ('amane', 'pleamar'):
        for mode in ('idle', 'active'):
            rows = [r for r in results if r['framework'] == name and r['mode'] == mode]
            summary.append(dict(framework=name, mode=mode, runs=len(rows),
                cpu_median=statistics.median(r['app']['cpu_one_core_percent'] for r in rows),
                cpu_min=min(r['app']['cpu_one_core_percent'] for r in rows),
                cpu_max=max(r['app']['cpu_one_core_percent'] for r in rows),
                pss_median_mib=statistics.median(r['app']['pss_mib'] for r in rows),
                compositor_cpu_median=statistics.median(r['compositor']['cpu_one_core_percent'] for r in rows)))
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == '__main__':
    main()
