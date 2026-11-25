#!/usr/bin/env python3
"""Extract registered RGBA cells from generated FL2VA frames.

The generated video sits on a uniform background. Each frame's background is
estimated from its border, the main connected character is kept, and edge
alpha is un-mixed against nearby interior paint. The canvas transform used by
generate.py is inverted so frame 0 lands exactly where the source key sits.
Frames are chosen at equal steps of cumulative motion, so the motion progresses
evenly; loops are chosen at equal steps of time.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

import cv2
import numpy as np
from PIL import Image
from scipy.ndimage import distance_transform_edt

HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[2]
OUT=ROOT/'.tmp/afk-motion'


def extract(rgb,cropped_bottom):
    a=np.asarray(rgb,dtype=np.float32)
    strips=[a[:10].reshape(-1,3),a[:,:10].reshape(-1,3),a[:,-10:].reshape(-1,3)]
    if not cropped_bottom:strips.append(a[-10:].reshape(-1,3))
    bg=np.median(np.concatenate(strips),axis=0)
    signal=np.max(np.abs(a-bg),axis=-1)
    count,labels,stats,_=cv2.connectedComponentsWithStats((signal>22).astype('uint8'),8)
    if count<2:raise RuntimeError('No foreground found')
    label=1+np.argmax(stats[1:,cv2.CC_STAT_AREA]);body=(labels==label).astype('uint8')
    body=cv2.morphologyEx(body,cv2.MORPH_CLOSE,np.ones((3,3),'uint8'))
    filled=body.copy();h,w=body.shape
    flood=np.zeros((h+2,w+2),'uint8');cv2.floodFill(filled,flood,(0,0),1)
    body=body|(1-filled)
    support=cv2.dilate(body,np.ones((5,5),'uint8'))
    core=cv2.erode(body,np.ones((7,7),'uint8'))
    _,nearest=distance_transform_edt(core==0,return_indices=True)
    local=a[nearest[0],nearest[1]]-bg
    alpha=np.clip(np.sum((a-bg)*local,axis=-1)/np.maximum(np.sum(local*local,axis=-1),1),0,1)
    alpha=np.where(core,1,alpha)*support*np.clip((signal-6)/8,0,1)
    # Membrane that lets the dark backdrop through stays translucent inside the
    # body, while backdrop enclosed between wing and body clears completely.
    alpha=np.where(core,np.clip((signal-14)/26,0,1),alpha)
    foreground=np.clip((a-(1-alpha[...,None])*bg)/np.maximum(alpha[...,None],1/255),0,255)
    # Edge colour comes from the nearest interior paint, so no dark backdrop fringe survives.
    foreground=np.where(core[...,None]>0,foreground,a[nearest[0],nearest[1]])
    foreground[alpha==0]=0
    return np.dstack([foreground,np.rint(alpha*255)]).astype('uint8')


def feather(image):
    """Wing tips that flutter past the bust's sides fade out instead of
    ending in a hard canvas-edge cut on screen."""
    rgba=np.asarray(image).copy();w=rgba.shape[1];edge=max(8,w//32)
    ramp=np.clip(np.minimum(np.arange(w),w-1-np.arange(w))/edge,0,1)
    rgba[...,3]=np.rint(rgba[...,3]*(ramp*ramp*(3-2*ramp))[None,:]).astype('uint8')
    return Image.fromarray(rgba)


def key(source,size):
    if 'cell' in source:
        atlas=Image.open(ROOT/'shell/assistant/dragon/afk.webp').convert('RGBA')
        cell=source['cell'];edge=atlas.width//8;x,y=cell%8*edge,cell//8*edge
        return atlas.crop((x,y,x+edge,y+edge)).resize((size,size),Image.Resampling.LANCZOS)
    return Image.open(ROOT/'tools/dragon/art/afk'/(source['image']+'-source.png')).convert('RGBA').resize((size,size),Image.Resampling.LANCZOS)


def choose(frames,count,loop,cycle):
    small=[np.asarray(Image.open(p).convert('L').resize((96,96)),dtype=np.float32) for p in frames]
    if cycle:
        # A seamless stretch of steady flapping: the window that closes on
        # itself best while still moving, sampled at equal time steps.
        def score(start,period):
            closure=float(np.mean(np.abs(small[start]-small[start+period])))
            motion=float(np.mean([np.mean(np.abs(small[start+k]-small[start])) for k in range(period)]))
            return closure-.3*motion
        start,period=min(((s,n) for s in range(4,64) for n in range(count,2*count+1) if s+n<len(small)-12),key=lambda c:score(*c))
        return [start+round(i*period/count) for i in range(count)]
    if loop:return [round(i*(len(frames)-1)/count) for i in range(count)]
    steps=[0.]+[float(np.mean(np.abs(small[i]-small[i-1]))) for i in range(1,len(small))]
    total=np.cumsum(steps);total/=max(total[-1],1e-6)
    picks=[int(np.searchsorted(total,i/(count-1))) for i in range(count)]
    picks[0],picks[-1]=0,len(frames)-1
    # Never repeat a frame: a held duplicate reads as a hitch.
    for i in range(1,count-1):picks[i]=max(picks[i],picks[i-1]+1)
    for i in range(count-2,0,-1):picks[i]=min(picks[i],picks[i+1]-1)
    return picks


def upscale(paths,target):
    """Real-ESRGAN x4 anime (shared alias realesrgan-x4plus-anime) for the
    large closeups, so a 768 px generation keeps native detail at 1254 px."""
    model=Path(os.environ['MODEL_ROOT'])/'by-name/realesrgan-x4plus-anime'
    ready=json.loads((Path(os.environ['MODEL_ROOT'])/'readiness.json').read_text())
    if ready['entries'].get('realesrgan-x4plus-anime',{}).get('status')!='ready':
        raise RuntimeError('realesrgan-x4plus-anime is not ready; host: systemctl --user start local-models-sync.service')
    with tempfile.TemporaryDirectory(dir=OUT) as tmp:
        src,dst=Path(tmp,'in'),Path(tmp,'out');src.mkdir();dst.mkdir()
        for i,p in enumerate(paths):shutil.copy(p,src/f'{i:03}.png')
        tool=['realesrgan-ncnn-vulkan'] if shutil.which('realesrgan-ncnn-vulkan') else ['nix','shell','nixpkgs#realesrgan-ncnn-vulkan','-c','realesrgan-ncnn-vulkan']
        command=[*tool,'-i',str(src),'-o',str(dst),'-m',str(model),'-n','realesrgan-x4plus-anime','-f','png']
        if not os.environ.get('GPU_LEASE_ID'):
            command=['gpu-run','--vram','3GiB','--name','Tohru AFK closeup upscale','--',*command]
        subprocess.run(command,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        return [Image.open(dst/f'{i:03}.png').convert('RGB').resize((target,target),Image.Resampling.LANCZOS) for i in range(len(paths))]


def process(name,count,cell,big):
    work=OUT/name;manifest=json.loads((work/'inputs/manifest.json').read_text())
    frames=sorted((work/'frames').glob('*.png'))
    if len(frames)!=124:raise RuntimeError(f'{name}: expected 124 decoded frames, found {len(frames)}')
    loop=manifest['first']==manifest['last']
    cycle=json.loads((HERE/'sequences.json').read_text())['sequences'][name].get('cycle',False)
    picks=choose(frames,count,loop,cycle)
    size,margin=manifest['size'],manifest['margin']
    cells=work/'cells';cells.mkdir(exist_ok=True)
    for old in cells.glob('*.png'):old.unlink()
    target=big if 'image' in manifest['first'] else cell
    keys={'first':key(manifest['first'],target),'last':key(manifest['last'],target)}
    sources={index:Image.open(frames[index]).convert('RGB') for index in picks}
    if target>size:sources=dict(zip(picks,upscale([frames[index] for index in picks],target)))
    scale=sources[picks[0]].width/size
    for i,index in enumerate(picks):
        # Endpoints are the exact canonical keys, so a sequence joins the
        # authored poses without a pop.
        if index==0 and not cycle:keys['first'].save(cells/f'{i:02}.png');continue
        if index==len(frames)-1 and not loop:keys['last'].save(cells/f'{i:02}.png');continue
        rgba=extract(sources[index],cropped_bottom=manifest['first'].get('image')=='bust')
        m=round(margin*scale);edge=sources[index].width
        image=Image.fromarray(rgba).crop((m,m,edge-m,edge-m))
        image=image.resize((target,target),Image.Resampling.LANCZOS)
        if manifest['first'].get('image')=='bust':image=feather(image)
        image.save(cells/f'{i:02}.png')
    (work/'cells.json').write_text(json.dumps(dict(sequence=name,picks=picks,loop=loop,cell=target,
        method='border-estimated uniform background, main component with holes filled, interior-paint edge unmix'
            +(', realesrgan-x4plus-anime upscale' if target>size else '')),indent=2)+'\n')
    print(name,'cells',picks)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('names',nargs='*')
    parser.add_argument('--count',type=int,default=16)
    parser.add_argument('--cell',type=int,default=240)
    parser.add_argument('--big',type=int,default=1254)
    args=parser.parse_args()
    cv2.setNumThreads(2)
    names=args.names or list(json.loads((HERE/'sequences.json').read_text())['sequences'])
    for name in names:process(name,args.count,args.cell,args.big)


if __name__=='__main__':main()
