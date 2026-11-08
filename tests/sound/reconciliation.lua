-- Execute the production controller with a native model/fact frontier fixture.
-- Model writes update values/counts, but never synthesize renderer/Lua events.
math.clamp=math.clamp or function(value,low,high)return math.max(low,math.min(high,value))end
fact={['audio.volume']=-1,['audio.muted']=false,['audio.input']=-1,['audio.input_muted']=false,
      ['sound_outputs.count']=0,['sound_inputs.count']=0,['sound_apps.count']=0,sound_tab=0,sound_open=false}
text,handlers,calls,timers={},{},{},{}
local rows,capacities={},{sound_outputs=32,sound_inputs=32,sound_apps=64}
model=setmetatable({}, {__index=function(_,key)return rows[key]end,
    __newindex=function(_,name,value)
        local count=math.min(#value,capacities[name]);rows[name]={};fact[name..'.count']=count
        for i=1,count do rows[name][i]=value[i];for key,item in pairs(value[i]) do
            local path=name..'.'..(i-1)..'.'..key
            if type(item)=='string' then text[path]=item else fact[path]=item end
        end end
    end})
on=function(name,callback)handlers[name]=callback end
local watched,watch_count,published,observed= nil,0,0,0
sys={watch=function(name,callback)assert(name=='audio');watched=callback;watch_count=watch_count+1;return true end,
     call=function(...)calls[#calls+1]=table.pack(...)end}
json={encode=function(value)status=value;published=published+1;return 'fixture status'end}
after=function(ms,callback)assert(ms==50);timers[1]=callback;return 1 end
cancel=function(id)timers[id]=nil end
local make=assert(loadfile('shell/sound/controller.luau'))()
make(function()observed=observed+1 end)
assert(watch_count==1 and watched)
local function app(id,name,volume)return {id=id,name=name,title=name..' stream',volume=volume,muted=false,playing=true}end
local value={volume=.4,muted=false,input=.3,input_muted=false,
    outputs={{id=0,name='Output A',default=true},{id=1,name='Output B',default=false}},
    inputs={{id=0,name='Input A',default=true},{id=1,name='Input B',default=false}},
    apps={app(1,'Alpha',.1),app(2,'Beta',.2),app(3,'Gamma',.3),app(4,'Delta',.4),app(5,'Epsilon',.5),app(6,'Zeta',.6)}}
local function report()
    local before=published;watched(value)
    assert(published==before+1,'whole native report must publish once')
end
report();assert(observed==1 and status.input_muted==false and status.outputs[1].id==0)
handlers.sound_tab(2);handlers.sound_row_move(1)
assert(fact.sound_row_focus==1 and fact.sound_row_id==2)
-- Same count, selected stream now on another page. Preserve identity and move outline.
local selected=value.apps[2]
value.apps={value.apps[1],value.apps[3],value.apps[4],value.apps[5],value.apps[6],selected}
selected.name,selected.title,selected.volume,selected.muted,selected.playing='Renamed beta','Changed title',.7,true,false
report()
assert(fact.sound_row_id==2 and fact.sound_row_focus==5 and fact.sound_list_page==1)
assert(status.apps[6].name=='Renamed beta' and status.apps[6].title=='Changed title')
assert(status.apps[6].volume==.7 and status.apps[6].muted==true and status.apps[6].playing==false)
handlers.sound_details_step(1)
assert(calls[#calls][1]=='audio.app_volume' and calls[#calls][2]==2 and calls[#calls][3]==.75)
-- Same-count replacement removes selected identity: reselect the outlined, clamped row.
value.apps[6]=app(9,'Replacement',.4);report()
assert(fact.sound_row_id==9 and fact.sound_row_focus==5 and fact.sound_list_page==1)
handlers.sound_details_activate();assert(calls[#calls][1]=='audio.app_mute' and calls[#calls][2]==9)
local issued=#calls;handlers.audio_app_mute(2);assert(#calls==issued,'explicit stale IDs remain rejected')
handlers.sound_tab(0);handlers.sound_row_move(1)
assert(fact.sound_row_id==1 and fact.sound_row_focus==1)
value.outputs={value.outputs[2],value.outputs[1]}
value.outputs[1].name,value.outputs[1].default,value.outputs[2].default='Renamed output',true,false
report()
assert(fact.sound_row_id==1 and fact.sound_row_focus==0)
assert(status.outputs[1].name=='Renamed output' and status.outputs[1].default==true)
handlers.sound_details_activate();assert(calls[#calls][1]=='audio.default' and calls[#calls][2]==1)
handlers.sound_tab(1)
value.inputs={value.inputs[2],value.inputs[1]};value.inputs[1].name='Renamed input'
report();assert(fact.sound_row_id==0 and fact.sound_row_focus==1 and status.inputs[1].name=='Renamed input')
local defaults=0;for _,call in ipairs(calls)do if call[1]=='audio.default'then defaults=defaults+1 end end
assert(defaults==1 and handlers.audio_input_select==nil,'input IDs never select an output')
handlers.sound_tab(2);value.apps={};report()
assert(fact.sound_row_id==-1 and fact.sound_row_focus==-1 and fact.sound_list_page==0 and #status.apps==0)
assert(next(timers)==nil,'report refresh never polls or schedules recurring work')
assert(watch_count==1,'one watch per VM, no declarative/raw duplicate')
-- VM replacement registers the same native watch shape; backend reuse/replay is
-- native HUBS behavior, not a thread simulated by this fixture.
local saved=watched;make(function()observed=observed+1 end)
assert(watch_count==2 and watched~=saved)
report();assert(next(timers)==nil)
print('sound same-count app/device reconciliation, status metadata and one-report watch: PASS')
