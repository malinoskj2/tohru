#!/usr/bin/env python3
"""Interpolate curated cloud-drawn inbetweens with the shared RIFE 4.9 model.
Run through gpu-run and frame-interp-python. It does not download/copy weights.
"""
import importlib.util
import json
import os
from pathlib import Path
import sys
import cv2
import numpy as np
from PIL import Image
import torch

ROOT=Path(__file__).resolve().parents[2]
ART=ROOT/'tools/dragon/art'
spec=importlib.util.spec_from_file_location('pack',Path(__file__).with_name('pack.py'));pack=importlib.util.module_from_spec(spec);spec.loader.exec_module(pack)
SHARE=Path('/nix/store/6shww2pb74ivg46z21s5bwcq0j7210wg-frame-interpolation-0-unstable-2026-11-18/share/frame-interpolation')
os.environ['FRAME_INTERP_SHARE']=str(SHARE)
os.environ['FRAME_INTERP_CKPTS']=str(Path(os.environ['MODEL_ROOT'])/'by-name/frame-interpolation')
sys.path.insert(0,str(SHARE/'comfyui-frame-interpolation'))
from vfi_models.rife import RIFE_VFI
from registration import eyes


def eye_center(im):
    return eyes(np.array(im)).mean(axis=0)


def register_mid(im,desired):
    delta=desired-eye_center(im)
    a=np.array(im);yy,xx=np.where(a[:,:,3]>128)
    tx=int(round(np.clip(delta[0],10-xx.min(),289-xx.max())))
    ty=int(round(np.clip(delta[1],10-yy.min(),289-yy.max())))
    out=Image.new('RGBA',(300,300));out.alpha_composite(im,(tx,ty))
    return out


def main():
    model_root=Path(os.environ['MODEL_ROOT'])
    ready=json.loads((model_root/'readiness.json').read_text())['entries']
    if ready['rife']['status'] != 'ready' or not (model_root/'by-name/frame-interpolation/rife/rife49.pth').is_file():
        raise SystemExit('Shared rife/rife49.pth is not ready; run local-models status on the host. No download attempted.')
    if not (os.environ.get('GPU_LEASE_ID') or os.environ.get('SLURM_JOB_ID')):
        raise SystemExit('Run inside a shared GPU reservation (gpu-run/resource-run).')
    old=Image.open(ART/'legacy-body.webp');neutral=old.crop((1500,6000,1800,6300))
    keys=pack.extract(ART/'hover-source.png');meta=json.loads((ART/'poses.json').read_text())
    sheets=[pack.extract(ART/'inbetweens'/f'row{i}.png',5,15) for i in range(4)]
    registration=json.loads((ART/'inbetweens/registration.json').read_text())
    vfi=RIFE_VFI()
    for i,(key,m) in enumerate(zip(keys,meta)):
        mids=[keys[k['key']] if 'key' in k else sheets[k['sheet']][k['cell']] for k in registration[i]['mids']]
        start,end=np.array([105,97]),np.array(m["eyes"]).mean(axis=0)
        mids=[register_mid(im,start*(1-t)+end*t) for im,t in zip(mids,[.25,.5,.75])]
        source=[neutral,*mids,key]
        arrays=np.stack([np.array(im).astype('float32')/255 for im in source])
        alpha=arrays[:,:,:,3:4]
        colors=arrays[:,:,:,:3]*alpha
        # Native alpha is interpolated as a separate image field. Keep the same
        # input keys and frame times, and clamp alpha above premultiplied RGB.
        rgb=vfi.vfi('rife49.pth',torch.from_numpy(colors),multiplier=4,fast_mode=True,ensemble=True,batch_size=1)[0].numpy()
        a=vfi.vfi('rife49.pth',torch.from_numpy(np.repeat(alpha,3,axis=3)),multiplier=4,fast_mode=True,ensemble=True,batch_size=1)[0].numpy().mean(axis=3)
        a=np.maximum(a,rgb.max(axis=3))
        strip=Image.new('RGBA',(17*300,300))
        for n in range(17):
            if n==0:im=neutral
            elif n==16:im=key
            else:
                rgba=np.dstack((np.clip(rgb[n]/np.maximum(a[n,:,:,None],1/255),0,1),a[n]))
                im=Image.fromarray(np.clip(rgba*255,0,255).astype('uint8'))
            strip.paste(im,(n*300,0))
        strip.save(ART/'transitions'/(m['name']+'.webp'),lossless=True,method=6)
        print('Packed '+m['name'],flush=True)
    # Process lifetime owns cached model and all GPU memory; the reservation is
    # held until this process exits, after CUDA cleanup.
    torch.cuda.synchronize()
    print('peak_gpu_bytes='+str(torch.cuda.max_memory_allocated()),flush=True)

if __name__=='__main__':main()
