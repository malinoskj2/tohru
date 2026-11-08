-- Execute the actual private entry with native frontier fixtures, not a parallel factory.
local events,watches={},{}
_G.fact={launcher_visible=false,launcher_closing=false,launcher_outputs_known=false,launcher_output_count=0}
_G.text={launcher_outputs="",["screen.0.name"]="DP-1",["screen.1.name"]="DP-2"}
_G.model={}
_G.on=function(name,fn)events[name]=fn end
_G.sys={ask=function(name)assert(name=="files.read");return {}end,call=function()end,watch=function(name,fn)watches[name]=fn;return true end}
-- Native scene entry returns no value. Execute just the chunk.
local function run()
 local loaded={}
 _G.require=function(name)
  if not loaded[name] then loaded[name]=assert(loadfile("shell/launcher/"..name..".luau"))() end
  return loaded[name]
 end
 local chunk=assert(loadfile("shell/launcher/entry.luau"));chunk()
end
run();watches.window({monitor="DP-2"});watches.apps({{id="fixture",name="Fixture",exec="private-marker",categories="Utility",icon=""}})
events.launcher_open();assert(fact.launcher_visible and text.launcher_outputs=="DP-2")
events.launcher_close();assert(not fact.launcher_visible and fact.launcher_closing and text.launcher_outputs=="DP-2")
-- Actual entry recreation preserves the closing ownership flag and role route.
events,watches={},{};run();assert(fact.launcher_closing and text.launcher_outputs=="DP-2")
events.launcher_unmap();assert(not fact.launcher_closing and text.launcher_outputs=="")
watches.window({monitor="DP-1"});events.launcher_open();events.launcher_close();events.launcher_open();events.launcher_unmap()
assert(fact.launcher_visible and not fact.launcher_closing and text.launcher_outputs=="DP-1")
print("Actual launcher entry resolves all modules and retains native close/reopen ownership")
