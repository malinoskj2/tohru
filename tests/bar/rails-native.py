"""Finite exact rail binding/shader/native timer ownership workload, private Sway."""
import json
from pathlib import Path
import shutil
import subprocess
import time


def verify(root,runtime,evidence,env,start,say,expect,wait_for):
    scene=evidence/'rails-scene';shutil.copytree(root/'shell',scene)
    entry=scene/'rails.plm'
    entry.write_text('''scene Rails {
    surface { size: full, 42; anchor: top; screens: all; reserve: 31; keyboard: exclusive while cal_open }
    permissions { services: "window", "apps", "media", "clock", "workspaces", "workspaces.focus" }
    include "bar/state.plm"
    include "bar/rail-state.plm"
    include "calendar/state.plm"
    fact fixture_ws_timer = false
    fact fixture_cal_timer = false
    event fixture_rail_timers ->
    box { from: 100, 0; size: screen.width - 200, 31; corner: 10; color: #160e14; opacity: 70% }
    include "bar/contents.plm"
    include "bar/workspace.plm"
    include "calendar/popup.plm"
}
''')
    logic=scene/'rails.luau'
    logic.write_text('''assert(tohru_workspace, "Missing real rail binding")
local publishers = {}
local factory = require("calendar/rail-publisher")
local function make(binding,role,prefix)
    local rail = factory(binding,role,prefix)
    publishers[role] = rail
    return rail
end
require("bar/controller")()
require("bar/workspace")(make,tohru_workspace)
require("calendar/controller")(make(tohru_workspace,"calendar","cal_rail_"))
on("fixture_rail_timers",function()
    fact.fixture_ws_timer = publishers.workspace.active()
    fact.fixture_cal_timer = publishers.calendar.active()
end)
''')
    app=start([runtime,'--scene',str(entry),'--answer-as','rails','--no-hud','--stall','0'],'rails',{'TOHRU_SHELL':'1','WAYLAND_DEBUG':'1'})
    expect('rails','ws_available','true')
    wait_for(lambda:'render · NVIDIA GeForce RTX 5090 (Vulkan)' in (evidence/'rails.log').read_text(),'app hardware adapter')

    def run(command):return subprocess.run(command,env=env,text=True,capture_output=True,timeout=10,check=True,close_fds=True)
    def sway(command):
        socket=Path(env['SWAYSOCK']);assert socket.parent==Path(env['XDG_RUNTIME_DIR']) and not socket.is_symlink()
        result=json.loads(run(['swaymsg','-s',str(socket),'-r',command]).stdout)
        assert all(item.get('success') for item in result),result
    def focused():
        return next((item['name'] for item in json.loads(run(['swaymsg','-s',env['SWAYSOCK'],'-r','-t','get_workspaces']).stdout) if item['focused']),None)
    def window(number):
        sway('workspace number '+str(number))
        path=evidence/f'workspace-{number}.plm'
        path.write_text(f'scene WorkspaceWindow {{ surface {{ kind: window; size: 320, 120; title: "B workspace {number}" }} box {{ from: 0, 0; size: 320, 120; color: #181825 }} }}')
        process=start([runtime,'--scene',str(path),'--answer-as',f'workspace-{number}','--no-hud','--stall','0'],f'workspace-{number}')
        wait_for(lambda:say(f'workspace-{number}','get screen.width'),'own advertised workspace window')
        return process
    def timers(workspace,calendar):
        say('rails','emit fixture_rail_timers')
        expect('rails','fixture_ws_timer','true' if workspace else 'false')
        expect('rails','fixture_cal_timer','true' if calendar else 'false')
    def resting(prefix,center):
        expect('rails',prefix+'moving','false')
        expect('rails',prefix+'v.0',str(center))
        expect('rails',prefix+'v.1','0');expect('rails',prefix+'v.2','2');expect('rails',prefix+'v.3','0')
    def reload():
        log=evidence/'rails.log';before=log.read_text().count('rails.luau running')
        logic.write_text(logic.read_text()+'\n-- own visible moving calendar reload\n')
        wait_for(lambda:log.read_text().count('rails.luau running')>before,'actual rail logic reload')

    first=window(1);fifth=window(5)
    expect('rails','ws_advertised.1','true');expect('rails','ws_advertised.5','true');expect('rails','ws_advertised.2','false')
    resting('ws_',72);timers(False,False)
    description=say('rails','describe json')
    panels=json.loads(description)
    nodes={node['name']:node for panel in panels for node in panel.get('nodes',[])}
    assert nodes['workspace_cell.1']['active'] is True
    assert nodes['workspace_cell.2']['active'] is False
    (evidence/'workspace-description.json').write_text(description+'\n')
    say('rails','emit workspace_select 2')
    expect('rails','ws_error','Workspace is not advertised');assert focused()=='5'
    say('rails','emit workspace_select 1')
    wait_for(lambda:focused()=='1','real native advertised workspace activation')
    expect('rails','ws_moving','true');resting('ws_',8);timers(False,False)
    expect('rails','ws_error','')
    run(['grim','-o','HEADLESS-1',str(evidence/'workspace-native-rest.png')])
    from PIL import Image
    expected=[float(say('rails','get ws_'+channel)) for channel in ('r','g','b')]
    with Image.open(evidence/'workspace-native-rest.png') as image:
        # One opaque core sample exercises the real16-value GPU shader/color path;
        # this is neither image-wide source parity nor a performance benchmark.
        pixel=image.convert('RGB').getpixel((120,15))
    assert all(abs(actual-target)<25 for actual,target in zip(pixel,expected)),{'core_pixel':pixel,'published_rgb':expected}

    say('rails','emit calendar_open');expect('rails','cal_open','true')
    month=int(say('rails','get cal_month'))
    expect('rails','cal_rail_width','192');resting('cal_rail_',16*(month-1)+8);timers(False,False)
    target=1 if month!=1 else 12
    say('rails',f'emit calendar_month {target}');expect('rails','cal_rail_moving','true')
    reload();expect('rails','cal_month',str(target))
    resting('cal_rail_',16*(target-1)+8);timers(False,False)
    say('rails','emit calendar_month 12');say('rails','emit calendar_month 5')
    resting('cal_rail_',72);timers(False,False)
    expect('rails','cal_rail_error','')
    run(['grim','-o','HEADLESS-1',str(evidence/'calendar-exact-native.png')])
    say('rails','emit calendar_month 1');expect('rails','cal_rail_moving','true')
    say('rails','emit calendar_close');expect('rails','cal_open','false');timers(False,False)
    time.sleep(.2);expect('rails','cal_rail_moving','false');timers(False,False)
    say('rails','emit calendar_open');expect('rails','cal_month',str(month))
    resting('cal_rail_',16*(month-1)+8);timers(False,False)
    say('rails','emit calendar_close')
    (evidence/'rails-summary.json').write_text(json.dumps({'result':'passed','checks':['real Rust binding observe/tick/reset','native16-scalar shader core/color pixel','advertised-only native workspace focus and absent refusal','240Hz reviewed physics through cancellable60Hz native timer','calendar12month width/reset/interruption','actual visible moving calendar VM reload','owned handles canceled at rest/close and resumed on observation'],'workspace_core_pixel':pixel,'published_rgb':expected,'app_gpu':'NVIDIA GeForce RTX 5090 Vulkan','compositor_gpu':'NVIDIA GeForce RTX 5090 GLES2','performance_or_visual_parity_acceptance':False},indent=2)+'\n')
    for number,process in ((5,fifth),(1,first)):
        say(f'workspace-{number}','quit');process.wait(timeout=10)
    say('rails','quit');app.wait(timeout=10)
