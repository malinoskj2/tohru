-- Runs actual native-notification entry logic with controlled snapshot/timer/audio boundaries.
local function boot(facts, texts, start)
    texts = texts or {}
    local callback, timers, tones, handlers = nil, {}, {}, {}
    local clock=start or 0
    local env=setmetatable({
        fact=facts,text=texts,model={},
        sound={tone=function(tone) tones[#tones+1]=tone end},
    }, {__index=_G})
    env.on=function(name,fn)handlers[name]=fn end
    env.sys={watch=function(name,fn) assert(name=='notifications');callback=fn end}
    env.after=function(ms,fn) local timer={due=clock+ms,fn=fn};timers[#timers+1]=timer;return timer end
    env.cancel=function(timer) timer.cancelled=true end
    assert(loadfile('shell/notifications/notifications.luau','t',env))()
    return {
        event=function(name,...)handlers[name](...)end,
        env=env,tones=tones,report=function(value)callback(value)end,
        advance=function(ms)
            clock=clock+ms
            for _,timer in ipairs(timers) do if not timer.cancelled and timer.due<=clock then timer.cancelled=true;timer.fn() end end
        end,
    }
end
local function notice(id,title,body,time)
    return {id=id,title=title or 'Title',body=body or 'Body',time=time or 1}
end
local facts={last_id=0,power_level=0};local n=boot(facts)
n.report({notice(1)});assert(#n.env.model.banners==1 and #n.tones==1)
n.report({notice(1)});assert(#n.tones==1)
n.report({notice(1,'Changed')});assert(#n.env.model.banners==1 and #n.tones==2)
n.advance(5999);assert(#n.env.model.banners==1)
n.advance(1);assert(#n.env.model.banners==0)
for id=2,10 do n.report({notice(id)}) end
assert(#n.env.model.banners==1 and #n.tones==11)
local burst={};for id=11,20 do table.insert(burst,1,notice(id)) end
n.report(burst);assert(#n.env.model.banners==4 and #n.tones==21)
for _,tone in ipairs(n.tones) do assert(tone==n.tones[1] and tone.freq==880 and tone.ms==80) end
n.report({});assert(#n.env.model.banners==0)
-- Reexecuting a VM retains only native scene protocol watermark, never replays old audio/history.
local reloaded=boot(facts);reloaded.report(burst)
assert(#reloaded.tones==0 and #reloaded.env.model.banners==0)
reloaded.report({notice(21,'ééé','🌊')});assert(#reloaded.tones==1)
local long=string.rep('🌊',1000);reloaded.report({notice(22,long,long)})
assert(utf8.len(reloaded.env.model.banners[1].title)==256)
assert(utf8.len(reloaded.env.model.banners[1].body)==768)

-- A valid old active replacement can move from outside the bounded cache to front.
local bounded_role=boot({last_id=0,power_level=0});local active={}
for id=1,66 do table.insert(active,1,notice(id)) end
bounded_role.report(active);local before=#bounded_role.tones
local replaced={notice(1,'Replacement outside cache')}
for id=66,2,-1 do replaced[#replaced+1]=notice(id) end
bounded_role.report(replaced)
assert(#bounded_role.tones==before+1)
assert(bounded_role.env.model.banners[1].title=='Replacement outside cache')
-- Removing the top does not replay unchanged old entries entering at the tail.
local after=#bounded_role.tones;table.remove(replaced,1);bounded_role.report(replaced)
assert(#bounded_role.tones==after)

local power=boot({last_id=0,power_level=0})
power.report({notice(1,'Application')})
power.event('power_warning_low',0.2)
assert(power.env.model.banners[1].title=='Battery low' and #power.tones==2)
power.event('power_warning_low',0.19);assert(#power.tones==2)
power.report({notice(1,'Application')});assert(#power.env.model.banners==2)
power.event('power_warning_critical',0.05);assert(#power.tones==3)
power.event('power_warning_empty',0.02);assert(#power.tones==4)
power.event('power_warning_clear',0.8)
assert(#power.env.model.banners==1 and power.env.model.banners[1].title=='Application')
power.event('power_warning_low',0/0);power.event('power_warning_low',math.huge)
power.event('power_warning_low',-1);power.event('power_warning_low',2)
assert(#power.tones==4)
print('native notification entry logic: passed')

