#!/usr/bin/env python3
"""Generate AFK in-between motion with H3 FL2VA between canonical AFK keys.

Each sequence animates from one existing key to another (or back to itself)
on a uniform background. All sequences run inside one exclusive scheduler
reservation, one model stage per process. Raw frames stay under
.tmp/afk-motion/<sequence>/ for matte.py.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from PIL import Image

HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[2]
ART=ROOT/'shell/assistant/dragon'
SOURCES=ROOT/'tools/dragon/art/afk'
OUT=ROOT/'.tmp/afk-motion'
CHECKPOINT='minimax_h3_fl2va_pruned_nvfp4.safetensors'
EXPECTED_SHA='72fa9269ce551fb63ff42a32d9b46d0c122e84b4b2c511e22fa698287b088f70'
BACKEND=Path('/home/jesse/projects/inclusive-skateboarding/.tmp/h3/ComfyUI')
PYTHON=Path('/home/jesse/projects/inclusive-skateboarding/.tmp/h3/venv/bin/python')
BACKEND_REV='eee59185ff4f43ae2d9ad122b245619d27d9daad'
BACKGROUND=(29,29,29)
FRAMES=124
# Normal keys come from the 8-column AFK atlas.
CELL,COLUMNS,MARGIN=320,8,24


def spec():return json.loads((HERE/'sequences.json').read_text())


def key_image(key,size):
    """Exact canvas transform for a key; matte.py inverts the same transform."""
    canvas=Image.new('RGBA',(size,size),BACKGROUND+(255,))
    if 'cell' in key:
        atlas=Image.open(ART/'afk.webp').convert('RGBA')
        edge=atlas.width//COLUMNS;cell=key['cell'];x,y=cell%COLUMNS*edge,cell//COLUMNS*edge
        sprite=atlas.crop((x,y,x+edge,y+edge)).resize((size-2*MARGIN,)*2,Image.Resampling.LANCZOS)
        canvas.alpha_composite(sprite,(MARGIN,MARGIN))
    else:
        source=Image.open(SOURCES/(key['image']+'-source.png')).convert('RGBA')
        canvas.alpha_composite(source.resize((size,size),Image.Resampling.LANCZOS))
    return canvas.convert('RGB')


def prompt(data,sequence):
    seconds=f'{(FRAMES-1)/24:.2f}'
    return ('How the reference pictures align with the target video — Picture 1 (from Shot 1) aligns with the 0.00-second mark '
        f'of the target video; Picture 2 (from Shot 1) aligns with the {seconds}-second mark of the target video.\n\n'
        'integrated_multimodal_description: [Shot 1] A single continuous illustrated character animation of Tohru, a tiny '
        'pearlescent glass baby dragon, on a perfectly uniform dark gray background, matching the two supplied drawings. '
        'The entire character stays comfortably inside the frame unless a picture crops her. '+data['identity']+' '
        +sequence['action']+' The motion reads as one complete living dragon with soft coherent contours and stable '
        'attachments throughout. '+data['background']+'\n\noverall_soundscape: Silence.\n\nnon_diegetic_music: None.\n')


def prepare(names):
    data=spec()
    for name in names:
        sequence=data['sequences'][name];size=sequence.get('size',480);work=OUT/name
        inputs=work/'inputs';inputs.mkdir(parents=True,exist_ok=True)
        for role in ('first','last'):key_image(sequence[role],size).save(inputs/(role+'.png'))
        (inputs/'prompt.txt').write_text(prompt(data,sequence))
        (inputs/'manifest.json').write_text(json.dumps(dict(sequence=name,size=size,margin=MARGIN if 'cell' in sequence['first'] else 0,
            cell=CELL,first=sequence['first'],last=sequence['last'],background=BACKGROUND,
            sha256={role:hashlib.sha256((inputs/(role+'.png')).read_bytes()).hexdigest() for role in ('first','last')}),indent=2)+'\n')
        print(inputs)


def preflight():
    model_root=Path(os.environ['MODEL_ROOT'])
    catalog=json.loads((model_root/'catalog.json').read_text());ready=json.loads((model_root/'readiness.json').read_text())
    entry=catalog['entries'].get('h3-fl2va')
    if entry is None:raise RuntimeError('Shared alias h3-fl2va is absent from the deployed catalog')
    if ready['entries'].get('h3-fl2va',{}).get('status')!='ready':raise RuntimeError('h3-fl2va is not ready; host: systemctl --user start local-models-sync.service')
    model_dir=model_root/'by-name/h3-fl2va'
    checkpoint=next((f for f in entry['files'] if f['target']==CHECKPOINT),None)
    if checkpoint is None or checkpoint.get('sha256')!=EXPECTED_SHA:raise RuntimeError('FL2VA checkpoint identity changed')
    for f in entry['files']:
        p=model_dir/f['target']
        if not p.is_file() or p.stat().st_size!=f['size']:raise RuntimeError('Missing shared model file: '+str(p))
    if not PYTHON.is_file():raise RuntimeError('H3 Python environment unavailable: '+str(PYTHON))
    if subprocess.check_output(['git','-C',str(BACKEND),'rev-parse','HEAD'],text=True).strip()!=BACKEND_REV:
        raise RuntimeError('ComfyUI revision changed; inspect its first/last API before running')
    return model_dir


def available_memory():
    return int(next(l.split()[1] for l in Path('/proc/meminfo').read_text().splitlines() if l.startswith('MemAvailable:')))*1024


def run_sequence(name,model_dir,steps,seed):
    work=(OUT/name).resolve();inputs=work/'inputs'
    if (work/'raw.mp4').exists():print('skip',name,'(raw.mp4 exists)');return
    manifest=json.loads((inputs/'manifest.json').read_text())
    job=dict(variant='FL2VA',models=str(model_dir),backend=str(BACKEND),backend_revision=BACKEND_REV,
        first=str(inputs/'first.png'),last=str(inputs/'last.png'),prompt=(inputs/'prompt.txt').read_text(),
        width=manifest['size'],height=manifest['size'],frames=FRAMES,fps=24,steps=steps,seed=seed,checkpoint_sha256=EXPECTED_SHA,
        input_sha256=manifest['sha256'])
    jobpath=work/'job.json';jobpath.write_text(json.dumps(job,indent=2)+'\n')
    env=os.environ.copy();env.update(PYTHONDONTWRITEBYTECODE='1',HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',OPENBLAS_NUM_THREADS='1',MALLOC_ARENA_MAX='2',PYTHONNOUSERSITE='1')
    for key in ('HF_TOKEN','HUGGING_FACE_HUB_TOKEN','PYTHONPATH','PYTHONHOME'):env.pop(key,None)
    for key,sub in [('TORCH_EXTENSIONS_DIR','torch'),('TRITON_CACHE_DIR','triton'),('TMPDIR','temp'),('XDG_CACHE_HOME','xdg')]:
        p=work/'cache'/sub;p.mkdir(parents=True,exist_ok=True);env[key]=str(p)
    report=dict(sequence=name,stages=[],started_at=time.time())
    for stage in ('probe','anchors','encode','sample','decode'):
        if available_memory()<2*1024**3:raise RuntimeError('Less than 2GiB available memory before '+stage)
        start=time.monotonic()
        with (work/(stage+'.log')).open('w') as log:
            child=subprocess.Popen([str(PYTHON),str(HERE/'h3_stage.py'),stage,str(jobpath)],stdout=log,stderr=subprocess.STDOUT,env=env,start_new_session=True)
            try:code=child.wait()
            except BaseException:
                os.killpg(child.pid,signal.SIGTERM);child.wait(timeout=60);raise
        report['stages'].append(dict(stage=stage,exit_code=code,seconds=round(time.monotonic()-start,1)))
        (work/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        if code:raise RuntimeError(f'{name}: {stage} failed; see {work/(stage+".log")}')
    print(name,'done',[s['seconds'] for s in report['stages']],flush=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=['prepare','run'])
    parser.add_argument('names',nargs='*')
    parser.add_argument('--steps',type=int,default=20)
    parser.add_argument('--seed',type=int,default=42)
    args=parser.parse_args()
    names=args.names or list(spec()['sequences'])
    if args.command=='prepare':prepare(names);return 0
    model_dir=preflight()
    if not os.environ.get('GPU_LEASE_ID'):
        os.execvp('gpu-run',['gpu-run','--exclusive','--name','Tohru AFK FL2VA in-betweens','--',sys.executable,str(Path(__file__).resolve()),*sys.argv[1:]])
    failed=[]
    for name in names:
        try:run_sequence(name,model_dir,args.steps,spec()['sequences'][name].get('seed',args.seed))
        except RuntimeError as e:print(e,file=sys.stderr,flush=True);failed.append(name)
    if failed:print('failed:',' '.join(failed),file=sys.stderr);return 1
    return 0


if __name__=='__main__':raise SystemExit(main())
