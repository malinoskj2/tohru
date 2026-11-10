#!/usr/bin/env python3
"""Private Sway check that an assigned video wallpaper reaches the native libmpv owner."""
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
    args = parser.parse_args()
    evidence = ROOT / '.tmp' / ('video-native-' + str(time.time_ns()))
    evidence.mkdir(parents=True)
    run = ROOT / '.tmp' / ('v' + str(os.getpid()))
    run.mkdir(mode=0o700)
    (run / 'ipc').mkdir(mode=0o700)
    for name in ['home', 'config/tohru', 'cache', 'data', 'state']:
        (evidence / name).mkdir(parents=True, mode=0o700)
    clip = evidence / 'data/clip.mp4'
    subprocess.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=30', '-t', '3',
                    '-pix_fmt', 'yuv420p', str(clip)], check=True, timeout=60,
                   env={k: v for k, v in os.environ.items() if k != 'LD_LIBRARY_PATH'})
    (evidence / 'config/tohru/config.toml').write_text(
        "[wallpaper]\nenabled=true\nchange='never'\n[wallpaper.assignments]\nHEADLESS-1='" + str(clip) + "'\n"
        "[idle]\nlock_seconds=0\nscreen_off_unlocked_seconds=0\nscreen_off_locked_seconds=0\n"
        "suspend_unlocked_seconds=0\nsuspend_locked_seconds=0\nfade_milliseconds=0\n")
    scene = evidence / 'scene'
    shutil.copytree(ROOT / 'shell', scene)
    env = wallpaper.private_environment(os.environ, run, evidence, args.runtime, scene)
    (run / 'bus.conf').write_text(base.private_bus_config(run))
    (evidence / 'sway.conf').write_text('output HEADLESS-1 mode 1280x720@60Hz\noutput HEADLESS-1 scale 1\noutput HEADLESS-1 bg #30232b solid_color\n')
    logs, events = [], []
    def log(name):
        handle = (evidence / name).open('w'); logs.append(handle); return handle
    app = sway = bus = None
    result = {'result': 'failed', 'runtime': str(args.runtime.resolve()), 'evidence': str(evidence), 'performance_acceptance': False}
    def command(argv, check=True):
        base.assert_private_transport(env, run)
        completed = subprocess.run(argv, env=env, text=True, capture_output=True, timeout=15, close_fds=True)
        events.append({'argv': [str(a) for a in argv], 'exit': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr})
        if check: assert completed.returncode == 0, events[-1]
        return completed.stdout.strip()
    def native(value): return command([str(args.runtime.resolve()), '--say', 'tohru', value])
    def output_status():
        status = json.loads(native('get wallpaper_video_status') or '{}')
        return status.get('outputs', {}).get('HEADLESS-1')
    try:
        bus = subprocess.Popen(['dbus-daemon', '--nofork', '--config-file=' + str(run / 'bus.conf')], env=env, stdout=log('bus.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: (run / 'bus').is_socket(), 'owned private bus')
        sway = subprocess.Popen(['sway', '--unsupported-gpu', '-d', '-c', str(evidence / 'sway.conf')], env=env, stdout=log('sway.log'), stderr=subprocess.STDOUT, close_fds=True)
        env['SWAYSOCK'] = str(base.wait_for(lambda: next(run.glob('sway-ipc.*.sock'), None), 'private Sway socket'))
        env['WAYLAND_DISPLAY'] = base.wait_for(lambda: next((p.name for p in run.glob('wayland-*') if not p.name.endswith('.lock')), None), 'private display')
        app = subprocess.Popen([str(ROOT / 'bin/tohru-run')], env=env, stdout=log('app.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: app.poll() is None and command([str(args.runtime.resolve()), '--say', 'tohru', 'get settings_status'], False).startswith('{'), 'private root IPC', 20)
        assert native('get wallpaper_video_error') == '', native('get wallpaper_video_error')
        status = base.wait_for(lambda: (s if (s := output_status()) and s.get('state') in ('paused', 'playing', 'failed') else None), 'video output reported', 20)
        result['status'] = status
        assert status.get('state') != 'failed', status
        # Protection and occupancy have no verified providers yet, so policy keeps it paused.
        assert status.get('desired_pause') is True, status
        time.sleep(0.5)
        command(['grim', '-o', 'HEADLESS-1', str(evidence / 'video-paused.png')])
        from PIL import Image
        with Image.open(evidence / 'video-paused.png') as image:
            colours = image.convert('RGB').crop((0, 100, 1280, 720)).getcolors(1 << 20) or []
        result['distinct_colours'] = len(colours)
        assert len(colours) > 50, 'video frame not presented behind the shell'
        native('quit'); base.clean_exit(app, 'video root', events)
        result['result'] = 'passed'
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        errors = base.finish_private_run((app, sway, bus), tuple(logs),
            lambda: (evidence / 'events.json').write_text(json.dumps(events, indent=2) + '\n'))
        try: shutil.rmtree(run)
        except BaseException as error: errors.append(error)
        result['cleanup_errors'] = [repr(error) for error in errors]
        (evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2), flush=True)
        if errors and sys.exc_info()[0] is None: raise BaseExceptionGroup('Owned cleanup failed', errors)


if __name__ == '__main__':
    main()
