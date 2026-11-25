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


def extract(rgb,cropped_bottom,panel=None):
    a=np.asarray(rgb,dtype=np.float32)
    if panel is None:
        strips=[a[:10].reshape(-1,3),a[:,:10].reshape(-1,3),a[:,-10:].reshape(-1,3)]
        if not cropped_bottom:strips.append(a[-10:].reshape(-1,3))
        bg=np.median(np.concatenate(strips),axis=0)
        signal=np.max(np.abs(a-bg),axis=-1)
    else:
        bg,signal=plate(a,panel)
    count,labels,stats,_=cv2.connectedComponentsWithStats((signal>22).astype('uint8'),8)
    if count<2 or stats[1:,cv2.CC_STAT_AREA].max()<40:
        if panel is not None:return np.zeros(a.shape[:2]+(4,),'uint8')
        raise RuntimeError('No foreground found')
    areas=stats[1:,cv2.CC_STAT_AREA]
    if panel is None:
        body=(labels==1+np.argmax(areas)).astype('uint8')
    else:
        # A paw gripping the edge is often detached from the peeking head.
        keep=1+np.flatnonzero(areas>=max(150,.04*areas.max()))
        body=np.isin(labels,keep).astype('uint8')
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


def plate(a,edge):
    """Two-tone backdrop for the panel sequences: open background left of the
    edge and the stand-in window right of it, each estimated per frame from
    the frame border. Pixels on the edge itself may be any mix of the two."""
    h,w,_=a.shape
    left=np.concatenate([a[:8,:edge-6].reshape(-1,3),a[-8:,:edge-6].reshape(-1,3),a[:,:8].reshape(-1,3)])
    right=np.concatenate([a[:8,edge+6:].reshape(-1,3),a[-8:,edge+6:].reshape(-1,3),a[:,-8:].reshape(-1,3)])
    bg_left,bg_right=np.median(left,axis=0),np.median(right,axis=0)
    bg=np.where((np.arange(w)<edge)[None,:,None],bg_left,bg_right)*np.ones((h,1,1))
    signal=np.max(np.abs(a-bg),axis=-1)
    band=slice(edge-2,edge+2)
    d=bg_right-bg_left;t=np.clip(np.sum((a[:,band]-bg_left)*d,axis=-1)/max(float(d@d),1),0,1)
    mixed=bg_left+t[...,None]*d
    signal[:,band]=np.max(np.abs(a[:,band]-mixed),axis=-1);bg[:,band]=mixed
    return bg.astype(np.float32),signal


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


def panel_key(source,manifest):
    """A key on the panel canvas as a cell: the canonical atlas cell shifted
    into the crop, or the shared model-drawn peek extracted like any frame."""
    scale=manifest['scale'];x0,y0=crop_origin(manifest);side=round(CELL*scale)
    cell=Image.new('RGBA',(CELL,CELL))
    if 'empty' in source:return cell
    if 'cell' in source:
        atlas=Image.open(ROOT/'shell/assistant/dragon/afk.webp').convert('RGBA');c=source['cell'];e=atlas.width//8
        dx,dy=(source['at'][0]-x0)/scale,(source['at'][1]-y0)/scale
        if dx!=round(dx) or dy!=round(dy) or e!=CELL:raise RuntimeError('panel keys must sit on whole cell pixels')
        cell.alpha_composite(atlas.crop((c%8*e,c//8*e,c%8*e+e,c//8*e+e)),(round(dx),round(dy)))
        return cell
    return panel_cell(Image.open(ROOT/'tools/dragon/art/afk'/(source['image']+'-source.png')).convert('RGB'),manifest)


CELL=240


def crop_origin(manifest):
    """The cell crop: CELL px at the canvas scale, its right side 31 canvas px
    past the panel edge so gripping fingers fit, vertically on the key row.
    Canonical keys then sit a whole 30 cell px right of the crop."""
    side=round(CELL*manifest['scale'])
    return manifest['panel']+31-side,40


def panel_cell(rgb,manifest):
    rgba=extract(rgb,False,panel=manifest['panel'])
    x0,y0=crop_origin(manifest);side=round(CELL*manifest['scale'])
    if rgba[:,:x0-12,3].max()>8 or rgba[:y0,:,3].max()>8 or rgba[y0+side:,:,3].max()>8 or rgba[:,x0+side:,3].max()>8:
        print('warning: foreground outside the cell crop',flush=True)
    cell=np.asarray(Image.fromarray(rgba).crop((x0,y0,x0+side,y0+side)).resize((CELL,CELL),Image.Resampling.LANCZOS)).copy()
    # The overshoot swings her tail tip a few px past the left of the crop
    # for a moment: it fades there instead of ending in a hard cut.
    ramp=np.clip(np.arange(CELL)/10,0,1);ramp=ramp*ramp*(3-2*ramp)
    cell[...,3]=np.rint(cell[...,3]*ramp[None,:]).astype('uint8')
    return Image.fromarray(cell)


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
    if manifest.get('panel'):return process_panel(name,work,manifest,frames,count,loop)
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


def process_panel(name,work,manifest,frames,count,loop):
    """Panel sequences keep the generated occlusion: whatever the stand-in
    window hides is absent and the gripping fingers stay in front, so the cell
    is drawn above the real window with the panel edge on the window edge."""
    if loop:picks=[round(i*(len(frames)-1)/(count-1)) for i in range(count)]
    else:
        # Half equal motion, half equal time: held glances keep frames too.
        small=[np.asarray(Image.open(p).convert('L').resize((96,64)),dtype=np.float32) for p in frames]
        steps=np.array([0.]+[float(np.mean(np.abs(small[i]-small[i-1]))) for i in range(1,len(small))])
        total=.5*np.cumsum(steps)/max(steps.sum(),1e-6)+.5*np.arange(len(frames))/(len(frames)-1)
        picks=[int(np.searchsorted(total,i/(count-1))) for i in range(count)]
        picks[0],picks[-1]=0,len(frames)-1
        for i in range(1,count-1):picks[i]=max(picks[i],picks[i-1]+1)
    cells=work/'cells';cells.mkdir(exist_ok=True)
    for old in cells.glob('*.png'):old.unlink()
    keys={'first':panel_key(manifest['first'],manifest),'last':panel_key(manifest['last'],manifest)}
    for i,index in enumerate(picks):
        if index==0:image=keys['first']
        elif index==len(frames)-1:image=keys['last']
        else:image=panel_cell(Image.open(frames[index]).convert('RGB'),manifest)
        image.save(cells/f'{i:02}.png')
    (work/'cells.json').write_text(json.dumps(dict(sequence=name,picks=picks,loop=loop,cell=CELL,crop=crop_origin(manifest),
        method='per-frame two-tone plate (open background | stand-in window), main component, interior-paint edge unmix; panel occlusion kept'),indent=2)+'\n')
    print(name,'cells',picks)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('names',nargs='*',help='sequence names, or name=take for a take directory')
    parser.add_argument('--count',type=int,default=16)
    parser.add_argument('--cell',type=int,default=240)
    parser.add_argument('--big',type=int,default=1254)
    args=parser.parse_args()
    cv2.setNumThreads(2)
    sequences=json.loads((HERE/'sequences.json').read_text())['sequences']
    names=args.names or [n for n,s in sequences.items() if not s.get('key_only')]
    for name in names:
        base=name.split('=')[0]
        process(name.split('=')[-1],sequences.get(base,{}).get('cells',args.count),args.cell,args.big)


if __name__=='__main__':main()
