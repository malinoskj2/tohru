local events,watches={},{}
_G.fact={launcher_visible=false,launcher_closing=false,launcher_output_count=0,launcher_outputs_known=false}
_G.text={launcher_outputs="",["screen.0.name"]="DP-1",["screen.1.name"]="DP-2"}
_G.on=function(name,fn)events[name]=fn end
_G.sys={watch=function(name,fn)watches[name]=fn end}
local output=assert(loadfile("shell/launcher/output.luau"))()()
assert(text.launcher_outputs=="")
watches.window({monitor="DP-2"});output.open();fact.launcher_visible=true
assert(text.launcher_outputs=="DP-2" and not fact.launcher_closing)
-- Focus shifts do not move a currently open launcher off its output.
watches.window({monitor="DP-1"});assert(text.launcher_outputs=="DP-2")
fact.launcher_visible=false;output.close();assert(fact.launcher_closing and text.launcher_outputs=="DP-2")
output.open();fact.launcher_visible=true;events.launcher_unmap();assert(text.launcher_outputs=="DP-1" and not fact.launcher_closing)
fact.launcher_visible=false;output.close();events.launcher_unmap();assert(text.launcher_outputs=="" and not fact.launcher_closing)
-- Native count excludes stale removed tail names; missing focus falls back first.
watches.window({monitor="missing"});fact.launcher_outputs_known=true;fact.launcher_output_count=1
output.open();fact.launcher_visible=true;assert(text.launcher_outputs=="DP-1")
text["screen.0.name"]="DP-3";events["text:screen.0.name"]();assert(text.launcher_outputs=="DP-3")
fact.launcher_output_count=0;events.launcher_outputs_changed();assert(text.launcher_outputs=="")
text["screen.0.name"]="DP-4";fact.launcher_output_count=1;events.launcher_outputs_changed();assert(text.launcher_outputs=="DP-4")
-- VM reload during closing must retain role/timer condition until native rest.
fact.launcher_visible=false;output.close();events,watches={},{}
output=assert(loadfile("shell/launcher/output.luau"))()()
assert(fact.launcher_closing and text.launcher_outputs=="DP-4")
events.launcher_unmap();assert(not fact.launcher_closing and text.launcher_outputs=="")
-- Coalesced close before motion still schedules native closing check, not timeout.
output.open();fact.launcher_visible=true;fact.launcher_visible=false;output.close()
assert(fact.launcher_closing);events.launcher_unmap();assert(text.launcher_outputs=="")
print("Production native output focus/fallback/hotplug/rapid reopen/closing reload routing passed")
