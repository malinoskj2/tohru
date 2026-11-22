#!/usr/bin/env python3
"""Bounded live text-only comparison via production adapters; never opens audio.

32 ephemeral Codex calls, plus one Claude availability sample. No tools/MCP or
persisted conversations. Authored quality rubrics are fixed before execution.
"""
import importlib.machinery
import importlib.util
import argparse
import json
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'bin'))
loader = importlib.machinery.SourceFileLoader('assistant_bench', str(ROOT / 'bin/tohru-assistant'))
spec = importlib.util.spec_from_loader(loader.name, loader)
a = importlib.util.module_from_spec(spec)
loader.exec_module(a)

CASES = [
    ('lookup', 'Why is a hash-map lookup usually fast, and is constant time guaranteed? Keep it to two short spoken sentences.',
     'Explains hashing/direct lookup; distinguishes average from worst case/collisions; no guarantee.'),
    ('arithmetic', 'I have three packs of eight batteries and give away seven batteries. How many remain? Reply briefly.',
     '17 batteries; concise and no incorrect arithmetic.'),
    ('microphone', 'Is a green microphone icon proof other people can hear me? Reply in two short spoken sentences.',
     'No guarantee; suggests checking app/input/permission or actual received sound.'),
    ('action', 'Turn my microphone off.',
     'Exactly <<needs-tools>>; must not claim action was performed.'),
]
CONFIGS = [('gpt-6.2-sol','low'), ('gpt-6.2-sol','medium'),
           ('gpt-6.1-sol','low'), ('gpt-6.1-sol','medium')]


def main():
    options = argparse.ArgumentParser()
    options.add_argument('--extra', action='store_true', help='Additional lighter/larger models; 24 Codex calls, no repeated Claude probe')
    options.add_argument('--confirm', action='store_true', help='Eight additional robustness samples for Luna/low and current default')
    opts = options.parse_args()
    extra = opts.extra
    configs = [('gpt-6-luna','low'), ('gpt-6-luna','medium'), ('gpt-6.2-astra','low')] if extra else CONFIGS
    cases = CASES
    if opts.confirm:
        configs = [('gpt-6-luna','low'), ('gpt-6.2-sol','medium')]
        cases = [
            ('probability', 'A box has three red and two blue marbles. Draw two without replacement. What is the probability both are red? Answer in at most two spoken sentences.', 'Three tenths or 30 percent.'),
            ('time', 'A meeting starts at 14:10 and lasts 95 minutes. When does it end? Reply briefly.', '15:45 or 3:45 PM.'),
            ('unknown_unit', 'How many grams are in a blarg? Reply briefly.', 'Do not invent a conversion; ask what the unit means.'),
            ('action', 'Please mute my microphone and confirm that you did it.', 'Exactly <<needs-tools>>; no false action claim.'),
        ]
    folder = ROOT / '.tmp' / ('voice-models-' + str(time.time_ns()))
    folder.mkdir(parents=True)
    a.SCRATCH = folder / 'scratch'
    rows = []
    def sample(provider, model, effort, case, repetition):
        name, prompt, rubric = case
        system = a.instructions('', '', True) + '\n\n' + a.DRAFT_RULE
        argv = a.provider_argv(provider, {'id': None, 'resume': False, 'ephemeral': True},
                               model, effort, system, tools=False)
        start = time.monotonic()
        deltas = []
        parser, code, errors, stopped = a.run_provider(a.BY_ID[provider], argv, prompt,
            lambda text: deltas.append((time.monotonic()-start, text)), timeout=25)
        elapsed = (time.monotonic()-start)*1000
        row = dict(provider=provider, model=model, effort=effort, case=name,
                   repetition=repetition, rubric=rubric, prompt=prompt,
                   first_text_ms=round(deltas[0][0]*1000,2) if deltas else None,
                   completed_ms=round(elapsed,2), exit=code, stopped=stopped,
                   error=(parser.error or ('; '.join(errors[-2:]) if code else ''))[:500],
                   text=parser.text[:2000], chars=len(parser.text))
        rows.append(row)
        (folder/'results.json').write_text(json.dumps(dict(kind='authored live text, production adapters, ephemeral tool-free',
            configs=configs, cases=cases, rows=rows),indent=2)+'\n')
        print(json.dumps({k:v for k,v in row.items() if k not in ('text','prompt','rubric')}), flush=True)
    # Interleave models, reverse order on second repeat; no tuning during run.
    for repetition in range(1 if opts.confirm else 2):
        for case in cases:
            for model, effort in configs if repetition == 0 else reversed(configs):
                sample('codex', model, effort, case, repetition)
    if not extra and not opts.confirm:
        sample('claude', 'claude-sonnet-5-5', 'low', CASES[0], 0)
    print(str(folder/'results.json'))


if __name__ == '__main__': main()
