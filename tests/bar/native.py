#!/usr/bin/env python3
"""Run only with coordinator's exclusive GPU slot; owns private Sway and D-Bus."""
import argparse
import json
import os
import secrets
import tempfile
from pathlib import Path
import subprocess
import sys
import time
sys.dont_write_bytecode = True
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]


def wait_for(operation, label, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = operation()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"Timed out: {label}")


def private_environment(inherited, evidence, runtime_dir):
    env = {**inherited, 'HOME': str(evidence / 'home'), 'XDG_RUNTIME_DIR': str(runtime_dir),
           **{f'XDG_{key}_HOME': str(evidence / key.lower()) for key in ('CONFIG', 'CACHE', 'DATA', 'STATE')},
           'WLR_BACKENDS': 'headless', 'WLR_HEADLESS_OUTPUTS': '1', 'WLR_LIBINPUT_NO_DEVICES': '1',
           'WLR_RENDERER': 'gles2', 'WLR_RENDER_DRM_DEVICE': '/dev/dri/renderD128',
           'VK_DRIVER_FILES': '/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json',
           'WGPU_BACKEND': 'vulkan', 'PLEAMAR_BACKEND': 'vulkan', 'PLEAMAR_GENERIC': '1'}
    env['PLEAMAR_SOCKETS'] = str(runtime_dir / 'ipc')
    # Native connectivity resolves the system bus separately from the session bus.
    # Default to an owned endpoint even before its optional fixture daemon exists.
    env['DBUS_SYSTEM_BUS_ADDRESS'] = 'unix:path=' + str(runtime_dir / 'system-bus')
    for key in ('WAYLAND_DISPLAY', 'SWAYSOCK', 'DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'TOHRU_RUNTIME', 'TOHRU_SCENE', 'TOHRU_MANAGED_CONFIG', 'WAYLAND_SOCKET', 'CUA_INJECT_SOCKET', 'TOHRU_B_PRIVATE_BUS', 'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE'):
        env.pop(key, None)
    return env


def assert_private_ipc(env, runtime_dir):
    expected = runtime_dir.absolute() / 'ipc'
    assert Path(env['PLEAMAR_SOCKETS']).absolute() == expected, 'IPC route escaped private directory'
    assert expected.resolve() == expected, 'Private IPC directory resolves outside its owner'
    system = runtime_dir.absolute() / 'system-bus'
    assert env['DBUS_SYSTEM_BUS_ADDRESS'] == 'unix:path=' + str(system), 'Outside system bus rejected'
    assert system.resolve() == system and not system.is_symlink(), 'System bus symlink rejected'
    if expected.exists():
        for endpoint in expected.iterdir():
            assert endpoint.resolve().parent == expected and not endpoint.is_symlink(), 'Outside IPC endpoint rejected'


def bootstrap_private_bus(child_dir, token, inherited, arguments, run=subprocess.run):
    # Only a fresh explicit child invocation can skip bootstrap; inherited markers
    # have no authority. The parent owns the bus and private provenance directory.
    if child_dir is not None:
        directory = Path(child_dir)
        assert directory.is_absolute() and directory.parent == ROOT / '.tmp'
        assert directory.name.startswith('b-bus-') and not directory.is_symlink()
        with (directory / 'token').open() as source:
            expected_token = source.read(65)
        assert token and secrets.compare_digest(expected_token, token), 'Private bus provenance rejected'
        address = inherited.get('DBUS_SESSION_BUS_ADDRESS', '')
        parts = [field.split('=', 1) for field in address.split(',')]
        assert all(len(field) == 2 for field in parts), 'Malformed private bus rejected'
        fields = dict(parts)
        assert len(fields) == len(parts), 'Duplicate session bus selectors rejected'
        assert fields.get('unix:path') == str(directory / 'bus'), 'Outside session bus rejected'
        assert set(fields) <= {'unix:path', 'guid'} and ';' not in address, 'Alternate session bus rejected'
        return False
    assert token is None, 'Private bus token without child provenance'
    ROOT.joinpath('.tmp').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='b-bus-', dir=ROOT / '.tmp') as temporary:
        directory = Path(temporary)
        private_token = secrets.token_hex(32)
        (directory / 'token').write_text(private_token)
        (directory / 'token').chmod(0o600)
        config = directory / 'bus.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:path=' + escape(str(directory / 'bus')) + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        clean = dict(inherited)
        for key in ('TOHRU_B_PRIVATE_BUS', 'DBUS_SESSION_BUS_ADDRESS', 'DBUS_SYSTEM_BUS_ADDRESS', 'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE', 'WAYLAND_SOCKET', 'CUA_INJECT_SOCKET'):
            clean.pop(key, None)
        run(['dbus-run-session', '--config-file', str(config), '--', sys.executable, __file__, *arguments,
             '--bus-child', str(directory), '--bus-token', private_token], env=clean, check=True, close_fds=True)
    return True


def finish_owned(processes, logs, evidence, events):
    # Ownership cleanup must finish before evidence writing can fail. Each owned
    # process/log gets its attempt even if another cleanup operation fails.
    failures = []
    for process in reversed(processes):
        try:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        except Exception as error:
            failures.append(error)
    for log in logs:
        try:
            log.close()
        except Exception as error:
            failures.append(error)
    try:
        (evidence / 'events.json').write_text(json.dumps(events, indent=2) + '\n')
    except Exception as error:
        failures.append(error)
    finally:
        print(evidence, flush=True)
    if failures:
        raise ExceptionGroup('Native test cleanup/evidence failures', failures)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--review-only', action='store_true')
    parser.add_argument('--launcher-only', action='store_true')
    parser.add_argument('--icons-only', action='store_true')
    parser.add_argument('--bus-child', type=Path, help=argparse.SUPPRESS)
    parser.add_argument('--bus-token', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if bootstrap_private_bus(args.bus_child, args.bus_token, os.environ, sys.argv[1:]):
        return
    evidence = ROOT / '.tmp' / f'b-native-{time.time_ns()}'
    evidence.mkdir()
    runtime_dir = ROOT / '.tmp' / f'b{os.getpid()}'
    runtime_dir.mkdir(mode=0o700)
    (evidence / 'home').mkdir()
    env = private_environment(os.environ, evidence, runtime_dir)
    env['TOHRU_B_PRIVATE_BUS'] = '1'  # Set only after explicit child/address validation.
    Path(env['PLEAMAR_SOCKETS']).mkdir(mode=0o700)
    sway_config = evidence / 'sway.conf'
    sway_config.write_text('output HEADLESS-1 mode 1280x720@60Hz\noutput HEADLESS-1 scale 1\noutput HEADLESS-1 bg #30232b solid_color\nseat seat0 hide_cursor 100\n')
    processes, logs, events = [], [], []
    private_session = env['DBUS_SESSION_BUS_ADDRESS']
    runtime = str(args.runtime.resolve())

    def start(command, label, extra=None, stdin=None):
        log = (evidence / f'{label}.log').open('w')
        logs.append(log)
        child_env = {**env, **(extra or {})}
        assert_private_ipc(child_env, runtime_dir)
        assert child_env.get('DBUS_SESSION_BUS_ADDRESS') == private_session, 'Outside session bus override rejected'
        process = subprocess.Popen(command, env=child_env, stdout=log, stderr=subprocess.STDOUT, close_fds=True, stdin=stdin)
        processes.append(process)
        return process

    def say(scene, command):
        assert_private_ipc(env, runtime_dir)
        assert env.get('DBUS_SESSION_BUS_ADDRESS') == private_session, 'Outside session bus override rejected'
        result = subprocess.run([runtime, '--say', scene, command], env=env, text=True, capture_output=True, timeout=10, close_fds=True)
        events.append({'scene': scene, 'command': command, 'exit': result.returncode, 'out': result.stdout, 'err': result.stderr})
        return result.stdout.strip() if result.returncode == 0 else None

    def expect(scene, name, expected):
        wait_for(lambda: say(scene, 'get ' + name) == str(expected), f'{scene}.{name}={expected}')

    def launch(scene, name, mouse=None, debug=False):
        command = [runtime, '--scene', str(scene), '--answer-as', name, '--no-hud', '--stall', '0']
        if mouse:
            command += ['--mouse', mouse]
        process = start(command, name, {'WAYLAND_DEBUG': '1'} if debug else None)
        wait_for(lambda: say(name, 'get cal_open' if name.startswith('calendar') else 'get bar_time'), name)
        assert 'NVIDIA GeForce RTX 5090' in (evidence / f'{name}.log').read_text(), 'app GPU unverified'
        return process

    try:
        start(['sway', '--unsupported-gpu', '-d', '-c', str(sway_config)], 'sway')
        socket = wait_for(lambda: next(runtime_dir.glob('sway-ipc.*.sock'), None), 'private Sway')
        display = wait_for(lambda: next((p for p in runtime_dir.glob('wayland-*') if not p.name.endswith('.lock')), None), 'private display')
        env.update(SWAYSOCK=str(socket), WAYLAND_DISPLAY=display.name)
        assert 'NVIDIA GeForce RTX 5090' in (evidence / 'sway.log').read_text(), 'compositor GPU unverified'
        if args.launcher_only:
            import importlib.util
            spec = importlib.util.spec_from_file_location('launcher_check', ROOT / 'tests/launcher/native.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            module.verify(ROOT, runtime, evidence, env, start, say, expect, wait_for)
            return
        if args.review_only or args.icons_only:
            from review import verify
            verify(ROOT, runtime, evidence, env, start, say, expect, wait_for, icons_only=args.icons_only)
            return
        launch(ROOT / 'shell/bar/entry.plm', 'bar', debug=True)
        wait_for(lambda: say('bar', 'get bar_date'), 'native clock')
        expect('bar', 'bar_mooncoil', 'false')
        expect('bar', 'bar_playing', 'false')
        expect('bar', 'bar_title', '')
        say('bar', 'fact bar_mooncoil 1')
        expect('bar', 'bar_mooncoil', 'true')
        player_ready = evidence / 'player-ready'
        start([sys.executable, str(ROOT / 'tests/bar/mpris.py'), str(player_ready)], 'mpris')
        wait_for(player_ready.exists, 'private MPRIS name')
        expect('bar', 'bar_playing', 'true')
        expect('bar', 'bar_media', '▶ Native track - Native artist')
        window_scene = evidence / 'window.plm'
        window_scene.write_text('scene TestWindow { surface { kind: window; size: 480, 180; title: "B Native Window" } box { from: 0, 0; size: 480, 180; color: #181825 } }')
        window = start([runtime, '--scene', str(window_scene), '--stall', '0', '--no-hud'], 'window')
        expect('bar', 'bar_title', 'B Native Window')
        tree = json.loads(subprocess.check_output(['swaymsg', '-t', 'get_tree', '-r'], env=env, text=True))
        def app_id(node):
            if node.get('name') == 'B Native Window':
                return node.get('app_id')
            for child in node.get('nodes', []) + node.get('floating_nodes', []):
                found = app_id(child)
                if found:
                    return found
            return None
        identity = app_id(tree)
        assert identity, 'test window app identity missing'
        apps = Path(env['XDG_DATA_HOME']) / 'applications'
        apps.mkdir(parents=True)
        (apps / 'tohru-test.desktop').write_text(f'[Desktop Entry]\nType=Application\nName=B Native Window\nExec=true\nStartupWMClass={identity}\nIcon=application-x-executable\n')
        expect('bar', 'bar_has_icon', 'true')
        expect('bar', 'bar_icon', 'application-x-executable')
        subprocess.run(['grim', '-o', 'HEADLESS-1', str(evidence / 'bar.png')], env=env, check=True, timeout=10)
        subprocess.run(['gdbus', 'call', '--session', '--dest', 'org.mpris.MediaPlayer2.tohru_test', '--object-path', '/org/mpris/MediaPlayer2', '--method', 'org.tohru.TestPlayer.SetPlaying', 'false'], env=env, check=True, capture_output=True, text=True, timeout=5)
        expect('bar', 'bar_playing', 'false')
        window.terminate()
        window.wait(timeout=10)
        say('bar', 'quit')
        processes[1].wait(timeout=10)
        launch(ROOT / 'shell/calendar/entry.plm', 'calendar')
        say('calendar', 'emit calendar_open')
        expect('calendar', 'cal_open', 'true')
        initial = say('calendar', 'get cal_title')
        say('calendar', 'emit calendar_month 2')
        expect('calendar', 'cal_month', '2')
        # The native clock's current year is 2026 in this fixture run.
        current_year = int(initial.split()[-1])
        import calendar
        lead, count = calendar.monthrange(current_year, 2)
        expected_weeks = ((lead + 1) % 7 + count + 6) // 7
        expect('calendar', 'cal_weeks', str(expected_weeks))
        say('calendar', 'emit calendar_previous')
        expect('calendar', 'cal_month', '1')
        say('calendar', 'emit calendar_previous')
        expect('calendar', 'cal_title', f'December {current_year - 1}')
        say('calendar', 'emit calendar_next')
        expect('calendar', 'cal_title', f'January {current_year}')
        say('calendar', 'emit calendar_month 13')
        expect('calendar', 'cal_month', '1')
        say('calendar', 'emit calendar_today')
        expect('calendar', 'cal_title', initial)
        subprocess.run(['grim', '-o', 'HEADLESS-1', str(evidence / 'calendar.png')], env=env, check=True, timeout=10)
        say('calendar', 'emit calendar_close')
        expect('calendar', 'cal_open', 'false')
        say('calendar', 'quit')
        processes[-1].wait(timeout=10)
        # Fake input goes through the native scene input path, not controller calls.
        launch(ROOT / 'shell/calendar/entry.plm', 'calendar-input', '1060,16@400 click@600 key:Escape@2500')
        wait_for(lambda: say('calendar-input', 'get cal_open') == 'true', 'native click opens calendar', 2)
        expect('calendar-input', 'cal_open', 'false')
        say('calendar-input', 'quit')
        processes[-1].wait(timeout=10)
        registry = '\n'.join(line for line in (evidence / 'bar.log').read_text().splitlines() if 'wl_registry' in line and 'global(' in line)
        (evidence / 'registry.txt').write_text(registry + '\n')
        summary = {'result': 'passed', 'app_gpu': 'NVIDIA GeForce RTX 5090 / Vulkan', 'compositor_gpu': 'NVIDIA GeForce RTX 5090 / GLES2', 'sway_ext_workspace': 'ext_workspace_manager_v1' in registry, 'sway_wlr_toplevel': 'zwlr_foreign_toplevel_manager_v1' in registry, 'checks': ['native bar clock', 'Mooncoil default off', 'native empty MPRIS state', 'MPRIS properties/play/pause', 'foreign toplevel title', 'native catalog icon matching', 'calendar open/month/year/Today/close', 'native click and Escape'], 'parity_acceptance': False, 'performance_acceptance': False}
        (evidence / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    finally:
        finish_owned(processes, logs, evidence, events)


if __name__ == '__main__':
    main()
