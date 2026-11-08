local handlers, calls = {}, {}
_G.on=function(name, fn) handlers[name]=fn end
_G.text, _G.fact = {}, {}
local observed
_G.sys={watch=function(name,fn) assert(name=="workspaces"); observed=fn; return true end,
 call=function(name,value) assert(name=="workspaces.focus"); calls[#calls+1]=value; if value==9 then error("not present") end end}
local role, prefix, states
local controller = assert(loadfile("shell/bar/workspace.luau"))()(function(binding,r,p)
    role,prefix=r,p
    return {observe=function(highest,active) states={highest,active} end,stop=function() end}
end,{})
assert(role=="workspace" and prefix=="ws_" and fact.ws_available)
observed({active=2,list={{id=1},{id=7},{id=-1},{id=2.5}}})
assert(states[1]==7 and states[2]==2)
handlers.workspace_select(2); assert(calls[1]==2)
handlers.workspace_select(2.5); assert(#calls==1)
handlers.workspace_select(9); assert(text.ws_error:find("not present",1,true))
observed({active=0,list={}}); assert(states[1]==5 and states[2]==nil)
print("Native workspace observation/focus entry policy passed")
