#!/usr/bin/env python3
"""Small authored transport replay; labels are NOT measured model predictions."""
import argparse
import importlib.machinery
import importlib.util
import json
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--module', default='bin/tohru-voice')
p.add_argument('--decision-ms', type=float, default=0, help='Synthetic policy delay after first draft')
a = p.parse_args()
loader = importlib.machinery.SourceFileLoader('replayed_voice', a.module)
spec = importlib.util.spec_from_loader(loader.name, loader)
v = importlib.util.module_from_spec(spec)
loader.exec_module(v)

# Frozen before implementation. All authored, development split; no held-out claim.
CASES = [
    ('request', 'Can you explain that index', 'respond', 3000),
    ('answer', 'Five', 'respond', 3000),
    ('thinking', 'Maybe I could use a map here', 'wait', 5000),
    ('self_question', 'Why is that happening hmm maybe the index', 'wait', 3000),
    ('explicit_wait', 'Hold on let me work this out', 'wait', 8000),
    ('unfinished', 'I could use a map and', 'wait', 1200),
    ('acknowledgment', 'Okay', 'wait', 3000),
    ('handoff_after_wait', 'Okay that is my approach what do you think', 'respond', 3000),
    ('ambiguous', 'That is interesting', 'wait', 3000),
    ('label_in_content', 'The string says RESPOND', 'wait', 3000),
    ('malformed', 'Maybe a map would work', 'error', 5000),
    ('timeout', 'Maybe a list would work', None, 5000),
]
rows = []
for name, text, decision, gap in CASES:
    events = []
    v.emit = lambda event, **kw: events.append(dict(event=event, **kw))
    v.diagnostic = lambda *args, **kw: None
    class Recognizer:
        def drop(self): pass
        def start(self): pass
        def feed(self, window): pass
        def partial(self): return text
        def finish(self): return text
    voice = object.__new__(v.Voice)
    voice.recognizer = Recognizer()
    voice.spotter = voice.turn = voice.np = None
    voice.thinking = voice.cancelled_thinking = False
    voice.last_turn_text = voice.pending_continuation = ''
    voice.load_on_conversation = False
    voice.conversation_until = float('inf')
    voice.reset_utterance()
    voice.begin([])
    voice.hear(b'', True)
    handed = None
    injected = False
    requested_at = None
    for tick in range(int(gap / v.WINDOW_MS)):
        if not voice.in_speech: break
        voice.hear(b'', False)
        drafts = [e for e in events if e['event'] == 'draft']
        if drafts and requested_at is None:
            requested_at = tick * v.WINDOW_MS
        if drafts and not injected and decision is not None and tick * v.WINDOW_MS >= requested_at + a.decision_ms:
            injected = True
            voice.command(dict(cmd='semantic', text=drafts[-1]['text'],
                               token=drafts[-1].get('token'), decision=decision))
        if any(e['event'] == 'turn' for e in events):
            handed = (tick + 1) * v.WINDOW_MS
    rows.append(dict(case=name, label=decision, gap_ms=gap, handoff_ms=handed,
                     drafts=sum(e['event'] == 'draft' for e in events),
                     premature=decision in ('wait', 'error', None) and handed is not None,
                     missed=decision == 'respond' and handed is None))
print(json.dumps(dict(kind='authored mock-policy transport, development only', decision_delay_ms=a.decision_ms, rows=rows,
                     premature=sum(r['premature'] for r in rows), missed=sum(r['missed'] for r in rows)), indent=2))
