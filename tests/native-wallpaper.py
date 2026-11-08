#!/usr/bin/env python3
"""Focused owned headless Sway check; never address a host display/audio/bus."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time
import tomllib
import zlib

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('native_settings', ROOT / 'tests/native-settings.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_info(path, expected_solid=None):
    data = path.read_bytes()
    assert len(data) <= 2 * 1024 * 1024 and data[:8] == b'\x89PNG\r\n\x1a\n'
    at, compressed = 8, bytearray()
    while at < len(data):
        size = struct.unpack('>I', data[at:at + 4])[0]
        kind, payload = data[at + 4:at + 8], data[at + 8:at + 8 + size]
        assert at + 12 + size <= len(data)
        if kind == b'IHDR':
            width, height, depth, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', payload)
            assert (width, height, depth, color, compression, filtering, interlace) == (640, 400, 8, 6, 0, 0, 0)
        elif kind == b'IDAT':
            compressed.extend(payload)
        at += 12 + size
    if expected_solid is not None:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(compressed, (640 * 4 + 1) * 400 + 1)
        assert decoder.eof and not decoder.unconsumed_tail and len(raw) == (640 * 4 + 1) * 400
        previous = bytearray(640 * 4)
        for y in range(400):
            start = y * (640 * 4 + 1)
            filtering, row = raw[start], bytearray(raw[start + 1:start + 1 + 640 * 4])
            assert filtering <= 4
            for x in range(len(row)):
                a, b, c = row[x - 4] if x >= 4 else 0, previous[x], previous[x - 4] if x >= 4 else 0
                if filtering == 1: predictor = a
                elif filtering == 2: predictor = b
                elif filtering == 3: predictor = (a + b) // 2
                elif filtering == 4:
                    p = a + b - c
                    predictor = min(((abs(p - a), 0, a), (abs(p - b), 1, b), (abs(p - c), 2, c)))[2]
                else: predictor = 0
                row[x] = (row[x] + predictor) & 255
            assert row == bytes(expected_solid) * 640, (y, row[:4])
            previous = row
    return {'path': str(path), 'sha256': file_hash(path), 'bytes': len(data), 'width': width, 'height': height, 'solid_rgba': expected_solid}


def private_environment(inherited, run, evidence, runtime, scene):
    env = base.isolated_transport(inherited, run)
    for key in ('TOHRU_PRIVATE_BUS', 'TOHRU_LOCKER', 'TOHRU_SHELL', 'TOHRU_MANAGED_CONFIG', 'LISTEN_FDNAMES'):
        env.pop(key, None)
    env.update(HOME=str(evidence / 'home'), XDG_CONFIG_HOME=str(evidence / 'config'), XDG_CACHE_HOME=str(evidence / 'cache'),
               XDG_DATA_HOME=str(evidence / 'data'), XDG_DATA_DIRS=str(evidence / 'data'), XDG_STATE_HOME=str(evidence / 'state'),
               XDG_CURRENT_DESKTOP='sway', WLR_BACKENDS='headless', WLR_HEADLESS_OUTPUTS='1', WLR_LIBINPUT_NO_DEVICES='1',
               WLR_RENDERER='gles2', WLR_RENDER_DRM_DEVICE='/dev/dri/renderD128', WGPU_BACKEND='vulkan', PLEAMAR_BACKEND='vulkan',
               VK_DRIVER_FILES='/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json',
               TOHRU_RUNTIME=str(runtime.resolve()), TOHRU_SCENE=str(scene / 'bar.plm'))
    base.assert_private_transport(env, run)
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--pointer', type=Path, required=True)
    args = parser.parse_args()
    evidence = ROOT / '.tmp' / ('wallpaper-native-' + str(time.time_ns()))
    evidence.mkdir(parents=True)
    run = ROOT / '.tmp' / ('w' + str(os.getpid()))
    run.mkdir(mode=0o700)
    (run / 'ipc').mkdir(mode=0o700)
    for name in ['home', 'config/tohru', 'cache', 'data', 'state']:
        (evidence / name).mkdir(parents=True, mode=0o700)
    user = evidence / 'config/tohru/config.toml'
    user.write_text("# private test\n[wallpaper]\nenabled=true\nchange='never'\n[idle]\nlock_seconds=0\nscreen_off_unlocked_seconds=0\nscreen_off_locked_seconds=0\nsuspend_unlocked_seconds=0\nsuspend_locked_seconds=0\nfade_milliseconds=0\n")
    scene = evidence / 'scene'
    shutil.copytree(ROOT / 'shell', scene)
    base.install_focus_probe(scene)
    env = private_environment(os.environ, run, evidence, args.runtime, scene)
    (run / 'bus.conf').write_text(base.private_bus_config(run))
    (evidence / 'sway.conf').write_text('output HEADLESS-1 mode 1600x1000@60Hz\noutput HEADLESS-1 scale 1\noutput HEADLESS-1 bg #30232b solid_color\ndefault_border none\ndefault_floating_border none\nfor_window [app_id="dev.tohru.Settings"] floating enable, resize set 800 800, move position 400 100\n')
    logs = []
    def log(name):
        handle = (evidence / name).open('w'); logs.append(handle); return handle
    app = pointer = sway = bus = None
    events, assets = [], []
    result = {'result': 'failed', 'runtime': str(args.runtime.resolve()), 'runtime_wrapper_sha256': file_hash(args.runtime.resolve()), 'evidence': str(evidence), 'performance_acceptance': False}
    def command(argv, check=True):
        base.assert_private_transport(env, run)
        completed = subprocess.run(argv, env=env, text=True, capture_output=True, timeout=15, close_fds=True)
        events.append({'argv': [str(a) for a in argv], 'exit': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr})
        if check: assert completed.returncode == 0, events[-1]
        return completed.stdout.strip()
    def native(value): return command([str(args.runtime.resolve()), '--say', 'tohru', value])
    def cli(*values): return command([str(ROOT / 'bin/tohru'), *values])
    def snapshot(): return json.loads(cli('status', '--json'))['settings']
    def expect(key, value):
        return base.wait_for(lambda: (s if (s := snapshot())[key].get('value') == value else None), key + '=' + str(value), 15)
    def views(): return base.windows_for_app(json.loads(command(['swaymsg', '-t', 'get_tree', '-r'])), 'dev.tohru.Settings')
    def keyboard(*keys): command(['wtype', *keys])
    def click(x, y):
        assert pointer and pointer.poll() is None
        base.assert_private_transport(env, run)
        pointer.stdin.write(f'click {int(x * 1280 / 1600)} {int(y * 720 / 1000)}\n'); pointer.stdin.flush()
        time.sleep(.15)
    def window_click(x, y):
        rect = views()[0]['rect']; click(rect['x'] + x, rect['y'] + y)
    def launch(label):
        nonlocal app
        base.assert_private_transport(env, run)
        app = subprocess.Popen([str(ROOT / 'bin/tohru-run')], env=env, stdout=log('app-' + label + '.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: command([str(args.runtime.resolve()), '--say', 'tohru', 'get settings_status'], False).startswith('{'), 'private root IPC', 20)
    def asset(settings, solid=None):
        path = Path(settings['wallpaper.assignments']['value']['HEADLESS-1'])
        assert path.resolve().is_relative_to((evidence / 'cache').resolve())
        assets.append(png_info(path, solid)); return path
    try:
        bus = subprocess.Popen(['dbus-daemon', '--nofork', '--config-file=' + str(run / 'bus.conf')], env=env, stdout=log('bus.log'), stderr=subprocess.STDOUT, close_fds=True)
        base.wait_for(lambda: (run / 'bus').is_socket(), 'owned private bus')
        sway = subprocess.Popen(['sway', '--unsupported-gpu', '-d', '-c', str(evidence / 'sway.conf')], env=env, stdout=log('sway.log'), stderr=subprocess.STDOUT, close_fds=True)
        env['SWAYSOCK'] = str(base.wait_for(lambda: next(run.glob('sway-ipc.*.sock'), None), 'private Sway socket'))
        env['WAYLAND_DISPLAY'] = base.wait_for(lambda: next((p.name for p in run.glob('wayland-*') if not p.name.endswith('.lock')), None), 'private display')
        base.assert_private_transport(env, run)
        pointer = subprocess.Popen([str(args.pointer.resolve()), str(run)], env=env, stdin=subprocess.PIPE, stdout=log('pointer.log'), stderr=subprocess.STDOUT, text=True, close_fds=True)
        base.wait_for(lambda: 'ready' in (evidence / 'pointer.log').read_text(), 'owned pointer')
        launch('first')
        cli('settings', 'open')
        base.wait_for(lambda: len(views()) == 1 and views()[0]['focused'], 'focused Settings')
        window_click(100, 223)
        base.wait_for(lambda: native('get settings_page') == '4' and native('get wallpaper_picker_open') == 'true', 'UI Wallpaper category')
        window_click(744, 614)
        state = expect('wallpaper.preset', 'solid'); assert state['appearance.palette']['value'] == 'mocha'
        original = asset(state, [30, 30, 46, 255])
        window_click(100, 151)
        base.wait_for(lambda: native('get settings_page') == '2', 'UI Appearance category')
        base.focused_search_submit(native, keyboard, 'appearance.palette')
        state = expect('appearance.palette', 'macchiato'); changed = asset(state, [36, 39, 58, 255])
        assert original != changed and state['wallpaper.preset']['value'] == 'solid'
        persisted = tomllib.loads(user.read_text())
        assert persisted['appearance']['palette'] == 'macchiato' and persisted['wallpaper']['assignments']['HEADLESS-1'] == str(changed)
        native('fact runner_focus_probe_enabled false')
        command(['grim', '-o', 'HEADLESS-1', str(evidence / 'palette-solid.png')])
        keyboard('-M', 'ctrl', '-k', 'a', '-m', 'ctrl', '-k', 'BackSpace')
        base.wait_for(lambda: native('get settings_search') == '', 'search cleared by keyboard')
        native('fact runner_focus_probe_enabled true')
        base.focused_search_submit(native, keyboard, 'wallpaper.change')
        expect('wallpaper.change', 'hourly')
        keyboard('-k', 'Return'); expect('wallpaper.change', 'daily')
        keyboard('-k', 'Return'); expect('wallpaper.change', 'login')
        cli('wallpaper', 'rotate'); state = expect('wallpaper.preset', 'spectrum'); asset(state)
        assert native('get wallpaper_rotation_error') == ''
        cli('settings', 'close'); native('quit'); base.clean_exit(app, 'first root', events)
        launch('restart')
        state = expect('wallpaper.preset', 'dusk'); asset(state)
        assert state['appearance.palette']['value'] == 'macchiato' and state['wallpaper.change']['value'] == 'login'
        time.sleep(.25)
        command(['grim', '-o', 'HEADLESS-1', str(evidence / 'login-rotation.png')])
        native('quit'); base.clean_exit(app, 'restarted root', events)
        result.update(result='passed', checks=['real pointer Solid preset and assignment', 'real CtrlF/search/Return palette activation', '640x400 native PNG exact Solid pixels', 'persisted atomic palette and assignment', 'UI hourly/daily/login policy routes', 'manual batch route', 'actual initial-login timer after restart', 'intentional clean exits'])
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        result['assets'] = assets
        result['adapters'] = {name: [line for line in (evidence / name).read_text().splitlines() if any(token in line for token in ('NVIDIA', 'adapter', 'GLES2', 'Vulkan'))][-12:] for name in ('sway.log', 'app-first.log') if (evidence / name).exists()}
        errors = base.finish_private_run((app, pointer, sway, bus), (pointer.stdin if pointer else None, *logs),
            lambda: (evidence / 'events.json').write_text(json.dumps(events, indent=2) + '\n'))
        try: shutil.rmtree(run)
        except BaseException as error: errors.append(error)
        result['cleanup_errors'] = [repr(error) for error in errors]
        (evidence / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2), flush=True)
        if errors and sys.exc_info()[0] is None: raise BaseExceptionGroup('Owned cleanup failed', errors)


if __name__ == '__main__':
    main()
