#!/usr/bin/env python3
"""Six authored cloud samples, isolated local history, no tools or live conversations.

Explicit invocation spends up to six provider calls; output contains metadata only.
"""
import json
import argparse
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CASES = [
    ('clear_request', 'Explain why a hash map lookup is usually fast.', 'respond', ''),
    ('thinking', 'Maybe I could use a map here.', 'wait', ''),
    ('self_question', 'Why is that happening hmm maybe the index.', 'wait', ''),
    ('explicit_wait', 'Hold on let me work this out.', 'wait', ''),
    ('short_answer', 'Five.', 'respond', 'What value does that expression return?'),
    ('request_after_wait', 'Okay, can you explain the map approach now?', 'respond', 'I will wait while you think.'),
]
def main():
    folder = ROOT / '.tmp' / ('semantic-cloud-' + str(time.time_ns()))
    folder.mkdir(parents=True)
    env = {**os.environ, 'TOHRU_ASSISTANT_DATA': str(folder / 'data'),
           'TOHRU_ASSISTANT_SCRATCH': str(folder / 'scratch')}
    helper = [sys.executable, str(ROOT / 'bin/tohru-assistant')]
    rows = []
    options = argparse.ArgumentParser()
    options.add_argument('--limit', type=int, choices=range(1, 7), default=6)
    options.add_argument('--provider', choices=('claude', 'codex'), default='claude')
    options.add_argument('--model', help='Explicit configured model for a bounded comparison')
    parsed = options.parse_args()
    limit, provider = parsed.limit, parsed.provider
    model = parsed.model or ('claude-sonnet-5-5' if provider == 'claude' else 'gpt-6.2-sol')
    for name, prompt, label, previous in CASES[:limit]:
        conversation = []
        if previous:
            result = subprocess.run(helper + ['commit', '--provider', provider, '--voice'],
                                    input=json.dumps(dict(prompt='I am working on this problem.', answer=previous)),
                                    env=env, text=True, capture_output=True, timeout=10, check=True)
            conversation = ['--conversation', json.loads(result.stdout)['conversation']]
        args = helper + ['chat', '--provider', provider, '--backend', 'local', '--voice', '--draft',
                         '--semantic', '--persona', 'off', '--memory', 'off', '--orca', 'false',
                         '--model', model, '--effort', 'low'] + conversation
        start = time.monotonic()
        events = []
        with (folder / (name + '.stderr')).open('w') as err:
            child = subprocess.Popen(args, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=err, text=True)
            child.stdin.write(prompt)
            child.stdin.close()
            for line in child.stdout:
                event = json.loads(line)
                events.append(dict(event=event['event'], decision=event.get('decision'),
                                   error=event.get('message', '')[:500] if event['event'] == 'error' else '',
                                   ms=round((time.monotonic()-start)*1000, 2)))
            code = child.wait(timeout=10)
        decision = next((e['decision'] for e in events if e['event'] == 'semantic'), None)
        rows.append(dict(case=name, label=label, decision=decision, correct=decision == label,
                         exit=code, events=events, elapsed_ms=round((time.monotonic()-start)*1000, 2)))
        print(json.dumps(rows[-1]), flush=True)
    result = dict(provider=provider, model=model, effort='low', calls=len(rows),
                  kind='authored labeled sample, not held out; actual provider, no tools', rows=rows)
    (folder / 'results.json').write_text(json.dumps(result, indent=2)+'\n')
    print(str(folder / 'results.json'))

if __name__ == "__main__": main()
