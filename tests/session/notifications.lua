-- Runs actual native-notification entry logic with controlled snapshot/timer/audio boundaries.
-- JSON codec is the controlled native boundary; this tests handler/schema,
-- not Pleamar's unchanged native JSON parser.
local decoded, next_json, decode_calls = {}, 0, 0
local function packet(value)
    next_json = next_json + 1
    local encoded = "json:" .. tostring(next_json)
    decoded[encoded] = value
    return encoded
end
local function boot(facts, texts, start)
    texts = texts or {capture_result="",capture_notice=""}
    local callback, timers, tones, handlers = nil, {}, {}, {}
    local clock=start or 0
    local env=setmetatable({
        fact=facts,text=texts,model={},
        os={clock=function() return 1000+clock/1000 end},
        json={encode=packet,decode=function(payload)
            decode_calls=decode_calls+1
            if decoded[payload]==nil then error("malformed JSON") end
            return decoded[payload]
        end},
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

local capture_facts={last_id=0,power_level=0}
local capture_text={capture_result="",capture_notice=""}
local capture=boot(capture_facts,capture_text)
local function result(status,message,extra)
    local value={status=status,message=message}
    if extra then for key,item in pairs(extra)do value[key]=item end end
    return packet(value)
end
capture.report({notice(1,'Native application')})
capture.event('submit:capture_result',result('saved','/tmp/capture.png'))
assert(capture.env.model.banners[1].title=='Screenshot saved')
assert(capture.env.model.banners[1].body=='/tmp/capture.png' and #capture.tones==2)
capture.report({notice(1,'Native application')})
assert(#capture.env.model.banners==2 and capture.env.model.banners[1].title=='Screenshot saved')
capture.report({});assert(#capture.env.model.banners==1)
capture.report({notice(2,'Another native application')})
for _,status in ipairs({'copied','saved_copied','partial','error'})do
    capture.event('submit:capture_result',result(status,'Fixed result detail'))
    assert(#capture.env.model.banners==2)
end
assert(capture.env.model.banners[1].title=='Screenshot failed')
local tone_count=#capture.tones
local retained=capture_text.capture_notice
for _,payload in ipairs({
    '', 'malformed JSON', packet(false), packet({}),
    result('unknown','No title injection'),result('saved',false),
    result('saved',string.rep('a',513)),result('saved',string.rep('🌊',513)),
    result('saved','bad'..string.char(255)),result('saved','bad'..string.char(0)),
    result('saved','bad'..string.char(127)),result('saved','body',{expires=100000}),
    result('saved','body',{title='Arbitrary title'}),
})do capture.event('submit:capture_result',payload)end
capture.event('submit:capture_result',false);capture.event('submit:capture_result',42)
local parsed=decode_calls
capture.event('submit:capture_result',string.rep('x',3073))
assert(decode_calls==parsed) -- Reject oversized wire payload before JSON decode.
assert(#capture.tones==tone_count and capture_text.capture_notice==retained)
capture.event('submit:capture_result',result('partial',string.rep('🌊',512)))
assert(#capture.env.model.banners[1].body==2048)
capture.advance(2000)
local reload=boot(capture_facts,capture_text,2000)
assert(#reload.tones==0 and reload.env.model.banners[1].title=='Screenshot partly completed')
reload.report({notice(1,'Native application')})
assert(#reload.env.model.banners==1) -- Restored result survives native replay/pruning.
reload.advance(3999);assert(#reload.env.model.banners==1)
reload.advance(1);assert(#reload.env.model.banners==0 and capture_text.capture_notice=='')
local expired=boot(capture_facts,capture_text,6000)
assert(#expired.env.model.banners==0 and #expired.tones==0)
-- New capture replaces old timer; old deadline cannot erase the replacement.
local timers=boot({last_id=0,power_level=0})
timers.event('submit:capture_result',result('saved','first'))
timers.advance(3000);timers.event('submit:capture_result',result('copied','second'))
timers.advance(3000);assert(timers.env.model.banners[1].body=='second')
timers.advance(3000);assert(#timers.env.model.banners==0)
-- Native app burst eviction clears capture persistence, preventing reload resurrection.
local evicted_text={capture_notice=''}
local evicted=boot({last_id=0,power_level=0},evicted_text)
evicted.event('submit:capture_result',result('saved','will be evicted'))
evicted.report({notice(4),notice(3),notice(2),notice(1)})
assert(#evicted.env.model.banners==4 and evicted_text.capture_notice=='')
-- Retained metadata cannot create arbitrary or extended notifications.
for _,expires in ipairs({999,1007,math.huge,0/0})do
    local bad_text={capture_notice=packet({status='saved',message='stale',expires=expires})}
    local bad=boot({last_id=0,power_level=0},bad_text)
    assert(#bad.env.model.banners==0 and #bad.tones==0 and bad_text.capture_notice=='')
end
local mixed=boot({last_id=0,power_level=0})
mixed.report({notice(1)})
mixed.event('power_warning_low',0.2)
mixed.event('submit:capture_result',result('error','Native export failed'))
mixed.report({notice(1)})
assert(#mixed.env.model.banners==3)
mixed.event('power_warning_clear',1)
assert(#mixed.env.model.banners==2 and mixed.env.model.banners[1].title=='Screenshot failed')
print('capture result native-notification hook: passed')
