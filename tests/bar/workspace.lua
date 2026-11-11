local handlers, calls = {}, {}
_G.on=function(name, fn) handlers[name]=fn end
_G.text, _G.fact = {}, {}
local observed
_G.sys={watch=function(name,fn) assert(name=="workspaces"); observed=fn; return true end,
 call=function(name,value) assert(name=="workspaces.focus"); calls[#calls+1]=value; if value==9 then error("not present") end end}
local states
local controller = assert(loadfile("shell/bar/workspace.luau"))()({observe=function(highest,active) states={highest,active} end})
assert(fact.ws_available)
observed({active=2,list={{id=1},{id=2},{id=7},{id=-1},{id=2.5}}})
assert(states[1]==7 and states[2]==2)
handlers.workspace_select(2); assert(calls[1]==2)
handlers.workspace_select(2.5); assert(#calls==1)
handlers.workspace_select(9); assert(#calls==1 and text.ws_error=="Workspace is not advertised")
assert(fact["ws_advertised.2"] and not fact["ws_advertised.9"])
observed({active=2,list={{id=1},{id=2},{id=9}}})
handlers.workspace_select(9); assert(#calls==2 and text.ws_error:find("not present",1,true))
observed({active=0,list={}}); assert(states[1]==5 and states[2]==nil)
assert(not fact["ws_advertised.2"] and not fact["ws_advertised.9"])
handlers.workspace_select(2);assert(#calls==2)
assert(text.bar_outputs=="")
controller.prefer("DP-9"); assert(text.bar_outputs=="DP-9" and states[1]==5 and states[2]==nil)
controller.prefer("")
local two={active=1,list={{id=3,monitor="DP-2",active=true},{id=1,monitor="DP-1",active=true},{id=4,monitor="DP-2"},{id=10,monitor="DP-1"},{id=6,monitor="a,b"},{id=7}}}
observed(two); assert(text.bar_outputs=="DP-1, DP-2" and states[1]==10 and states[2]==1)
controller.prefer("DP-2"); assert(text.bar_outputs=="DP-2" and states[1]==5 and states[2]==3)
assert(fact["ws_advertised.3"] and fact["ws_advertised.4"] and not fact["ws_advertised.1"] and not fact["ws_advertised.10"])
handlers.workspace_select(10); assert(#calls==2 and text.ws_error=="Workspace is not advertised")
controller.prefer("DP-1, DP-2"); assert(text.bar_outputs=="DP-1, DP-2" and states[1]==10 and states[2]==1)
controller.prefer(nil); assert(text.bar_outputs=="DP-1, DP-2" and states[1]==10)
observed({active=12,list={{id=1},{id=12}}}); assert(states[1]==10 and states[2]==12)
print("Native workspace observation/focus entry policy passed")
