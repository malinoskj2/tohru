#!/usr/bin/env python3
"""Matched CPU/memory trials using the existing prototype sampler; no GPU timings."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import signal
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('prototype_benchmark', ROOT / 'prototypes/benchmark_gpu.py')
sampler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sampler)


def identity(pid):
    fields = (Path('/proc') / str(pid) / 'stat').read_text().rsplit(') ', 1)[1].split()
    return int(fields[19])  # starttime: protects against PID reuse


def descendants(pid):
    found = set()
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():
            continue
        try:
            fields = (proc / 'stat').read_text().rsplit(') ', 1)[1].split()
            if int(fields[1]) == pid:
                child = int(proc.name)
                found.add(child)
                found.update(descendants(child))
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
    return found


def hardware(log, adapter):
    text = log.read_text(errors='replace')
    if adapter not in text or any(s in text.lower() for s in ('llvmpipe', 'lavapipe', 'software rasterizer')):
        raise ValueError(f'hardware adapter not confirmed in {log}')


def trial(side, label, out, warmup, seconds):
    log = out / f'{label}.log'
    # Commands must exec the application; no new shell transport is introduced.
    with log.open('w') as stream:
        app = subprocess.Popen(side['command'], stdout=stream, stderr=stream, start_new_session=True)
        try:
            time.sleep(warmup)
            if app.poll() is not None:
                raise RuntimeError(f'{label}: app exited during warm-up')
            hardware(log, side['adapter'])
            hardware(Path(side['compositor_log']), side['adapter'])
            pids = {'app': app.pid, 'compositor': int(side['compositor_pid']),
                    **{f'helper:{k}': int(v) for k, v in side['helper_pids'].items()}}
            if len(set(pids.values())) != len(pids):
                raise ValueError('each accounted process must have a distinct PID')
            expected_children = set(pids.values()) - {app.pid, pids['compositor']}
            for owner in (app.pid, pids['compositor']):
                missing = descendants(owner) - expected_children
                if missing:
                    raise ValueError(f'unaccounted descendants of {owner}: {sorted(missing)}')
            starts = {k: identity(pid) for k, pid in pids.items()}
            before = {k: sampler.sample(pid) for k, pid in pids.items()}
            time.sleep(seconds)
            if app.poll() is not None:
                raise RuntimeError(f'{label}: app exited during measurement')
            after = {k: sampler.sample(pid) for k, pid in pids.items()}
            if starts != {k: identity(pid) for k, pid in pids.items()}:
                raise RuntimeError('process identity changed during measurement')
            for owner in (app.pid, pids['compositor']):
                if descendants(owner) - expected_children:
                    raise RuntimeError('unaccounted children appeared during measurement')
            hz = os.sysconf('SC_CLK_TCK')
            metrics = {}
            for key in pids:
                a, b = before[key], after[key]
                elapsed = b['wall'] - a['wall']
                metrics[key] = {'elapsed_seconds': elapsed, 'cpu_ticks': b['ticks'] - a['ticks'],
                    'cpu_one_core_percent': (b['ticks'] - a['ticks']) / hz / elapsed * 100,
                    'rss_mib': b['rss_kib'] / 1024, 'pss_mib': b['pss_kib'] / 1024}
            return {'label': label, 'pids': pids, 'start_ticks': starts, 'before': before,
                    'after': after, 'metrics': metrics,
                    'total_cpu_one_core_percent': sum(v['cpu_one_core_percent'] for v in metrics.values()),
                    'total_pss_mib': sum(v['pss_mib'] for v in metrics.values()),
                    'host_load_after': os.getloadavg()}
        finally:
            # Only this runner's process group is owned; external compositor/helpers stay alive.
            try:
                os.killpg(app.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(app.pid, signal.SIGKILL)
                app.wait()
            # A child may ignore SIGTERM even after its parent exits.
            try:
                os.killpg(app.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--gpu-slot', required=True, help='coordinator allocation receipt/identifier')
    parser.add_argument('--rounds', default=5, type=int)
    parser.add_argument('--warmup', default=10, type=float)
    parser.add_argument('--seconds', default=30, type=float)
    args = parser.parse_args()
    if args.rounds < 5 or not all(math.isfinite(v) for v in (args.warmup, args.seconds)) or args.warmup < 10 or args.seconds < 30:
        parser.error('acceptance sampling requires >=5 rounds, >=10s warm-up, >=30s measured')
    output = args.output.resolve()
    if not output.is_relative_to(ROOT / '.tmp'):
        parser.error('output must be inside worktree-root .tmp/')
    config = json.loads(args.spec.read_text())
    for side in ('reference', 'candidate'):
        entry = config[side]
        if not entry['command'] or not isinstance(entry['command'], list) or not all(isinstance(v, str) for v in entry['command']):
            parser.error(f'{side}.command must be nonempty argv strings')
        for key in ('adapter', 'compositor_log', 'compositor_pid', 'helper_pids'):
            if key not in entry:
                parser.error(f'{side}.{key} is required (use empty helper_pids only if none)')
        if not isinstance(entry['adapter'], str) or not entry['adapter'].strip():
            parser.error(f'{side}.adapter must identify the hardware')
        if not isinstance(entry['helper_pids'], dict):
            parser.error(f'{side}.helper_pids must map names to PIDs')
        if any(type(pid) is not int or pid <= 0 for pid in [entry['compositor_pid'], *entry['helper_pids'].values()]):
            parser.error(f'{side} process PIDs must be positive integers')
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    record = {'spec': config, 'gpu_slot': args.gpu_slot, 'warmup': args.warmup,
              'seconds': args.seconds, 'rounds': args.rounds, 'trials': rows,
              'limits': 'CPU/memory snapshots only; no GPU duration/presentation/latency/soak acceptance; short-lived children may escape boundary census'}
    (output / 'raw.json').write_text(json.dumps(record, indent=2) + '\n')
    for round_index in range(args.rounds):
        order = ('reference', 'candidate') if round_index % 2 == 0 else ('candidate', 'reference')
        for side in order:
            row = trial(config[side], f'{round_index + 1}-{side}', output, args.warmup, args.seconds)
            row.update(side=side, round=round_index + 1)
            rows.append(row)
            (output / 'raw.json').write_text(json.dumps(record, indent=2) + '\n')
    summary = {}
    for side in ('reference', 'candidate'):
        selected = [r for r in rows if r['side'] == side]
        summary[side] = {'n': len(selected)}
        for metric in ('total_cpu_one_core_percent', 'total_pss_mib'):
            values = [r[metric] for r in selected]
            summary[side][metric] = {'median': statistics.median(values), 'min': min(values), 'max': max(values)}
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(output)


if __name__ == '__main__':
    main()
