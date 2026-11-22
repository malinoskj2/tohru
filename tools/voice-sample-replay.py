#!/usr/bin/env python3
"""Replay recorded cloud decision timings through production capture policy."""
import argparse
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import runpy

p=argparse.ArgumentParser()
p.add_argument('--module', default='bin/tohru-voice')
p.add_argument('results')
a=p.parse_args()
root=Path(__file__).resolve().parents[1]
cases=runpy.run_path(str(root/'tools/voice-semantic-sample.py'))['CASES']
records={r['case']:r for r in json.loads(Path(a.results).read_text())['rows']}
loader=importlib.machinery.SourceFileLoader('sample_voice',a.module)
spec=importlib.util.spec_from_loader(loader.name,loader)
v=importlib.util.module_from_spec(spec)
loader.exec_module(v)
f=runpy.run_path(str(root/'tests/assistant/voice.py'))
f['make_voice'].__globals__['voice_module']=v
rows=[]
for name,prompt,label,previous in cases:
    record=records[name]
    intent=next((e for e in record['events'] if e['event']=='semantic'),None)
    events=[]
    v.emit=lambda event,**kw: events.append(dict(event=event,**kw))
    v.diagnostic=lambda *args,**kw: None
    voice=f['make_voice'](f['FakeRecognizer']([prompt],prompt))
    voice.conversation_until=float('inf')
    voice.begin([])
    voice.hear(b'',True)
    requested=None
    handoff=None
    injected=False
    for tick in range(1125):
        now=(tick+1)*32
        if not voice.in_speech: break
        voice.hear(b'',False)
        drafts=[e for e in events if e['event']=='draft']
        if drafts and requested is None: requested=now
        if intent and drafts and not injected and now >= requested+intent['ms']:
            injected=True
            voice.command(dict(cmd='semantic',decision=intent['decision'],text=drafts[-1]['text'],token=drafts[-1].get('token')))
        if any(e['event']=='turn' for e in events): handoff=now
    rows.append(dict(case=name,label=label,recorded_decision_ms=intent['ms'] if intent else None,
                     handoff_ms=handoff,premature=label=='wait' and handoff is not None,
                     missed_bounded=label=='respond' and handoff is None))
print(json.dumps(dict(kind='recorded isolated cloud decisions + simulated capture; not live audio',rows=rows),indent=2))
