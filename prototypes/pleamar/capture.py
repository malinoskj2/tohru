#!/usr/bin/env python3
"""Capture only the native bar; derives an enlarged crop from those real pixels."""
import argparse
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--seconds', type=float, default=13)
parser.add_argument('--fps', type=int, default=30)
args = parser.parse_args()
root = Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
media = root / '.tmp'
media.mkdir(exist_ok=True)
base = Path(__file__).resolve().parent
with (media / 'replay-native.log').open('w') as log:
    app = subprocess.Popen([str(base/'run.sh'),'replay','--seconds',str(args.seconds+1)],stdout=log,stderr=subprocess.STDOUT)
    encoder = subprocess.Popen(['ffmpeg','-y','-loglevel','warning','-f','image2pipe','-vcodec','ppm','-framerate',str(args.fps),'-i','-',
        '-c:v','libx264','-preset','veryfast','-crf','15','-pix_fmt','yuv420p',str(media/'pleamar-replay-1x.mp4')],stdin=subprocess.PIPE)
    start = time.monotonic()
    for i in range(round(args.seconds * args.fps)):
        frame = subprocess.check_output(['grim','-t','ppm','-g','0,0 1280x42','-'])
        encoder.stdin.write(frame)
        time.sleep(max(0,start+(i+1)/args.fps-time.monotonic()))
    encoder.stdin.close()
    encoder.wait()
    app.wait()
subprocess.run(['ffmpeg','-y','-loglevel','warning','-i',str(media/'pleamar-replay-1x.mp4'),'-vf','crop=104:42:110:0,scale=832:336:flags=neighbor',
    '-c:v','libx264','-crf','15','-pix_fmt','yuv420p',str(media/'pleamar-replay-8x.mp4')],check=True)
print(media/'pleamar-replay-1x.mp4')
print(media/'pleamar-replay-8x.mp4')
