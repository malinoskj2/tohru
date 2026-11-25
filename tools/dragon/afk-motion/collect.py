#!/usr/bin/env python3
"""Retain selected generated cells as committed sources with provenance.

Normal-size sequences become one lossless strip each under
tools/dragon/art/afk/motion/. Large closeup sequences become square-grid
runtime atlases beside the canonical closeups.
"""
import hashlib
import json
import math
import sys
from pathlib import Path

from PIL import Image

HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[2]
OUT=ROOT/'.tmp/afk-motion'
ART=ROOT/'tools/dragon/art/afk/motion'
RUNTIME=ROOT/'shell/assistant/dragon'
BIG={'bust':'afk-bust-motion.webp','closeup':'afk-closeup-motion.webp'}


def main():
    """Arguments name=dir pick which take (a work dir under .tmp/afk-motion)
    supplies a sequence, e.g. edge_pull=edge_pull-h42."""
    takes=dict(arg.split('=',1) for arg in sys.argv[1:])
    ART.mkdir(parents=True,exist_ok=True)
    spec=json.loads((HERE/'sequences.json').read_text())
    provenance={'model':'h3-fl2va (minimax_h3_fl2va_pruned_nvfp4.safetensors)','backend':'ComfyUI MiniMaxH3 FL2VA','keys':{},'sequences':{}}
    for name,sequence in spec['sequences'].items():
        if sequence.get('key_only'):
            job=json.loads((OUT/takes.get(name,name)/'job.json').read_text())
            provenance['keys'][name]=dict(note=sequence['key_only'],seed=job['seed'],steps=job['steps'],canvas=[job['width'],job['height']],
                prompt_sha256=hashlib.sha256(job['prompt'].encode()).hexdigest(),input_sha256=job['input_sha256'],
                output='tools/dragon/art/afk/edge-peek-source.png',
                output_sha256=hashlib.sha256((ROOT/'tools/dragon/art/afk/edge-peek-source.png').read_bytes()).hexdigest())
            continue
        work=OUT/takes.get(name,name)
        if not (work/'cells.json').exists():raise SystemExit('missing matte output for '+name)
        cells=sorted((work/'cells').glob('*.png'));meta=json.loads((work/'cells.json').read_text())
        job=json.loads((work/'job.json').read_text())
        images=[Image.open(p).convert('RGBA') for p in cells];size=images[0].width
        if name in BIG:
            grid=math.ceil(math.sqrt(len(images)))
            sheet=Image.new('RGBA',(grid*size,grid*size))
            for i,image in enumerate(images):sheet.paste(image,(i%grid*size,i//grid*size))
            target=RUNTIME/BIG[name];sheet.save(target,lossless=True,method=4)
        else:
            sheet=Image.new('RGBA',(len(images)*size,size))
            for i,image in enumerate(images):sheet.paste(image,(i*size,0))
            target=ART/(name+'.png');sheet.save(target,optimize=True)
        provenance['sequences'][name]=dict(first=spec['sequences'][name]['first'],last=spec['sequences'][name]['last'],
            frames=len(images),cell=size,picks=meta['picks'],loop=meta['loop'],seed=job['seed'],steps=job['steps'],
            canvas=job['width'],prompt_sha256=hashlib.sha256(job['prompt'].encode()).hexdigest(),
            input_sha256=job['input_sha256'],output=str(target.relative_to(ROOT)),
            output_sha256=hashlib.sha256(target.read_bytes()).hexdigest(),matte=meta['method'])
        if 'panel' in sequence:
            provenance['sequences'][name].update(canvas=[job['width'],job['height']],panel=sequence['panel'],scale=sequence['scale'],
                crop=meta['crop'],guide=sequence.get('guide'))
        print(target)
    (ART/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')


if __name__=='__main__':main()
