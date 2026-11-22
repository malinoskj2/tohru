#!/usr/bin/env python3
"""Run the shipped renderer/puppet in the sandbox, without a voice service or models.
Use pleamar --say dragon-body-review 'emit review_listen' (or review_think,
review_speak, review_pause, review_interrupt, review_calm, review_reduced 1).
Set text ai_draft, then emit review_sentence to exercise speech gestures.
"""
import argparse
import os
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--runtime',required=True)
p.add_argument('--seconds',type=int,default=0)
p.add_argument('--baseline',action='store_true')
p.add_argument('--record',default='')
a=p.parse_args()
d=ROOT/('.tmp/full-body/baseline' if a.baseline else '.tmp/full-body/native'); d.mkdir(parents=True,exist_ok=True)
for item in (ROOT/'shell').iterdir():
    if item.name not in ['bar.plm','bar.luau'] and not (d/item.name).exists(): (d/item.name).symlink_to(item)
if a.baseline:
    # Reproduce the checked-out branch's starting behavior, alongside the new
    # overlay. This only copies the needed original files; it creates no worktree.
    assistant=d/'assistant'
    if assistant.is_symlink(): assistant.unlink()
    assistant.mkdir(exist_ok=True)
    for item in (ROOT/'shell/assistant').iterdir():
        if item.name not in ['puppet.luau','state.plm','dragon.plm','dragon'] and not (assistant/item.name).exists(): (assistant/item.name).symlink_to(item)
    revision=json.loads((ROOT/'tools/dragon/art/legacy-provenance.json').read_text())['revision']
    for name in ['puppet.luau','state.plm','dragon.plm']:
        original=subprocess.check_output(['git','show',revision+':shell/assistant/'+name],cwd=ROOT).decode()
        if name=='dragon.plm': original=original.replace('margin: 0, 18, 18, 0','margin: 0, 228, 18, 0')
        (assistant/name).write_text(original)
    dragon=assistant/'dragon';dragon.mkdir(exist_ok=True)
    for item in (ROOT/'shell/assistant/dragon').iterdir():
        if item.name!='body.webp' and not (dragon/item.name).exists(): (dragon/item.name).symlink_to(item)
    body=dragon/'body.webp'
    if not body.exists(): body.symlink_to(ROOT/'tools/dragon/art/legacy-body.webp')
s=(ROOT/'shell/bar.plm').read_text().replace('scene Tohru {','scene DragonReview {',1)
events=['review_listen','review_think','review_speak','review_sentence','review_calm','review_pause','review_interrupt','review_reduced','review_key','review_live','review_finish']
s=s.replace(' include "panel-ink.plm"','\n'.join(' event '+k+' ->' for k in events)+'\n include "panel-ink.plm"')
s=s.replace(' include "panel-ink.plm"',' text review_pose = ""\n include "panel-ink.plm"')
(d/'review.plm').write_text(s)
(d/'review.luau').write_text(Path(__file__).with_suffix('.luau').read_text().replace('@ART@',str(d/'assistant/dragon' if a.baseline else ROOT/'shell/assistant/dragon')))
cmd=[a.runtime,'--scene',str(d/'review.plm'),'--answer-as','dragon-body-baseline' if a.baseline else 'dragon-body-review','--no-hud','--stall','0']
if a.record: cmd+=['--record',a.record]
if a.seconds: cmd+=['--seconds',str(a.seconds)]
raise SystemExit(subprocess.call(cmd,env=dict(os.environ,WAYLAND_DISPLAY='hyprland-1')))
