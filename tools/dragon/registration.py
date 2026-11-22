"""Locate native dot eyes inside the peach head, excluding dark tail/wing edges."""
import itertools
import cv2
import numpy as np


def eyes(rgba):
    a=np.asarray(rgba)
    peach=((a[:,:,0].astype(float)>a[:,:,1]*1.05)&(a[:,:,0].astype(float)>a[:,:,2]*.95)&(a[:,:,0]>150)&(a[:,:,3]>180)).astype('uint8')
    n,_,stats,_=cv2.connectedComponentsWithStats(peach)
    j=1+np.argmax(stats[1:,4]);x,y,w,h,_=stats[j]
    dark=((a[:,:,0]<150)&(a[:,:,1]<110)&(a[:,:,2]<165)&(a[:,:,3]>200)).astype('uint8')
    n,_,stats,centers=cv2.connectedComponentsWithStats(dark)
    candidates=[j for j in range(1,n) if 3<=stats[j,4]<=180 and x-25<=centers[j,0]<=x+w+25 and y-5<=centers[j,1]<=y+h+8 and stats[j,3]/max(1,stats[j,2])>.65]
    pairs=[]
    for i,j in itertools.combinations(candidates,2):
        dx,dy=abs(centers[i,0]-centers[j,0]),abs(centers[i,1]-centers[j,1])
        if 18<dx<65 and dy<24:
            midpoint=(centers[i]+centers[j])/2
            score=abs(midpoint[1]-(y+h*.6))+.3*abs(midpoint[0]-(x+w*.5))+.2*dy-.4*(stats[i,4]+stats[j,4])
            pairs.append((score,i,j))
    if not pairs:
        # A very short blurred interval may merge the pupils. Keep the inferred
        # features within the actual head rather than selecting a dark tail rim.
        cx,cy=x+w*.5,y+h*.6
        return np.array([[cx-16,cy],[cx+16,cy]],dtype=float)
    _,i,j=min(pairs)
    return centers[[i,j]][np.argsort(centers[[i,j],0])]
