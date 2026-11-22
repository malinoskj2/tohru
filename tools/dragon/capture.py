#!/usr/bin/env python3
"""Capture native runtime behavior with CPU encoding. Start review.py and --baseline first."""
import argparse
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import time
from PIL import Image
ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--runtime',required=True);p.add_argument('--only',choices=['all','idle','speech','settling'],default='all');p.add_argument('--idle-seconds',type=int,default=80);args=p.parse_args()
OUT=ROOT/'.tmp/full-body/native-captures';OUT.mkdir(parents=True,exist_ok=True)
env=os.environ|{'WAYLAND_DISPLAY':'hyprland-1'}

def say(*commands,baseline=False):
    subprocess.run([args.runtime,'--say','dragon-body-baseline' if baseline else 'dragon-body-review',*commands],env=env,check=True,stdout=subprocess.DEVNULL)

def screenshot(path,box=None):
    data=subprocess.check_output(['grim','-t','ppm','-'],env=env)
    im=Image.open(io.BytesIO(data))
    if box: im=im.crop(box)
    im.save(path)
    return im.size

w,h=screenshot(OUT/'desktop.png')
box=(w-228,h-248,w-18,h-18)
meta=json.loads((ROOT/'tools/dragon/art/poses.json').read_text())
for i,m in enumerate(meta if args.only == "all" else []):
    say('emit review_key '+str(i));time.sleep(.15)
    screenshot(OUT/(m['name']+'-native.png'),box)
    # Check all views with a genuinely open mouth and their own closed eyes.
    say('fact ai_dg_face '+str(i*24+18));time.sleep(.08)
    screenshot(OUT/(m['name']+'-blink-talk.png'),box)
say('emit review_live');say('emit review_calm',baseline=True)

def record(name,seconds,events,comparison=False):
    x=w-(438 if comparison else 228);y=h-248; width=420 if comparison else 210
    log=open(OUT/(name+'.log'),'w')
    proc=subprocess.Popen(['nix','shell','nixpkgs#wf-recorder','-c','wf-recorder','-y','--no-dmabuf','-D','-g',f'{x},{y} {width}x230','-r','24','-c','libx264','-x','yuv420p','-p','preset=fast','-p','crf=18','-f',str(OUT/(name+'.mp4'))],env=env,stdout=log,stderr=log,start_new_session=True)
    start=time.monotonic();pending=list(events)
    try:
        while time.monotonic()-start < seconds:
            elapsed=time.monotonic()-start
            while pending and elapsed>=pending[0][0]:
                _,commands=pending.pop(0);say(*commands)
            if proc.poll() is not None: raise RuntimeError('Recorder exited early; inspect '+str(OUT/(name+'.log')))
            time.sleep(.1)
    finally:
        os.killpg(proc.pid,signal.SIGINT);proc.wait(timeout=15);log.close()
    if not (OUT/(name+'.mp4')).exists(): raise RuntimeError('No video written')
    print(name,flush=True)

if args.only in ['all','idle']: record('idle-comparison',args.idle_seconds,[(0,['emit review_calm'])],comparison=True)
if args.only == 'all': record('listening-thinking',70,[(0,['emit review_listen']),(35,['emit review_think']),(67,['emit review_calm'])])
events=[(0,['emit review_speak'])]
sentences=['The window uses a small panel.','That is wonderful!','There are several ways to arrange the panels.','The display is ready to show the controls.','Does that layout feel comfortable?','The icon remains near the corner.','A small amount of space keeps the view clear.','The panel can stay open while we talk.']
for i,s in enumerate(sentences): events.append((1+i*8,['text ai_draft '+s,'emit review_sentence']))
events.extend([(60,['emit review_pause']),(64,['emit review_speak']),(68,['emit review_interrupt']),(69,['emit review_calm'])]);events.sort(key=lambda x:x[0])
if args.only in ['all','speech']: record('speaking-interruption',72,events)
if args.only in ['all','speech']: record('reduced-motion',12,[(0,['emit review_reduced 1','emit review_speak']),(2,['text ai_draft That is wonderful!','emit review_sentence']),(10,['emit review_interrupt'])])
if args.only in ['all','settling']:
    record('settling',30,[(0,['emit review_speak']),(1,['text ai_draft That is wonderful!','emit review_sentence']),(12,['emit review_finish'])])
say('emit review_reduced 0','emit review_calm')
print(OUT)
