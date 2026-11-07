#!/usr/bin/env python3
"""Capture only this native bar; ffmpeg derives a nearest-neighbour enlarged crop."""
import pathlib, subprocess, time, os
root = pathlib.Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
out = root/'.tmp'
app = root/'prototypes/amane/target/release/workspace-amane'
log = (out/'replay-native.log').open('w')
process = subprocess.Popen([app,'--mode','replay','--seconds','14'],stdout=log,stderr=log)
encoder = subprocess.Popen(['ffmpeg','-y','-hide_banner','-loglevel','warning',
    '-f','image2pipe','-vcodec','ppm','-framerate','30','-i','-',
    '-filter_complex','[0:v]split=2[a][b];[b]crop=104:42:8:0,scale=624:252:flags=neighbor[z]',
    '-map','[a]','-c:v','libx264','-crf','16','-pix_fmt','yuv420p',str(out/'amane-replay-1x.mp4'),
    '-map','[z]','-c:v','libx264','-crf','16','-pix_fmt','yuv420p',str(out/'amane-replay-6x.mp4')],stdin=subprocess.PIPE)
start=time.monotonic()
stamps=[]
for frame in range(390):
    time.sleep(max(0,start+frame/30-time.monotonic()))
    capture=subprocess.run(['grim','-g','100,0 1080x42','-t','ppm','-'],stdout=subprocess.PIPE,check=True)
    encoder.stdin.write(capture.stdout)
    stamps.append(time.monotonic()-start)
    if frame in (15,40,220,375):
        subprocess.run(['grim','-g','100,0 1080x42',str(out/f'amane-frame-{frame}.png')],check=True)
encoder.stdin.close()
assert encoder.wait()==0
process.wait()
(out/'capture-seconds.csv').write_text('\n'.join(map(str,stamps)))
print(out/'amane-replay-1x.mp4')
print(out/'amane-replay-6x.mp4')
