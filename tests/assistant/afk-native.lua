-- Integrated composition/policy/readiness lifetime test; shared actor is a
-- token-checking fixture. Real renderer/actor behavior is separately captured.
local now,next_id=0,0
local timers,events={},{}
_G.fact={ai_dg_reduced=false}
_G.text={}
_G.on=function(name,f)events[name]=f end
_G.every=function(ms,f)next_id=next_id+1;timers[next_id]={f=f,period=ms/1000,next=now+ms/1000};return next_id end
_G.cancel=function(id)timers[id]=nil end
local original_clock=os.clock
os.clock=function()return now end
local function encode(value)
    if type(value)~='table' then return tostring(value) end
    local keys={};for key in pairs(value) do keys[#keys+1]=key end
    table.sort(keys,function(a,b)return tostring(a)<tostring(b) end)
    local rows={};for _,key in ipairs(keys) do rows[#rows+1]=tostring(key)..'='..encode(value[key]) end
    return '{'..table.concat(rows,',')..'}'
end
_G.json={encode=encode,decode=function(value)return value end}
_G.require=function(name)return assert(loadfile('shell/'..name..'.luau'))() end
local function advance(seconds)
    local until_time=now+seconds
    while now<until_time do
        now=math.min(until_time,now+.016)
        local due={};for id,t in pairs(timers) do if t.next<=now then due[#due+1]=id end end
        for _,id in ipairs(due) do
            local t=timers[id]
            if t then t.next=now+t.period;t.f() end
        end
    end
end
local serial,current=0,nil
local acquired,played,registered,controls={},{},{},{}
local shared={}
shared.acquire=function(ready,stopped,options)
    serial=serial+1;current={token=serial,ready=ready,stopped=stopped,options=options}
    acquired[#acquired+1]=current;return serial
end
shared.register_skin=function(path,rows,token)
    assert(current and token==current.token)
    registered[#registered+1]={path=path,rows=rows,token=token};return true
end
shared.register_closeup=function(path,token,grid)
    assert(current and token==current.token)
    registered[#registered+1]={path=path,token=token,grid=grid};return true
end
shared.play=function(token,sample)
    if not current or current.token~=token then return false end
    if not current.until_time then current.until_time=now+.2 end
    played[#played+1]=sample
    return true,now<current.until_time and 'preparing' or 'ready'
end
shared.release=function(token)
    if not current or token~=current.token then return false end
    local old=current;current=nil;old.stopped(token,'released');return true
end
local afk=require('assistant/afk-native')(shared,{art='/fixture/art',control=function(action,done)
    controls[#controls+1]=action;if done then done(true) end
end})
local function config(scene,quiet)
    return {['assistant.afk_enabled']={value=true},['assistant.afk_seed']={value='17'},
        ['assistant.afk_probability_percent']={value=100},['assistant.afk_scene']={value=scene},
        ['assistant.afk_quiet_rest']={value=quiet or false}}
end
local geometry={w=1920,h=1080,scale=1,output='left',output_id=1,fullscreen=false,unlocked=true,session_active=true}
local function idle()
    events.afk_source(77);events.afk_threshold(300);events.afk_safe(77);events.afk_idle(77)
end
afk.configure(config('wing_grooming'));idle()
local pending=acquired[#acquired]
assert(#registered==0 and current)
events.afk_resume(77)
pending.ready(pending.token,geometry) -- An old geometry callback cannot bind art.
assert(#registered==0 and not current)
idle();current.ready(current.token,geometry)
assert(#registered==1 and registered[1].rows==44)
advance(.1)
for _,sample in ipairs(played) do assert(sample.opacity==0) end
advance(.3)
assert(fact.ai_afk_active and #played>3)
local warm=#played
advance(2.4) -- crosses the first beat boundary with the binding warm
for i=warm+1,#played do assert(played[i].opacity>0) end
local count=#controls
afk.configure(config('tail_chase'))
assert(#controls==count) -- A policy-only edit does not restart idle/lock ownership.
events.afk_resume(77);idle();current.ready(current.token,geometry);advance(.3)
fact.ai_speaking=true;advance(.6)
assert(not current and not fact.ai_afk_active)
fact.ai_speaking=false
events.afk_resume(77);idle();current.ready(current.token,geometry)
events.afk_unsafe(77)
assert(not current and not fact.ai_afk_active)
events.afk_resume(77)
afk.configure(config('screen_window'));idle()
current.ready(current.token,geometry)
assert(registered[#registered].path=='/fixture/art/afk-closeup-motion.webp' and registered[#registered].grid==4)
events.afk_resume(77)
afk.configure(config('glass_inspection'));idle();current.ready(current.token,geometry)
assert(text.ai_afk_body_src=='/fixture/art/afk-closeup-motion.webp' and fact.ai_afk_body_open)
afk.configure({['assistant.afk_enabled']={value=false}})
assert(not current and controls[#controls]=='stop')
assert(text.ai_afk_body_src=='' and not fact.ai_afk_body_open)
events.afk_source(99);events.afk_safe(99);events.afk_idle(99)
assert(not current)
fact.ai_dg_reduced=true
afk.configure(config('quiet_rest',true));events.afk_source(77);events.afk_resume(77);idle();assert(current.options.quiet)
current.ready(current.token,geometry)
advance(.5)
local last=played[#played]
assert(last.scale<=1 and last.airborne==0 and last.rotation==0 and last.bank==0)
afk.close();assert(not current and not fact.ai_afk_active)
-- Actual composition queue serializes systemd control calls. Old stop commands
-- cannot race a newly enabled source and terminate its helper afterward.
local requests={}
_G.run=function(program,args,done)
    assert(program=='tohru_afk_idle.py')
    requests[#requests+1]={args=args,done=done}
end
fact.ai_dg_reduced=false
local queued=require('assistant/afk-native')(shared,{art='/fixture/art'})
queued.configure(config('wing_grooming'))
assert(#requests==1 and requests[1].args[2]=='start')
local changed=config('wing_grooming');changed['assistant.afk_idle_seconds']={value=420}
queued.configure(changed)
assert(#requests==1)
requests[1].done('',0)
assert(#requests==2 and requests[2].args[2]=='stop')
requests[2].done('',0)
assert(#requests==3 and requests[3].args[2]=='start' and requests[3].args[4]=='420')
queued.configure({['assistant.afk_enabled']={value=false}})
requests[3].done('',0)
assert(#requests==4 and requests[4].args[2]=='stop')
requests[4].done('',0)
queued.close()
os.clock=original_clock
print('AFK native composition: source/lease/registration fencing, cold readiness, voice/lock/disable and quiet lease passed')
