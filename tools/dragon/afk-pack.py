#!/usr/bin/env python3
"""Pack authored AFK keys and the retained generated in-between strips.

Each authored row has eight frames of the SAME pose with registered local
paw/wing/tail deformation. Generated FL2VA sequences (tools/dragon/afk-motion)
follow as 16-frame runs from the committed strips; packing itself runs no model.
"""
import hashlib
import json
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

ROOT=Path(__file__).resolve().parents[2]
ART=ROOT/'tools/dragon/art/afk'
# The hide-and-peek edge clips carry more frames than the 16 of the others.
SEQUENCES=json.loads((ROOT/'tools/dragon/afk-motion/sequences.json').read_text())['sequences']
OUT=ROOT/'shell/assistant/dragon'
CELL=240
# Generated in-between sequences, in atlas order after the authored rows.
MOTION=['turn','arrive_right','arrive_left','curl','perch','bank','edge_reach','edge_pull','edge_head']
MOTION_FRAMES=16
# Authored keys packed whole after the motion: the hide-and-peek wave holds
# the horned pose its pull-out ends on, centred like the canonical cells.
KEYS=['horned-56']
NAMES=['fly_left','head_tilt','fly_right','wing_straighten','fly_up','horn_polish',
       'chin_on_paws','hopeful_wave','curled_sleep','perch_tail_dangle','tail_follow_left','tail_follow_right']


def keys(path,columns,rows):
    source=Image.open(path).convert('RGBA')
    result=[]
    for row in range(rows):
        for column in range(columns):
            box=(round(column*source.width/columns),round(row*source.height/rows),
                 round((column+1)*source.width/columns),round((row+1)*source.height/rows))
            key=source.crop(box)
            data=np.array(key)
            # The generated source's faint disconnected alpha haze is not part
            # of the glass contour. Match the existing canonical packer matte.
            alpha=data[:,:,3]
            count,labels,stats,_=cv2.connectedComponentsWithStats((alpha>150).astype('uint8'))
            largest=1+int(np.argmax(stats[1:,cv2.CC_STAT_AREA]))
            component=cv2.dilate((labels==largest).astype('uint8'),np.ones((5,5),'uint8'))
            data[:,:,3]=np.where(component, np.clip((alpha.astype(float)-70)*255/185,0,255),0)
            key=Image.fromarray(data).resize((CELL,CELL),Image.Resampling.LANCZOS)
            result.append(key)
    return result


def articulate(key,index,phase):
    rgba=np.array(key.convert('RGBa'))
    y,x=np.indices((CELL,CELL),dtype=np.float32)
    u,v=x/CELL,y/CELL
    dx=np.zeros_like(x);dy=np.zeros_like(y)
    wave=np.sin(phase)
    def local(cx,cy,sx,sy):return np.exp(-((u-cx)/sx)**2-((v-cy)/sy)**2)
    if index==0:  # Paw gently presses and releases against the glass.
        paw=local(.40,.46,.09,.07)
        dx += .016*CELL*wave*paw
        dy += .009*CELL*(1-np.cos(phase))*paw
    elif index in (2,3):  # A visible short smoothing stroke along the membrane.
        paw=local(.66,.49,.15,.09)
        dx += .035*CELL*wave*paw
        dy += .012*CELL*np.sin(phase+.5)*paw
        dy += .009*CELL*wave*local(.79,.51,.12,.10)
    elif index in (4,5):  # Paw rubs its authored horn-contact point.
        paw=local(.59,.23,.095,.12)
        dx += .017*CELL*wave*paw
        dy += .025*CELL*wave*paw
    elif index==7:
        paw=local(.43,.45,.09,.13)
        dx += .035*CELL*wave*paw
        dy += .015*CELL*(1-np.cos(phase))*paw
    elif index==8:
        # Breathing deforms the torso around a fixed resting/head/tail contact.
        dy += .006*CELL*wave*local(.48,.55,.23,.12)
        dx += .004*CELL*np.sin(phase*2)*local(.67,.42,.16,.10)
    elif index==9:
        dx += .018*CELL*wave*local(.54,.72,.10,.25)
    elif index in (10,11):
        dx += .028*CELL*wave*local(.78 if index==10 else .24,.60,.18,.22)
        dy += .022*CELL*np.cos(phase)*local(.78 if index==10 else .24,.60,.18,.22)
        dy += .010*CELL*wave*local(.50,.27,.23,.18)
    else:
        dx += .008*CELL*wave*local(.48,.30,.23,.20)
    pixels=cv2.remap(rgba,x-dx,y-dy,cv2.INTER_CUBIC,borderMode=cv2.BORDER_CONSTANT)
    return Image.frombytes('RGBa',(CELL,CELL),pixels.tobytes()).convert('RGBA')


def main():
    cv2.setNumThreads(1)
    images=keys(ART/'source.png',4,2)+keys(ART/'nap-perch-tail-source.png',2,2)
    rows=12+sum(SEQUENCES[name].get('cells',MOTION_FRAMES) for name in MOTION)//8+(len(KEYS)+7)//8
    sheet=Image.new('RGBA',(CELL*8,CELL*rows))
    flight=Image.open(OUT/'flight.webp').convert('RGBA')
    body=Image.open(OUT/'body.webp').convert('RGBA')
    manifest={'cell':CELL,'columns':8,'rows':rows,'frames':rows*8,'poses':{},'sources':{},'motion':{}}
    for path in ART.glob('*.png'):
        with Image.open(path) as image:
            manifest['sources'][path.name]={'size':list(image.size),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
    for index,key in enumerate(images):
        occupied=key.getchannel('A').point(lambda a:255 if a>180 else 0).getbbox()
        manifest['poses'][NAMES[index]]={'first':index*8,'count':8,'occupied':occupied}
        bounds=[]
        for frame in range(8):
            if index in (0,2,4):
                flight_row={0:0,2:1,4:2}[index]
                cell=round(frame*12/8)%12
                sprite=flight.crop((cell*240,flight_row*240,(cell+1)*240,(flight_row+1)*240)).resize((CELL,CELL),Image.Resampling.LANCZOS)
            elif index==1 and frame==7:
                cell=543 # Existing authored bright_hover stretch, not new character art.
                sprite=body.crop((cell%24*240,cell//24*240,(cell%24+1)*240,(cell//24+1)*240)).resize((CELL,CELL),Image.Resampling.LANCZOS)
            else:sprite=articulate(key,index,frame*np.pi/4)
            sheet.paste(sprite,(frame*CELL,index*CELL))
            bounds.append(sprite.getchannel('A').point(lambda a:255 if a>180 else 0).getbbox())
        manifest['poses'][NAMES[index]]['occupied']=[min(b[0] for b in bounds),min(b[1] for b in bounds),max(b[2] for b in bounds),max(b[3] for b in bounds)]
        manifest['poses'][NAMES[index]]['original_source_cell']=240 if index in (0,2,4) else None
    first=96
    for name in MOTION:
        count=SEQUENCES[name].get('cells',MOTION_FRAMES)
        strip=Image.open(ART/'motion'/(name+'.png')).convert('RGBA')
        if strip.size!=(CELL*count,CELL):raise SystemExit('unexpected generated strip size for '+name)
        for frame in range(count):
            index=first+frame
            sheet.paste(strip.crop((frame*CELL,0,(frame+1)*CELL,CELL)),(index%8*CELL,index//8*CELL))
        manifest['motion'][name]={'first':first,'count':count,
            'sha256':hashlib.sha256((ART/'motion'/(name+'.png')).read_bytes()).hexdigest()}
        first+=count
    manifest['keys']={}
    for index,name in enumerate(KEYS):
        key=Image.open(ART/(name+'.png')).convert('RGBA')
        if key.size!=(CELL,CELL):raise SystemExit('unexpected key size for '+name)
        sheet.paste(key,((first+index)%8*CELL,(first+index)//8*CELL))
        manifest['keys'][name]={'frame':first+index,'sha256':hashlib.sha256((ART/(name+'.png')).read_bytes()).hexdigest()}
    OUT.mkdir(exist_ok=True)
    sheet.save(OUT/'afk.webp',lossless=True,method=2)
    # Preserve full-resolution canonical closeup separately from the motion atlas.
    for name in ('closeup','bust'):
        image=Image.open(ART/(name+'-source.png')).convert('RGBA')
        image.save(OUT/('afk-'+name+'.png'))
        alpha=np.array(image.getchannel('A'))
        mask=Image.fromarray(np.where(alpha>180,255,0).astype('uint8'))
        manifest[name]={'source':list(image.size),'occupied':mask.getbbox(),
                        'rgba_bytes':image.width*image.height*4,'decode':'native 0,0'}
    manifest['closeup']['head_detail_budget']=[580,610]
    manifest['bust']['head_detail_budget']=[1020,1000]
    manifest['closeup']['head_annotation']=[340,40,930,655]
    manifest['bust']['head_annotation']=[110,0,1220,1000]
    manifest['annotation_note']='Conservative manually registered head bounds (including horns/frill); transparent sheet size is not head detail.'
    manifest['atlas_rgba_bytes']=sheet.width*sheet.height*4
    manifest['reused_flight_source']={'path':'shell/assistant/dragon/flight.webp','cell':240,
        'sha256':hashlib.sha256((OUT/'flight.webp').read_bytes()).hexdigest()}
    (ART/'packing.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print('Packed',rows*8,'AFK frames (96 authored, generated in-betweens after); provenance in',ART/'packing.json')


if __name__=='__main__':main()
