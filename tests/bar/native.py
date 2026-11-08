#!/usr/bin/env python3
"""Run only with coordinator's exclusive GPU slot; owns private Sway and D-Bus."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('TOHRU_B_PRIVATE_BUS') != '1':
        ROOT.joinpath('.tmp').mkdir(exist_ok=True)
        config = ROOT / '.tmp' / f'b-bus-{os.getpid()}.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' + escape(str(ROOT / '.tmp')) + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        try:
            subprocess.run(['dbus-run-session', '--config-file', str(config), '--', sys.executable, __file__, *sys.argv[1:]], env={**os.environ, 'TOHRU_B_PRIVATE_BUS': '1'}, check=True)
        finally:
            config.unlink()
        return
    evidence = ROOT / '.tmp' / f'b-native-{time.time_ns()}'
    evidence.mkdir()
    runtime_dir = ROOT / '.tmp' / f'b{os.getpid()}'
    runtime_dir.mkdir(mode=0o700)
    (evidence / 'home').mkdir()
    env = {**os.environ, 'HOME': str(evidence / 'home'), 'XDG_RUNTIME_DIR': str(runtime_dir),
           **{f'XDG_{key}_HOME': str(evidence / key.lower()) for key in ('CONFIG', 'CACHE', 'DATA', 'STATE')},
           'WLR_BACKENDS': 'headless', 'WLR_HEADLESS_OUTPUTS': '1', 'WLR_LIBINPUT_NO_DEVICES': '1',
           'WLR_RENDERER': 'gles2', 'WLR_RENDER_DRM_DEVICE': '/dev/dri/renderD128',
           'VK_DRIVER_FILES': '/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json',
           'WGPU_BACKEND': 'vulkan', 'PLEAMAR_BACKEND': 'vulkan', 'PLEAMAR_GENERIC': '1'}
    for key in ('WAYLAND_DISPLAY', 'SWAYSOCK', 'DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE'):
        env.pop(key, None)
    sway_config = evidence / 'sway.conf'
    sway_config.write_text('output HEADLESS-1 mode 1280x720@60Hz\noutput HEADLESS-1 scale 1\noutput HEADLESS-1 bg #30232b solid_color\nseat seat0 hide_cursor 100\n')
    processes, logs, events = [], [], []
    runtime = str(args.runtime.resolve())

    def start(command, label, extra=None):
        log = (evidence / f'{label}.log').open('w')
        logs.append(log)
        process = subprocess.Popen(command, env={**env, **(extra or {})}, stdout=log, stderr=subprocess.STDOUT)
        processes.append(process)
        return process

    def say(scene, command):
        result = subprocess.run([runtime, '--say', scene, command], env=env, text=True, capture_output=True, timeout=10)
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
        (evidence / 'events.json').write_text(json.dumps(events, indent=2) + '\n')
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        for log in logs:
            log.close()
        print(evidence, flush=True)


if __name__ == '__main__':
    main()
