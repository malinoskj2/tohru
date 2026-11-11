#!/usr/bin/env python3
"""Private native scene/protocol checks; power and lock commands reach only logging fakes."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import sys

ROOT = Path(__file__).resolve().parents[2]

def wait(fn, label, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = fn()
        if result:
            return result
        time.sleep(.05)
    raise AssertionError('timed out: ' + label)

def private_ipc_environment(inherited, run):
    ipc = run / 'ipc'
    for directory in (run, ipc):
        info = directory.stat()
        assert not directory.is_symlink() and directory.resolve() == directory
        assert info.st_uid == os.getuid() and info.st_mode & 0o777 == 0o700
    result = dict(inherited)
    for key in ['WAYLAND_DISPLAY', 'WAYLAND_SOCKET', 'SWAYSOCK', 'DISPLAY',
                'HYPRLAND_INSTANCE_SIGNATURE', 'CUA_INJECT_SOCKET', 'XDG_SESSION_ID',
                'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE', 'TOHRU_SHELL']:
        result.pop(key, None)
    result['PLEAMAR_SOCKETS'] = str(ipc)
    result['PULSE_SERVER'] = 'unix:' + str(run/'pulse-native')
    result['PULSE_RUNTIME_PATH'] = str(run)
    result['PULSE_COOKIE'] = str(run/'pulse-cookie')
    result['DBUS_SESSION_BUS_ADDRESS'] = 'unix:path=' + str(run / 'bus')
    result['DBUS_SYSTEM_BUS_ADDRESS'] = result['DBUS_SESSION_BUS_ADDRESS']
    return result

def check_ipc_directory(env, run):
    ipc = run / 'ipc'
    assert env['PLEAMAR_SOCKETS'] == str(ipc)
    private_ipc_environment(env, run)
    for endpoint in ipc.rglob('*'):
        assert not endpoint.is_symlink() and endpoint.resolve().is_relative_to(ipc)
        assert endpoint.stat().st_uid == os.getuid()

def cleanup(processes, files, run, evidence, events):
    errors = []
    try:
        (evidence/'events.json').write_text(json.dumps(events,indent=2)+'\n')
    except Exception as error:
        errors.append('evidence: '+str(error))
    for proc in reversed(processes):
        try:
            if proc.poll() is not None: continue
            try:
                proc.terminate()
            except Exception as error:
                errors.append('terminate: '+str(error))
                proc.kill()
            try:
                proc.wait(timeout=8)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=3)
        except Exception as error:
            errors.append('process cleanup: '+str(error))
    for file in files:
        try: file.close()
        except Exception as error: errors.append('log close: '+str(error))
    try: shutil.rmtree(run)
    except Exception as error: errors.append('runtime cleanup: '+str(error))
    return errors

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime', type=Path, required=True)
    p.add_argument('--notifications',action='store_true')
    p.add_argument('--pulseaudio',type=Path)
    p.add_argument('--gdbus',type=Path)
    p.add_argument('--pactl',type=Path)
    a = p.parse_args()
    if a.notifications:
        for name in ('pulseaudio','gdbus','pactl'):
            tool=getattr(a,name)
            if not tool or not tool.is_file() or not os.access(tool,os.X_OK):
                p.error('notification validation requires explicit executable --'+name)
    evidence = ROOT / '.tmp' / ('native-session-' + str(time.time_ns()))
    evidence.mkdir()
    run = ROOT / '.tmp' / ('l' + str(os.getpid()))
    run.mkdir(mode=0o700)
    (run/'ipc').mkdir(mode=0o700)
    scenes = evidence / 'shell'
    shutil.copytree(ROOT / 'shell', scenes)
    home = evidence / 'home'; home.mkdir()
    # Native processes cannot reach the live authority through either bus.
    env = {**private_ipc_environment(os.environ, run), 'HOME':str(home), 'XDG_RUNTIME_DIR':str(run),
           'XDG_CONFIG_HOME':str(evidence/'config'), 'XDG_CACHE_HOME':str(evidence/'cache'),
           'XDG_DATA_HOME':str(evidence/'data'), 'XDG_STATE_HOME':str(evidence/'state'),
           
           'PLEAMAR_SOCKETS':str(run/'ipc'),
           'WLR_BACKENDS':'headless', 'WLR_HEADLESS_OUTPUTS':'1', 'WLR_RENDERER':'gles2',
           'WLR_LIBINPUT_NO_DEVICES':'1', 'WLR_RENDER_DRM_DEVICE':'/dev/dri/renderD128',
           'VK_DRIVER_FILES':'/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json',
           'WGPU_BACKEND':'vulkan', 'PLEAMAR_BACKEND':'vulkan', 'WAYLAND_DEBUG':'client'}
    for key in ['WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','HYPRLAND_INSTANCE_SIGNATURE','TOHRU_SHELL']:
        env.pop(key, None)
    # No systemctl/loginctl implementation can act on host; each call is only logged.
    fakebin = evidence/'bin'; fakebin.mkdir()
    power_log = evidence/'power.log'; power_log.touch()
    for name in ['systemctl','loginctl']:
        f = fakebin/name; f.write_text('#!/bin/sh\nprintf "%s\\n" "'+name+' $*" >> \''+str(power_log)+'\'\n'); f.chmod(0o755)
    env['PATH'] = str(fakebin)+':'+env['PATH']
    config = evidence/'sway.conf'
    config.write_text('output HEADLESS-1 mode 1280x800@60Hz\noutput HEADLESS-1 bg #30232b solid_color\nseat seat0 hide_cursor 100\n')
    processes=[]; files=[]; events=[]; checks=[]
    def start(args, log):
        check_ipc_directory(env, run)
        f=(evidence/log).open('w');files.append(f)
        proc=subprocess.Popen(args, env=env, stdout=f, stderr=subprocess.STDOUT)
        processes.append(proc);return proc
    def cli(scene, command, check=True):
        check_ipc_directory(env, run)
        result=subprocess.run([str(a.runtime),'--say',scene,command],env=env,text=True,capture_output=True,timeout=6)
        events.append({'scene':scene,'command':command,'exit':result.returncode,'out':result.stdout,'err':result.stderr})
        if check: assert result.returncode==0, events[-1]
        return result.stdout.strip() if result.returncode==0 else None
    def value(scene, name, expected):
        return wait(lambda:cli(scene,'get '+name,False) in ({'true','1'} if expected=='1' else {'false','0'} if expected=='0' else {expected}),scene+'.'+name+'='+expected)
    def launch(role, name):
        path=scenes/role
        arguments=[str(a.runtime),'--scene',str(path),'--answer-as',name,'--no-hud']
        proc=start(arguments,name+'.log')
        wait(lambda: cli(name,'get '+('open' if name=='tohru-session' else 'last_id'),False) is not None,name+' ready')
        wait(lambda:'NVIDIA GeForce RTX 5090' in (evidence/(name+'.log')).read_text(),name+' hardware')
        return proc
    try:
        bus_config=evidence/'bus.conf'
        from xml.sax.saxutils import escape
        bus_config.write_text('<busconfig><type>session</type><listen>unix:path='+escape(str(run/'bus'))+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        bus=start(['dbus-daemon','--nofork','--config-file',str(bus_config)],'bus.log')
        wait(lambda:(run/'bus').exists() and bus.poll() is None,'owned private bus')
        assert (run/'bus').stat().st_uid==os.getuid() and not (run/'bus').is_symlink()
        sway=start(['sway','--unsupported-gpu','-d','-c',str(config)],'sway.log')
        env['SWAYSOCK']=str(wait(lambda:next(run.glob('sway-ipc.*.sock'),None),'private sway socket'))
        env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in run.glob('wayland-*') if not p.name.endswith('.lock')),None),'private display')
        assert 'NVIDIA GeForce RTX 5090' in (evidence/'sway.log').read_text()
        if a.notifications:
            import re
            cookie=run/'pulse-cookie';cookie.write_bytes(os.urandom(256));cookie.chmod(0o600)
            pulse=start([str(a.pulseaudio),'-n','--daemonize=no','--exit-idle-time=-1',
                '--log-target=stderr','--load=module-native-protocol-unix socket='+str(run/'pulse-native')+' auth-anonymous=1',
                '--load=module-null-sink sink_name=private'], 'pulse.log')
            wait(lambda:(run/'pulse-native').exists() and pulse.poll() is None,'owned private audio socket')
            notification=launch('notifications/notifications.plm','tohru-notifications')
            def bus(method,*args):
                result=subprocess.run([str(a.gdbus),'call','--session','--dest','org.freedesktop.Notifications',
                    '--object-path','/org/freedesktop/Notifications','--method',method,*args],
                    env=env,check=True,capture_output=True,text=True,timeout=6)
                return result.stdout
            def owner():
                return subprocess.run([str(a.gdbus),'call','--session','--dest','org.freedesktop.DBus',
                    '--object-path','/org/freedesktop/DBus','--method','org.freedesktop.DBus.GetNameOwner',
                    'org.freedesktop.Notifications'],env=env,capture_output=True,text=True,timeout=6).stdout
            def notify(replace,title,body,timeout=0):
                out=bus('org.freedesktop.Notifications.Notify','private-fixture',str(replace),'',title,body,'[]','{}',str(timeout))
                return int(re.search(r'uint32 (\d+)',out).group(1))
            wait(lambda:owner().startswith('('),'dedicated private notification bus owner')
            identity=owner()
            first=notify(0,'Synthetic title','Synthetic body')
            value('tohru-notifications','last_id',str(first))
            wait(lambda:'Synthetic title' in cli('tohru-notifications','describe json'),'actual native banner')
            wait(lambda:bool(subprocess.run([str(a.pactl),'list','short','sink-inputs'],env=env,
                check=True,capture_output=True,text=True,timeout=6).stdout.strip()),'native placeholder tone on private null sink')
            assert notify(first,'Replacement title','Changed body')==first
            wait(lambda:'Replacement title' in cli('tohru-notifications','describe json'),'native notification replacement')
            cli('tohru-notifications','emit power_warning_low 0.2');value('tohru-notifications','power_level','1')
            cli('tohru-notifications','emit power_warning_clear 0.5');value('tohru-notifications','power_level','0')
            assert 'Replacement title' in cli('tohru-notifications','describe json')
            previous=(evidence/'tohru-notifications.log').read_text().count('notifications.luau running')
            logic=scenes/'notifications/notifications.luau';logic.write_text(logic.read_text()+'\n-- private owner reload\n')
            wait(lambda:(evidence/'tohru-notifications.log').read_text().count('notifications.luau running')>previous,'actual notification VM reload')
            assert owner()==identity
            value('tohru-notifications','last_id',str(first))
            assert notify(first,'Close fixture','Visible before close')==first
            wait(lambda:'Close fixture' in cli('tohru-notifications','describe json'),'native close fixture visible')
            bus('org.freedesktop.Notifications.CloseNotification',str(first))
            wait(lambda:'Close fixture' not in cli('tohru-notifications','describe json'),'native notification close')
            expired=notify(0,'Expiring fixture','Transient body',1000)
            value('tohru-notifications','last_id',str(expired))
            wait(lambda:'Expiring fixture' in cli('tohru-notifications','describe json'),'native expiring fixture visible')
            wait(lambda:'Expiring fixture' not in cli('tohru-notifications','describe json'),'native expiry')
            notification.terminate();notification.wait(timeout=8)
            checks.append('native private Notify/replacement/close/expiry/power events/VM singleton reload and tone routed to owned null sink')
        menu=launch('session/menu.plm','tohru-session')
        cli('tohru-session','emit menu_open');value('tohru-session','open','1')
        cli('tohru-session','emit choose 3');value('tohru-session','confirming','3')
        cli('tohru-session','emit cancel_choice');value('tohru-session','confirming','0')
        cli('tohru-session','emit choose 5');value('tohru-session','confirming','5')
        previous=(evidence/'tohru-session.log').read_text().count('menu.luau running')
        logic=scenes/'session/menu.luau';logic.write_text(logic.read_text()+'\n-- native reload probe\n')
        wait(lambda:(evidence/'tohru-session.log').read_text().count('menu.luau running')>previous,'actual menu VM reload')
        value('tohru-session','confirming','0')
        cli('tohru-session','emit menu_close');value('tohru-session','open','0')
        checks.append('menu native open/confirm/cancel/logic reload/close')
        cli('tohru-session','emit menu_open');cli('tohru-session','emit choose 1');value('tohru-session','open','0')
        wait(lambda:power_log.read_text().splitlines()==['loginctl lock-session'],'lock requests loginctl lock-session')
        cli('tohru-session','emit menu_open');cli('tohru-session','emit choose 2');value('tohru-session','open','0')
        wait(lambda:power_log.read_text().splitlines()==['loginctl lock-session','loginctl lock-session','systemctl suspend'],'sleep locks then suspends')
        checks.append('menu Lock runs loginctl lock-session; Sleep runs loginctl lock-session, then systemctl suspend')
        menu.terminate();menu.wait(timeout=8)
        (evidence/'summary.json').write_text(json.dumps({'checks':checks,'runtime':str(a.runtime),
            'hardware':'NVIDIA RTX 5090 app Vulkan/compositor GLES2','power_commands':power_log.read_text().splitlines(),
            'visual_acceptance':False},indent=2)+'\n')
    finally:
        original_failure = sys.exc_info()[0] is not None
        errors = cleanup(processes, files, run, evidence, events)
        print(evidence,flush=True)
        if errors:
            message='; '.join(errors)
            if original_failure: print('cleanup diagnostics: '+message,file=sys.stderr)
            else: raise RuntimeError(message)

if __name__=='__main__':main()
