-- Runs actual native-notification entry logic with controlled snapshot/timer/audio boundaries.
local function boot(facts, texts, start, files)
    texts = texts or {}
    files = files or {}
    local callback, timers, tones, handlers, said = nil, {}, {}, {}, {}
    local clock=start or 0
    local env=setmetatable({
        fact=facts,text=texts,model={},
        sound={tone=function(tone) tones[#tones+1]=tone end},
    }, {__index=_G})
    env.on=function(name,fn)handlers[name]=fn end
    env.sys={
        watch=function(name,fn) assert(name=='notifications');callback=fn end,
        ask=function(name,file,kind) assert(name=='files.read' and kind=='json');return files[file] end,
        call=function(name,a,b)
            if name=='files.write' then files[a]=b
            else assert(name=='say' and a=='tohru');said[#said+1]=b end
        end,
    }
    env.after=function(ms,fn) local timer={due=clock+ms,fn=fn};timers[#timers+1]=timer;return timer end
    env.cancel=function(timer) timer.cancelled=true end
    assert(loadfile('shell/notifications/notifications.luau','t',env))()
    return {
        event=function(name,...)handlers[name](...)end,
        env=env,tones=tones,said=said,files=files,report=function(value)callback(value)end,
        advance=function(ms)
            clock=clock+ms
            for _,timer in ipairs(timers) do if not timer.cancelled and timer.due<=clock then timer.cancelled=true;timer.fn() end end
        end,
    }
end
local function notice(id,title,body,time,urgency)
    return {id=id,title=title or 'Title',body=body or 'Body',time=time or 1,urgency=urgency}
end
local function fresh() return {last_id=0,power_level=0,unread=false,dnd=false} end
local facts=fresh();local n=boot(facts)
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
local bounded_role=boot(fresh());local active={}
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

local power=boot(fresh())
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
-- Unread: set by a new ID that is not low urgency, kept across restarts, cleared only when seen.
local bell=boot(fresh())
assert(bell.said[#bell.said]=='emit notices_state 0' and not bell.env.fact.unread)
bell.report({notice(1,'Quiet',nil,1,0)});assert(not bell.env.fact.unread and #bell.tones==1)
bell.report({notice(2),notice(1,'Quiet',nil,1,0)})
assert(bell.env.fact.unread and bell.files['notices.json'].unread==true and bell.said[#bell.said]=='emit notices_state 1')
bell.event('notices_seen',0);assert(not bell.env.fact.unread and bell.files['notices.json'].unread==false)
bell.report({notice(2,'Replaced'),notice(1,'Quiet',nil,1,0)})
assert(not bell.env.fact.unread and #bell.tones==3)
bell.report({});assert(not bell.env.fact.unread)
bell.report({notice(3)});assert(bell.env.fact.unread)
local restarted=boot(fresh(),nil,nil,bell.files)
assert(restarted.env.fact.unread and restarted.said[1]=='emit notices_state 1')
restarted.event('notices_report',0);assert(restarted.said[#restarted.said]=='emit notices_state 1')

-- Do Not Disturb: no banner or tone, still unread; on hides what shows; power warnings bypass it.
local quiet=boot(fresh())
quiet.report({notice(1)});assert(#quiet.env.model.banners==1)
quiet.event('notices_dnd',2);assert(quiet.env.fact.dnd and #quiet.env.model.banners==0)
assert(quiet.said[#quiet.said]=='emit notices_state 3')
quiet.event('notices_seen',0);assert(quiet.said[#quiet.said]=='emit notices_state 2')
quiet.report({notice(2),notice(1)})
assert(#quiet.env.model.banners==0 and #quiet.tones==1 and quiet.env.fact.unread)
quiet.event('power_warning_low',0.2);assert(#quiet.env.model.banners==1 and #quiet.tones==2)
quiet.event('notices_dnd',1);quiet.event('notices_dnd',7);assert(quiet.env.fact.dnd)
quiet.event('notices_dnd',2);assert(not quiet.env.fact.dnd and #quiet.env.model.banners==1)
quiet.report({notice(3),notice(2),notice(1)});assert(#quiet.env.model.banners==2 and #quiet.tones==3)
quiet.event('notices_dnd',0);assert(not quiet.env.fact.dnd)
print('native notification entry logic: passed')

