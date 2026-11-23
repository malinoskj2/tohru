#!/usr/bin/env python3
"""Index native AFK evidence, make unscaled crops and a media-gallery batch.

Does not render, change scene state or transcode originals. Run after capture.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess

from PIL import Image

ROOT=Path(__file__).resolve().parents[2]
CAPTURES=ROOT/'.tmp/afk-captures'
OUT=ROOT/'.tmp/afk-review'
SCENES=['screen_window','corner_nap','glass_inspection','upside_down_peek','window_perch',
        'tail_chase','sleepy_drift','distant_flight','hide_peek','wing_grooming','waiting_for_you','quiet_rest']
LABELS=['Screen-window camera approach','Corner nap','Glass inspection','Upside-down peek','Real-window perch',
        'Tail chase','Sleepy drift','Distant flight','Real-window hide and peek','Wing grooming','Waiting for you','Quiet reduced-motion rest']


def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()


def timing(path):
    header=None;visible=[];first=None;last=None
    for line in path.read_text().splitlines():
        if line.startswith('ms\t'):header=line.split('\t');continue
        if header is None:continue
        try:values=dict(zip(header,map(float,line.split('\t'))))
        except ValueError:continue
        if values.get('ai_actor_open',0)>.5 and values.get('ai_flight_alpha',0)>.05:
            visible.append(values['ms'])
            first=first or values;last=values
    delta=[b-a for a,b in zip(visible,visible[1:]) if b>a]
    delta.sort()
    return {'kind':'native decision/record tick intervals; NOT GPU presentation latency',
        'visible_samples':len(visible),'median_ms':statistics.median(delta) if delta else None,
        'p95_ms':delta[min(len(delta)-1,int(len(delta)*.95))] if delta else None,
        'max_ms':max(delta) if delta else None,'first_visible':first,'last_visible':last}


def probe(path):
    result=subprocess.run(['ffprobe','-v','error','-select_streams','v:0','-show_entries',
        'stream=width,height,codec_name,pix_fmt,avg_frame_rate:format=duration','-of','json',str(path)],
        capture_output=True,text=True,check=True,timeout=8)
    return json.loads(result.stdout)


def crop(path,box,name):
    with Image.open(path) as image:
        box=(max(0,box[0]),max(0,box[1]),min(image.width,box[2]),min(image.height,box[3]))
        image.crop(box).save(OUT/name)
    return OUT/name


def manifest():
    roots=[CAPTURES,OUT,ROOT/'.tmp/afk-art',ROOT/'tools/dragon/art/afk']
    paths=set()
    for root in roots:
        if root.exists():paths.update(p for p in root.rglob('*') if p.is_file())
    for pattern in ('shell/assistant/afk*','shell/assistant/dragon/afk*','bin/tohru_afk_idle.py','tests/assistant/afk*','tools/dragon/afk*','.tmp/afk-*.log'):
        paths.update(p for p in ROOT.glob(pattern) if p.is_file())
    target=OUT/'preservation-manifest.json'
    paths.discard(target)
    gallery=ROOT/'.tmp/gallery/gallery.json'
    if gallery.exists():paths.add(gallery)
    payload={'schema':1,'root':str(ROOT),'note':'Original bytes retained; no originals transcoded or removed; includes gallery metadata',
        'files':[{'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)} for path in sorted(paths)]}
    target.write_text(json.dumps(payload,indent=2)+'\n')
    return target


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--revision',required=True)
    parser.add_argument('--manifest-only',action='store_true')
    args=parser.parse_args();OUT.mkdir(exist_ok=True)
    if args.manifest_only:print(manifest());return
    runs=[]
    for path in CAPTURES.glob('*/evidence.json'):
        meta=json.loads(path.read_text());directory=path.parent
        monitors=json.loads((directory/'monitors.json').read_text())
        meta['directory']=str(directory)
        meta['monitor']=next(m for m in monitors if m['name']==meta['output'])
        runs.append(meta)
    def latest(predicate):
        matches=[run for run in runs if predicate(run)]
        if not matches:return None
        return max(matches,key=lambda run:run['directory'].rsplit('-',1)[-1])
    items=[];selected={};audit={}
    def item(path,caption,section='Native scenes',review=False,note=''):
        entry={'path':str(Path(path).relative_to(ROOT)),'caption':caption,'section':section,
               'status':'review' if review else 'new','note':note}
        if review:
            entry['ask']='Does the action read clearly and stay visually coherent at normal speed?' if Path(path).suffix=='.mp4' else 'Are eyes, paws and contours clean at100% size, without blur, seams or halos?'
        items.append(entry)
    for scene,label in zip(SCENES,LABELS):
        run=latest(lambda r:r['scene']==scene and r.get('revision')==args.revision and r['cancel']=='none'
            and not r.get('compositor_inactivity_observed') and not r.get('cold_shared_art') and not r.get('auto')
            and r['monitor']['scale']==1 and r['monitor'].get('transform',0)==0)
        if run is None:raise SystemExit('Missing final native scene: '+scene)
        selected[scene]=run
        directory=Path(run['directory'])
        statuses=json.loads((directory/'status.json').read_text())
        if not any(entry['trace'].strip() and json.loads(entry['trace']).get('selected')==scene for entry in statuses):
            raise SystemExit('No native selected-scene evidence: '+scene)
        audit[scene]={'media':probe(directory/'native.mp4'),'timing':timing(directory/'native.log'),
            'bindings':json.loads((directory/'bindings-after.json').read_text())}
        item(directory/'native.mp4',label,review=scene!='quiet_rest',
             note='Actual native renderer/actor; explicit forced source and seed17, probability100. Real compositor window; no microphone or models. '+('Opacity-only stationary rest.' if scene=='quiet_rest' else 'Source revision '+args.revision[:7]))
    for scene in ('screen_window','glass_inspection'):
        run=selected[scene];directory=Path(run['directory'])
        png=directory/('native-7.0s.png' if scene=='screen_window' else 'native-10.0s.png')
        item(png,('Camera' if scene=='screen_window' else 'Glass')+' · raw native 1920×1080 PNG','Physical pixels',True,
             'Original compositor PNG, not a video frame or resized thumbnail.')
        if scene=='screen_window':box=(686,231,1230,627);source_box=(350,250,900,650);source='afk-closeup.png'
        else:box=(730,365,1247,778);source_box=(360,420,960,900);source='afk-bust.png'
        detail=crop(png,box,scene+'-native-detail.png')
        reference=crop(ROOT/'shell/assistant/dragon'/source,source_box,scene+'-source-detail.png')
        item(detail,scene+' · unscaled native detail','Physical pixels',True,'Pixel crop only; no enlargement. Compare original-source detail beside it.')
        item(reference,scene+' · unscaled source detail','Physical pixels',False,'Original source pixels; native canvas is <=1254 physical pixels.')
    for scene in ('wing_grooming','window_perch'):
        directory=Path(selected[scene]['directory'])
        box=(960,300,1280,620) if scene=='wing_grooming' else (580,20,900,340)
        for seconds in ('7.0','10.0'):
            detail=crop(directory/('native-'+seconds+'s.png'),box,scene+'-'+seconds+'-native-detail.png')
            item(detail,scene+' · '+seconds+'s unscaled detail','Authored actions',True,'Native pixel crop; continuous source video remains the motion evidence.')
        detail_video=OUT/(scene+'-native-detail.mp4')
        x,y,x1,y1=box
        subprocess.run(['ffmpeg','-v','error','-y','-ss','5','-i',str(directory/'native.mp4'),
            '-t','10','-vf',f'crop={x1-x}:{y1-y}:{x}:{y}','-c:v','libx264','-preset','ultrafast',
            '-threads','2','-crf','18','-an',str(detail_video)],check=True,timeout=20)
        item(detail_video,scene+' · normal-speed native pixel crop','Authored actions',True,
             '320×320 pixels, crop only; no zoom, interpolation or speed change. Original full-frame recording is preserved.')
    for label,predicate in [
        ('Real compositor inactivity · failed50%, rearm, no-repeat',lambda r:r.get('compositor_inactivity_observed') and r.get('auto') and r.get('probability')==50),
        ('Real injected keyboard input cancels glass',lambda r:r.get('compositor_inactivity_observed') and r['scene']=='glass_inspection' and r['cancel']=='input'),
        ('Cold shared artwork · no voice preload',lambda r:r.get('cold_shared_art')),
        ('Lock safety event (not a real logind lock)',lambda r:r['cancel']=='lock'),
        ('Voice/hearing priority',lambda r:r['cancel']=='voice'),
        ('Moving the real window cancels perch',lambda r:r['cancel']=='move'),
        ('Closing the real window cancels hide/peek',lambda r:r['cancel']=='close'),
    ]:
        run=latest(predicate)
        if run:
            selected[label]=run
            item(Path(run['directory'])/'native.mp4',label,'Idle and cancellation',False,
                'Maintained private-seat events with actual injected keyboard resume; login-session safety is a fixture.' if run.get('compositor_inactivity_observed') else 'Actual native presentation; safety event/geometry is identified in caption, not real logind/suspend acceptance.')
    for label,predicate in [
        ('Fractional 1.25× camera',lambda r:r['scene']=='screen_window' and r['monitor']['scale']==1.25 and r['monitor'].get('transform')==0),
        ('Rotated left portrait glass',lambda r:r['scene']=='glass_inspection' and r['monitor'].get('transform')==1),
        ('Rotated left real-window perch',lambda r:r['scene']=='window_perch' and r['monitor'].get('transform')==1),
        ('4K camera correctly unavailable',lambda r:r['scene']=='screen_window' and r['monitor']['scale']==2),
        ('4K retains native-size grooming',lambda r:r['scene']=='wing_grooming' and r['monitor']['scale']==2),
    ]:
        run=latest(predicate)
        if run:
            selected[label]=run
            item(Path(run['directory'])/'native.mp4',label,'Output and source caps',False,
                'Real private output configuration, physical dimensions/origin/scale preserved in monitors.json. Availability is based on source pixels, not monitor labels.')
    (OUT/'selected.json').write_text(json.dumps(selected,indent=2)+'\n')
    (OUT/'native-audit.json').write_text(json.dumps(audit,indent=2)+'\n')
    (OUT/'gallery-batch.json').write_text(json.dumps({'title':'Native AFK dragon · eleven scenes and real inactivity',
        'sections':{'Native scenes':'All eleven are actual native recordings. Explicit forced-source clips are distinct from real swayidle evidence.',
                    'Physical pixels':'Full compositor PNGs and unscaled crops; giant scenes have dedicated1254px sources.',
                    'Idle and cancellation':'Real compositor timeout/resume is identified explicitly; logind/session safety is a fixture, not acceptance.',
                    'Output and source caps':'Fractional scaling, negative-origin portrait, and honest4K closeup rejection.'},
        'items':items},indent=2)+'\n')
    print(OUT/'gallery-batch.json')
    print(manifest())


if __name__=='__main__':main()
