#!/usr/bin/env python3
"""Private native scene/protocol checks; no auth.check or power request is issued."""
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

def validate_pam_tools(mode, wtype, bwrap, modules):
    if not mode: return
    if not bwrap or not modules:
        raise ValueError('controlled PAM requires explicit --bwrap and --pam-modules; no host fallback')
    for name, path in [('wtype',wtype),('bwrap',bwrap)]:
        if not path or not path.is_file() or not os.access(path,os.X_OK):
            raise ValueError('controlled PAM requires explicit executable --'+name+' before startup')
    if not modules.is_dir():
        raise ValueError('controlled PAM requires existing private fixture module directory')

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime', type=Path, required=True)
    p.add_argument('--wtype', type=Path)
    p.add_argument('--notifications',action='store_true')
    p.add_argument('--pulseaudio',type=Path)
    p.add_argument('--gdbus',type=Path)
    p.add_argument('--pactl',type=Path)
    p.add_argument('--pam-mode', choices=['permit','deny','account-deny'])
    p.add_argument('--bwrap', type=Path)
    p.add_argument('--pam-modules', type=Path)
    a = p.parse_args()
    try: validate_pam_tools(a.pam_mode,a.wtype,a.bwrap,a.pam_modules)
    except ValueError as error: p.error(str(error))
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
    # Last-resort denial: no systemctl/loginctl implementation can act on host.
    fakebin = evidence/'bin'; fakebin.mkdir()
    for name in ['systemctl','loginctl']:
        f = fakebin/name; f.write_text('#!/bin/sh\nexit 125\n'); f.chmod(0o755)
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
        if name=='tohru-lock': arguments += ['--record','password']
        if name=='tohru-lock' and a.pam_mode:
            from pam_fixture import command
            arguments,digest=command(arguments,a.bwrap,a.pam_modules,evidence,run,a.pam_mode)
            (evidence/'pam-fixture.sha256').write_text(digest+'\n')
        proc=start(arguments,name+'.log')
        wait(lambda: cli(name,'get '+('open' if name=='tohru-session' else 'auth.ready' if name=='tohru-polkit' else 'last_id' if name=='tohru-notifications' else 'engaged'),False) is not None,name+' ready')
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
            bus('org.freedesktop.Notifications.CloseNotification',str(first))
            wait(lambda:'Replacement title' not in cli('tohru-notifications','describe json'),'native notification close')
            expired=notify(0,'Expiring fixture','Transient body',150)
            value('tohru-notifications','last_id',str(expired))
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
        lock=launch('lock/lock.plm','tohru-lock')
        cli('tohru-session','emit menu_open');cli('tohru-session','emit choose 1')
        value('tohru-lock','lock.held','1');value('tohru-lock','acknowledged','1')
        checks.append('menu routes lock to dedicated native owner; actual held acknowledgment mirror')
        menu.terminate();menu.wait(timeout=8)
        value('tohru-lock','lock.held','1')
        checks.append('ordinary menu process termination leaves lock held')
        previous=(evidence/'tohru-lock.log').read_text().count('lock.luau running')
        logic=scenes/'lock/lock.luau';logic.write_text(logic.read_text()+'\n-- native locked reload probe\n')
        wait(lambda:(evidence/'tohru-lock.log').read_text().count('lock.luau running')>previous,'actual lock VM reload')
        value('tohru-lock','lock.held','1');value('tohru-lock','engaged','1')
        checks.append('engaged lock retained after logic reload')
        subprocess.run(['swaymsg','create_output'],env=env,check=True,capture_output=True,timeout=6)
        wait(lambda:'arrived while locked: it has its face' in (evidence/'tohru-lock.log').read_text(),'hotplug lock face')
        subprocess.run(['swaymsg','output','HEADLESS-1','disable'],env=env,check=True,capture_output=True,timeout=6)
        value('tohru-lock','lock.held','1')
        checks.append('native lock output arrival and first-output removal')
        # Changed-size configure must ACK/update viewport before any lock commit.
        for width,height in [(1600,900),(1024,768),(1920,1080),(1280,800)]:
            subprocess.run(['swaymsg','output','HEADLESS-2','mode',f'{width}x{height}@60Hz'],
                           env=env,check=True,capture_output=True,timeout=6)
            def resized():
                outputs=json.loads(subprocess.run(['swaymsg','-t','get_outputs','-r'],
                    env=env,check=True,capture_output=True,text=True,timeout=6).stdout)
                return any(output['name']=='HEADLESS-2' and
                    output['current_mode']['width']==width and
                    output['current_mode']['height']==height for output in outputs)
            wait(resized,'private output resize')
            value('tohru-lock','lock.held','1')
            time.sleep(.15)
            assert lock.poll() is None and 'Protocol error' not in (evidence/'tohru-lock.log').read_text()
        checks.append('four actual surviving-lock output resizes retain held faces; raw ACK/viewport/commit trace recorded')
        # These are synthetic sentinels, never real credentials and never submitted.
        marker='synthetic-lock-secret-7c9d'
        cli('tohru-lock','text password '+marker)
        assert 'secret input cannot be read' in cli('tohru-lock','get password')
        assert 'secret input cannot be read' in cli('tohru-lock','wait password == "'+marker+'" 1ms')
        assert marker not in cli('tohru-lock','describe json')
        assert marker not in cli('tohru-lock','watch 0.1 --now password')
        assert marker not in (evidence/'tohru-lock.log').read_text()
        cli('tohru-lock','text password ')
        checks.append('native secret get/wait predicate/describe/watch/record hide synthetic lock input')
        definition=scenes/'lock/lock.plm'
        definition.write_text(definition.read_text()+'\n// definition freeze probe\n')
        wait(lambda:'scene definition is frozen' in (evidence/'tohru-lock.log').read_text(),'native locked definition freeze')
        cli('tohru-lock','fact engaged false');value('tohru-lock','lock.held','1')
        if a.wtype:
            import select
            cli('tohru-lock','focus password')
            observer=subprocess.Popen([str(a.runtime),'--say','tohru-lock','watch 0 --now password'],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            processes.append(observer)
            def next_line():
                ready,_,_=select.select([observer.stdout],[],[],3)
                if not ready: raise AssertionError('secret change observer did not respond')
                return observer.stdout.readline()
            assert next_line().startswith('hidden password')
            subprocess.run([str(a.wtype),'synthetic-editable-field'],env=env,check=True,capture_output=True,timeout=8)
            assert next_line().startswith('hidden password')
            observer.terminate();observer.communicate(timeout=5)
            cli('tohru-lock','text password ')
            checks.append('actual private virtual keyboard edits drawn secret input while public open=false')

        cli('tohru-lock','fact lock.held false');value('tohru-lock','lock.held','1')
        cli('tohru-lock','emit lock_request');value('tohru-lock','engaged','1')
        checks.append('public open/held fact writes and definition reload cannot release native lock')
        if a.pam_mode:
            digest=(evidence/'pam-fixture.sha256').read_text().strip()
            wait(lambda:('PRIVATE_PAM_FIXTURE_VERIFIED '+digest) in (evidence/'tohru-lock.log').read_text(),'verified private PAM mount')
            cli('tohru-lock','submit password synthetic-fixture-credential')
            if a.pam_mode=='permit':
                wait(lambda:(evidence/'pam-started').exists(),'native PAM fixture entered')
                # Queue a new lock behind blocking PAM, while outputs configure.
                cli('tohru-lock','emit lock_request')
                subprocess.run(['swaymsg','create_output'],env=env,check=True,capture_output=True,timeout=6)
                (evidence/'pam-continue').write_text('continue')
                wait(lambda:'lock   · authenticated release' in (evidence/'tohru-lock.log').read_text(),'controlled PAM native release')
                wait(lambda:(evidence/'tohru-lock.log').read_text().count('lock   · the session is locked')>=2,'fresh lock cycle after queued request')
                value('tohru-lock','lock.held','1')
                released=(evidence/'tohru-lock.log').read_text().count('lock   · authenticated release')
                assert released==1
                cli('tohru-lock','fact engaged false');value('tohru-lock','lock.held','1')
                cli('tohru-lock','fact acknowledged true');value('tohru-lock','lock.held','1')
                assert (evidence/'tohru-lock.log').read_text().count('lock   · authenticated release')==released
                checks.append('new cycle cannot reuse consumed PAM authority through public state writes')
                checks.append('controlled PAM success with queued new lock and hotplug produces authenticated release and fresh held cycle')
            else:
                wait(lambda:'Password not accepted' in (cli('tohru-lock','get status',False) or ''),'controlled PAM denial')
                value('tohru-lock','lock.held','1');value('tohru-lock','checking','0')
                assert 'lock   · authenticated release' not in (evidence/'tohru-lock.log').read_text()
                checks.append('controlled native '+a.pam_mode+' retains held lock without release')
        polkit=launch('polkit/polkit.plm','tohru-polkit')
        value('tohru-polkit','auth.ready','0')
        marker='synthetic-polkit-secret-183a'
        cli('tohru-polkit','text answer '+marker)
        assert 'secret input cannot be read' in cli('tohru-polkit','get answer')
        assert 'secret input cannot be read' in cli('tohru-polkit','wait answer has "probe" 1ms')
        assert marker not in cli('tohru-polkit','describe json')
        cli('tohru-polkit','text answer ')
        polkit.terminate();polkit.wait(timeout=8)
        checks.append('private native polkit absent-authority state and secret get/wait/describe protection')
        before=(evidence/'tohru-lock.log').read_text().count('lock   · authenticated release')
        lock.kill();lock.wait(timeout=8)
        assert (evidence/'tohru-lock.log').read_text().count('lock   · authenticated release')==before
        wait(lambda:'session lock abandoned' in (evidence/'sway.log').read_text(),'compositor abandoned-lock fallback')
        checks.append('locker SIGKILL emits no unlock; compositor reports session lock abandoned')
        (evidence/'summary.json').write_text(json.dumps({'checks':checks,'runtime':str(a.runtime),
            'hardware':'NVIDIA RTX 5090 app Vulkan/compositor GLES2','auth_power_tested':False,'controlled_pam_fixture':a.pam_mode,
            'security_acceptance':False,'visual_acceptance':False},indent=2)+'\n')
    finally:
        original_failure = sys.exc_info()[0] is not None
        errors = cleanup(processes, files, run, evidence, events)
        print(evidence,flush=True)
        if errors:
            message='; '.join(errors)
            if original_failure: print('cleanup diagnostics: '+message,file=sys.stderr)
            else: raise RuntimeError(message)

if __name__=='__main__':main()
