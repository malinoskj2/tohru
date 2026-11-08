"""Private synthetic NM/BlueZ native backend + actual feature UI integration."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time
from xml.sax.saxutils import escape


def verify(root,runtime,evidence,env,start,say,expect,wait_for):
    runtime_dir=Path(env['XDG_RUNTIME_DIR'])
    expected='unix:path='+str(runtime_dir/'system-bus')
    assert env['DBUS_SYSTEM_BUS_ADDRESS']==expected and not (runtime_dir/'system-bus').is_symlink()
    config=evidence/'system-bus.conf'
    config.write_text('<busconfig><type>system</type><listen>'+escape(expected)+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    start(['dbus-daemon','--nofork','--config-file',str(config)],'private-system-bus')
    wait_for((runtime_dir/'system-bus').exists,'owned private system bus')
    ready=evidence/'services-ready'
    start([sys.executable,str(root/'tests/connectivity/services.py'),str(runtime_dir),str(ready)],'synthetic-services')
    wait_for(ready.exists,'synthetic NM/BlueZ names')
    scene=evidence/'feature-scene';shutil.copytree(root/'shell',scene)
    entry=scene/'feature-check.plm'
    entry.write_text('''scene FeatureCheck {
    surface { size: full, 31; anchor: top; screens: all; keyboard: exclusive while net_open or bt_open or info_visible or about_visible }
    permissions { services: "network.*", "bluetooth.*", "clock", "battery", "say:tohru-notifications" }
    include "network/state.plm"
    include "bluetooth/state.plm"
    include "info/state.plm"
    include "about/state.plm"
    fact fixture_reads = 0
    event fixture_network ->
    event fixture_bluetooth ->
    event fixture_info ->
    event fixture_about ->
    zone box net_button { from: 100, 0; size: 100, 31; cursor: pointer }
    zone box bt_button { from: 210, 0; size: 100, 31; cursor: pointer }
    zone box info_button { from: 320, 0; size: 100, 31; cursor: pointer }
    zone box about_button { from: 430, 0; size: 100, 31; cursor: pointer }
    text "Network" { at: 150, 16; anchor: center; size: 13; color: #cdd6f4 }
    text "Bluetooth" { at: 260, 16; anchor: center; size: 13; color: #cdd6f4 }
    text "Information" { at: 370, 16; anchor: center; size: 13; color: #cdd6f4 }
    text "About" { at: 480, 16; anchor: center; size: 13; color: #cdd6f4 }
    on press net_button { emit fixture_network }
    on press bt_button { emit fixture_bluetooth }
    on press info_button { emit fixture_info }
    on press about_button { emit fixture_about }
    include "network/popup.plm"
    include "bluetooth/popup.plm"
    include "info/popup.plm"
    include "about/popup.plm"
}
''')
    logic=scene/'feature-check.luau'
    logic.write_text('''local function snapshot()
    fact.fixture_reads = fact.fixture_reads + 1
    assert(tohru_info, "Missing real information binding")
    return tohru_info.snapshot()
end
local info = require("info/controller")(snapshot)
local about = require("about/controller")(snapshot, info.open)
local network = require("network/controller")()
local bluetooth = require("bluetooth/controller")()
require("info/power")()
on("fixture_network", function() bluetooth.close(); about.close(); info.close(); network.open() end)
on("fixture_bluetooth", function() network.close(); about.close(); info.close(); bluetooth.open() end)
on("fixture_info", function() network.close(); bluetooth.close(); about.close(); info.open(0) end)
on("fixture_about", function() network.close(); bluetooth.close(); info.close(); about.open() end)
''')
    app=start([runtime,'--scene',str(entry),'--answer-as','features','--no-hud','--stall','0'],'features',{'TOHRU_SHELL':'1','WAYLAND_DEBUG':'1'})
    wait_for(lambda:say('features','get net_rows.count')=='3','real native network catalog')
    expect('features','bt.present','true');expect('features','bt_devices.count','1')
    wait_for(lambda:'NVIDIA GeForce RTX 5090' in (evidence/'features.log').read_text(),'app hardware adapter')
    fixture_records=evidence/'service-record.json'

    def record():return json.loads(fixture_records.read_text())
    def run(command):return subprocess.run(command,env=env,text=True,capture_output=True,timeout=15,check=True,close_fds=True)
    def control(kind):
        assert kind in ('pin','passkey','confirm','authorize','display')
        run(['gdbus','call','--address',expected,'--dest','org.bluez','--object-path','/org/tohru/TestConnectivity','--method','org.tohru.TestConnectivity.PairingKind',kind])
    def complete_display():run(['gdbus','call','--address',expected,'--dest','org.bluez','--object-path','/org/tohru/TestConnectivity','--method','org.tohru.TestConnectivity.CompleteDisplay'])
    def keyboard(*args):run(['wtype','-s','300',*args])
    def index(ssid):
        for number in range(3):
            if say('features',f'get net_rows.{number}.ssid')==ssid:return number
        return None
    def select(ssid):
        selected=wait_for(lambda:(value+1) if (value:=index(ssid)) is not None else None,'native SSID '+ssid)-1
        say('features',f'emit network_select {selected}')
    def reload(label):
        log=evidence/'features.log';before=log.read_text().count('feature-check.luau running')
        logic.write_text(logic.read_text()+'\n-- owned fixture reload '+label+'\n')
        wait_for(lambda:log.read_text().count('feature-check.luau running')>before,'actual '+label+' logic reload')

    say('features','emit fixture_network')
    select('Fixture Open');expect('features','net.online','true');expect('features','net.name','Fixture Open')
    select('Fixture Secure');expect('features','net_joining','true')
    keyboard('fixture-password')
    assert 'fixture-password' not in (say('features','get net_password') or ''), 'synthetic password exported before submit'
    keyboard('-k','Return')
    expect('features','net.name','Fixture Secure');expect('features','net_joining','false')
    wait_for(lambda:any(call.get('password_matches_fixture') is True for call in record()['calls']),'native NM received synthetic credentials')
    assert say('features','get net_password') in (None,'','[secret]','<secret>'), 'secret input observation must be absent/masked'
    select('Fixture Secure');say('features','emit network_disconnect');expect('features','net.online','false')
    say('features','emit network_forget')
    wait_for(lambda:any(call['event']=='ForgetNetwork' for call in record()['calls']),'native NM forget')
    say('features','emit network_scan');wait_for(lambda:any(call['event']=='Scan' for call in record()['calls']),'native scan')
    say('features','emit network_power');expect('features','net.wifi','false')
    say('features','emit network_power');expect('features','net.wifi','true')
    run(['grim','-o','HEADLESS-1',str(evidence/'network-native.png')])
    say('features','emit network_close')

    say('features','emit fixture_bluetooth');say('features','emit bluetooth_scan')
    expect('features','bt.discovering','true');expect('features','bt_scan_owned','true')
    reload('discovering');say('features','emit bluetooth_close')
    expect('features','bt.discovering','false');expect('features','bt_scan_owned','false')
    pairing=[]
    for kind in ('pin','passkey','confirm','authorize','display'):
        control(kind)
        say('features','emit fixture_bluetooth');say('features','emit bluetooth_select 0');say('features','emit bluetooth_connect')
        expect('features','bt.pairing',kind)
        question=say('features','get bt.pairing_id');assert float(question)>0
        reload('pairing-'+kind)
        expect('features','bt.pairing_id',question)
        if kind in ('pin','passkey'):
            keyboard('000042');keyboard('-k','Return')
        elif kind=='display':
            expect('features','bt.pairing_code','000042')
            run(['grim','-o','HEADLESS-1',str(evidence/'bluetooth-display-native.png')]);complete_display()
        else:say('features','emit bluetooth_answer')
        expect('features','bt.pairing','none');expect('features','bt_devices.0.connected','true')
        pairing.append({'kind':kind,'question':question,'connected':True})
        say('features','emit bluetooth_select 0');say('features','emit bluetooth_connect')
        expect('features','bt_devices.0.connected','false')
    control('confirm');say('features','emit bluetooth_select 0');say('features','emit bluetooth_connect')
    expect('features','bt.pairing','confirm');say('features','emit bluetooth_cancel');expect('features','bt.pairing','none')
    say('features','emit bluetooth_forget');wait_for(lambda:any(call['event']=='ForgetDevice' for call in record()['calls']),'native device forget')
    say('features','emit bluetooth_close')

    say('features','emit fixture_info')
    expect('features','info_rows.2.value',Path('/proc/sys/kernel/hostname').read_text().strip())
    assert 'binding' not in (say('features','get info_error') or '').lower(),'real snapshot binding failed'
    reads=say('features','get fixture_reads')
    reload('visible-information');expect('features','fixture_reads',reads)
    say('features','emit info_scroll 1');expect('features','fixture_reads',reads)
    say('features','emit info_refresh');expect('features','fixture_reads',str(int(reads)+1))
    run(['grim','-o','HEADLESS-1',str(evidence/'information-native.png')])
    say('features','emit fixture_about');expect('features','about_specs.count','6')
    run(['grim','-o','HEADLESS-1',str(evidence/'about-native.png')])
    say('features','emit about_more');expect('features','about_visible','false');expect('features','info_visible','true')
    say('features','emit info_open 2');wait_for(lambda:bool(say('features','get info_clock.time')),'native clock page')
    say('features','emit info_open 6')
    # Explicitly synthetic presentation stimulus; native battery unit/controller
    # behavior is covered by actual-source CPU fixtures, not a fake sysfs result.
    for name,value in (('power_present',1),('power_known',1),('power_percent',.2),('power_charging',1)):say('features',f'fact {name} {value}')
    run(['grim','-o','HEADLESS-1',str(evidence/'power-presentation-stimulus.png')])
    say('features','emit info_close')
    assert all(call.get('fixture_code_matches',True) for call in record()['calls']),'synthetic agent code mismatch'
    (evidence/'services-summary.json').write_text(json.dumps({'result':'passed','scope':'synthetic private system bus with real native service/backend/controllers/scenes','pairing':pairing,'info_reads_retain_across_vm_reload':True,'power_image':'injected private presentation, not native battery device evidence','app_gpu':'NVIDIA GeForce RTX 5090 Vulkan','compositor_gpu':'NVIDIA GeForce RTX 5090 GLES2','real_radio_or_network':False,'visual_or_performance_acceptance':False},indent=2)+'\n')
    say('features','quit');app.wait(timeout=10)
