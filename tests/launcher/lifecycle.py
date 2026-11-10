"""Finite private launcher role/input/topology workload; requires allocated GPU.

Only the runner imports verify. Standalone --cpu checks protocol evidence parsing.
"""
import json
from pathlib import Path
import re
import shutil
import shlex
import subprocess
import time

NAMESPACE='tohru-launcher-backdrop'
CREATE=re.compile(r'get_layer_surface\(new id zwlr_layer_surface_v1[#@](\d+), wl_surface[#@](\d+),.*?, "([^"]+)"\)')
DESTROY=re.compile(r'zwlr_layer_surface_v1[#@](\d+)\.destroy\(')


def layers(log):
    live={}
    for line in log.splitlines():
        created=CREATE.search(line)
        if created:
            role,surface,namespace=created.groups()
            live[role]={'surface':surface,'namespace':namespace}
        destroyed=DESTROY.search(line)
        if destroyed:live.pop(destroyed.group(1),None)
    return list(live.values())


def empty_inputs(log):
    # wl_surface input state is double-buffered. set_input_region copies the
    # region now, but only commit applies it; object IDs can later be reused.
    regions,pending,empty={},{},set()
    for line in log.splitlines():
        created_surface=re.search(r'create_surface\(new id wl_surface[#@](\d+)\)',line)
        destroyed_surface=re.search(r'wl_surface[#@](\d+)\.destroy\(',line)
        for surface in (created_surface,destroyed_surface):
            if surface:
                pending.pop(surface.group(1),None)
                empty.discard(surface.group(1))
        created=re.search(r'create_region\(new id wl_region[#@](\d+)\)',line)
        if created:regions[created.group(1)]=True
        changed=re.search(r'wl_region[#@](\d+)\.(?:add|subtract)\(',line)
        if changed:regions[changed.group(1)]=False
        assigned=re.search(r'wl_surface[#@](\d+)\.set_input_region\(wl_region[#@](\d+)\)',line)
        if assigned:
            surface,region=assigned.groups()
            pending[surface]=regions.get(region) is True
        reset=re.search(r'wl_surface[#@](\d+)\.set_input_region\((?:nil|null)\)',line)
        if reset:pending[reset.group(1)]=False
        destroyed_region=re.search(r'wl_region[#@](\d+)\.destroy\(',line)
        if destroyed_region:regions.pop(destroyed_region.group(1),None)
        committed=re.search(r'wl_surface[#@](\d+)\.commit\(',line)
        if committed and committed.group(1) in pending:
            surface=committed.group(1)
            if pending.pop(surface):empty.add(surface)
            else:empty.discard(surface)
    return empty


def escape_presses(log):
    # The helper uses a fixed evdev US keymap: Escape is keycode1. Count actual
    # compositor-to-app presses, not helper acknowledgments or key releases.
    return len(re.findall(r'wl_keyboard[#@]\d+\.key\(\d+, \d+, 1, 1\)', log))


def build_keyboard(root,evidence,env):
    build=evidence/'keyboard-build';build.mkdir()
    def build_command(command):
        subprocess.run(command,env=env,text=True,capture_output=True,timeout=30,check=True,close_fds=True)
    build_command(['wayland-scanner','client-header',str(root/'tests/launcher/virtual-keyboard.xml'),str(build/'virtual-keyboard.h')])
    build_command(['wayland-scanner','private-code',str(root/'tests/launcher/virtual-keyboard.xml'),str(build/'virtual-keyboard.c')])
    flags=subprocess.run(['pkg-config','--cflags','--libs','wayland-client','xkbcommon'],env=env,text=True,capture_output=True,timeout=10,check=True,close_fds=True).stdout
    build_command(['cc','-std=c11','-Wall','-Wextra','-Werror','-I'+str(build),str(root/'tests/launcher/keyboard.c'),str(build/'virtual-keyboard.c'),'-o',str(build/'keyboard'),*shlex.split(flags)])
    return build


def verify(root,runtime,evidence,env,start,say,expect,wait_for):
    scene=evidence/'launcher-scene'
    shutil.copytree(root/'shell/launcher',scene)
    data=evidence/'private-catalog';data.mkdir();env['XDG_DATA_DIRS']=str(data)
    apps=Path(env['XDG_DATA_HOME'])/'applications';apps.mkdir(parents=True)
    (apps/'fixture.desktop').write_text('[Desktop Entry]\nType=Application\nName=Fixture Editor\nExec=/bin/true\nCategories=Utility;\n')
    # Tiny test-only protocol client, not a runtime build. No display is opened
    # by generation/compilation; all child owners below use the reviewed start.
    build=build_keyboard(root,evidence,env)
    log_path=evidence/'launcher-lifecycle.log'
    app=start([runtime,'--scene',str(scene/'entry.plm'),'--answer-as','launcher','--no-hud','--stall','0'],'launcher-lifecycle',{'WAYLAND_DEBUG':'1'})
    wait_for(lambda:say('launcher','get screen.0.name')=='HEADLESS-1','native bootstrap output name')
    wait_for(lambda:'render · NVIDIA GeForce RTX 5090 (Vulkan)' in log_path.read_text(),'app hardware adapter')
    expect('launcher','launcher_visible','false')
    expect('launcher','launcher_outputs','')

    def log():return log_path.read_text()
    def named():return [layer for layer in layers(log()) if layer['namespace']==NAMESPACE]
    def run(command):return subprocess.run(command,env=env,text=True,capture_output=True,timeout=10,check=True,close_fds=True)
    def sway(command):
        socket=Path(env['SWAYSOCK']);assert socket.parent==Path(env['XDG_RUNTIME_DIR']) and not socket.is_symlink()
        result=json.loads(run(['swaymsg','-s',str(socket),'-r',command]).stdout)
        assert all(item.get('success') for item in result),result
    keyboard_process=None
    keyboard_sequence=0
    def keyboard(command):
        nonlocal keyboard_sequence
        assert keyboard_process is not None and keyboard_process.poll() is None
        before=escape_presses(log())
        keyboard_process.stdin.write((command+'\n').encode());keyboard_process.stdin.flush()
        keyboard_sequence+=1
        wait_for(lambda:('done '+str(keyboard_sequence)+'\n') in (evidence/'keyboard.log').read_text(),'persistent keyboard acknowledgment')
        if command=='escape':
            wait_for(lambda:escape_presses(log())>before,'actual Escape delivered to app')
    def opened(output):
        say('launcher','emit launcher_open');expect('launcher','launcher_visible','true')
        expect('launcher','launcher_outputs',output)
        wait_for(lambda:len(named())==1,'one mapped exact namespace')
    def closed():
        expect('launcher','launcher_visible','false');expect('launcher','launcher_closing','false')
        expect('launcher','launcher_outputs','');wait_for(lambda:not named(),'actual namespace role destruction')
    def seen():
        result=say('launcher','get launcher_seen')
        try:return float(result)
        except (TypeError,ValueError):return -1
    def reload(label):
        file=scene/'entry.luau';before=log().count('entry.luau running')
        file.write_text(file.read_text()+'\n-- owned reload '+label+'\n')
        wait_for(lambda:log().count('entry.luau running')>before,'actual '+label+' VM reload')

    assert not named(),'namespace must start unmapped'
    bootstrap=[layer for layer in layers(log()) if layer['namespace']=='pleamar']
    assert bootstrap,'default namespace bootstrap missing'
    wait_for(lambda:all(layer['surface'] in empty_inputs(log()) for layer in bootstrap),'bootstrap empty input regions')
    run(['grim','-o','HEADLESS-1',str(evidence/'launcher-initial-closed.png')])
    from PIL import Image
    with Image.open(evidence/'launcher-initial-closed.png') as image:
        assert image.convert('RGB').getpixel((0,0))==(48,35,43),'bootstrap is not transparent at its pixel'
    opened('HEADLESS-1');wait_for(lambda:seen()>.99,'native reveal opening')
    keyboard_process=start([str(build/'keyboard'),env['XDG_RUNTIME_DIR']],'keyboard',stdin=subprocess.PIPE)
    wait_for(lambda:'ready\n' in (evidence/'keyboard.log').read_text(),'persistent private keyboard ready')
    wait_for(lambda:re.search(r'wl_keyboard[#@]\d+\.enter\(',log()),'actual private keyboard focus')
    keyboard('query');expect('launcher','launcher_query','fixture')
    expect('launcher','launcher_total','1')
    run(['grim','-o','HEADLESS-1',str(evidence/'launcher-native-open.png')])
    keyboard('escape');expect('launcher','launcher_query','');expect('launcher','launcher_visible','true')
    keyboard('escape');closed()

    # Interrupt close before it settles, then start a new close cycle.
    opened('HEADLESS-1');wait_for(lambda:seen()>.99,'reopen reveal')
    say('launcher','emit launcher_close');say('launcher','emit launcher_open')
    expect('launcher','launcher_visible','true');expect('launcher','launcher_closing','false')
    assert len(named())==1
    say('launcher','emit launcher_close');reload('closing');closed()
    # Coalesced open/close can leave reveal atzero; the conditional check still ends.
    say('launcher','emit launcher_open');say('launcher','emit launcher_close');closed()
    opened('HEADLESS-1');reload('visible');expect('launcher','launcher_outputs','HEADLESS-1')
    say('launcher','emit launcher_close');closed()

    # Real private compositor output lifecycle, never a live desktop endpoint.
    sway('create_output')
    wait_for(lambda:say('launcher','get screen.1.name')=='HEADLESS-2','second native output name')
    sway('focus output HEADLESS-2')
    window=evidence/'focus.plm'
    window.write_text('scene Focus { surface { kind: window; size: 420, 160; title: "B Private Focus" } box { from: 0, 0; size: 420, 160; color: #181825 } }')
    focus=start([runtime,'--scene',str(window),'--answer-as','focus','--no-hud','--stall','0'],'focus')
    wait_for(lambda:say('focus','get screen.width'),'private native focus window')
    # Native window service event is asynchronous. Its route is the observable.
    expect('launcher','launcher_focused_output','HEADLESS-2')
    opened('HEADLESS-2')
    sway('output HEADLESS-2 disable')
    # Preserve Pleamar's native screens snapshot during output disable; this
    # shell does not invent a fallback route or patch the compositor API.
    wait_for(lambda:say('launcher','get launcher_outputs') in ('HEADLESS-1','HEADLESS-2'),
             'native output route after disable')
    wait_for(lambda:len(named())<=1,'no duplicate native launcher role')
    sway('output HEADLESS-2 enable')
    wait_for(lambda:say('launcher','get screen.1.name')=='HEADLESS-2','native output reconnect')
    # Reopening is explicit after a native output close; the shell does not
    # manufacture a route or reopen event for an output reconnect.
    if say('launcher','get launcher_visible') == 'true':
        say('launcher','emit launcher_close');closed()
    else:
        wait_for(lambda:not named(),'native output close removed its role')
    say('focus','quit');focus.wait(timeout=10)

    # Passive finite observation, not an idle-performance acceptance benchmark.
    time.sleep(.15)
    before=len(re.findall(r'wl_surface[#@]\d+\.frame\(',log()))
    time.sleep(1)
    after=len(re.findall(r'wl_surface[#@]\d+\.frame\(',log()))
    assert after==before,{'before':before,'after':after,'scope':'closed1s frame requests'}
    (evidence/'lifecycle-summary.json').write_text(json.dumps({'result':'passed','checks':['exact namespace starts unmapped','transparent empty-input bootstrap','real virtual keyboard search/Escape','native role destroyed after settled close','rapid/coalesced close/reopen','actual visible/closing VM reload','private output disable/reconnect with native routing','closed1s no new Wayland frame requests'],'app_gpu':'NVIDIA GeForce RTX 5090 Vulkan','compositor_gpu':'NVIDIA GeForce RTX 5090 GLES2','closed_frame_requests':[before,after],'performance_acceptance':False,'visual_parity':False,'plugin_or_physical_acceptance':False},indent=2)+'\n')
    say('launcher','quit');app.wait(timeout=10)


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser();parser.add_argument('--cpu',action='store_true',required=True);parser.parse_args()
    sample='\n'.join(['-> zwlr_layer_shell_v1#1.get_layer_surface(new id zwlr_layer_surface_v1#7, wl_surface#8, wl_output#2, 4, "tohru-launcher-backdrop")','-> zwlr_layer_surface_v1#7.destroy()','-> zwlr_layer_shell_v1#1.get_layer_surface(new id zwlr_layer_surface_v1#7, wl_surface#9, wl_output#2, 4, "tohru-launcher-backdrop")'])
    assert layers(sample)==[{'surface':'9','namespace':NAMESPACE}]
    sample+='\n-> wl_compositor#1.create_surface(new id wl_surface#9)'
    sample+='\n-> wl_compositor#1.create_region(new id wl_region#3)\n-> wl_surface#9.set_input_region(wl_region#3)'
    assert '9' not in empty_inputs(sample), 'pending state is not applied evidence'
    sample+='\n-> wl_region#3.destroy()\n-> wl_surface#9.commit()'
    assert '9' in empty_inputs(sample), 'set copies region before its destruction'
    sample+='\n-> wl_surface#9.set_input_region(nil)'
    assert '9' in empty_inputs(sample), 'reset is also pending until commit'
    sample+='\n-> wl_surface#9.commit()'
    assert '9' not in empty_inputs(sample), 'nil restores default full input'
    sample+='\n-> wl_compositor#1.create_region(new id wl_region#3)\n-> wl_surface#9.set_input_region(wl_region#3)\n-> wl_surface#9.commit()'
    assert '9' in empty_inputs(sample)
    sample+='\n-> wl_surface#9.destroy()'
    assert '9' not in empty_inputs(sample), 'destroyed surface has no live evidence'
    sample+='\n-> wl_compositor#1.create_surface(new id wl_surface#9)\n-> wl_surface#9.commit()'
    assert '9' not in empty_inputs(sample), 'reused surface starts with default input'
    sample+='\n-> wl_compositor#1.create_region(new id wl_region#3)\n-> wl_region#3.add(0, 0, 1, 1)\n-> wl_surface#9.set_input_region(wl_region#3)\n-> wl_surface#9.commit()'
    assert '9' not in empty_inputs(sample)
    assert escape_presses('wl_keyboard#3.key(9, 0, 1, 0)')==0
    assert escape_presses('wl_keyboard#3.key(9, 0, 33, 1)')==0
    assert escape_presses('wl_keyboard#3.key(9, 0, 1, 1)')==1
    assert escape_presses('wl_keyboard@4.key(10, 12, 1, 1)')==1
    print('Native evidence parser requires committed input state and handles role/region/surface ID reuse and nil reset')
