#!/usr/bin/env python3
"""Bake small native mouth patches and anchors for every new moving frame."""
import json
from pathlib import Path
import cv2
import numpy as np
from PIL import Image
from registration import eyes
ROOT=Path(__file__).resolve().parents[2]
ART=ROOT/'tools/dragon/art'
OUT=ROOT/'shell/assistant/dragon'


def mouth_center(rgba):
    mask=((rgba[:,:,0]<150)&(rgba[:,:,1]<110)&(rgba[:,:,2]<165)&(rgba[:,:,3]>200)).astype('uint8')
    n,labels,stats,centers=cv2.connectedComponentsWithStats(mask)
    points=eyes(rgba)
    expected=points.mean(axis=0)+[1,10]
    candidates=[j for j in range(1,n) if min(np.linalg.norm(centers[j]-e) for e in points)>8 and np.linalg.norm(centers[j]-expected)<12]
    center=centers[min(candidates,key=lambda j:np.linalg.norm(centers[j]-expected))] if candidates else expected
    points=points[np.argsort(points[:,0])]
    angle=float(np.degrees(np.arctan2(*(points[1]-points[0])[::-1])))
    return center,angle


def patches(im,center,angle):
    cx,cy=center;x,y=int(round(cx))-22,int(round(cy))-16
    assert x>=0 and y>=0 and x+44<=300 and y+32<=300
    a=np.array(im)[y:y+32,x:x+44].copy();mx,my=int(round(cx))-x,int(round(cy))-y
    erase=np.zeros((32,44),'uint8');cv2.ellipse(erase,(mx,my),(11,6),angle,0,360,255,-1)
    skin=cv2.inpaint(a[:,:,:3],erase,4,cv2.INPAINT_TELEA)
    cover=np.zeros((32,44),'uint8');cv2.ellipse(cover,(mx,my),(14,11),angle,0,360,255,-1)
    cover=cv2.GaussianBlur(cover.astype('float32')/255,(7,7),1.2)
    result=[]
    for v in range(8):
        p=a.copy()
        if v:
            p[:,:,:3]=np.where((erase>0)[:,:,None],skin,p[:,:,:3])
            w,h=[(0,0),(5,1),(6,7),(7,4),(7,2),(4,6),(3,5),(6,2)][v]
            cv2.ellipse(p,(mx,my),(w,h),angle,0,360,(85,44,73,255),-1,cv2.LINE_AA)
            if v in [2,3,5]:cv2.ellipse(p,(mx,my+max(1,h//2)),(max(2,w-2),1),angle,0,180,(204,132,157,255),1,cv2.LINE_AA)
            p[:,:,3]=np.minimum(p[:,:,3],np.round(cover*255).astype('uint8'))
        else:p[:,:,3]=0
        # A two-pixel gutter prevents cell bleed at fractional display scales.
        cell=Image.new('RGBA',(48,36));cell.paste(Image.fromarray(p),(2,2));result.append(cell)
    return result,[(x-2)/2,(y-2)/2]


def main():
    meta=json.loads((ART/'poses.json').read_text());atlas=Image.new('RGBA',(16*48,170*36));registration=[]
    for i,m in enumerate(meta):
        strip=Image.open(ART/'transitions'/(m['name']+'.webp'));row=[]
        for f in range(17):
            im=strip.crop((f*300,0,(f+1)*300,300));center,angle=mouth_center(np.array(im))
            # Endpoints use the hand-measured registrations.
            if f==0:center,angle=np.array([107,109]),-10
            elif f==16:center,angle=np.array(m['mouth']),angle
            images,anchor=patches(im,center,angle);row.append(anchor)
            for v,cell in enumerate(images):
                n=(i*17+f)*8+v;atlas.paste(cell,(n%16*48,n//16*36))
        registration.append(row)
    staging=ROOT/'.tmp/full-body/packing';staging.mkdir(parents=True,exist_ok=True)
    atlas.save(staging/'travel.webp',lossless=True,method=6);(staging/'travel.webp').replace(OUT/'travel.webp')
    (ART/'travel-registration.json').write_text(json.dumps(registration,indent=2)+'\n')
    print('travel.webp',atlas.size,(OUT/'travel.webp').stat().st_size)

if __name__=='__main__':main()
