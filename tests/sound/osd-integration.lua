-- Actual sound + OSD controllers. Native model assignment does not loop back
-- renderer events; sys.watch/timers/commands are explicit boundary fixtures.
math.clamp=math.clamp or function(v,a,b)return math.max(a,math.min(b,v))end
fact={['audio.volume']=-1,['audio.input']=-1,['audio.muted']=false,['audio.input_muted']=false,
      ['sound_outputs.count']=0,['sound_inputs.count']=0,['sound_apps.count']=0,sound_tab=0}
text,handlers,calls,timers={},{},{},{}
local capacities={sound_outputs=32,sound_inputs=32,sound_apps=64}
local values={}
model=setmetatable({}, {__newindex=function(_,name,entries)
    for i,row in ipairs(values[name] or {})do for key,value in pairs(row)do
        local target=type(value)=='string' and text or fact
        target[name..'.'..(i-1)..'.'..key]=nil
    end end
    values[name]={}
    local count=math.min(capacities[name],#entries)
    fact[name..'.count']=count
    for i=1,count do
        values[name][i]=entries[i]
        for key,value in pairs(entries[i])do
            local target=type(value)=='string' and text or fact
            target[name..'.'..(i-1)..'.'..key]=value
        end
    end
end})
function on(name,callback)
    handlers[name]=handlers[name] or {};table.insert(handlers[name],callback)
end
local function fire(name,...)
    for _,callback in ipairs(handlers[name] or {})do callback(...)end
end
local next_timer=0
function after(ms,callback)
    assert(ms==50 or ms==1600);next_timer=next_timer+1
    timers[next_timer]={ms=ms,callback=callback};return next_timer
end
function cancel(id)assert(timers[id]);timers[id]=nil end
function every()error('No recurring publication or audio polling')end
local function count()
    local n=0;for _ in pairs(timers)do n=n+1 end;return n
end
local function expire()
    local prior=timers;timers={}
    for _,timer in pairs(prior)do assert(timer.ms==1600);timer.callback()end
end
local watches,observations,status_count=0,0,0
local watched,last_status
json={encode=function(value)last_status=value;status_count=status_count+1;return 'fixture'end}
sys={watch=function(name,callback)
    assert(name=='audio');watches=watches+1;watched=callback;return true
end,call=function(...)
    calls[#calls+1]=table.pack(...)
    if refuse then error('native request refused')end
end}
local make_osd=assert(loadfile(arg[1] or 'shell/osd/controller.luau'))()
local osd=make_osd()
local sound=assert(loadfile('shell/sound/controller.luau'))()(function()
    observations=observations+1;osd.observe()
end)
assert(watches==1 and watched and observations==0 and count()==0)
local report={volume=.2,muted=false,input=.3,input_muted=false,
    outputs={{id=0,name='Speakers',default=true}},inputs={{id=0,name='Microphone',default=true}},apps={}}
local function send()
    local prior=status_count;local seen=observations
    watched(report)
    assert(status_count==prior+1 and observations==seen+1 and watches==1)
    assert(last_status.outputs[1]==nil or last_status.outputs[1].name==report.outputs[1].name)
end
send();assert(not fact.osd_open and fact.osd_known and fact.osd_input_known)
report.volume=.4;send()
assert(fact.osd_open and not fact.osd_input and text.osd_device=='Speakers' and text.osd_label=='40' and count()==1)
local held=next_timer
report.outputs[1].name='Renamed speakers';send()
assert(text.osd_device=='Renamed speakers' and next_timer==held and count()==1, 'visible same-ID output rename must refresh without restarting hold')
assert(last_status.outputs[1].name==text.osd_device)
expire();assert(not fact.osd_open and count()==0)
report.outputs[1].name='Hidden rename';send()
assert(not fact.osd_open and count()==0)
-- Native request acceptance cannot optimistically change observed presentation.
fire('audio_step',1)
assert(calls[#calls][1]=='audio.step' and fact['audio.volume']==.4 and not fact.osd_open)
report.volume=.45;send();assert(fact.osd_open and text.osd_label=='45' and count()==1)
fire('osd_enter');assert(fact.osd_hover and count()==0)
osd.hide();assert(not fact.osd_open and not fact.osd_hover, 'hide must clear hover before next presentation')
fire('osd_enter');assert(not fact.osd_hover and count()==0,'queued hidden enter must not retain hover')
report.muted=true;send()
assert(fact.osd_open and fact.osd_muted and text.osd_label=='Mute' and count()==1)
-- Opening the actual sound controller suppresses the vessel; a later change
-- after close gets one finite hold even if it was hovered before suppression.
fire('osd_enter');fire('sound_open_request');fire('fact:sound_open')
assert(fact.sound_open and not fact.osd_open and not fact.osd_hover and count()==0)
report.muted=false;send();assert(not fact.osd_open)
sound.close();send();assert(not fact.osd_open)
report.input=.35;send()
assert(fact.osd_input and fact.osd_open and text.osd_device=='Microphone' and count()==1)
held=next_timer;report.inputs[1].name='Renamed microphone';send()
assert(text.osd_device=='Renamed microphone' and next_timer==held)
fire('osd_enter');report.inputs={};send()
assert(not fact.osd_open and not fact.osd_hover and count()==0)
report.inputs={{id=9,name='New microphone',default=true}};send()
assert(not fact.osd_open,'reappearing source establishes baseline')
report.input_muted=true;send()
assert(fact.osd_input and fact.osd_muted and text.osd_device=='New microphone' and count()==1)
-- Output wins a combined real change, and device identity changes are observed.
report.volume=.5;report.input=.4;send();assert(not fact.osd_input)
report.outputs[1].id=7;send();assert(text.osd_device=='Hidden rename' and not fact.osd_input)
osd.hide();refuse=true;fire('audio_mute')
assert(fact.audio_failed and text.audio_error:find('native request refused') and not fact.osd_open)
refuse=false;fire('audio_mute');assert(not fact.audio_failed and not fact.osd_open)
-- VM replacement cancels owned callbacks natively, then retains scene facts.
report.volume=.6;send();assert(fact.osd_open and count()==1)
timers={};handlers={};fact.osd_hover=true
osd=make_osd();assert(not fact.osd_hover and count()==1)
held=next_timer;osd.observe();assert(count()==1 and next_timer==held)
expire();assert(not fact.osd_open and count()==0)
assert(watches==1,'OSD reinstall must never subscribe to audio')
print('PASS actual whole-report sound/OSD ordering, names, hover/hide, commands and finite reload hold')
