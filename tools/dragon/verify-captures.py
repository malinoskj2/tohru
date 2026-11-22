#!/usr/bin/env python3
"""Check native recording dimensions/durations and detect blank dragon frames."""
import json
from pathlib import Path
import subprocess
import numpy as np
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'.tmp/full-body/native-captures'
report=[]
for name in ['idle-comparison','listening-thinking','speaking-interruption','reduced-motion','settling']:
    path=OUT/(name+'.mp4')
    probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-select_streams','v:0','-show_entries','stream=width,height,nb_frames,duration','-of','json',str(path)]))['streams'][0]
    w,h=int(probe['width']),int(probe['height']);assert h==230 and w in [210,420]
    proc=subprocess.Popen(['ffmpeg','-v','error','-i',str(path),'-f','rawvideo','-pix_fmt','rgb24','-'],stdout=subprocess.PIPE)
    counts=[]
    while True:
        data=proc.stdout.read(w*h*3)
        if not data:break
        assert len(data)==w*h*3
        frame=np.frombuffer(data,dtype='uint8').reshape(h,w,3)[:,-210:]
        chroma=frame.max(axis=2).astype(int)-frame.min(axis=2)
        counts.append(int(((chroma>12)&(frame.max(axis=2)>70)).sum()))
    proc.stdout.close();assert proc.wait()==0
    assert min(counts)>1000,(name,'blank/mostly missing body',min(counts))
    report.append(dict(name=name,**probe,decoded_frames=len(counts),minimum_coloured_body_pixels=min(counts)))
(ROOT/'.tmp/full-body/recording-verification.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
