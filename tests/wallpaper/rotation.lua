-- Production scheduler with native timer/settings/action boundaries as fixtures.
local timers, history, serial = {}, {}, 0
function after(ms, callback)
    serial = serial + 1
    timers[serial] = {ms=ms, callback=callback}
    history[serial] = timers[serial]
    local count=0;for _ in pairs(timers) do count=count+1 end
    assert(count<=1,"more than one wallpaper deadline")
    return serial
end
function cancel(id) timers[id]=nil end
local function fire(id)
    local timer=assert(timers[id]);timers[id]=nil;timer.callback()
end
local function current(ms)
    local id,timer=next(timers);assert(id and timer.ms==ms);return id
end
local make=assert(loadfile("shell/wallpaper/rotation.luau"))()
local calls, messages=0,{}
local owner=make(function() calls=calls+1 end,function(err) messages[#messages+1]=err or "ok" end,"login")
local first=current(1000);fire(first)
assert(calls==1 and not next(timers) and messages[1]=="ok")
owner.configure("login");assert(not next(timers))
owner.configure("hourly");local hour=current(3600000);fire(hour)
assert(calls==2);current(3600000)
owner.configure("daily");local day=current(86400000)
assert(not pcall(owner.configure,"sometimes"));assert(current(86400000)==day)
owner.configure("never");assert(not next(timers))
history[day].callback();assert(calls==2 and not next(timers))
owner.stop();assert(not pcall(owner.configure,"hourly"))
-- Initial login cancellation by unrelated accepted settings also matches source.
owner=make(function() calls=calls+1 end,nil,"login")
first=current(1000);owner.configure("login");history[first].callback()
assert(calls==2 and not next(timers));owner.stop()
-- Failure does not spin; it retries only at the next retained interval.
owner=make(function() error("decode failed") end,function(err) assert(err:find("decode failed")) end,"hourly")
fire(current(3600000));current(3600000);owner.stop()
-- A successful rotate may synchronously commit settings/reconfigure this owner.
owner=make(function() owner.configure("never") end,nil,"daily")
fire(current(86400000));assert(not next(timers));owner.stop()
-- A notification callback may stop the owner; an already-selected callback is stale.
owner=make(function() calls=calls+1 end,function() owner.stop() end,"hourly")
first=current(3600000);fire(first);assert(not next(timers))
history[first].callback();assert(calls==3 and not next(timers))
-- Constructor requires a real root action and never invents an implicit backend.
assert(not pcall(make,nil,nil,"hourly"))
assert(not pcall(make,function() end,nil,"invalid"))
print("Wallpaper rotation: login/interval/accepted-config reset/stale/reentry/stop/failure fixtures passed")
