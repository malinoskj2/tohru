#!/usr/bin/env python3
"""Stage the real AFK composition/actor on a private display, without models.

This is explicit forced-scene evidence: the driver supplies qualified source
events. It does not claim that the test driver observes physical inactivity.
"""
import json
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
out=ROOT/'.tmp/afk-native-demo'
out.mkdir(parents=True,exist_ok=True)
for name in ('assistant','brand'):
    link=out/name
    if not link.exists():link.symlink_to(ROOT/'shell'/name)
state=(ROOT/'shell/assistant/state.plm').read_text()
state=state[:state.index('    let ai_colour')].replace('file "','file "assistant/')
(out/'state.plm').write_text(state+'}\n')
(out/'afk.plm').write_text('''scene AfkNativeTest {
surface { size: 1, 1; keyboard: none; reserve: -1 }
permissions { services: "window"; run: "tohru-flight-scene" }
fact notices_dnd = false
text test_output = ""
event test_scene ->
event test_voice ->
event test_probability ->
event test_auto ->
event test_idle_seconds ->
include "state.plm"
include "assistant/afk-render.plm"
include "assistant/dragon.plm"
}
''')
(out/'afk.luau').write_text('''
local puppet=require("assistant/puppet")(require("assistant/full-body"))
local flight=require("assistant/flight")(require("assistant/flight-path"),puppet,require("assistant/flight-actor"))
sys.watch("window",flight.window)
puppet.load(ART)
local afk=require("assistant/afk-native")(flight.actor,{art=ART,control=function(_,done)if done then done(true) end end})
local scenes=require("assistant/afk-scenes").catalog
local probability,auto,idle_seconds=100,false,300
on("test_probability",function(n)probability=n end)
on("test_auto",function(n)auto=n~=0 end)
on("test_idle_seconds",function(n)idle_seconds=n end)
on("test_scene",function(n)
    flight.outputs({text.test_output})
    puppet.reduced(n==12)
    flight.configure(function(_,default)return default end)
    afk.configure({
        ["assistant.afk_enabled"]={value=true},["assistant.afk_seed"]={value="17"},
        ["assistant.afk_probability_percent"]={value=probability},["assistant.afk_scene"]={value=auto and "auto" or scenes[n].id},
        ["assistant.afk_idle_seconds"]={value=idle_seconds},
        ["assistant.afk_quiet_rest"]={value=n==12},
    })
end)
on("test_voice",function(n)
    fact.ai_speaking=n~=0
    if n~=0 then flight.hearing(true) else flight.hearing(false) end
    afk.changed()
end)
'''.replace('puppet.load(ART)','' if '--cold' in sys.argv else 'puppet.load(ART)').replace('ART',json.dumps(str(ROOT/'shell/assistant/dragon'))))
print(out/'afk.plm')
