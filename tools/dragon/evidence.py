#!/usr/bin/env python3
"""Build review media from retained art; runtime captures use review.py separately."""
import argparse
import json
from pathlib import Path
import subprocess
from PIL import Image,ImageDraw,ImageFont

ROOT=Path(__file__).resolve().parents[2]
ART=ROOT/'tools/dragon/art'
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,default=ROOT/'.tmp/full-body/evidence');args=p.parse_args();out=args.output;out.mkdir(parents=True,exist_ok=True)
meta=json.loads((ART/'poses.json').read_text())
old=Image.open(ART/'legacy-body.webp')
neutral=old.crop((1500,6000,1800,6300))
font=ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf',14) if Path('/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf').exists() else ImageFont.load_default()

def canvas(key,scale=1):
    im=Image.new('RGB',(210*scale,230*scale),'#16151e')
    d=ImageDraw.Draw(im); d.ellipse((59*scale,207*scale,151*scale,221*scale),fill='#111018')
    key=key.resize((150*scale,150*scale),Image.Resampling.LANCZOS)
    im.paste(key,(30*scale,40*scale),key)
    return im

contact=Image.new('RGB',(5*300,4*340),'#242330');d=ImageDraw.Draw(contact)
items=[]
for i,m in enumerate(meta):
    strip=Image.open(ART/'transitions'/(m['name']+'.webp'));key=strip.crop((4800,0,5100,300))
    im=canvas(key);im.save(out/(m['name']+'-actual.png'))
    big=canvas(key,2);big.thumbnail((280,290));contact.paste(big,(i%5*300+10,i//5*340+30));d.text((i%5*300+10,i//5*340+10),f'{i+1:02} {m["name"].replace("_"," ")}',fill='white',font=font)
    # A complete transition out, quiet hold, and reversible return. Intermediate
    # frames count as animation, never as extra poses.
    sequence=list(range(17))+[16]*40+list(range(16,-1,-1))+[0]*24
    frames=[]
    for k in sequence:
        frames.append(canvas(strip.crop((k*300,0,(k+1)*300,300)),2).tobytes())
    target=out/(m['name']+'.mp4')
    proc=subprocess.Popen(['ffmpeg','-v','error','-y','-f','rawvideo','-pix_fmt','rgb24','-s','420x460','-r','24','-i','-','-an','-c:v','libx264','-preset','fast','-crf','19','-pix_fmt','yuv420p',str(target)],stdin=subprocess.PIPE)
    for frame in frames: proc.stdin.write(frame)
    proc.stdin.close();assert proc.wait()==0
    items.append(dict(path=str(target.relative_to(ROOT)),section='20 airborne sequences',caption=m['name'].replace('_',' '),note=m['note']+' · 2× display for inspection; actual size in the matching PNG.'))
    items.append(dict(path=str((out/(m['name']+'-actual.png')).relative_to(ROOT)),section='Actual-size poses',caption=m['name'].replace('_',' ')+' · 150 px cell',note=m['note']))
contact.save(out/'twenty-poses.jpg')
# Comparison is one fixed neutral on the left against a varied full body on the
# right, not a claim that the old puppet lacked head/hand animation.
compare=Image.new('RGB',(840,500),'#16151e');d=ImageDraw.Draw(compare)
compare.paste(canvas(neutral,2),(0,40));d.text((30,10),'Previous neutral / fixed lower-body hub',fill='white',font=font)
strip=Image.open(ART/'transitions'/'reclined_hover.webp');compare.paste(canvas(strip.crop((4800,0,5100,300)),2),(420,40));d.text((450,10),'New airborne recline / changed hips and feet',fill='white',font=font)
compare.save(out/'before-after.png')
(out/'gallery-batch.json').write_text(json.dumps(dict(title='Airborne full-body expansion',sections={'20 airborne sequences':'Twenty redrawn keys, each with a reversible transition. Loops are slowed by quiet holds; these are art inspection clips, not live timing.','Actual-size poses':'210 × 230 overlay crop; dragon cell is 150 × 150 logical pixels.'},items=items),indent=2)+'\n')
print(out)
