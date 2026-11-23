#!/usr/bin/env python3
"""Forced private-display recordings through production AFK/actor modules.

Run under gpu-run. Source events here are explicit harness injections, not
evidence of real keyboard/pointer inactivity. No microphone or models run.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import shlex
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[2]
SCENES=['screen_window','corner_nap','glass_inspection','upside_down_peek','window_perch',
        'tail_chase','sleepy_drift','distant_flight','hide_peek','wing_grooming','waiting_for_you','quiet_rest']


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime',required=True)
    parser.add_argument('--recorder',default=os.environ.get('AFK_RECORDER','wf-recorder'))
    parser.add_argument('--swayidle',default=os.environ.get('AFK_SWAYIDLE','swayidle'))
    parser.add_argument('--scene',choices=SCENES,default='screen_window')
    parser.add_argument('--seconds',type=float,default=29)
    parser.add_argument('--cancel',choices=('none','input','lock','voice','move','close'),default='none')
    parser.add_argument('--output',default='NESTED-1')
    parser.add_argument('--real-idle',action='store_true')
    parser.add_argument('--probability',type=int,default=100)
    parser.add_argument('--auto',action='store_true')
    parser.add_argument('--cycle-input',default='')
    parser.add_argument('--still-at',default='7,10')
    parser.add_argument('--cold',action='store_true')
    args=parser.parse_args()
    if not os.environ.get('GPU_LEASE_ID'):raise SystemExit('gpu-run reservation required')
    if not os.environ.get('AGENT_SANDBOX_INSTANCE'):raise SystemExit('private sandbox required')
    runtime_dir=Path(os.environ['XDG_RUNTIME_DIR'])
    instance=None
    for lock in (runtime_dir/'hypr').glob('*/hyprland.lock'):
        lines=lock.read_text().splitlines()
        try:
            if (runtime_dir/lines[1]).samefile(runtime_dir/'hyprland-1'):instance=lock.parent.name
        except (OSError,IndexError):pass
    if not instance:raise SystemExit('private nested Hyprland unavailable')
    tag='afk-native-'+str(os.getpid())
    out=ROOT/'.tmp/afk-captures'/(args.scene+'-'+('cold-' if args.cold else '')+('real-idle-' if args.real_idle else '')+args.cancel+'-'+time.strftime('%Y%m%d-%H%M%S'))
    out.mkdir(parents=True)
    env={**os.environ,'WAYLAND_DISPLAY':'hyprland-1','HYPRLAND_INSTANCE_SIGNATURE':instance,
         'PATH':str(ROOT/'bin')+':'+os.environ['PATH'],'XDG_CONFIG_HOME':str(out/'config'),
         'XDG_CACHE_HOME':str(out/'cache'),'TOHRU_MANAGED_CONFIG':''}
    def command(parts,check=True):return subprocess.run(parts,env=env,capture_output=True,text=True,timeout=4,check=check)
    def say(*messages):return command([args.runtime,'--say',tag,*messages],False)
    command(['python3',str(ROOT/'tests/assistant/afk-demo.py')]+(['--cold'] if args.cold else []))
    monitors=json.loads(command(['hyprctl','-j','monitors']).stdout)
    (out/'monitors.json').write_text(json.dumps(monitors,indent=2))
    chosen=next(m for m in monitors if m['name']==args.output)
    size,position=('432 768','136 195') if chosen.get('transform',0)%2 else ('940 570','160 145')
    command(['hyprctl','eval',"afk_capture_rule=hl.window_rule({name='"+tag+"',match={class='"+tag+"'},float=true,monitor='"+args.output+"',size='"+size+"',move='"+position+"'})"])
    processes=[];handles=[]
    def start(parts,name):
        log=(out/name).open('w');handles.append(log)
        process=subprocess.Popen(parts,env=env,stdout=log,stderr=log,start_new_session=True)
        processes.append(process);return process
    try:
        terminal=start(['foot','--app-id',tag,'--title','AFK native real-window fixture',
                        'sh','-c','printf "Real compositor window: perch / occlusion fixture\\n"; exec sleep 90'],'window.log')
        time.sleep(.8)
        (out/'window-before.json').write_text(command(['hyprctl','-j','activewindow']).stdout)
        native=start([args.runtime,'--scene',str(ROOT/'.tmp/afk-native-demo/afk.plm'),'--screen',args.output,
                      '--answer-as',tag,'--no-hud','--stall','0','--record',
                      'ai_afk_active,ai_actor_open,ai_actor_skin_open,ai_actor_skin_frame,ai_actor_closeup_open,ai_actor_scale,ai_actor_rotation,ai_flight_x,ai_flight_y,ai_flight_alpha,ai_flight_behind,ai_flight_art,ai_dg_warm'],
                     'native.log')
        time.sleep(3.5)
        if native.poll() is not None:raise RuntimeError('native exited; inspect '+str(out/'native.log'))
        recorder=start([args.recorder,'-D','-o',args.output,'-y','-f',str(out/'native.mp4'),'-c','libx264',
                        '-p','preset=ultrafast','-p','threads=2','-r','24'],'recorder.log')
        time.sleep(.3)
        seconds=3 if args.real_idle else 300
        say('text test_output '+args.output,'emit test_idle_seconds '+str(seconds),
            'emit test_probability '+str(args.probability),'emit test_auto '+str(int(args.auto)),
            'emit test_scene '+str(SCENES.index(args.scene)+1))
        time.sleep(.1)
        observer=None
        if args.real_idle:
            say('emit afk_source 777','emit afk_threshold 3','emit afk_safe 777','emit afk_resume 777')
            command(['wtype','idle-source-start\n'])
            callback=out/'compositor-callback.py'
            callback.write_text('import sys,subprocess,json,time\nfrom pathlib import Path\n'
                +'event=sys.argv[1]\n'
                +'with Path('+repr(str(out/'compositor-events.jsonl'))+').open("a") as f:f.write(json.dumps({"event":event,"monotonic":time.monotonic()})+"\\n")\n'
                +'messages=["emit afk_source 777","emit afk_threshold 3","emit afk_safe 777","emit afk_idle 777"] if event=="idle" else ["emit afk_resume 777"]\n'
                +'subprocess.run(['+repr(args.runtime)+',"--say",'+repr(tag)+']+messages,check=True,timeout=2)\n')
            observer=start([args.swayidle,
                '-C','/dev/null','timeout','3',shlex.join([sys.executable,str(callback),'idle']),
                'resume',shlex.join([sys.executable,str(callback),'resume'])],'compositor-observer.log')
        else:
            say('emit afk_source 777','emit afk_threshold 300','emit afk_safe 777','emit afk_resume 777','emit afk_idle 777')
        start_time=time.monotonic();next_heartbeat=start_time+1.5;cancelled=False
        samples=[]
        cycle_inputs=[float(value) for value in args.cycle_input.split(',') if value]
        stills=[float(value) for value in args.still_at.split(',') if value]
        while time.monotonic()-start_time<args.seconds:
            elapsed=time.monotonic()-start_time
            if cycle_inputs and elapsed>=cycle_inputs[0]:
                command(['wtype','resume-episode-'+str(len(cycle_inputs))+'\n'])
                cycle_inputs.pop(0)
            if stills and elapsed>=stills[0]:
                command(['grim','-o',args.output,str(out/('native-'+str(stills.pop(0))+'s.png'))])
            if time.monotonic()>=next_heartbeat:
                say('emit afk_source 777')
                samples.append({'t':elapsed,'status':say('get ai_afk_status').stdout,'trace':say('get ai_afk_trace').stdout})
                next_heartbeat=time.monotonic()+1.5
            if args.cancel!='none' and not cancelled and elapsed>8:
                cancelled=True
                if args.cancel=='input':
                    if args.real_idle:
                        command(['wtype','resume-input-not-swallowed\n'])
                        time.sleep(.4)
                        observer.terminate();observer.wait(timeout=2)
                    else:say('emit afk_resume 777')
                elif args.cancel=='lock':say('emit afk_unsafe 777')
                elif args.cancel=='voice':say('emit test_voice 1')
                elif args.cancel=='move':
                    command(['hyprctl','eval',"hl.dispatch(hl.dsp.window.move({x=320,y=220,relative=false,window='class:"+tag+"'}))"])
                    (out/'window-moved.json').write_text(command(['hyprctl','-j','activewindow']).stdout)
                elif args.cancel=='close':terminal.terminate()
            time.sleep(.08)
        (out/'status.json').write_text(json.dumps(samples,indent=2))
        (out/'bindings-after.json').write_text(json.dumps({key:say('get '+key).stdout.strip() for key in
            ('ai_actor_skin_src','ai_actor_closeup_src','ai_afk_body_src','ai_actor_open','ai_afk_active')},indent=2))
        recorder.send_signal(signal.SIGINT);recorder.wait(timeout=5)
        (out/'layers-after.json').write_text(command(['hyprctl','-j','layers']).stdout)
        (out/'evidence.json').write_text(json.dumps({'kind':'forced native AFK scene','scene':args.scene,
            'cancel':args.cancel,'runtime':args.runtime,'compositor_inactivity_observed':args.real_idle,
            'logind_acceptance':False,'probability':args.probability,'auto':args.auto,
            'cold_shared_art':args.cold,
            'revision':command(['git','rev-parse','HEAD']).stdout.strip(),
            'source_sha256':{str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in
                list((ROOT/'shell/assistant').glob('afk*'))+[ROOT/'shell/assistant/dragon.plm',ROOT/'shell/assistant/dragon/afk-detail.wgsl'] if path.is_file()},
            'source':'maintained swayidle private-seat notifications and injected keyboard resume; session safety fixture' if args.real_idle else 'explicit harness events through production AFK composition','output':args.output,
            'clip':str(out/'native.mp4')},indent=2))
        print(out,flush=True)
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGTERM)
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
        for handle in handles:handle.close()
        command(['hyprctl','eval','if afk_capture_rule then afk_capture_rule:set_enabled(false); afk_capture_rule=nil end'],False)


if __name__=='__main__':main()
