#!/usr/bin/env python3
"""Private native scene/protocol checks; no auth.check or power request is issued."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]

def wait(fn, label, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = fn()
        if result:
            return result
        time.sleep(.05)
    raise AssertionError('timed out: ' + label)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime', type=Path, required=True)
    a = p.parse_args()
    assert os.environ.get('TOHRU_PRIVATE_SESSION_TEST') == '1'
    assert os.environ.get('DBUS_SESSION_BUS_ADDRESS', '').startswith('unix:')
    evidence = ROOT / '.tmp' / ('native-session-' + str(time.time_ns()))
    evidence.mkdir()
    run = ROOT / '.tmp' / ('l' + str(os.getpid()))
    run.mkdir(mode=0o700)
    (run/'ipc').mkdir(mode=0o700)
    scenes = evidence / 'shell'
    shutil.copytree(ROOT / 'shell', scenes)
    home = evidence / 'home'; home.mkdir()
    # Native processes cannot reach the live authority through either bus.
    env = {**os.environ, 'HOME':str(home), 'XDG_RUNTIME_DIR':str(run),
           'XDG_CONFIG_HOME':str(evidence/'config'), 'XDG_CACHE_HOME':str(evidence/'cache'),
           'XDG_DATA_HOME':str(evidence/'data'), 'XDG_STATE_HOME':str(evidence/'state'),
           'DBUS_SYSTEM_BUS_ADDRESS':os.environ['DBUS_SESSION_BUS_ADDRESS'],
           'PLEAMAR_SOCKETS':str(run/'ipc'),
           'WLR_BACKENDS':'headless', 'WLR_HEADLESS_OUTPUTS':'1', 'WLR_RENDERER':'gles2',
           'WLR_LIBINPUT_NO_DEVICES':'1', 'WLR_RENDER_DRM_DEVICE':'/dev/dri/renderD128',
           'VK_DRIVER_FILES':'/run/opengl-driver/share/vulkan/icd.d/nvidia_icd.json',
           'WGPU_BACKEND':'vulkan', 'PLEAMAR_BACKEND':'vulkan'}
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
        f=(evidence/log).open('w');files.append(f)
        proc=subprocess.Popen(args, env=env, stdout=f, stderr=subprocess.STDOUT)
        processes.append(proc);return proc
    def cli(scene, command, check=True):
        result=subprocess.run([str(a.runtime),'--say',scene,command],env=env,text=True,capture_output=True,timeout=6)
        events.append({'scene':scene,'command':command,'exit':result.returncode,'out':result.stdout,'err':result.stderr})
        if check: assert result.returncode==0, events[-1]
        return result.stdout.strip() if result.returncode==0 else None
    def value(scene, name, expected):
        return wait(lambda:cli(scene,'get '+name,False) in ({'true','1'} if expected=='1' else {'false','0'} if expected=='0' else {expected}),scene+'.'+name+'='+expected)
    def launch(role, name):
        path=scenes/role
        proc=start([str(a.runtime),'--scene',str(path),'--answer-as',name,'--no-hud'],name+'.log')
        wait(lambda: cli(name,'get '+('open' if name=='tohru-session' else 'engaged'),False) is not None,name+' ready')
        wait(lambda:'NVIDIA GeForce RTX 5090' in (evidence/(name+'.log')).read_text(),name+' hardware')
        return proc
    try:
        sway=start(['sway','--unsupported-gpu','-d','-c',str(config)],'sway.log')
        env['SWAYSOCK']=str(wait(lambda:next(run.glob('sway-ipc.*.sock'),None),'private sway socket'))
        env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in run.glob('wayland-*') if not p.name.endswith('.lock')),None),'private display')
        assert 'NVIDIA GeForce RTX 5090' in (evidence/'sway.log').read_text()
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
        # Demonstrate known unpatched bypass on private compositor; no security acceptance.
        cli('tohru-lock','fact engaged false');value('tohru-lock','lock.held','0')
        checks.append('known unpatched native IPC bypass reproduced privately')
        cli('tohru-lock','emit lock_request');value('tohru-lock','lock.held','1')
        before=(evidence/'tohru-lock.log').read_text().count('lock   · unlocked')
        lock.kill();lock.wait(timeout=8)
        assert (evidence/'tohru-lock.log').read_text().count('lock   · unlocked')==before
        wait(lambda:'session lock abandoned' in (evidence/'sway.log').read_text(),'compositor abandoned-lock fallback')
        checks.append('locker SIGKILL emits no unlock; compositor reports session lock abandoned')
        (evidence/'summary.json').write_text(json.dumps({'checks':checks,'runtime':str(a.runtime),
            'hardware':'NVIDIA RTX 5090 app Vulkan/compositor GLES2','auth_power_tested':False,
            'security_acceptance':False,'visual_acceptance':False},indent=2)+'\n')
    finally:
        (evidence/'events.json').write_text(json.dumps(events,indent=2)+'\n')
        for proc in reversed(processes):
            if proc.poll() is None:
                proc.terminate()
                try:proc.wait(timeout=8)
                except subprocess.TimeoutExpired:proc.kill();proc.wait(timeout=3)
        for f in files:f.close()
        shutil.rmtree(run)
        print(evidence,flush=True)

if __name__=='__main__':main()
