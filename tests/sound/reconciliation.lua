-- Execute the production controller with a native model/fact frontier fixture.
-- Model writes update values/counts, but never synthesize renderer/Lua events.
math.clamp=math.clamp or function(value,low,high)return math.max(low,math.min(high,value))end
fact={['audio.volume']=-1,['audio.muted']=false,['audio.input']=-1,['audio.input_muted']=false,
      ['sound_outputs.count']=0,['sound_inputs.count']=0,['sound_apps.count']=0,sound_open=false,sound_unfolded=false}
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
function require(name)assert(name=='bar/rail');return assert(loadfile('shell/bar/rail.luau'))()end
local watched,watch_count,published,observed= nil,0,0,0
sys={watch=function(name,callback)assert(name=='audio');watched=callback;watch_count=watch_count+1;return true end,
     call=function(...)calls[#calls+1]=table.pack(...)end}
json={encode=function(value)status=value;published=published+1;return 'fixture status'end}
local next_timer=0
after=function(ms,callback)next_timer=next_timer+1;timers[next_timer]={ms=ms,callback=callback};return next_timer end
cancel=function(id)timers[id]=nil end
local function run(ms)
    for id,timer in pairs(timers)do if timer.ms==ms then timers[id]=nil;timer.callback()end end
end
local make=assert(loadfile('shell/sound/controller.luau'))()
make(function()observed=observed+1 end)
assert(watch_count==1 and watched)
local function app(id,name,volume)return {id=id,name=name,title=name..' stream',volume=volume,muted=false,playing=true}end
local value={volume=.4,muted=false,input=.3,input_muted=false,
    outputs={{id=10,name='Output A',default=false},{id=11,name='Output B',default=true},{id=12,name='Output C',default=false},
             {id=13,name='Output D',default=false},{id=14,name='Output E',default=false}},
    inputs={{id=0,name='Input A',default=true},{id=1,name='Input B',default=false}},
    apps={app(1,'Alpha',.1),app(2,'Beta',.2),app(3,'Gamma',.3),app(4,'Delta',.4),app(5,'Epsilon',.5),app(6,'Zeta',.6)}}
local function report()
    local before=published;watched(value)
    assert(published==before+1,'whole native report must publish once')
end
report();assert(observed==1 and status.input_muted==false and status.outputs[1].id==10)
assert(fact.sound_default_row==1 and text.sound_device=='Output B','the default output heads the rows')

-- Opening starts folded with the master rail parked at its left end.
handlers.sound_open_request()
assert(fact.sound_open and not fact.sound_unfolded and fact['rail.goal.2']==8 and fact['rail.land.2']==1)
-- Unfolding parks the bead again and switches it to the reported volume after 180 ms.
handlers.sound_fold_toggle();assert(fact.sound_unfolded)
run(120);assert(fact['rail.land.2']==2)
run(180);assert(fact['rail.serial.2']==1 and math.abs(fact['rail.goal.2']-(8+.4*178))<1e-9 and fact['rail.kind.2']==2)
-- A report that moves the volume switches the bead; one that does not move it leaves it alone.
value.volume=.5;report();assert(fact['rail.serial.2']==2 and math.abs(fact['rail.goal.2']-97)<1e-9)
report();assert(fact['rail.serial.2']==2)

-- Rail press is a switch; dragging chases with a crest on every change of direction.
handlers.sound_rail_press(.9);assert(fact['rail.serial.2']==3 and fact.sound_dragging and fact.sound_requested==.9)
assert(fact.sound_big_held,'a level set by hand holds the tank number until the water arrives')
handlers.sound_rail_drag(.8);assert(fact['rail.crest.2']==1 and fact['rail.dir.2']==-1 and fact['rail.gain.2']==.95)
assert(fact.sound_requested==.8,'the label shows the requested level on the bead goal frame')
handlers.sound_rail_drag(.7);assert(fact['rail.crest.2']==1)
handlers.sound_rail_drag(.75);assert(fact['rail.crest.2']==2 and fact['rail.dir.2']==1)
assert(math.abs(fact['rail.target.2']-(8+.75*178))<1e-9)
local issued=#calls;run(50)
assert(#calls==issued+1 and calls[#calls][1]=='audio.volume' and calls[#calls][2]==.75,'drags write at most once per 50 ms')
-- Released, the requested level stays until a report agrees; a stale report does not undo it.
handlers.sound_rail_release();assert(fact.sound_dragging and fact.sound_requested==.75)
report();assert(fact.sound_dragging,'a report still at the old level keeps the request shown')
local serial_before=fact['rail.serial.2']
value.volume=.75;report();assert(not fact.sound_dragging and fact['rail.serial.2']==serial_before,'agreement releases without a glide')
handlers.sound_rail_press(.2);handlers.sound_rail_release();run(500);assert(not fact.sound_dragging,'a report that never agrees times out')
value.volume=.2;report()

-- Keyboard rows: master, outputs default-first, microphone, input devices (when > 1), apps.
handlers.sound_focus_move(1);assert(fact.sound_focus_kind==1 and fact.sound_focus_index==0)
handlers.sound_focus_activate();assert(calls[#calls][1]=='audio.default' and calls[#calls][2]==11)
handlers.sound_focus_move(1);handlers.sound_focus_activate();assert(calls[#calls][2]==10)
for _=1,3 do handlers.sound_focus_move(1)end
assert(fact.sound_focus_kind==1 and fact.sound_focus_index==4 and fact.sound_out_page==1,'three other outputs to a page')
handlers.sound_focus_move(1);assert(fact.sound_focus_kind==2)
handlers.sound_focus_step(1);assert(calls[#calls][1]=='audio.input' and math.abs(calls[#calls][2]-.35)<1e-9)
handlers.sound_focus_move(1);assert(fact.sound_focus_kind==4 and fact.sound_focus_index==0)
handlers.sound_focus_activate();assert(calls[#calls][1]=='audio.default_input' and calls[#calls][2]==0)
handlers.sound_focus_move(1);assert(fact.sound_focus_kind==4 and fact.sound_focus_index==1)
handlers.sound_focus_activate();assert(calls[#calls][1]=='audio.default_input' and calls[#calls][2]==1)
handlers.audio_input_select(99);assert(calls[#calls][2]==1,'stale input ID rejected')
handlers.sound_focus_move(1);assert(fact.sound_focus_kind==3 and fact.sound_focus_index==0)
handlers.sound_page(1);assert(fact.sound_focus_index==4 and fact.sound_app_page==1)
handlers.sound_focus_step(-1);assert(calls[#calls][1]=='audio.app_volume' and calls[#calls][2]==5 and math.abs(calls[#calls][3]-.45)<1e-9)

-- Same count, focused stream moved to another place: identity is kept.
local selected=value.apps[5]
value.apps={value.apps[1],selected,value.apps[2],value.apps[3],value.apps[4],value.apps[6]}
report();assert(fact.sound_focus_index==1 and fact.sound_app_page==0)
handlers.sound_focus_activate();assert(calls[#calls][1]=='audio.app_mute' and calls[#calls][2]==5)
-- Same-count replacement removes the identity: the clamped row is kept.
value.apps[2]=app(9,'Replacement',.4);report()
assert(fact.sound_focus_index==1);handlers.sound_focus_activate();assert(calls[#calls][2]==9)
issued=#calls;handlers.audio_app_mute(5);assert(#calls==issued,'explicit stale IDs remain rejected')

-- App sliders send row * 2 + level and are throttled per stream.
handlers.audio_app_set(2*2+.25);handlers.audio_app_set(2*2+.3)
assert(#calls==issued);run(50)
assert(calls[#calls][1]=='audio.app_volume' and calls[#calls][2]==value.apps[3].id and math.abs(calls[#calls][3]-.3)<1e-9)
handlers.audio_app_set(1*2+.5);handlers.audio_app_set(3*2+.6)
assert(calls[#calls][2]==9 and calls[#calls][3]==.5,'another stream flushes the pending write first')
handlers.audio_flush();assert(calls[#calls][2]==value.apps[4].id and math.abs(calls[#calls][3]-.6)<1e-9)

-- Outputs disappear: pages clamp, focus falls back, the heading says so.
value.outputs={};value.apps={};report()
assert(fact.sound_out_page==0 and fact.sound_app_page==0 and text.sound_device=='No output reported')
assert(fact.sound_focus_kind==2,'focus falls back to the microphone')

-- Folding resets pages and focus; closing folds.
handlers.sound_fold_toggle();assert(not fact.sound_unfolded and fact.sound_focus_kind==0)
local serial=fact['rail.serial.2'];handlers.sound_fold_toggle();handlers.panel_close()
assert(not fact.sound_open and not fact.sound_unfolded)
assert(not fact.sound_big_held,'closing drops the hold')
run(120);run(180);assert(fact['rail.serial.2']==serial,'a closed popup ignores its stale unfold timers')
assert(watch_count==1,'one watch per VM, no declarative/raw duplicate')
local saved=watched;make(function()observed=observed+1 end)
assert(watch_count==2 and watched~=saved)
print('sound default-first rows, focus identity, paging, rail switch/chase, throttled app writes and one-report watch: PASS')
