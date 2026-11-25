#!/usr/bin/env python3
"""Isolated FL2VA stages using the installed native ComfyUI H3 implementation.

The disk-backed loader initialization follows the existing tested local H3
runner. Each stage exits before the next loads its model; no server is started.
"""
import ctypes
import gc
import json
import os
from pathlib import Path
import sys
import time

if not os.environ.get('GPU_LEASE_ID'):
    raise SystemExit('FL2VA stage requires foreground shared-scheduler admission')
stage,job_path=sys.argv[1:]
job=json.loads(Path(job_path).read_text());work=Path(job_path).parent
models=Path(job['models']);backend=Path(job['backend'])
sys.path.insert(0,str(backend))
sys.argv=[sys.argv[0],'--disable-pinned-memory','--fast-disk','--reserve-vram','3']
import comfy.options
comfy.options.enable_args_parsing()
import comfy_aimdo.control
comfy_aimdo.control.init(simple_vram_headroom=3*1024**3,nvml_pressure=True)
import torch
import comfy.sd
import comfy.model_management as mm
import comfy.memory_management
import comfy.model_patcher
import comfy.nested_tensor
import comfy.utils
import folder_paths
import nodes
import node_helpers
from comfy_extras.nodes_minimax_h3 import MiniMaxH3SigmaShift,_empty_av_latent

torch.set_num_threads(2)
for name,fn in [('output',folder_paths.set_output_directory),('temp',folder_paths.set_temp_directory),('user',folder_paths.set_user_directory)]:
    path=work/'runtime'/name;path.mkdir(parents=True,exist_ok=True);fn(str(path))
if not comfy_aimdo.control.init_devices((d.index,0) for d in mm.get_all_torch_devices()):
    raise RuntimeError('Disk-backed loader failed; refusing an unbounded CPU-copy fallback')
comfy.model_patcher.CoreModelPatcher=comfy.model_patcher.ModelPatcherDynamic
comfy.memory_management.aimdo_enabled=True

def event(name):
    gc.collect();ctypes.CDLL(None).malloc_trim(0);torch.cuda.synchronize()
    free,total=torch.cuda.mem_get_info()
    data=dict(stage=stage,event=name,time=time.time(),gpu_device_used=total-free,gpu_allocated=torch.cuda.memory_allocated(),gpu_reserved=torch.cuda.memory_reserved(),gpu_peak=torch.cuda.max_memory_allocated())
    with (work/'events.jsonl').open('a') as f:f.write(json.dumps(data)+'\n')
    print(json.dumps(data),flush=True)

def save(value,name):
    def cpu(v):
        if isinstance(v,torch.Tensor):return v.detach().cpu()
        if isinstance(v,dict):return {k:cpu(x) for k,x in v.items()}
        if isinstance(v,(list,tuple)):return [cpu(x) for x in v]
        return v
    torch.save(cpu(value),work/name)

def load(name):return torch.load(work/name,map_location='cpu',weights_only=True)
def image(path):
    import numpy as np
    from PIL import Image
    with Image.open(path) as im:
        if im.size!=(job['width'],job['height']):raise ValueError('Keyframe must already match the exact canvas')
        return torch.from_numpy(np.array(im.convert('RGB'))).float().unsqueeze(0)/255
def video_vae():return comfy.sd.VAE(sd=comfy.utils.load_torch_file(str(models/'minimax_h3_video_vae_fp16.safetensors')))

with torch.inference_mode():
    event('start')
    if stage=='probe':
        import comfy_kitchen as ck
        if torch.cuda.get_device_capability()[0]<10:raise RuntimeError('Selected NVFP4 checkpoint requires Blackwell')
        scale=torch.ones((),device='cuda');x=torch.randn(128,128,device='cuda',dtype=torch.bfloat16)
        q,blocks=ck.quantize_nvfp4(x,scale);result=ck.scaled_mm_nvfp4(q,q,scale,scale,blocks,blocks)
        if not torch.isfinite(result).all():raise RuntimeError('NVFP4 kernel probe failed')
    elif stage=='anchors':
        vae=video_vae();anchors=[]
        keyframes=[(0,job['first']),(job['frames']-1,job['last'])]
        if job.get('guide'):keyframes.append((job['guide']['frame'],job['guide']['path']))
        for frame,path in sorted(keyframes):
            anchors.append(dict(resolved_frame_index=frame,latent=vae.encode(image(path)).detach().cpu()))
            event('anchor:'+str(frame))
        save(anchors,'anchors.pt')
    elif stage=='encode':
        clip=comfy.sd.load_clip([str(models/'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors')],clip_type=comfy.sd.CLIPType.MINIMAX)
        event('encoder-loaded')
        # These are actual FL2VA first/last latent anchors, not Ref2VA media.
        tokens=clip.tokenize(job['prompt'],images=[image(job['first']),image(job['last'])])
        positive=clip.encode_from_tokens_scheduled(tokens)
        anchors=load('anchors.pt')
        expected=sorted([0,job['frames']-1]+([job['guide']['frame']] if job.get('guide') else []))
        if [a['resolved_frame_index'] for a in anchors]!=expected:raise RuntimeError('Incorrect image anchor indices')
        print('Native minimax_keyframes indices:',expected,flush=True)
        positive=node_helpers.conditioning_set_values(positive,{'minimax_keyframes':anchors})
        latent,count=_empty_av_latent(job['width'],job['height'],job['frames'])
        if count!=job['frames']:raise RuntimeError('Unexpected H3 frame-grid adjustment')
        save(dict(positive=positive,latent=latent['samples'].tensors),'conditioning.pt')
    elif stage=='sample':
        data=load('conditioning.pt')
        model=comfy.sd.load_diffusion_model(str(models/'minimax_h3_fl2va_pruned_nvfp4.safetensors'))
        model=MiniMaxH3SigmaShift.execute(model,12.,3.)[0];event('fl2va-generator-loaded')
        import latent_preview
        latent_preview.prepare_callback=lambda *_:lambda step,*_args:event('step:'+str(step+1))
        latent={'samples':comfy.nested_tensor.NestedTensor(data['latent'])}
        result=nodes.common_ksampler(model,job['seed'],job['steps'],1.,'res_multistep','beta',data['positive'],[],latent)[0]
        save(result['samples'].tensors,'latents.pt')
    elif stage=='decode':
        import subprocess
        from PIL import Image
        vae=video_vae();event('video-vae-loaded')
        frames=vae.decode(load('latents.pt')[0]).reshape(-1,job['height'],job['width'],3)
        event('video-decoded')
        if len(frames)!=job['frames']:raise RuntimeError(f'Decoder returned {len(frames)} frames; expected {job["frames"]}; refusing to trim away an endpoint')
        folder=work/'frames';folder.mkdir(exist_ok=True)
        cmd=['ffmpeg','-v','error','-y','-f','rawvideo','-pix_fmt','rgb24','-s',f'{job["width"]}x{job["height"]}','-r','24','-i','pipe:0','-an','-c:v','libx264','-threads','2','-crf','17','-pix_fmt','yuv420p',str(work/'raw.mp4')]
        encoder=subprocess.Popen(cmd,stdin=subprocess.PIPE)
        try:
            for i,frame in enumerate(frames):
                pixels=(frame.clamp(0,1)*255).byte().cpu().numpy();Image.fromarray(pixels).save(folder/f'{i:04}.png');encoder.stdin.write(pixels.tobytes())
        finally:encoder.stdin.close()
        if encoder.wait():raise RuntimeError('Video encoding failed')
    else:raise ValueError(stage)
    event('done')
