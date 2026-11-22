#!/usr/bin/env python3
"""Synthetic provider/TTS timing through the production Speaker; never acoustic onset."""
import os
import argparse, importlib.machinery, importlib.util, json, statistics, threading, time
import numpy as np
p = argparse.ArgumentParser()
p.add_argument('--module', default='bin/tohru-voice')
p.add_argument('--real-tts', action='store_true', help='Real espeak-ng synthesis, synthetic provider, fake player')
p.add_argument('--kokoro-folder', help='Explicit pinned local Kokoro folder; run inside resource reservation')
p.add_argument('--warm', action='store_true', help='Preload and reuse model across repetitions; default is fresh engine each time')
p.add_argument('--pulse-sink', help='Use actual pacat on this explicit sink; no acoustic measurement')
p.add_argument('--interrupt-ms', type=float, help='Cancel this many ms after first write')
p.add_argument('--sentences', type=int, default=2, help='Number of delayed complete sentences, followed by a final fragment')
p.add_argument('--repetitions', type=int, default=8)
a = p.parse_args()
if a.kokoro_folder and not (os.environ.get('GPU_LEASE_ID') or os.environ.get('SLURM_JOB_ID')):
    p.error('Kokoro must run inside gpu-run or resource-run; keep the reservation through model unload')
loader = importlib.machinery.SourceFileLoader('voice_bench', a.module)
spec = importlib.util.spec_from_loader(loader.name, loader)
v = importlib.util.module_from_spec(spec); loader.exec_module(v)
real_popen = v.subprocess.Popen
class Player:
    def __init__(self, *args, **kwargs):
        self.child = real_popen(*args, **kwargs) if a.pulse_sink else None
        self.stdin = self
        self.rate = int(next(arg.split('=',1)[1] for arg in args[0] if arg.startswith('--rate=')))
    def write(self, data):
        global awaiting_sentence, previous_sample_end
        now = time.monotonic()
        if hush_return_at: stale_writes.append(now)
        if not onset: onset.append(now)
        if awaiting_sentence:
            if previous_sample_end is not None: gaps.append((now-previous_sample_end)*1000)
            awaiting_sentence = False
        previous_sample_end = now + len(data)/(2*self.rate)
        if self.child: return self.child.stdin.write(data)
    def flush(self):
        if self.child: self.child.stdin.flush()
    def close(self):
        if self.child: self.child.stdin.close()
    def kill(self):
        if self.child: self.child.kill()
    def poll(self): return self.child.poll() if self.child else None
    def wait(self):
        if self.child:
            result = self.child.wait()
            if cancel_requested_at: reaped.append((time.monotonic()-cancel_requested_at)*1000)
            return result
class Engine:
    def stream(self, text, np):
        for part in v.sentences(text):
            time.sleep(.06)
            yield np.ones(2400, dtype=np.float32)*.1, 24000, part, []
class TimedEngine:
    def __init__(self): self.engine = v.Kokoro({"model": a.kokoro_folder, "voicepack": "af_heart"}) if a.kokoro_folder else v.Espeak({}) if a.real_tts else Engine()
    def stream(self, text, np):
        yield from self._run(text, np)
    def synthesize(self, text, np):
        yield from self._run(text, np, single=True)
    def _run(self, text, np, single=False):
        before = time.monotonic()
        method = self.engine.synthesize if single and hasattr(self.engine, "synthesize") else self.engine.stream
        for clip in method(text, np):
            synthesis.append((time.monotonic()-before)*1000)
            start, end = v.voiced(np, clip[0], clip[1], floor=.001)
            clips.append({'duration_ms':len(clip[0])/clip[1]*1000, 'lead_ms_at_minus60db':start*1000, 'tail_ms_at_minus60db':(len(clip[0])/clip[1]-end)*1000})
            yield clip
            before = time.monotonic()
class Voice:
    def spoke(self): done.set()
v.subprocess.Popen = lambda argv, *args, **kw: Player(argv, *args, **kw) if argv[0] == "pacat" else real_popen(argv, *args, **kw)
def event(event, **kw):
    global awaiting_sentence
    if event == 'sentence':
        awaiting_sentence = True
        spoken.append(kw['text'])
    if event == 'error': errors.append(kw.get('message'))
v.emit = event
v.diagnostic = lambda *args, **kw: None
rows = []
shared = TimedEngine() if a.warm else None
if shared and hasattr(shared.engine, "load"): shared.engine.load()
for i in range(a.repetitions):
    onset, done, synthesis, gaps, cancellation, errors, clips = [], threading.Event(), [], [], [], [], []
    awaiting_sentence, previous_sample_end = False, None
    cancel_requested_at, hush_return_at = None, None
    stale_writes, reaped, spoken = [], [], []
    s = v.Speaker(np, Voice()); s.engine = shared or TimedEngine(); s.sink = lambda: a.pulse_sink
    def interrupt():
        global cancel_requested_at, hush_return_at
        while not onset and not done.is_set(): time.sleep(.001)
        if onset:
            time.sleep(a.interrupt_ms/1000)
            before = time.monotonic(); cancel_requested_at = before; s.hush()
            hush_return_at = time.monotonic()
            cancellation.append((hush_return_at-before)*1000)
    if a.interrupt_ms is not None: threading.Thread(target=interrupt, daemon=True).start()
    start = time.monotonic()
    parts = ['First complete sentence. ', 'Second complete sentence. '] + [f'Additional complete sentence {j}. ' for j in range(max(0,a.sentences-2))] + ['Final fragment']
    if hasattr(v, 'SpeechStream'):
        stream = v.SpeechStream(); s.say(stream, 'bench')
        for part in parts:
            time.sleep(.12); stream.feed(part)
        stream.feed('', final=True)
    else:
        for part in parts: time.sleep(.12)
        s.say(''.join(parts), 'bench')
    assert done.wait(60)
    completed_at = time.monotonic()
    if not shared and hasattr(s.engine.engine, "unload"): s.engine.engine.unload()
    if errors or not onset: raise RuntimeError(errors or "No application write observed")
    rows.append({'first_application_write_ms': (onset[0]-start)*1000, 'completion_ms': (completed_at-start)*1000, 'synthesis_ms': synthesis, 'clips':clips, 'application_gaps_ms': gaps, 'hush_return_ms': cancellation, 'player_reaped_ms':reaped, 'writes_after_hush_return':len(stale_writes), 'delivered_text_matches': ' '.join(' '.join(spoken).split()) == ' '.join(''.join(parts).split())})
if shared and hasattr(shared.engine, "unload"): shared.engine.unload()
print(json.dumps({'kind':('synthetic provider + '+('real Kokoro '+('preloaded' if a.warm else 'fresh engine') if a.kokoro_folder else 'real espeak-ng' if a.real_tts else 'synthetic TTS')+' + '+('pacat sink '+a.pulse_sink if a.pulse_sink else 'fake player')+'; no capture/ASR/acoustic onset'), 'n':len(rows), 'samples':rows, 'median':{k:statistics.median(r[k] for r in rows) for k in ('first_application_write_ms', 'completion_ms')}, 'p95_nearest_rank':{k:sorted(r[k] for r in rows)[max(0, __import__('math').ceil(.95*len(rows))-1)] for k in ('first_application_write_ms', 'completion_ms')}}, indent=2))
