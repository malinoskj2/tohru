#!/usr/bin/env python3
"""Run real native queue/admission/IPC tests without a model or physical audio."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TESTS = [
    'src/main/runtime/orchestration/terminal-authority/structured-sessions/structured-chat-worker-start.test.ts',
    'src/main/runtime/orchestration/terminal-authority/structured-sessions/structured-chat-worker-start-unknown.test.ts',
    'src/main/runtime/orchestration/terminal-authority/structured-sessions/structured-chat-worker-reuse.test.ts',
    'src/main/runtime/rpc/methods/orchestration/worker/start/placement/worker-terminal-custody-at-creation.test.ts',
    'src/main/bots/desktop-voice-task-queue.test.ts',
    'src/main/bots/desktop-voice-continuations.test.ts',
    'src/main/native-chat/agent-session-wire/publication/history/agent-read-journal-page.test.ts',
    'src/cli/arguments/args.test.ts',
    'src/cli/orchestration/agent-view.test.ts',
    'src/main/bots/desktop-voice-side.test.ts',
    'src/main/runtime/rpc/methods/bots-voice-side.test.ts',
    'src/main/bots/desktop-bot-auto-steer.test.ts',
    'src/main/runtime/rpc/orchestration/callers/orchestration-session-voice-caller.test.ts',
    'src/shared/agent-session/view/structured-agent-session-queued-cards.test.ts',
]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / '.tmp/orca-owner-admission')
    args = parser.parse_args()
    source, output = args.source.resolve(), args.output.resolve()
    if not source.is_relative_to(ROOT / '.tmp') or not output.is_relative_to(ROOT / '.tmp'):
        parser.error('source and output must be isolated under this worktree .tmp')
    output.mkdir(parents=True, exist_ok=True)
    commands = [
        ['node', 'node_modules/vitest/vitest.mjs', 'run', '--config', 'config/vitest.config.ts', *TESTS, '--maxWorkers=2'],
        ['node', 'node_modules/typescript/bin/tsc', '--noEmit', '-p', 'config/tsconfig.node.json', '--composite', 'false'],
        ['node', 'node_modules/typescript/bin/tsc', '--noEmit', '-p', 'config/tsconfig.tc.cli.json', '--composite', 'false'],
    ]
    for name, command in zip(['native-tests', 'node-types', 'cli-types'], commands):
        path = output / (name + '.log')
        with path.open('w') as log:
            result = subprocess.run(command, cwd=source, stdout=log, stderr=subprocess.STDOUT)
        print(f'{name}: exit={result.returncode}; {path}', flush=True)
        if result.returncode:
            raise SystemExit(result.returncode)

if __name__ == '__main__':
    main()
