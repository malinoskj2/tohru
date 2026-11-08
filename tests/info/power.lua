local events,watch,calls={},{},{}
_G.fact={power_warning_level=0};_G.text={}
_G.on=function(name,fn)events[name]=fn end
_G.sys={watch=function(name,fn)assert(name=="battery");watch=fn end}
local fail=false
local function notify(level,charge) if fail then error("Owner not ready") end;calls[#calls+1]={level,charge} end
local function battery(percent,charging,present)watch({percent=percent,charging=charging or false,present=present~=false})end
local power=assert(loadfile("shell/info/power.luau"))()(notify)
battery(80);assert(#calls==0)
battery(20);assert(calls[#calls][1]==1 and fact.power_warning_level==1)
battery(21);assert(#calls==1);battery(23);assert(calls[#calls][1]==0)
battery(5);assert(calls[#calls][1]==2);battery(2);assert(calls[#calls][1]==3)
battery(4);assert(fact.power_warning_level==3);battery(4.1);assert(fact.power_warning_level==2)
battery(7.1);assert(fact.power_warning_level==1);battery(7.1,true);assert(fact.power_warning_level==0)
fail=true;battery(1);assert(fact.power_warning_level==0 and text.power_error:find("not ready",1,true))
fail=false;power.retry();assert(fact.power_warning_level==3)
local count=#calls
-- Recreate actual production controller with retained native facts, as Shared.fresh does.
events={};power=assert(loadfile("shell/info/power.luau"))()(notify)
battery(1);assert(#calls==count)
battery(nil);assert(not fact.power_known and fact.power_warning_level==0)
battery(0/0);assert(not fact.power_known);battery(math.huge);assert(not fact.power_known)
battery(80,false,false);assert(not fact.power_present)
local target,command
_G.sys.call=function(name,role,line)assert(name=="say");target,command=role,line end
assert(loadfile("shell/info/power.luau"))()()
battery(18);assert(target=="tohru-notifications" and command=="emit power_warning_low 0.18")
print("Production native battery thresholds/hysteresis/reload/delivery retry and exact notification route passed")

-- Native0..100 is normalized once; presentation/notification receive0..1.
for _,native in ipairs({0,2,5,20,100}) do battery(native);assert(fact.power_known and fact.power_percent==native/100) end
battery(-1);assert(not fact.power_known);battery(101);assert(not fact.power_known)
-- Failed transition which is no longer needed must not leave stale error.
assert(loadfile("shell/info/power.luau"))()(notify)
fact.power_warning_level=0;fail=true;battery(20);assert(text.power_error~="")
fail=false;battery(80);assert(fact.power_warning_level==0 and text.power_error=="")
print("Actual native0/2/5/20/100 battery values normalize once; obsolete failure clears on recovery")
