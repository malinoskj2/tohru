#!/usr/bin/env python3
"""Replay canonical workspace stimuli through an explicit private-compositor argv."""
import argparse
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gpu-slot', required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or not any('{workspace}' in s for s in command):
        parser.error('provide compositor argv after --, with a {workspace} argument')
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    if not output.is_relative_to(root / '.tmp'):
        parser.error('output must be inside worktree-root .tmp/')
    timeline = json.loads((root / 'tools/reference/workspace-replay.json').read_text())
    output.parent.mkdir(parents=True, exist_ok=True)
    start = time.monotonic()
    # Keep raw scheduling/completion times: dispatch timing is not presentation latency.
    with output.open('x') as log:
        for at, workspace in timeline['events_ms']:
            time.sleep(max(0, start + at / 1000 - time.monotonic()))
            argv = [s.replace('{workspace}', str(workspace)) for s in command]
            before = time.monotonic() - start
            result = subprocess.run(argv, capture_output=True, text=True, timeout=2)
            row = {'at_ms': at, 'workspace': workspace,
                   'dispatch_ms': before * 1000, 'completed_ms': (time.monotonic() - start) * 1000,
                   'returncode': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr,
                   'gpu_slot': args.gpu_slot}
            log.write(json.dumps(row) + '\n')
            log.flush()
            if result.returncode:
                raise SystemExit(f'workspace request failed; see {output}')
        time.sleep(max(0, start + timeline['end_ms'] / 1000 - time.monotonic()))
    print(output)


if __name__ == '__main__':
    main()
