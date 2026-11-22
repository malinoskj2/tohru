#!/usr/bin/env python3
"""Controlled pacing clock through production Speaker.play; no audio hardware."""
import argparse
import importlib.machinery
import importlib.util
import json
import threading
import numpy as np

p = argparse.ArgumentParser()
p.add_argument('--module', default='bin/tohru-voice')
a = p.parse_args()
loader = importlib.machinery.SourceFileLoader('history_voice', a.module)
spec = importlib.util.spec_from_loader(loader.name, loader)
v = importlib.util.module_from_spec(spec)
loader.exec_module(v)
v.emit = v.diagnostic = lambda *args, **kw: None
rows = []
for cut in (0, .01, .1, .5, .99, 1.01, 1.5, 1.99, 2.1):
    clock = [0.0]
    stop = threading.Event()
    class Condition:
        def __enter__(self): return self
        def __exit__(self, *args): pass
        def wait(self, seconds=0):
            clock[0] += seconds
            if clock[0] >= cut: stop.set()
    class Player:
        def __init__(self, *args, **kw): self.stdin = self
        def write(self, data): pass
        def flush(self): pass
        def close(self): pass
        def kill(self): pass
        def poll(self): return None
        def wait(self): pass
    v.subprocess.Popen = Player
    v.time.monotonic = lambda: clock[0]
    v.synthesized = lambda *args: iter([
        (np.ones(24000, dtype=np.float32) * .1, 24000, 'First complete sentence.', []),
        (np.ones(24000, dtype=np.float32) * .1, 24000, 'Second complete sentence.', []),
    ])
    s = object.__new__(v.Speaker)
    s.np = np
    s.voice = type('Voice', (), {'thinking': False})()
    s.cond = Condition()
    s.engine = s.player = None
    s.stop = stop
    s.speaking = s.paused = False
    s.recent = []
    s.sink = lambda: None
    if cut == 0: stop.set()
    s.play('unused', 'fixture', stop)
    context = s.spoken_so_far()
    rows.append(dict(cut_s=cut, completed=list(s.spoken_parts), context=context,
                     fraction_presented_as_heard=bool(s.current_part and context and 'uncertain' not in context)))
print(json.dumps(dict(kind='controlled application clock, synthetic clips, fake pipe; not physical hearing', rows=rows), indent=2))
