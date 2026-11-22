from pathlib import Path
import json, hashlib, html
from PIL import Image
root=Path(__file__).resolve().parents[2] if 'docs' in str(Path(__file__)) else Path.cwd()
folder=root/'.tmp/discord-avatars'
data=json.loads((folder/'prompts.json').read_text())
names=['Pearl hello','Three-quarter greeting','Delighted','Listening','Soft wink','Cozy','Paws under chin','Wing frame','Curled emblem','Over shoulder']
notes=['Canonical warm full-character portrait; balanced open smile.','Raised greeting paw and a friendly three-quarter face.','Open joyful smile; strongest expression at tiny sizes.','Inquisitive diagonal tilt and an attentive gentle smile.','One open eye and a playful curved wink.','Closed smiling eyes and a relaxed tucked pose.','Two little paws directly beneath her chin; thoughtful and inviting.','Wider wings give a distinctive dragon silhouette.','Tail wraps beneath her feet into a compact emblem.','Quiet turned perspective with one prominent near wing.']
cards=[]; items=[]; metadata=[]
for p,name,note in zip(data['generations'],names,notes):
 file=folder/p['filename']; im=Image.open(file)
 row=dict(number=p['number'],name=name,note=note,path=str(file),filename=file.name,dimensions=list(im.size),format=im.format,mode=im.mode,alpha='A' in im.getbands(),bytes=file.stat().st_size,sha256=hashlib.sha256(file.read_bytes()).hexdigest(),original_path=p['original_path'],prompt=p['prompt'])
 metadata.append(row)
 items.append(dict(path=str(file),status='review',section='Ten avatar concepts',caption=f"{p['number']:02d} — {name}",note=note,ask='Which avatar do you prefer for Tohru? Reply with its #id.'))
 samples=''
 for theme in ['light','dark']:
  sizes=''.join(f'<figure><img class="circle" src="{file.name}" width="{s}" height="{s}" alt="{name} {s}px circular crop"><figcaption>{s}px</figcaption></figure>' for s in [128,64,32])
  samples+=f'<div class="samples {theme}">{sizes}</div>'
 cards.append(f'<article id="avatar-{p["number"]:02d}"><h2>{p["number"]:02d} · {name}</h2><a href="{file.name}"><img class="original" src="{file.name}" alt="{name} square original"></a><p>{note}</p>{samples}</article>')
page='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Tohru · ten avatar concepts</title><style>
*{box-sizing:border-box}body{margin:0;background:#13121d;color:#f4edf8;font:16px/1.5 system-ui,sans-serif}header{max-width:1500px;margin:auto;padding:32px 24px 18px}h1{margin:0;font-size:clamp(28px,4vw,46px)}header p{max-width:780px;color:#cdc1dc}main{max-width:1500px;margin:auto;display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,340px),1fr));gap:20px;padding:0 24px 40px}article{background:#24202e;border:1px solid #44364e;border-radius:18px;overflow:hidden}h2{font-size:20px;padding:16px 18px;margin:0}article p{min-height:48px;margin:16px 18px;color:#d6c8dd}.original{display:block;width:100%;height:auto}.samples{display:flex;align-items:center;justify-content:space-evenly;gap:8px;padding:16px 8px}.light{background:#f5f1f7;color:#33263c}.dark{background:#15131b;color:#e0d7e8}figure{margin:0;text-align:center}.circle{border-radius:50%;display:block}figcaption{font-size:12px;margin-top:5px}a{color:inherit}
</style><header><h1>Tohru · ten avatar concepts</h1><p>One little pearl dragon, ten moods. Compare the untouched square originals with circular crops at 128, 64 and 32px on light and dark surroundings. Choose your favorite in the review gallery by #id.</p></header><main>'''+''.join(cards)+'</main></html>'
(folder/'index.html').write_text(page)
(folder/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
(folder/'gallery-batch.json').write_text(json.dumps(dict(title='Tohru · choose her Discord avatar',sections={'Ten avatar concepts':'Compare expressions, silhouette and readability. Originals are unchanged.'},items=items),indent=2)+'\n')
print('Built preview, metadata and gallery batch for',len(metadata),'verified images.')

