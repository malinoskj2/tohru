"""Focused Q fixes, same-root popup and actual VM reload stability checks."""
from collections import Counter
import json
from pathlib import Path
import shutil
import subprocess
import time


def verify(root, runtime, evidence, env, start, say, expect, wait_for, icons_only=False):
    scene_dir = evidence / 'scene'
    shutil.copytree(root / 'shell', scene_dir)
    scene = scene_dir / 'composed.plm'
    if icons_only:
        # Three affected bar-only SVG captures do not need calendar/scalar patch.
        scene.write_text('scene BarIcons { surface { size: full, 31; anchor: top; screens: all } permissions { services: "window", "apps", "media", "clock" } include "bar/state.plm" box { from: 100, 0; size: screen.width - 200, 31; corner: 10; color: #160e14; opacity: 70% } include "bar/contents.plm" }')
        logic = scene_dir / 'composed.luau'
        logic.write_text('require("bar/controller")()\n')
    else:
        scene.write_text('''scene ComposedBar {
    surface { size: full, 31; anchor: top; screens: all; reserve: 31; keyboard: exclusive while cal_open }
    permissions { services: "window", "apps", "media", "clock" }
    include "bar/state.plm"
    include "calendar/state.plm"
    box { from: 100, 0; size: screen.width - 200, 31; corner: 10; color: #160e14; opacity: 70% }
    include "bar/contents.plm"
    include "calendar/popup.plm"
}''')
        logic = scene_dir / 'composed.luau'
        logic.write_text('require("bar/controller")()\nlocal rail = require("calendar/rail-publisher")(tohru_workspace,"calendar","cal_rail_")\nrequire("calendar/controller")(rail)\n')
    app = start([runtime, '--scene', str(scene), '--answer-as', 'review', '--stall', '0', '--no-hud'], 'review', {'WAYLAND_DEBUG': '1'})
    wait_for(lambda: say('review', 'get bar_date'), 'composed native clock')
    if not icons_only:
        expect('review', 'cal_open', 'false')
    apps = Path(env['XDG_DATA_HOME']) / 'applications'
    apps.mkdir(parents=True)
    (apps / 'org.example.Review.desktop').write_text(f'[Desktop Entry]\nType=Application\nName=Review\nExec=true\nIcon={scene_dir / "bar/mooncoil.svg"}\n')
    titles = [('short-icon', 'Hi', 'Review'), ('long-icon', ('A very long focused window title ' * 10).strip(), 'Review'), ('empty-icon', '', 'Review'), ('short-no-icon', 'Hi', 'org.example.Unmatched'), ('long-no-icon', ('A very long focused window title ' * 10).strip(), 'org.example.Unmatched'), ('empty-no-icon', '', 'org.example.Unmatched')]
    if icons_only:
        titles = [case for case in titles if case[2] == 'Review']
    measurements = []
    for label, title, identity in titles:
        window_scene = evidence / f'{label}.plm'
        window_scene.write_text(f'scene Window {{ surface {{ kind: window; size: 480, 180; title: {json.dumps(title or "Empty title fixture")}; app_id: {json.dumps(identity)} }} box {{ from: 0, 0; size: 480, 180; color: #181825 }} }}')
        window = start([runtime, '--scene', str(window_scene), '--answer-as', label, '--no-hud', '--stall', '0'], label)
        wait_for(lambda: say(label, 'get screen.count'), label)
        if not title:
            expect('review', 'bar_title', 'Empty title fixture')
            say('review', 'text bar_title ')
        expect('review', 'bar_title', title)
        expect('review', 'bar_has_icon', 'true' if identity == 'Review' else 'false')
        # Native layout settles within a few frames; get measures as well as capture.
        time.sleep(0.15)
        measured = say('review', 'get bar_title_measure.width')
        measurements.append({'case': label, 'title_measure_width': measured})
        if title == 'Hi':
            assert 0 < float(measured) < 40, measured
        elif not title:
            assert float(measured) == 0, measured
        else:
            assert float(measured) <= (340 if identity == 'Review' else 360), measured
        subprocess.run(['grim', '-o', 'HEADLESS-1', str(evidence / f'{label}.png')], env=env, check=True, timeout=10)
        if identity == 'Review':
            from PIL import Image
            import math
            left = (1280 - min(360, float(measured) + 20)) / 2
            with Image.open(evidence / f'{label}.png') as captured:
                rgb = captured.convert('RGB')
                pixels = rgb.load()
                white = sum(1 for y in range(8, 24) for x in range(math.floor(left), math.ceil(left + 14)) if min(pixels[x, y]) > 230)
            assert white > 5, 'absolute source SVG icon did not render in the title icon cell'
            measurements[-1]['white_icon_pixels'] = white

        window.terminate()
        window.wait(timeout=10)
        wait_for(lambda: say('review', 'get bar_has_icon') == 'false', 'window closed')
    assert 'NVIDIA GeForce RTX 5090' in (evidence / 'review.log').read_text()
    if icons_only:
        (evidence / 'title-measures.json').write_text(json.dumps(measurements, indent=2) + '\n')
        (evidence / 'summary.json').write_text(json.dumps({'result': 'passed', 'checks': ['only affected short/long/empty title icon cells', 'fixture-owned absolute source SVG pixels'], 'app_gpu': 'NVIDIA GeForce RTX 5090 / Vulkan', 'compositor_gpu': 'NVIDIA GeForce RTX 5090 / GLES2', 'performance_acceptance': False, 'parity_acceptance': False}, indent=2) + '\n')
        say('review', 'quit')
        app.wait(timeout=10)
        return
    # Keep the virtual pointer alive until the app binds its no-input seat.
    # A one-shot wlrctl device can disappear before wl_pointer is bound.
    pointer = start([str(root / '.tmp/pointer-build/pointer'), env['XDG_RUNTIME_DIR']], 'pointer', stdin=subprocess.PIPE)
    wait_for(lambda: 'ready' in (evidence / 'pointer.log').read_text(), 'persistent private pointer')
    wait_for(lambda: '.get_pointer(' in (evidence / 'review.log').read_text(), 'app pointer binding')
    pointer.stdin.write(b'click 1060 16\n')
    pointer.stdin.flush()
    expect('review', 'cal_open', 'true')
    wait_for(lambda: 'xdg_popup' in (evidence / 'review.log').read_text() and '.grab(' in (evidence / 'review.log').read_text(), 'native popup grab')
    subprocess.run(['grim', '-o', 'HEADLESS-1', str(evidence / 'composed-calendar.png')], env=env, check=True, timeout=10)
    pointer.stdin.write(b'click 400 300\n')
    pointer.stdin.flush()
    expect('review', 'cal_open', 'false')

    def resources():
        proc = Path('/proc') / str(app.pid)
        names = Counter(path.read_text().strip() for path in (proc / 'task').glob('*/comm'))
        return {'threads': sum(names.values()), 'thread_names': dict(sorted(names.items())), 'fds': len(list((proc / 'fd').iterdir()))}

    # Warm both controllers, popup and GPU resources before taking the baseline.
    samples = [resources()]
    original = logic.read_text()
    for iteration in range(4):
        loads = (evidence / 'review.log').read_text().count('composed.luau running')
        logic.write_text(original + f'-- reload {iteration}\n')
        wait_for(lambda: (evidence / 'review.log').read_text().count('composed.luau running') > loads, 'actual logic reload')
        wait_for(lambda: say('review', 'get bar_date') and say('review', 'get cal_title'), 'cached service replay')
        time.sleep(0.1)
        samples.append(resources())
    baseline = samples[0]
    for sample in samples[1:]:
        for name in ('apps', 'clock', 'media'):
            assert sample['thread_names'].get(name) == baseline['thread_names'].get(name), samples
        assert sample['threads'] <= baseline['threads'] + 2, samples
        assert sample['fds'] <= baseline['fds'] + 2, samples
    # Both feature controllers share one raw native clock subscription in this VM.
    assert baseline['thread_names'].get('clock') == 1, baseline
    (evidence / 'title-measures.json').write_text(json.dumps(measurements, indent=2) + '\n')
    (evidence / 'reload-resources.json').write_text(json.dumps(samples, indent=2) + '\n')
    (evidence / 'summary.json').write_text(json.dumps({'result': 'passed', 'checks': ['Q title short/long/empty with/without icons', 'same-root real click popup grab', 'real outside click closes popup', 'four actual VM reloads', 'one shared clock thread', 'native service thread/FD plateau'], 'app_gpu': 'NVIDIA GeForce RTX 5090 / Vulkan', 'compositor_gpu': 'NVIDIA GeForce RTX 5090 / GLES2', 'performance_acceptance': False, 'parity_acceptance': False}, indent=2) + '\n')
    pointer.stdin.write(b'quit\n')
    pointer.stdin.flush()
    pointer.wait(timeout=5)
    say('review', 'quit')
    app.wait(timeout=10)
