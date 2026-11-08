"""Finite private launcher role/input/topology workload; requires allocated GPU.

Only the runner imports verify. Standalone --cpu checks protocol evidence parsing.
"""
import json
from pathlib import Path
import re
import shutil
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
    regions,empty={},set()
    for line in log.splitlines():
        created=re.search(r'create_region\(new id wl_region[#@](\d+)\)',line)
        if created:regions[created.group(1)]=True
        changed=re.search(r'wl_region[#@](\d+)\.(?:add|subtract)\(',line)
        if changed:regions[changed.group(1)]=False
        assigned=re.search(r'wl_surface[#@](\d+)\.set_input_region\(wl_region[#@](\d+)\)',line)
        if assigned:
            surface,region=assigned.groups()
            if regions.get(region) is True:empty.add(surface)
            else:empty.discard(surface)
    return empty


def verify(root,runtime,evidence,env,start,say,expect,wait_for):
    scene=evidence/'launcher-scene'
    shutil.copytree(root/'shell/launcher',scene)
    data=evidence/'private-catalog';data.mkdir();env['XDG_DATA_DIRS']=str(data)
    apps=Path(env['XDG_DATA_HOME'])/'applications';apps.mkdir(parents=True)
    (apps/'fixture.desktop').write_text('[Desktop Entry]\nType=Application\nName=Fixture Editor\nExec=/bin/true\nCategories=Utility;\n')
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
    def keyboard(*arguments):run(['wtype','-s','300',*arguments])
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
    keyboard('fixture');expect('launcher','launcher_query','fixture')
    expect('launcher','launcher_total','1')
    run(['grim','-o','HEADLESS-1',str(evidence/'launcher-native-open.png')])
    keyboard('-k','Escape');expect('launcher','launcher_query','');expect('launcher','launcher_visible','true')
    keyboard('-k','Escape');closed()

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
    expect('launcher','launcher_outputs','HEADLESS-1')
    wait_for(lambda:len(named())==1,'native fallback has only one role')
    sway('output HEADLESS-2 enable')
    wait_for(lambda:say('launcher','get screen.1.name')=='HEADLESS-2','native output reconnect')
    expect('launcher','launcher_outputs','HEADLESS-1')
    say('launcher','emit launcher_close');closed()
    say('focus','quit');focus.wait(timeout=10)

    # Passive finite observation, not an idle-performance acceptance benchmark.
    time.sleep(.15)
    before=len(re.findall(r'wl_surface[#@]\d+\.frame\(',log()))
    time.sleep(1)
    after=len(re.findall(r'wl_surface[#@]\d+\.frame\(',log()))
    assert after==before,{'before':before,'after':after,'scope':'closed1s frame requests'}
    (evidence/'lifecycle-summary.json').write_text(json.dumps({'result':'passed','checks':['exact namespace starts unmapped','transparent empty-input bootstrap','real virtual keyboard search/Escape','native role destroyed after settled close','rapid/coalesced close/reopen','actual visible/closing VM reload','private output disable/reconnect/focused fallback','closed1s no new Wayland frame requests'],'app_gpu':'NVIDIA GeForce RTX 5090 Vulkan','compositor_gpu':'NVIDIA GeForce RTX 5090 GLES2','closed_frame_requests':[before,after],'performance_acceptance':False,'visual_parity':False,'plugin_or_physical_acceptance':False},indent=2)+'\n')
    say('launcher','quit');app.wait(timeout=10)


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser();parser.add_argument('--cpu',action='store_true',required=True);parser.parse_args()
    sample='\n'.join(['-> zwlr_layer_shell_v1#1.get_layer_surface(new id zwlr_layer_surface_v1#7, wl_surface#8, wl_output#2, 4, "tohru-launcher-backdrop")','-> zwlr_layer_surface_v1#7.destroy()','-> zwlr_layer_shell_v1#1.get_layer_surface(new id zwlr_layer_surface_v1#7, wl_surface#9, wl_output#2, 4, "tohru-launcher-backdrop")'])
    assert layers(sample)==[{'surface':'9','namespace':NAMESPACE}]
    sample+='\n-> wl_compositor#1.create_region(new id wl_region#3)\n-> wl_surface#9.set_input_region(wl_region#3)'
    assert '9' in empty_inputs(sample)
    sample+='\n-> wl_compositor#1.create_region(new id wl_region#3)\n-> wl_region#3.add(0, 0, 1, 1)\n-> wl_surface#9.set_input_region(wl_region#3)'
    assert '9' not in empty_inputs(sample)
    print('Native lifecycle evidence parser handles role/region ID reuse and real empty input assignment')
