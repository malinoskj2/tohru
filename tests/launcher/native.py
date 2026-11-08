"""Scoped native launcher entry/input/catalog/launch/persistence workload."""
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import time


def verify(root, runtime, evidence, env, start, say, expect, wait_for):
    scene_dir = evidence / 'scene'
    shutil.copytree(root / 'shell/launcher', scene_dir)
    apps = Path(env['XDG_DATA_HOME']) / 'applications'
    apps.mkdir(parents=True)
    icon = evidence / 'icon.svg'
    icon.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="44" height="44"><rect x="2" y="2" width="40" height="40" rx="8" fill="#89dceb"/><circle cx="22" cy="22" r="10" fill="#181825"/></svg>')
    marker = evidence / 'launched.txt'
    executable = evidence / 'record.sh'
    executable.write_text('#!/bin/sh\nprintf "%s\\n" "$2" >> "$1"\n')

    def command(label):
        return ' '.join(shlex.quote(part) for part in ('/bin/sh', str(executable), str(marker), label))

    def desktop(identifier, name, category, label, actions=False):
        contents = f'[Desktop Entry]\nType=Application\nName={name}\nExec={command(label)}\nIcon={icon}\nCategories={category};\n'
        if actions:
            contents += f'Actions=New;\n[Desktop Action New]\nName=New Window\nExec={command("action")}\n'
        (apps / f'{identifier}.desktop').write_text(contents)

    desktop('art', 'Art Studio', 'Graphics', 'art', True)
    desktop('code', 'Code Editor', 'Development', 'code')
    for index in range(12):
        desktop(f'dummy{index:02}', f'Dummy App {index:02}', 'Utility', f'dummy{index:02}')

    def launch(label):
        process = start([runtime, '--scene', str(scene_dir / 'entry.plm'), '--answer-as', 'launcher', '--no-hud', '--stall', '0'], label)
        wait_for(lambda: say('launcher', 'get launcher_total') == '14', 'native desktop catalog')
        say('launcher', 'emit launcher_open')
        expect('launcher', 'launcher_visible', 'true')
        wait_for(lambda: 'NVIDIA GeForce RTX 5090' in (evidence / f'{label}.log').read_text(), 'launcher hardware adapter')
        return process

    def input_query(query):
        say('launcher', 'key Ctrl+a')
        say('launcher', 'key BackSpace')
        if query:
            result = say('launcher', 'type launcher_query ' + query)
            assert result is not None, 'native field input refused'
        expect('launcher', 'launcher_query', query)

    first = launch('launcher-first')
    input_query('co')
    expect('launcher', 'launcher_total', '1')
    expect('launcher', 'launcher_tiles.0.name', 'Code Editor')
    say('launcher', 'key Escape')
    expect('launcher', 'launcher_query', '')
    expect('launcher', 'launcher_visible', 'true')
    expect('launcher', 'launcher_total', '14')
    say('launcher', 'key Tab')
    expect('launcher', 'launcher_tab', '1')
    expect('launcher', 'launcher_total', '1')
    say('launcher', 'key Shift+ISO_Left_Tab')
    expect('launcher', 'launcher_tab', '0')
    input_query('dummy')
    expect('launcher', 'launcher_total', '12')
    say('launcher', 'key Down')
    expect('launcher', 'launcher_selected', '8')
    say('launcher', 'key Return')
    expect('launcher', 'launcher_visible', 'false')
    wait_for(lambda: marker.exists() and marker.read_text().strip() == 'dummy08', 'native launch execution')
    say('launcher', 'emit launcher_open')
    input_query('art')
    expect('launcher', 'launcher_total', '1')
    description = say('launcher', 'describe json')
    (evidence / 'launcher-description.json').write_text(description or '')
    say('launcher', 'key Menu')
    expect('launcher', 'launcher_menu', 'true')
    expect('launcher', 'launcher_choices.2.label', 'Pin')
    say('launcher', 'key Down')
    say('launcher', 'key Down')
    expect('launcher', 'launcher_menu_selected', '2')
    say('launcher', 'key Return')
    expect('launcher', 'launcher_menu', 'false')
    say('launcher', 'key Escape')
    expect('launcher', 'launcher_pins', '1')
    expect('launcher', 'launcher_tiles.0.id', 'art')
    # Use the real desktop action path, not app launch directly from the test.
    say('launcher', 'key Shift+F10')
    expect('launcher', 'launcher_menu', 'true')
    say('launcher', 'key Down')
    expect('launcher', 'launcher_menu_selected', '1')
    say('launcher', 'key Return')
    expect('launcher', 'launcher_visible', 'false')
    wait_for(lambda: marker.read_text().splitlines() == ['dummy08', 'action'], 'native desktop action execution')
    say('launcher', 'emit launcher_open')
    expect('launcher', 'launcher_pins', '1')
    time.sleep(0.15)
    subprocess.run(['grim', '-o', 'HEADLESS-1', str(evidence / 'launcher-grid.png')], env=env, check=True, timeout=10)
    say('launcher', 'quit')
    first.wait(timeout=10)
    second = launch('launcher-restart')
    expect('launcher', 'launcher_pins', '1')
    expect('launcher', 'launcher_tiles.0.id', 'art')
    state = Path(env['XDG_DATA_HOME']) / 'pleamar/entry/launcher.json'
    saved = json.loads(state.read_text())
    assert saved['counts']['dummy08'] == 1 and saved['counts']['art'] == 1
    assert saved['recent'][:2] == ['art', 'dummy08']
    assert saved['pins'] == ['art']
    say('launcher', 'key Escape')
    expect('launcher', 'launcher_visible', 'false')
    say('launcher', 'quit')
    second.wait(timeout=10)
    (evidence / 'summary.json').write_text(json.dumps({'result': 'passed', 'checks': ['private native launcher entry', 'native catalog icons', 'native search field simple fuzzy', 'Escape clears then closes', 'actual Tab/ISO_Left_Tab aliases', 'Down result row and Enter launch', 'native pin/menu/action input', 'native scene-local usage across restart', 'Sway without optional compositor plugin'], 'app_gpu': 'NVIDIA GeForce RTX 5090 / Vulkan', 'compositor_gpu': 'NVIDIA GeForce RTX 5090 / GLES2', 'horizontal_key_scope': 'native caret behavior pending decision, no parity claim', 'metadata_gap_scope': 'unchanged pending decision', 'performance_acceptance': False, 'parity_acceptance': False}, indent=2) + '\n')
