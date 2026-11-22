#!/usr/bin/env python3
"""Pack cloud-drawn hover keys, registered native faces and curated drawn in-betweens.
No model, GPU or subprocess per frame. See README.md for the input contract.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

import cv2
import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / 'tools/dragon/art'
OUT = ROOT / 'shell/assistant/dragon'
CELL, COLS, BETWEEN = 240, 24, 17


def extract(source, rows=4, count=20):
    rgba = np.array(Image.open(source).convert('RGBA'))
    n, labels, stats, centers = cv2.connectedComponentsWithStats((rgba[:, :, 3] > 180).astype('uint8'))
    components = [j for j in range(1, n) if stats[j, 4] > 5000]
    # Sort by row then x without assuming that a horn or tail stops at a grid line.
    components.sort(key=lambda j: (int(centers[j, 1] / (rgba.shape[0] / rows)), centers[j, 0]))
    assert len(components) == count, f'Expected {count} connected whole dragons, found {len(components)}'
    keys = []
    for j in components:
        x, y, w, h, _ = stats[j]
        pad = 2
        x0, y0, x1, y1 = max(0, x-pad), max(0, y-pad), min(rgba.shape[1], x+w+pad), min(rgba.shape[0], y+h+pad)
        a = rgba[y0:y1, x0:x1].copy()
        component = (labels[y0:y1, x0:x1] == j).astype('uint8')
        component = cv2.dilate(component, np.ones((5, 5), np.uint8))
        a[:, :, 3] = np.where(component > 0, np.clip((a[:, :, 3].astype(float)-100)*255/155, 0, 255), 0)
        im = Image.fromarray(a)
        im.thumbnail((264, 260), Image.Resampling.LANCZOS)
        key = Image.new('RGBA', (300, 300))
        # Same centre of occupied space, with generous safe margins for hover and shadow.
        key.alpha_composite(im, ((300-im.width)//2, (300-im.height)//2))
        keys.append(key)
    return keys


def native_face(key, registration):
    """A patch uses THIS view's skin, eyes and smile, never another view's eyes."""
    rgba = np.array(key)
    eyes = np.array(registration['eyes'], dtype=float)
    mouth = np.array(registration['mouth'], dtype=float)
    x, y = min(204, max(0, int(np.mean(eyes[:, 0])-48))), min(236, max(0, int(min(eyes[:, 1])-16)))
    assert 0 <= x <= 204 and 0 <= y <= 236
    patch = rgba[y:y+64, x:x+96].copy()
    eyes -= [x, y]
    mouth -= [x, y]
    angle = np.degrees(np.arctan2(*(eyes[1]-eyes[0])[::-1]))
    eye_mask, mouth_mask = np.zeros((64, 96), 'uint8'), np.zeros((64, 96), 'uint8')
    for e in eyes:
        cv2.ellipse(eye_mask, tuple(np.round(e).astype(int)), (7, 10), 0, 0, 360, 255, -1)
    cv2.ellipse(mouth_mask, tuple(np.round(mouth).astype(int)), (11, 6), angle, 0, 360, 255, -1)
    skin = cv2.inpaint(patch[:, :, :3], eye_mask | mouth_mask, 4, cv2.INPAINT_TELEA)
    # Only replace changed features. Feather into this view's own skin; a full
    # rectangular face patch can reveal codec/resampling boundaries at 1x scale.
    mouth_cover = np.zeros((64, 96), 'uint8')
    cv2.ellipse(mouth_cover, tuple(np.round(mouth).astype(int)), (14, 11), angle, 0, 360, 255, -1)
    patches = []
    for blink in range(3):
        for viseme in range(8):
            p = patch.copy()
            if blink:
                p[:, :, :3] = np.where((eye_mask > 0)[:, :, None], skin, p[:, :, :3])
                for e in eyes:
                    ex, ey = np.round(e).astype(int)
                    if blink == 1:
                        # Half blink: shrink the actual eye pixels toward their own centre.
                        original = patch[ey-10:ey+11, ex-7:ex+8]
                        tiny = cv2.resize(original, (15, 9), interpolation=cv2.INTER_AREA)
                        p[ey-4:ey+5, ex-7:ex+8] = tiny
                    else:
                        cv2.ellipse(p, (ex, ey-1), (4, 2), angle, 0, 180, (101, 56, 87, 255), 1, cv2.LINE_AA)
            if viseme:
                p[:, :, :3] = np.where((mouth_mask > 0)[:, :, None], skin, p[:, :, :3])
                mx, my = np.round(mouth).astype(int)
                sizes = [(0,0),(5,1),(6,7),(7,4),(7,2),(4,6),(3,5),(6,2)]
                w,h = sizes[viseme]
                cv2.ellipse(p, (mx,my), (w,h), angle, 0,360,(85,44,73,255),-1,cv2.LINE_AA)
                if viseme in (2,3,5):
                    cv2.ellipse(p,(mx,my+max(1,h//2)),(max(2,w-2),1),angle,0,180,(204,132,157,255),1,cv2.LINE_AA)
            coverage = np.zeros((64, 96), 'uint8')
            if blink: coverage |= eye_mask
            if viseme: coverage |= mouth_cover
            feather = cv2.GaussianBlur(coverage.astype('float32') / 255, (7, 7), 1.2)
            p[:, :, 3] = np.minimum(p[:, :, 3], np.round(feather * 255).astype('uint8'))
            patches.append(Image.fromarray(p))
    return patches, (x/2, y/2)


def asset_report(cols, rows, legacy):
    logical = {'body.webp': (cols*150, rows*150), 'faces.webp': (400,2040),
               'travel.webp': (384,3060), 'eyes.webp': (200,26),
               'mouths.webp': (176,20), 'heart.png': (24,24)}
    assets = {}
    for name, (lw,lh) in logical.items():
        path=OUT/name
        with Image.open(path) as im: w,h=im.size
        assets[name]=dict(bytes=path.stat().st_size,width=w,height=h,rgba_bytes=w*h*4,
                          logical_width=lw,logical_height=lh)
    current=dict(disk_bytes=sum(a['bytes'] for a in assets.values()),
                 source_rgba_bytes=sum(a['rgba_bytes'] for a in assets.values()),
                 logical_rgba_1x_bytes=sum(a['logical_width']*a['logical_height']*4 for a in assets.values()))
    extra=[assets[n] for n in ['eyes.webp','mouths.webp','heart.png']]
    previous=dict(disk_bytes=legacy.stat().st_size+sum(a['bytes'] for a in extra),
                  source_rgba_bytes=2400*7800*4+sum(a['rgba_bytes'] for a in extra),
                  logical_rgba_1x_bytes=1200*3900*4+sum(a['logical_width']*a['logical_height']*4 for a in extra))
    for values in [current,previous]: values['logical_rgba_2x_bytes']=values['logical_rgba_1x_bytes']*4
    memory=dict(current=current,previous=previous,note='RGBA8 pixel budgets; excludes decoder temporaries, mipmaps, fonts and driver allocations. Not a measured VRAM delta.')
    return assets,memory


def pack(source, legacy, registration, preview):
    keys = extract(source)
    travel_spec=importlib.util.spec_from_file_location('travel_mouths',ROOT/'tools/dragon/travel-mouths.py')
    travel=importlib.util.module_from_spec(travel_spec);travel_spec.loader.exec_module(travel)
    travel.main()
    travel_anchors=json.loads((ART/'travel-registration.json').read_text())
    meta=json.loads(registration.read_text())
    assert len(meta)==len(keys)
    frames=[]
    old=Image.open(legacy)
    for i in range(202):
        frames.append(old.crop((i%8*300,i//8*300,i%8*300+300,i//8*300+300)))
    neutral=frames[165]
    faces=Image.new('RGBA',(8*100,20*3*68))
    entries=[]
    for i,(key,m) in enumerate(zip(keys,meta)):
        patches,(fx,fy)=native_face(key,m)
        for k,p in enumerate(patches): faces.paste(p,(k%8*100+2,(i*3+k//8)*68+2))
        first=len(frames)
        strip = Image.open(ART/'transitions'/(m['name']+'.webp'))
        assert strip.size == (BETWEEN*300,300)
        frames.extend(strip.crop((k*300,0,(k+1)*300,300)) for k in range(BETWEEN))
        rest=len(frames); frames.append(key)
        entries.append(dict(m,first=first,count=BETWEEN,cell=rest-165,face=i,fx=fx-1,fy=fy-1))
        if preview:
            key.save(preview/f'{m["name"]}.png')
    rows=(len(frames)+COLS-1)//COLS
    atlas=Image.new('RGBA',(COLS*CELL,rows*CELL))
    for i,im in enumerate(frames): atlas.paste(im.resize((CELL,CELL),Image.Resampling.LANCZOS),(i%COLS*CELL,i//COLS*CELL))
    staging=ROOT/'.tmp/full-body/packing'
    staging.mkdir(parents=True,exist_ok=True)
    atlas.save(staging/'body.webp',quality=88,method=6)
    faces.save(staging/'faces.webp',lossless=True,method=6)
    (staging/'body.webp').replace(OUT/'body.webp')
    (staging/'faces.webp').replace(OUT/'faces.webp')
    lua=['-- Generated by tools/dragon/pack.py; registrations live in tools/dragon/art/poses.json.','return {']
    for e in entries:
        lua.append('    {name = "%s", cell = %d, first = %d, count = %d, face = %d, fx = %.1f, fy = %.1f, mode = "%s", shadow = %.2f},' % (e['name'],e['cell'],e['first'],e['count'],e['face'],e['fx'],e['fy'],e['mode'],e['shadow']))
        anchors=', '.join('{%.1f, %.1f}' % tuple(a) for a in travel_anchors[e['face']])
        lua[-1]=lua[-1][:-2]+', mouths = {'+anchors+'}},'
    lua.append('}')
    (ROOT/'shell/assistant/full-body.luau').write_text('\n'.join(lua)+'\n')
    assets,memory=asset_report(COLS,rows,legacy)
    report=dict(frames=len(frames),cols=COLS,rows=rows,cell=CELL,logical_cell=150,poses=len(keys),
                source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),assets=assets,memory=memory)
    (ART/'packing.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,default=ART/'hover-source.png')
    parser.add_argument('--legacy',type=Path,default=ART/'legacy-body.webp')
    parser.add_argument('--registration',type=Path,default=ART/'poses.json')
    parser.add_argument('--preview',type=Path)
    args=parser.parse_args()
    if args.preview: args.preview.mkdir(parents=True,exist_ok=True)
    pack(args.source,args.legacy,args.registration,args.preview)
