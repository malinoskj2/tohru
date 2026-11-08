local events,watch,calls={},{},{}
_G.fact={power_warning_level=0};_G.text={}
_G.on=function(name,fn)events[name]=fn end
_G.sys={watch=function(name,fn)assert(name=="battery");watch=fn end}
local fail=false
local function notify(level,charge) if fail then error("Owner not ready") end;calls[#calls+1]={level,charge} end
local function battery(percent,charging,present)watch({percent=percent,charging=charging or false,present=present~=false})end
local power=assert(loadfile("shell/info/power.luau"))()(notify)
battery(.8);assert(#calls==0)
battery(.2);assert(calls[#calls][1]==1 and fact.power_warning_level==1)
battery(.21);assert(#calls==1);battery(.23);assert(calls[#calls][1]==0)
battery(.05);assert(calls[#calls][1]==2);battery(.02);assert(calls[#calls][1]==3)
battery(.04);assert(fact.power_warning_level==3);battery(.041);assert(fact.power_warning_level==2)
battery(.071);assert(fact.power_warning_level==1);battery(.071,true);assert(fact.power_warning_level==0)
fail=true;battery(.01);assert(fact.power_warning_level==0 and text.power_error:find("not ready",1,true))
fail=false;power.retry();assert(fact.power_warning_level==3)
local count=#calls
-- Recreate actual production controller with retained native facts, as Shared.fresh does.
events={};power=assert(loadfile("shell/info/power.luau"))()(notify)
battery(.01);assert(#calls==count)
battery(nil);assert(not fact.power_known and fact.power_warning_level==0)
battery(0/0);assert(not fact.power_known);battery(math.huge);assert(not fact.power_known)
battery(.8,false,false);assert(not fact.power_present)
local target,command
_G.sys.call=function(name,role,line)assert(name=="say");target,command=role,line end
assert(loadfile("shell/info/power.luau"))()()
battery(.18);assert(target=="tohru-notifications" and command=="emit power_warning_low 0.18")
print("Production native battery thresholds/hysteresis/reload/delivery retry and exact notification route passed")
