#!/usr/bin/env python3
"""Prepare/rehearse an independently supervised Orca GUI reload; never stop its PTY daemon."""
import argparse
import hashlib
import json
import os
import pwd
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import time
import uuid

DESKTOP_KEYS = {
    'HOME', 'USER', 'LOGNAME', 'DISPLAY', 'WAYLAND_DISPLAY', 'XAUTHORITY',
    'DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME',
    'XDG_STATE_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_DIRS', 'XDG_SESSION_TYPE',
    'XDG_CURRENT_DESKTOP', 'XDG_SESSION_DESKTOP', 'LANG', 'XMODIFIERS',
    'GTK_IM_MODULE', 'QT_IM_MODULE', 'ELECTRON_OZONE_PLATFORM_HINT',
}
TRUSTED_PATH = '/run/current-system/sw/bin:/etc/profiles/per-user/jesse/bin'


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def write(path, value):
    path = Path(path)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')
    path.chmod(0o600)


def checked(command, **kwargs):
    return subprocess.run(command, check=True, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, **kwargs).stdout.strip()


def birth(pid):
    try:
        return Path('/proc', str(pid), 'stat').read_text().rsplit(')', 1)[1].split()[19]
    except FileNotFoundError:
        return None


def desktop_environment(original, coreutils):
    # Do not copy provider handoff, Node mode/options or old bundle library variables.
    env = {k: v for k, v in original.items() if k in DESKTOP_KEYS or k.startswith('LC_')}
    env['PATH'] = str(coreutils) + ':' + TRUSTED_PATH
    return env


class Host:
    def __init__(self, plan):
        self.plan = plan

    def status(self):
        try:
            result = subprocess.run([self.plan['cli'], 'status', '--json'], env=self.plan['env'],
                                    text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=12)
            if result.returncode:
                return {'running': False, 'ready': False}
            data = json.loads(result.stdout)['result']
        except (subprocess.TimeoutExpired, ValueError, KeyError):
            # API unavailability while the old app is down must not prevent the
            # independent GUI launch. It is never evidence of readiness.
            return {'running': False, 'ready': False, 'unknown': True}
        return {'running': data['app']['running'], 'pid': data['app'].get('pid'),
                'ready': data['runtime']['state'] == 'ready',
                'runtime': data['runtime'].get('runtimeId')}

    def daemon_pids(self):
        found = []
        socket = str(Path(self.plan['env']['HOME']) / '.config/orca/daemon/daemon-v61.sock').encode()
        for path in Path('/proc').iterdir():
            if not path.name.isdigit():
                continue
            try:
                args = (path / 'cmdline').read_bytes().split(b'\0')
                if socket in args and any(b'daemon-entry.js' in a for a in args):
                    found.append(int(path.name))
            except OSError:
                pass
        return sorted(found)

    def alive(self, pid, expected_birth):
        return birth(pid) == expected_birth

    def stop(self, pid):
        os.kill(pid, signal.SIGTERM)

    def pause(self, seconds):
        time.sleep(seconds)

    def launch(self, unit, log):
        args = [self.plan['systemd_run'], '--user', '--unit=' + unit,
                '--service-type=exec', '--property=Restart=no', '--property=RemainAfterExit=yes',
                '--property=StandardOutput=append:' + str(log),
                '--property=StandardError=append:' + str(log)]
        args += ['--setenv=' + k + '=' + v for k, v in self.plan['env'].items()]
        checked([*args, self.plan['gui'], *self.plan['profile']])

    def unit(self, unit):
        result = subprocess.run([self.plan['systemctl'], '--user', 'show', unit,
                                 '-p', 'MainPID', '-p', 'ActiveState', '-p', 'SubState', '-p', 'ExecMainStatus'],
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        fields = dict(line.split('=', 1) for line in result.stdout.splitlines() if '=' in line)
        pid = int(fields.get('MainPID', '0'))
        return {'pid': pid,
                'active': fields.get('ActiveState') in ('active', 'activating') and pid > 0,
                'unknown': result.returncode != 0,
                'exit_status': fields.get('ExecMainStatus')}


def execute(plan, host, record, stop_timeout=30, ready_timeout=120):
    """One guarded stop, separately supervised GUI units, at most one safe retry."""
    current = host.status()
    if (current.get('pid') != plan['pid'] or current.get('runtime') != plan['runtime']
            or not host.alive(plan['pid'], plan['birth'])):
        record('refused', reason='prepared application identity changed; nothing stopped')
        return False
    if host.daemon_pids() != plan['daemons']:
        record('refused', reason='daemon identity changed; nothing stopped')
        return False
    record('stop_requested', pid=plan['pid'])
    host.stop(plan['pid'])
    end = time.monotonic() + stop_timeout
    while host.alive(plan['pid'], plan['birth']) and time.monotonic() < end:
        host.pause(.1)
    if host.alive(plan['pid'], plan['birth']):
        record('failed', reason='graceful shutdown timed out; no force kill or duplicate launch')
        return False
    record('stopped', pid=plan['pid'])
    host.pause(plan['delay'])
    for attempt in (1, 2):
        # A manual or unrelated recovery is never credited to this supervisor.
        if host.status().get('running'):
            record('failed', reason='application appeared outside this launch; manual recovery required')
            return False
        if host.daemon_pids() != plan['daemons']:
            record('failed', reason='persistent daemon changed')
            return False
        unit = plan['unit'] + '-gui-' + str(attempt)
        log = Path(plan['directory']) / ('gui-' + str(attempt) + '.log')
        record('launch_requested', attempt=attempt, unit=unit, log=str(log))
        try:
            host.launch(unit, log)
        except subprocess.SubprocessError as error:
            record('launch_failed', attempt=attempt, error=type(error).__name__)
            continue
        end = time.monotonic() + ready_timeout
        while time.monotonic() < end:
            owned, observed = host.unit(unit), host.status()
            if observed.get('running') and observed.get('ready'):
                if (observed.get('pid') != owned['pid'] or not owned['active']
                        or observed.get('runtime') == plan['runtime']):
                    record('failed', reason='ready application is not owned by this launch unit')
                    return False
                if host.daemon_pids() != plan['daemons']:
                    record('failed', reason='persistent daemon changed during startup')
                    return False
                record('succeeded', pid=owned['pid'], runtime=observed['runtime'], unit=unit)
                return True
            if not owned['active'] and not owned.get('unknown'):
                record('launch_exited', attempt=attempt, exit_status=owned['exit_status'])
                break
            host.pause(.25)
        else:
            # Never start a second GUI while an uncertain first process still exists.
            record('failed', reason='readiness timeout; existing launch unit retained for inspection', unit=unit)
            return False
        host.pause(plan['delay'])
    record('failed', reason='both independent launch attempts exited before readiness; inspect launch logs')
    return False


def load(path):
    envelope = json.loads(Path(path).read_text())
    if digest(envelope['plan']) != envelope['sha256']:
        raise ValueError('Prepared plan changed')
    return envelope


def prepare(args):
    directory = args.output.resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    cli = args.cli.resolve()
    raw = json.loads(checked([str(cli), 'status', '--json']))['result']
    pid = raw['app']['pid']
    old = dict(v.split(b'=', 1) for v in Path('/proc', str(pid), 'environ').read_bytes().split(b'\0') if b'=' in v)
    original = {k.decode(): v.decode() for k, v in old.items()}
    found_dirname = shutil.which('dirname', path=TRUSTED_PATH)
    if not found_dirname:
        raise ValueError('No coreutils dirname in the active host profiles')
    dirname = Path(found_dirname).resolve()
    systemctl = shutil.which('systemctl', path=TRUSTED_PATH)
    manager = dict(line.split('=', 1) for line in checked([systemctl, '--user', 'show-environment']).splitlines() if '=' in line)
    # Electron may scrub its inherited environment. The user manager retains the
    # desktop session; explicit process values win, with uid identity as fallback.
    account = pwd.getpwuid(os.getuid())
    inherited = {**manager, **original}
    inherited.setdefault('HOME', account.pw_dir)
    inherited.setdefault('USER', account.pw_name)
    inherited.setdefault('LOGNAME', account.pw_name)
    inherited.setdefault('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))
    inherited.setdefault('DBUS_SESSION_BUS_ADDRESS', 'unix:path=' + inherited['XDG_RUNTIME_DIR'] + '/bus')
    env = desktop_environment(inherited, dirname.parent)
    if not (env.get('WAYLAND_DISPLAY') or env.get('DISPLAY')):
        raise ValueError('No verified desktop display in the process or user-manager environment')
    gui = cli.parents[2] / 'orca-ide'
    if not gui.is_file() or gui == cli:
        raise ValueError('Installed CLI does not identify a separate GUI executable')
    argv = [v.decode() for v in Path('/proc', str(pid), 'cmdline').read_bytes().split(b'\0')[:-1]]
    profile = []
    for i, value in enumerate(argv):
        if value.startswith('--user-data-dir='):
            profile.append(value)
        elif value == '--user-data-dir' and i + 1 < len(argv):
            profile.extend([value, argv[i + 1]])
    plan = {'pid': pid, 'birth': birth(pid), 'runtime': raw['runtime']['runtimeId'],
            'cli': str(cli), 'gui': str(gui), 'profile': profile, 'env': env,
            'directory': str(directory), 'delay': 2,
            'revision': checked(['/run/current-system/sw/bin/nixos-version', '--configuration-revision']),
            'supervisor_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            'systemd_run': shutil.which('systemd-run', path=env['PATH']),
            'systemctl': shutil.which('systemctl', path=env['PATH']),
            'unit': 'tohru-orca-reload-' + uuid.uuid4().hex[:12]}
    if not all(plan.get(key) and os.access(plan[key], os.X_OK) for key in ('cli', 'gui', 'systemd_run', 'systemctl')):
        raise ValueError('Required installed executable is unavailable')
    plan['daemons'] = Host(plan).daemon_pids()
    if not plan['birth'] or not plan['daemons']:
        raise ValueError('Missing application/daemon identity')
    closure = checked(['/run/current-system/sw/bin/nix-store', '-qR', '/run/current-system']).splitlines()
    if str(Path(*gui.resolve().parts[:4])) not in closure:
        raise ValueError('GUI executable is outside the active system closure')
    write(directory / 'plan.json', {'plan': plan, 'sha256': digest(plan)})
    print(directory / 'plan.json')


def rehearse(args):
    envelope = load(args.plan)
    plan = envelope['plan']
    before = Host(plan).status()
    directory = Path(plan['directory'])
    bad = {**plan['env'], 'PATH': '/missing'}
    failed = subprocess.run([plan['cli'], '--help'], env=bad, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15)
    (directory / 'missing-path.log').write_text(failed.stderr)
    if failed.returncode == 0 or 'dirname' not in failed.stderr:
        raise ValueError('Historical missing-PATH failure was not reproduced')
    help_result = checked([plan['cli'], 'open', '--help'], env=plan['env'], timeout=20)
    (directory / 'launcher-help.log').write_text(help_result)
    after = Host(plan).status()
    if not after.get('ready') or before != after or after.get('pid') != plan['pid']:
        raise ValueError('Read-only rehearsal changed application identity or failed readiness')
    write(directory / 'rehearsal.json', {'status': 'passed', 'plan_sha256': envelope['sha256'],
          'historical_missing_path_exit': failed.returncode, 'current_app_untouched': True,
          'pid': after['pid'], 'runtime': after['runtime'],
          'limits': 'No live GUI stop/relaunch was exercised; supervisor lifecycle has separate tests.'})
    print('Read-only launcher/PATH rehearsal passed; current application was not stopped.')


def run_plan(args):
    envelope = load(args.plan)
    plan = envelope['plan']
    proof = json.loads((Path(plan['directory']) / 'rehearsal.json').read_text())
    if proof['status'] != 'passed' or proof['plan_sha256'] != envelope['sha256']:
        raise ValueError('This exact plan has not passed read-only rehearsal')
    if plan['supervisor_sha256'] != hashlib.sha256(Path(__file__).read_bytes()).hexdigest():
        raise ValueError('Supervisor changed since preparation/rehearsal')
    if checked(['/run/current-system/sw/bin/nixos-version', '--configuration-revision']) != plan['revision']:
        raise ValueError('Active system changed since preparation/rehearsal')
    if not args.execute:
        print('No action: execution requires explicit --execute after coordination.')
        return
    state = Path(plan['directory']) / 'result.json'
    if state.exists():
        raise ValueError('This plan already ran; prepare a fresh plan instead of repeating shutdown')
    def record(stage, **fields):
        row = {'at_ms': int(time.time() * 1000), 'stage': stage, **fields}
        with (Path(plan['directory']) / 'events.jsonl').open('a') as log:
            log.write(json.dumps(row) + '\n')
        write(state, row)
    try:
        ok = execute(plan, Host(plan), record)
    except Exception as error:
        record('failed', reason=str(error), error=type(error).__name__)
        raise
    if not ok:
        raise SystemExit(1)


def queue_plan(args):
    envelope = load(args.plan)
    plan = envelope['plan']
    command = [plan['systemd_run'], '--user', '--unit=' + plan['unit'], '--collect',
               '--service-type=exec', '--property=Restart=no', sys.executable,
               str(Path(__file__).resolve()), 'run', '--plan', str(args.plan.resolve()), '--execute']
    if args.execute:
        # run performs the same rehearsal/identity checks again inside the independent unit.
        print(checked(command))
    else:
        print(json.dumps(command))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subs = parser.add_subparsers(dest='action', required=True)
    p = subs.add_parser('prepare')
    p.add_argument('--cli', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.set_defaults(function=prepare)
    for name, function in [('rehearse', rehearse), ('run', run_plan), ('queue', queue_plan)]:
        p = subs.add_parser(name)
        p.add_argument('--plan', type=Path, required=True)
        if name in ('run', 'queue'):
            p.add_argument('--execute', action='store_true')
        p.set_defaults(function=function)
    args = parser.parse_args()
    args.function(args)


if __name__ == '__main__':
    main()
