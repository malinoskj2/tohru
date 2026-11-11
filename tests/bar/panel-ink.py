"""Panel ink: the patched `bright:` popup property, the panels that use it, and j2bar's 55%/45% hysteresis.

The parser and scene checks need only the runtime. The hysteresis run opens a small scene on
$WAYLAND_DISPLAY and drives `panel_bright` with `--say`, so it needs a running compositor.
"""
import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--runtime', type=Path, required=True)
parser.add_argument('--no-display', action='store_true', help='skip the hysteresis run')
args = parser.parse_args()
runtime = str(args.runtime.resolve())
root = Path(__file__).resolve().parents[2]
shell = root / 'shell'
(root / '.tmp').mkdir(exist_ok=True)


def check(path):
    return subprocess.run([runtime, '--check', str(path)], text=True, capture_output=True, timeout=30, check=False)


with tempfile.TemporaryDirectory(prefix='panel-ink-', dir=root / '.tmp') as directory:
    directory = Path(directory)
    cases = [
        ('fact', 'fact seen = 0', 'bright: seen', True, ''),
        ('none', '', '', True, ''),
        ('unknown', '', 'bright: nowhere', False, 'no fact'),
    ]
    for name, declaration, bright, ok, message in cases:
        path = directory / f'{name}.plm'
        path.write_text('scene Bright {\n surface { size: 300, 40 }\n fact shown = false\n ' + declaration
                        + '\n popup p {\n  at: 0, 40\n  size: 100, 100\n  open: shown\n  ' + bright
                        + '\n  body { color: #181825; glass: 72%\n   box { from: 0, 0; size: 100, 100; corner: 12 }\n  }\n }\n}\n')
        result = check(path)
        assert (result.returncode == 0) == ok, (name, result.stdout, result.stderr)
        assert message in result.stdout + result.stderr, (name, result.stdout, result.stderr)

scenes = ['bar.plm', 'session/menu.plm'] + [f'{f}/entry.plm' for f in ('calendar', 'network', 'bluetooth', 'info', 'about')]
for scene in scenes:
    result = check(shell / scene)
    assert result.returncode == 0, (scene, result.stdout, result.stderr)

# Every panel tells its brightness, and none keeps a dark-only colour of its own.
popups = {'bar.plm': 1, 'calendar/popup.plm': 1, 'network/popup.plm': 1, 'bluetooth/popup.plm': 1,
          'info/popup.plm': 1, 'about/popup.plm': 1, 'session/menu.plm': 1}
for name, count in popups.items():
    text = (shell / name).read_text()
    assert len(re.findall(r'\bbright: panel_bright\b', text)) == count, name
panel_files = ['sound/panel.plm', 'sound/details.plm', 'calendar/popup.plm', 'network/popup.plm',
               'bluetooth/popup.plm', 'info/popup.plm', 'info/content.plm', 'about/popup.plm']
for name in panel_files:
    literals = re.findall(r'#[0-9a-fA-F]{6}\b', (shell / name).read_text())
    assert not literals, (name, literals)
print('Patched bright: parser contract and the panels using it passed')

if args.no_display:
    raise SystemExit(0)
assert os.environ.get('WAYLAND_DISPLAY'), 'the hysteresis run needs a compositor: set WAYLAND_DISPLAY or pass --no-display'
role = f'tohru-panel-ink-test-{os.getpid()}'
with tempfile.TemporaryDirectory(prefix='panel-ink-run-', dir=root / '.tmp') as directory:
    scene = Path(directory) / 'ink.plm'
    scene.write_text(f'scene Ink {{\n surface {{ size: 40, 40; anchor: top_left }}\n include "{shell / "panel-ink.plm"}"\n'
                     ' box { from: 0, 0; size: 40, 40; color: ink_body }\n}\n')
    process = subprocess.Popen([runtime, '--scene', str(scene), '--no-hud', '--stall', '0', '--answer-as', role],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def say(*commands):
        result = subprocess.run([runtime, '--say', role, *commands], text=True, capture_output=True, timeout=10, check=False)
        assert result.returncode == 0, (commands, result.stdout, result.stderr)
        return result.stdout.strip()

    def light_after(value):
        say(f'fact panel_bright {value}')
        time.sleep(0.15)
        return say('get panel_light') == 'true'

    try:
        for _ in range(100):
            if subprocess.run([runtime, '--say', role, 'get panel_light'], capture_output=True, timeout=10).returncode == 0:
                break
            time.sleep(0.1)
        steps = [(0.5, False), (0.55, False), (0.56, True), (0.5, True), (0.45, True), (0.44, False), (0.54, False), (1, True), (0, False)]
        for value, light in steps:
            assert light_after(value) == light, (value, light)
        say('fact panel_bright 1')
        time.sleep(0.6)
        assert float(say('get panel_level')) > 0.99
        say('quit')
        process.wait(timeout=10)
    finally:
        if process.poll() is None:
            process.kill()
print('Panel ink turns light above 55% bright and dark below 45%, and holds in between')
