local handlers, calls, watchers = {}, {}, {}
_G.on=function(name, fn) handlers[name]=fn end
_G.text, _G.fact = {}, {}
_G.sys={watch=function(name,fn) watchers[name]=fn; return name~="workspaces" end,
 call=function(name,value) assert(name=="workspaces.focus"); calls[#calls+1]=value end}
local states
local controller=assert(loadfile("shell/bar/workspace.luau"))()({observe=function(n,active)states={n,active}end})
assert(not fact.ws_available and text.bar_outputs==nil)
-- Outputs arrive before any workspace protocol/report. Physical topology alone owns the bar.
watchers.outputs({list={{name="HDMI-A-1",scale=2,transform=1},{name="eDP-1",scale=1}}})
assert(text.bar_outputs=="HDMI-A-1, eDP-1" and not fact.ws_available)
watchers.workspaces({list={}})
assert(text.bar_outputs=="HDMI-A-1, eDP-1" and states[1]==0)
-- Names and opaque IDs, no monitor metadata. Slots activate actual IDs, not slot numbers.
watchers.workspaces({active="dev",list={{id="dev",name="Development"},{id="chat",name="Chat",can_activate=false}}})
assert(fact.ws_available and text['ws_name.2']=="Development" and states[2]==2)
handlers.workspace_select(1);assert(#calls==0)
handlers.workspace_select(2);assert(calls[1]=="dev")
controller.prefer("HDMI-A-1");assert(text.bar_outputs=="HDMI-A-1")
-- Order/scale/rotation changes must not remap names or lose a preferred connected output.
watchers.outputs({list={{name="eDP-1",scale=1.5,transform=3},{name="HDMI-A-1",scale=1}}})
assert(text.bar_outputs=="HDMI-A-1")
watchers.outputs({list={{name="eDP-1"}}});assert(text.bar_outputs=="eDP-1")
watchers.outputs({list={{name="HDMI-A-1"},{name="eDP-1"}}});assert(text.bar_outputs=="HDMI-A-1")
controller.prefer("");assert(text.bar_outputs=="HDMI-A-1, eDP-1")
-- Hyprland IDs beyond ten and named/special IDs remain routable.
watchers.workspaces({active=42,list={{id=42,name="work",monitor="eDP-1",active=true},{id=-99,name="special:scratch"}}})
assert(states[1]==2 and states[2]==2)
handlers.workspace_select(2);assert(calls[2]==42)
controller.prefer("eDP-1");assert(states[1]==2 and states[2]==2)
watchers.outputs({list={}});assert(text.bar_outputs=="")
print("PASS independent output topology, empty metadata, opaque IDs, capabilities and hotplug policy")
