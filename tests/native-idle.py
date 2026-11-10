#!/usr/bin/env python3
"""Private Sway check of native idle: fade, output power-off, input wake and inhibitors."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native_wallpaper', ROOT / 'tests/native-wallpaper.py')
wallpaper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wallpaper)
base = wallpaper.base


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--pointer', type=Path, required=True)
    args = parser.parse_args()
    evidence = ROOT / '.tmp' / ('idle-native-' + str(time.time_ns()))
    evidence.mkdir(parents=True)
    run = ROOT / '.tmp' / ('i' + str(os.getpid()))
    run.mkdir(mode=0o700)
    (run / 'ipc').mkdir(mode=0o700)
    for name in ['home', 'config/tohru', 'cache', 'data', 'state']:
        (evidence / name).mkdir(parents=True, mode=0o700)
    (evidence / 'config/tohru/config.toml').write_text(
        "[wallpaper]\nenabled=false\n[idle]\nlock_seconds=0\nscreen_off_unlocked_seconds=2\nscreen_off_locked_seconds=0\n"
        "suspend_unlocked_seconds=0\nsuspend_locked_seconds=0\nfade_milliseconds=600\n")
    scene = evidence / 'scene'
    shutil.copytree(ROOT / 'shell', scene)
    env = wallpaper.private_environment(os.environ, run, evidence, args.runtime, scene)
    # One private bus stands in for both the session bus and logind's system bus.
    (run / 'bus.conf').write_text(base.private_bus_config(run))
    (evidence / 'sway.conf').write_text('output HEADLESS-1 mode 1280x720@60Hz\noutput HEADLESS-1 scale 1\n')
    logs, events = [], []
    def log(name):
        handle = (evidence / name).open('w'); logs.append(handle); return handle
    app = pointer = sway = bus = logind = None
    result = {'result': 'failed', 'runtime': str(args.runtime.resolve()), 'evidence': str(evidence), 'checks': []}
    def command(argv, check=True):
        base.assert_private_transport(env, run)
        completed = subprocess.run(argv, env=env, text=True, capture_output=True, timeout=15, close_fds=True)
        events.append({'argv': [str(a) for a in argv], 'exit': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr})
        if check: assert completed.returncode == 0, events[-1]
        return completed.stdout.strip()
    def native(value): return command([str(args.runtime.resolve()), '--say', 'tohru', value])
    def powered(): return json.loads(command(['swaymsg', '-t', 'get_outputs', '-r']))[0]['power']
    def alpha(): return float(native('get idle_alpha') or 0)
    def logind_control(method, *values):
        command(['gdbus', 'call', '--address', env['DBUS_SYSTEM_BUS_ADDRESS'], '--dest', 'org.tohru.TestLogind',
                 '--object-path', '/org/freedesktop/login1', '--method', 'org.tohru.TestLogind.' + method, *values])
    # Inhibitors belong to the client connection, so the client must stay connected.
    inhibitor_source = """
import sys
import gi
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
def call(method, arguments):
    return bus.call_sync('org.freedesktop.ScreenSaver', '/org/freedesktop/ScreenSaver', 'org.freedesktop.ScreenSaver',
                         method, arguments, None, Gio.DBusCallFlags.NONE, 3000, None)
cookie = call('Inhibit', GLib.Variant('(ss)', ('tohru-test', 'private check'))).unpack()[0]
print('inhibited', flush=True)
sys.stdin.readline()
call('UnInhibit', GLib.Variant('(u)', (cookie,)))
print('released', flush=True)
sys.stdin.readline()
"""
    def wake():
        pointer.stdin.write('click 640 400\n'); pointer.stdin.flush()
        base.wait_for(lambda: powered() and alpha() == 0, 'input restores output power and clears the veil', 10)
    def stays_awake(label, seconds=4):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            assert powered() and alpha() == 0, label
            time.sleep(0.2)
    try:
        bus = subprocess.Popen(['dbus-daemon', '--nofork', '--config-file=' + str(run / 'bus.conf')], env=env, stdout=log('bus.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: (run / 'bus').is_socket(), 'owned private bus')
        logind = subprocess.Popen([sys.executable, str(ROOT / 'tests/session/logind_fixture.py'), '--bus', env['DBUS_SYSTEM_BUS_ADDRESS']], env=env, stdout=log('logind.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: 'PRIVATE_LOGIND_FIXTURE_READY' in (evidence / 'logind.log').read_text(), 'fake logind')
        sway = subprocess.Popen(['sway', '--unsupported-gpu', '-d', '-c', str(evidence / 'sway.conf')], env=env, stdout=log('sway.log'), stderr=subprocess.STDOUT, close_fds=True)
        env['SWAYSOCK'] = str(base.wait_for(lambda: next(run.glob('sway-ipc.*.sock'), None), 'private Sway socket'))
        env['WAYLAND_DISPLAY'] = base.wait_for(lambda: next((p.name for p in run.glob('wayland-*') if not p.name.endswith('.lock')), None), 'private display')
        pointer = subprocess.Popen([str(args.pointer.resolve()), str(run)], env=env, stdin=subprocess.PIPE, stdout=log('pointer.log'), stderr=subprocess.STDOUT, text=True, close_fds=True)
        base.wait_for(lambda: 'ready' in (evidence / 'pointer.log').read_text(), 'owned pointer')
        app = subprocess.Popen([str(ROOT / 'bin/tohru-run')], env=env, stdout=log('app.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: app.poll() is None and command([str(args.runtime.resolve()), '--say', 'tohru', 'get settings_status'], False).startswith('{'), 'private root IPC', 20)
        # No dedicated locker runs here; answer its unlocked hint so automatic actions may proceed.
        native('emit idle_lock_state 0')
        result['initial_status'] = native('get idle_status')
        base.wait_for(lambda: alpha() > 0, 'idle fade starts', 10)
        base.wait_for(lambda: not powered(), 'output powered off after the fade', 10)
        result['checks'].append('screen-off fade then native output power-off')
        wake()
        result['checks'].append('pointer input wakes output and clears veil')
        inhibitor = subprocess.Popen([sys.executable, '-c', inhibitor_source], env=env, stdin=subprocess.PIPE, stdout=log('inhibitor.log'), stderr=subprocess.STDOUT, text=True, close_fds=True)
        base.wait_for(lambda: 'inhibited' in (evidence / 'inhibitor.log').read_text(), 'ScreenSaver inhibitor connected')
        stays_awake('ScreenSaver inhibitor held idle')
        inhibitor.stdin.write('\n'); inhibitor.stdin.flush()
        base.wait_for(lambda: 'released' in (evidence / 'inhibitor.log').read_text(), 'ScreenSaver inhibitor released')
        base.wait_for(lambda: not powered(), 'idle resumes after ScreenSaver release', 10)
        wake()
        inhibitor.stdin.close(); inhibitor.wait(timeout=5)
        result['checks'].append('ScreenSaver Inhibit/UnInhibit from a connected client')
        logind_control('Blocked', 'idle')
        stays_awake('logind idle block held idle')
        logind_control('Blocked', '')
        base.wait_for(lambda: not powered(), 'idle resumes after logind block clears', 10)
        wake()
        result['checks'].append('logind BlockInhibited=idle')
        result['final_status'] = native('get idle_status')
        native('quit'); base.clean_exit(app, 'idle root', events)
        result['result'] = 'passed'
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        errors = base.finish_private_run((app, pointer, sway, logind, bus), (pointer.stdin if pointer else None, *logs),
            lambda: (evidence / 'events.json').write_text(json.dumps(events, indent=2) + '\n'))
        try: shutil.rmtree(run)
        except BaseException as error: errors.append(error)
        result['cleanup_errors'] = [repr(error) for error in errors]
        (evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2), flush=True)
        if errors and sys.exc_info()[0] is None: raise BaseExceptionGroup('Owned cleanup failed', errors)


if __name__ == '__main__':
    main()
