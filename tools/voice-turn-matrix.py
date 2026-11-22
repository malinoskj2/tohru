#!/usr/bin/env python3
"""Expanded authored development matrix: real capture policy, mocked policy transport.

Answer-ready times are simulated validation deadlines, never measured audio onset.
"""
import argparse
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import runpy

p = argparse.ArgumentParser()
p.add_argument('--module', default='bin/tohru-voice')
a = p.parse_args()
loader = importlib.machinery.SourceFileLoader('matrix_voice', a.module)
spec = importlib.util.spec_from_loader(loader.name, loader)
v = importlib.util.module_from_spec(spec)
loader.exec_module(v)
f = runpy.run_path(str(Path(__file__).resolve().parents[1] / 'tests/assistant/voice.py'))
f['make_voice'].__globals__['voice_module'] = v
scenarios = [
    ('request', 'Explain this index', 'respond', None, []),
    ('short_answer', 'Five', 'respond', None, []),
    ('thinking', 'Maybe a map would work', 'wait', None, []),
    ('self_question', 'Why is that hmm perhaps the index', 'wait', None, []),
    ('explicit_wait', 'Hold on let me think', 'wait', None, []),
    ('malformed_request', 'Explain this index', 'error', None, []),
    ('unsupported_local', 'Explain this index', 'unsupported', None, []),
    ('unsupported_orca', 'Explain this index', 'unsupported', None, []),
    ('final_revision', 'Explain this index', 'respond', 'Explain the second index instead', []),
    ('multi_revision', 'Explain this index', 'respond', None,
     [(320, 'Explain the next index'), (640, 'Explain the second index instead')]),
    ('after_wait', 'Can you explain that now', 'respond', None, []),
    ('ambiguous', 'That is interesting', 'wait', None, []),
]
rows=[]
for name, initial, decision, final, revisions in scenarios:
    for delay in (0, 1000, 5000, None):
        events=[]
        v.emit=lambda event, **kw: events.append(dict(event=event, **kw))
        v.diagnostic=lambda *args, **kw: None
        recognizer=f['FakeRecognizer']([initial], final or initial)
        voice=f['make_voice'](recognizer)
        voice.conversation_until=float('inf')
        if name == 'after_wait': voice.pending_continuation='Maybe a map would work'
        voice.begin([])
        voice.hear(b'', True)
        requested={}
        sent=set()
        handoff=None
        for tick in range(1125):  # 36 seconds, beyond capture and decision deadlines
            now=(tick+1)*32
            if not voice.in_speech: break
            changed=False
            for at, text in revisions:
                if now == at:
                    recognizer.partials=[text]
                    recognizer.final=text
                    changed=True
            voice.hear(b'', changed)
            for draft in [e for e in events if e['event']=='draft']:
                token=draft.get('token', str(len(requested)))
                requested.setdefault(token, now)
                if token not in sent and delay is not None and now-requested[token] >= delay:
                    sent.add(token)
                    voice.command(dict(cmd='semantic', text=draft['text'], token=token, decision=decision))
                    if decision == 'unsupported':
                        voice.command(dict(cmd='draft', ok=False))
            if any(e['event']=='turn' for e in events):
                handoff=now
        intended = decision != 'wait'
        for answer_ms in (600, 6000, 31000):
            # Separate synthetic answer validation from actual production handoff.
            answer_deadline = min(requested.values(), default=0) + answer_ms
            ready = max(handoff, answer_deadline) if handoff is not None and answer_ms <= 30000 else None
            rows.append(dict(case=name, decision=decision, policy_delay_ms=delay,
                             full_answer_ms=answer_ms, handoff_ms=handoff,
                             simulated_validated_answer_ms=ready,
                             simulated_answer_timeout=answer_ms>30000,
                             outside_3s=handoff is not None and handoff>3000,
                             missed_bounded=intended and handoff is None,
                             premature_wait=not intended and handoff is not None,
                             confirmed_wait_takeover=not intended and delay is not None and handoff is not None and handoff >= delay+224,
                             drafts=sum(e['event']=='draft' for e in events)))
print(json.dumps(dict(kind='authored expanded development matrix; not held out', rows=rows),indent=2))
