#!/usr/bin/env python3
"""Tohru verification: isolated native admission, bounded execution, durable evidence."""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import subprocess
import sys
import time
import uuid

from evidence import atomic, digest, jsonl, publish, read, records, validate
from process import execute, private_environment
import session_units

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
EXIT = {'passed': 0, 'failed': 1, 'blocked': 3, 'incomplete': 4,
        'cleanup_failed': 5, 'unverified': 6}
OMITTED = ['real Home/provider', 'physical audio/hearing', 'live host recovery',
           'real models', 'desktop GUI entrypoints', 'owner satisfaction']


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args], text=True).strip()


def fingerprint(source):
    paths = [source / 'package.json', source / 'pnpm-lock.yaml']
    for directory in ('src', 'packages', 'config'):
        paths.extend(p for p in (source / directory).rglob('*')
                     if p.is_file() and 'node_modules' not in p.parts and '.git' not in p.parts)
    hashes = {str(p.relative_to(source)): digest(p) for p in sorted(set(paths))}
    return {'sha256': hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest(),
            'files': len(hashes), 'package': read(source / 'package.json')['version'],
            'lock_sha256': digest(source / 'pnpm-lock.yaml')}


def doctor(scenario, target, source):
    scenarios = read(HERE / 'scenarios.json')
    checks = []

    def check(name, okay, detail):
        checks.append({'name': name, 'status': 'present' if okay else 'absent', 'detail': detail})

    check('implemented', scenarios[scenario]['implemented'], scenarios[scenario]['prerequisites'])
    check('target', target == 'sandbox', 'Only isolated sandbox execution is implemented; no caller override exists')
    requirements = read(HERE / 'environment.json')['session_units' if scenario == 'session-units' else 'native_authority']
    tools = {name: shutil.which(name) for name in requirements['required_tools']}
    for name, path in tools.items():
        check(name, bool(path), path or requirements['diagnostic'])
    identity = None
    if scenario == 'session-units':
        files = ['bin/tohru', 'bin/tohru_portability.py', 'share/tohru/session/services.json']
        for name in files:
            check(name, (ROOT / name).is_file(), 'Required shipped source command/data')
        check('executable_cli', os.access(ROOT / 'bin/tohru', os.X_OK), 'Invoke actual bin/tohru through its shebang')
        identity = {name: digest(ROOT / name) for name in files if (ROOT / name).is_file()}
    if scenario == 'native-authority':
        check('private_source', source is not None and source.is_dir()
              and source.is_relative_to(ROOT / '.tmp'), 'Prepared writable Orca source must resolve under this checkout .tmp')
        if source and source.is_dir() and source.is_relative_to(ROOT / '.tmp'):
            required = ['package.json', 'pnpm-lock.yaml', 'node_modules/vitest/vitest.mjs',
                        'node_modules/typescript/bin/tsc']
            required += runpy.run_path(str(ROOT / 'integrations/orca/verify-owner-admission.py'))['TESTS']
            for name in required:
                check(name, (source / name).is_file(), 'See integrations/orca/README.md for pinned offline preparation')
            check('writable_source', os.access(source, os.W_OK), str(source))
            if (source / 'pnpm-lock.yaml').is_file() and (source / 'package.json').is_file():
                identity = fingerprint(source)
                check('source_version', identity['package'] == '1.4.268', identity)
            patch = ROOT / 'integrations/orca/desktop-voice-completion-observability.patch'
            applied = subprocess.run(['git', 'apply', '--unsafe-paths', '--directory=' + str(source),
                                      '--reverse', '--check', str(patch)],
                                     cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            check('final_reviewed_patch', applied.returncode == 0, 'Final completion patch must be present, not just upstream')
    namespaces = {}
    for name in ('pid', 'mnt', 'user'):
        namespaces[name] = os.readlink('/proc/self/ns/' + name)
    return {'schema_version': 1, 'scenario': scenario, 'target': target,
            'outcome': 'passed' if all(c['status'] == 'present' for c in checks) else 'blocked',
            'checks': checks, 'root': str(ROOT), 'commit': git('rev-parse', 'HEAD'),
            'dirty': git('status', '--porcelain').splitlines(), 'source': str(source) if source else None,
            'source_identity': identity, 'tools': tools, 'hostname': socket.gethostname(),
            'uid': os.getuid(), 'pid': os.getpid(), 'namespaces': namespaces,
            'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
            'harness_sha256': {p.name: digest(p) for p in HERE.iterdir()
                               if p.is_file() and p.suffix in ('.py', '.json')},
            'provenance': {p.name: digest(p) for p in (ROOT / 'integrations/orca').glob('*provenance.json')},
            'patches': {p.name: digest(p) for p in (ROOT / 'integrations/orca').glob('*.patch')},
            'preparation': ({'path': str(source.parent / 'preparation.json'),
                             'sha256': digest(source.parent / 'preparation.json')}
                            if source and (source.parent / 'preparation.json').is_file() else None),
            'environment_keys': sorted(os.environ), 'running_host_identity': 'unverified; no host access required',
            'authority': ('ordinary local CLI, no Home caller required' if scenario == 'session-units'
                          else 'native test fixture, not coordinating Home'),
            'effects': 'private files and owned CPU process groups only'}


def launch(args):
    report = doctor(args.scenario, args.target, args.orca_source)
    run = ROOT / '.tmp/verify' / args.scenario / str(uuid.uuid4())
    run.mkdir(parents=True, mode=0o700)
    atomic(run / 'doctor.json', report)
    state = {'schema_version': 1, 'run_id': run.name, 'scenario': args.scenario,
             'source': report['source'], 'phase': 'launched', 'doctor': report,
             'action_start_ns': 0, 'action_end_ns': 0, 'outcome': report['outcome']}
    for directory in ('home', 'config', 'data', 'state', 'cache', 'runtime', 'tmp'):
        (run / directory).mkdir(mode=0o700)
    atomic(run / 'state.json', state)
    atomic(run / 'before.json', {'host_touched': False, 'source_identity': report['source_identity']})
    atomic(run / 'resources.json', {'run_id': run.name, 'phase': 'private-directories', 'processes': [], 'lease': None})
    return run


def drive(run, timeout):
    state = read(run / 'state.json')
    if state['phase'] != 'launched':
        raise ValueError('drive is single-use; start a new run instead of retrying')
    state['phase'] = 'driving'
    state['action_start_ns'] = time.time_ns()
    atomic(run / 'state.json', state)
    actions = []
    observations, assertions = [], []
    outcome = state['outcome']
    if outcome == 'passed' and state['scenario'] == 'session-units':
        current = {name: digest(ROOT / name) for name in state['doctor']['source_identity']}
        if current != state['doctor']['source_identity']:
            outcome = 'blocked'
        else:
            outcome, actions, observations, assertions = session_units.drive(ROOT, run, timeout)
            atomic(run / 'resources.json', {'run_id': run.name, 'phase': 'children-reaped',
                                           'processes': actions, 'lease': None})
            if current != {name: digest(ROOT / name) for name in current}:
                outcome = 'incomplete'
    elif outcome == 'passed':
        source = Path(state['source'])
        if fingerprint(source) != state['doctor']['source_identity']:
            outcome = 'blocked'
        else:
            tests = runpy.run_path(str(ROOT / 'integrations/orca/verify-owner-admission.py'))['TESTS']
            node = state['doctor']['tools']['node']
            commands = [[node, 'node_modules/vitest/vitest.mjs', 'run', '--config', 'config/vitest.config.ts',
                         *tests, '--maxWorkers=2'],
                        [node, 'node_modules/typescript/bin/tsc', '--noEmit', '-p', 'config/tsconfig.node.json', '--composite', 'false'],
                        [node, 'node_modules/typescript/bin/tsc', '--noEmit', '-p', 'config/tsconfig.tc.cli.json', '--composite', 'false']]
            env = private_environment(run)
            outcome = 'passed'
            for index, command in enumerate(commands):
                action = execute(command, source, env, run / f'action-{index}.log', timeout)
                action.update({'sequence': index + 1, 'entrypoint': 'native CLI/schema/RPC/SQLite/IPC test',
                               'wall_end_ns': time.time_ns(), 'clock': 'monotonic duration; wall end ns', 'deadline_seconds': timeout})
                actions.append(action)
                atomic(run / 'resources.json', {'run_id': run.name, 'phase': 'children-reaped', 'processes': actions, 'lease': None})
                if action['exit'] != 0 or action['reason']:
                    outcome = 'failed'
                    break
            if fingerprint(source) != state['doctor']['source_identity']:
                outcome = 'incomplete'
    state.update(phase='driven', outcome=outcome, action_end_ns=time.time_ns())
    atomic(run / 'state.json', state)
    jsonl(run / 'actions.jsonl', actions)
    jsonl(run / 'observations.jsonl', observations or [{'native_test_logs': [a['log'] for a in actions],
                                      'scripted_boundaries': ['provider transport', 'physical audio'],
                                      'host_observation': None}])
    atomic(run / 'assertions.json', {'outcome': outcome, 'checks': assertions,
                                   'required': 'real CLI generation, persisted content and negative cases' if state['scenario'] == 'session-units' else '9 native files plus both typechecks',
                                   'actual_commands': len(actions), 'omitted_boundaries': OMITTED})
    atomic(run / 'timings.json', {'started_ns': state['action_start_ns'], 'ended_ns': state['action_end_ns'],
                                'durations': [a['duration_seconds'] for a in actions], 'performance_claim': None})
    atomic(run / 'manifest.json', {**state['doctor'], 'run_id': run.name,
                                 'tier': read(HERE / 'scenarios.json')[state['scenario']]['tier'],
                                 'scenario': state['scenario'], 'argv': [a['argv'] for a in actions]})
    return outcome


def cleanup(run):
    state = read(run / 'state.json')
    if state['phase'] == 'cleaned':
        return state['outcome']
    if state['phase'] == 'launched':
        state.update(phase='cleaned', outcome='incomplete')
        atomic(run / 'state.json', state)
        atomic(run / 'after.json', {'host_touched': False, 'owned_children_reaped': True,
                                   'private_files_retained': True, 'drive_never_started': True})
        return 'incomplete'
    if state['phase'] != 'driven':
        # A crashed driver has no durable proof of reaping. Never signal a reused PID.
        state['outcome'] = 'cleanup_failed'
        atomic(run / 'state.json', state)
        return 'cleanup_failed'
    # The foreground runner has already killed its session and waited. Keep private
    # files for diagnostics; no recursive deletion, host routing or PID guessing.
    after = {'host_touched': False, 'owned_children_reaped': True, 'private_files_retained': True}
    if state['scenario'] == 'session-units' and state['outcome'] in ('passed', 'failed'):
        after['generated_files'] = session_units.snapshot(run / 'units')
        observed = records(run / 'observations.jsonl')
        after['generated_files_unchanged'] = bool(observed) and after['generated_files'] == observed[-1].get('files')
        if not after['generated_files_unchanged']:
            state['outcome'] = 'cleanup_failed'
    atomic(run / 'after.json', after)
    resources = read(run / 'resources.json')
    resources['phase'] = 'cleaned'
    atomic(run / 'resources.json', resources)
    state['phase'] = 'cleaned'
    atomic(run / 'state.json', state)
    atomic(run / 'result.json', {'schema_version': 1, 'run_id': run.name, 'scenario': state['scenario'],
                               'outcome': state['outcome'],
                               'proof_level': (('shipped source CLI with actual private file effects' if state['scenario'] == 'session-units'
                                                else 'isolated native fixture') if state['outcome'] == 'passed' else 'no passing scenario proof'),
                               'remaining_gates': OMITTED, 'evidence': str(run), 'cleanup': 'complete'})
    return state['outcome']


def owned_run(path):
    run = path.resolve()
    if not run.is_relative_to(ROOT / '.tmp/verify') or not (run / 'state.json').is_file():
        raise ValueError('run must be an existing owned run under this checkout .tmp/verify')
    if read(run / 'state.json')['run_id'] != run.name:
        raise ValueError('run identity mismatch')
    return run


@contextmanager
def source_lock(run):
    state = read(run / 'state.json')
    if state['outcome'] != 'passed' or state['scenario'] != 'native-authority':
        yield
        return
    # The prepared source contains writable native indexes/caches: one driver
    # owns that source at a time even when run UUIDs differ.
    with (Path(state['source']) / '.tohru-verify.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    scenarios = read(HERE / 'scenarios.json')
    for command in ('doctor', 'launch', 'run'):
        child = sub.add_parser(command)
        if command == 'doctor':
            child.add_argument('--scenario', choices=scenarios, required=True)
        else:
            child.add_argument('scenario', choices=scenarios)
        child.add_argument('--target', choices=['sandbox', 'host'], default='sandbox')
        child.add_argument('--orca-source', type=lambda p: Path(p).resolve())
        child.add_argument('--json', action='store_true')
        if command == 'run':
            child.add_argument('--timeout', type=float, default=180)
    for command in ('drive', 'collect', 'cleanup', 'validate'):
        child = sub.add_parser(command)
        child.add_argument('--run', type=Path, required=True)
        if command == 'drive':
            child.add_argument('--timeout', type=float, default=180)
    args = parser.parse_args()
    if hasattr(args, 'timeout') and not 0 < args.timeout <= 1800:
        parser.error('timeout must be positive and at most 1800 seconds per command')
    try:
        if args.command == 'doctor':
            result = doctor(args.scenario, args.target, args.orca_source)
        elif args.command in ('run', 'launch'):
            run = launch(args)
            if args.command == 'launch':
                result = {'outcome': read(run / 'state.json')['outcome'], 'run': str(run)}
            else:
                with (run / 'lock').open('a') as lock:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    with source_lock(run):
                        drive(run, args.timeout)
                    cleanup(run)
                    collection = publish(run)
                    result = {**validate(run, collection), 'collection': str(collection)}
        else:
            run = owned_run(args.run)
            with (run / 'lock').open('a') as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                if args.command == 'drive':
                    with source_lock(run):
                        result = {'outcome': drive(run, args.timeout), 'run': str(run)}
                elif args.command == 'cleanup':
                    result = {'outcome': cleanup(run), 'run': str(run)}
                elif args.command == 'collect':
                    collection = publish(run)
                    result = {**validate(run, collection), 'collection': str(collection)}
                else:
                    result = validate(run)
        print(json.dumps(result, indent=2))
        return EXIT[result['outcome']]
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        outcome = 'incomplete'
        if not isinstance(error, BlockingIOError) and 'run' in locals() and (run / 'state.json').is_file():
            # The operation's context has unwound. Reacquire ownership before
            # writing recovery evidence or state; a competing owner may exist.
            try:
                with (run / 'lock').open('a') as lock:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    atomic(run / ('error-' + str(uuid.uuid4()) + '.json'),
                           {'command': args.command, 'error': str(error), 'wall_ns': time.time_ns()})
                    if read(run / 'state.json')['phase'] == 'driving':
                        outcome = cleanup(run)
            except BlockingIOError:
                pass
        print(json.dumps({'outcome': outcome, 'error': str(error),
                          'run': str(run) if 'run' in locals() else None}))
        return EXIT[outcome]


if __name__ == '__main__':
    sys.exit(main())
