-- Runs actual native-notification entry logic with controlled snapshot/timer/audio boundaries.
local function boot(facts, texts, start, files)
    texts = texts or {}
    files = files or {}
    local callback, timers, tones, handlers, said, calls = nil, {}, {}, {}, {}, {}
    local clock=start or 0
    local env=setmetatable({
        fact=facts,text=texts,model={},
        sound={tone=function(tone) tones[#tones+1]=tone end},
        os={clock=function() return clock/1000 end,date=os.date},
    }, {__index=_G})
    env.on=function(name,fn)handlers[name]=fn end
    env.sys={
        watch=function(name,fn) assert(name=='notifications');callback=fn end,
        ask=function(name,file,kind) assert(name=='files.read' and kind=='json');return files[file] end,
        call=function(name,a,b)
            if name=='files.write' then files[a]=b
            elseif name:match('^notifications%.') then calls[#calls+1]={name,a,b}
            else assert(name=='say' and a=='tohru');said[#said+1]=b end
        end,
    }
    env.after=function(ms,fn) local timer={due=clock+ms,fn=fn};timers[#timers+1]=timer;return timer end
    env.cancel=function(timer) timer.cancelled=true end
    assert(loadfile('shell/notifications/notifications.luau','t',env))()
    local n
    n={
        event=function(name,...)handlers[name](...)end,
        env=env,tones=tones,said=said,files=files,calls=calls,report=function(value)callback(value)end,
        advance=function(ms)
            local goal=clock+ms
            while true do
                local next
                for _,timer in ipairs(timers) do
                    if not timer.cancelled and timer.due<=goal and (not next or timer.due<next.due) then next=timer end
                end
                if not next then break end
                clock=next.due;next.cancelled=true;next.fn()
            end
            clock=goal
        end,
        -- Live toasts from the top of the stack down.
        live=function()
            local rows={}
            for _,row in ipairs(env.model.toasts) do if row.live then rows[#rows+1]=row end end
            table.sort(rows,function(a,b) return a.y<b.y end)
            return rows
        end,
        slot=function(title)
            for index,row in ipairs(env.model.toasts) do if row.live and row.title==title then return index-1,row end end
        end,
    }
    return n
end
local function notice(id,title,body,time,urgency,extra)
    local value={id=id,app='App',title=title or 'Title',body=body or 'Body',time=time or 1,urgency=urgency,timeout=6000,left=6000}
    for key,field in pairs(extra or {}) do value[key]=field end
    return value
end
local function fresh() return {last_id=0,power_level=0,unread=false,dnd=false,dnd_pill=false,toasts_open=false,corner='top_right',covered=false} end
local function near(a,b) return math.abs(a-b)<0.002 end

local facts=fresh();local n=boot(facts)
assert(#n.env.model.toasts==4 and #n.live()==0)
n.report({notice(1)});assert(#n.live()==1 and #n.tones==1 and facts.toasts_open)
n.report({notice(1)});assert(#n.tones==1)
n.report({notice(1,'Changed')});assert(#n.live()==1 and n.live()[1].title=='Changed' and #n.tones==2)
n.report({});assert(#n.live()==0)
n.advance(449);assert(facts.toasts_open,'the popup stays open while the last toast fades')
n.advance(1);assert(not facts.toasts_open)
for id=2,10 do n.report({notice(id)}) end
assert(#n.live()==1 and #n.tones==11)
local burst={};for id=11,20 do table.insert(burst,1,notice(id,'T'..id)) end
n.report(burst);assert(#n.live()==3 and #n.tones==21)
for _,tone in ipairs(n.tones) do assert(tone==n.tones[1] and tone.freq==880 and tone.ms==80) end
n.report({});assert(#n.live()==0)
-- Reexecuting a VM retains only native scene protocol watermark, never replays old audio/history.
local reloaded=boot(facts);reloaded.report(burst)
assert(#reloaded.tones==0 and #reloaded.live()==0)
reloaded.report({notice(21,'ééé','🌊')});assert(#reloaded.tones==1)
local long=string.rep('🌊',1000);reloaded.report({notice(22,long,long)})
local _,row=reloaded.slot(string.rep('🌊',256))
assert(row and utf8.len(row.body)==768 and row.lines==3)

-- Stacking: three at most, the newest at the top just under the bar, each one keeping its slot as others come and go.
local stack=boot(fresh())
stack.report({notice(1,'One')});local one=stack.slot('One')
assert(stack.live()[1].y==6,'the first toast sits 6 px under the bar, where its popups open')
stack.report({notice(2,'Two'),notice(1,'One')})
stack.report({notice(3,'Three',nil,1,1,{actions={{key='open',label='Open'},{key='dismiss',label='Dismiss'}}}),notice(2,'Two'),notice(1,'One')})
local rows=stack.live()
assert(rows[1].title=='Three' and rows[2].title=='Two' and rows[3].title=='One')
assert(rows[1].depth==0 and rows[2].depth==1 and rows[3].depth==2)
assert(rows[1].y==6 and rows[1].y+rows[1].h+9==rows[2].y and rows[2].y+rows[2].h+9==rows[3].y)
assert(rows[1].acts==2 and rows[1].a1=='Open' and rows[1].h==rows[2].h+30)
assert(stack.slot('One')==one)
stack.report({notice(4,'Four'),notice(3,'Three'),notice(2,'Two'),notice(1,'One')})
rows=stack.live();assert(#rows==3 and rows[1].title=='Four' and rows[3].title=='Two')
assert(not stack.env.model.toasts[one+1].live and stack.slot('Four')~=one,'a fading slot is not reused at once')
-- A replacement is updated in place.
stack.report({notice(3,'Three again'),notice(4,'Four'),notice(2,'Two')})
rows=stack.live();assert(rows[2].title=='Three again' and rows[1].title=='Four')
-- The bottom right keeps the old order: the newest at the bottom edge, older ones above it.
stack.event('notices_position',1);assert(stack.env.fact.corner=='bottom_right')
rows=stack.live();assert(rows[3].title=='Four' and rows[3].y+rows[3].h==608 and rows[2].y+rows[2].h+9==rows[3].y)
stack.event('notices_position',7);assert(stack.env.fact.corner=='bottom_right')
stack.event('notices_position',0);assert(stack.env.fact.corner=='top_right' and stack.live()[1].title=='Four')
-- Actions: buttons from the footer, the default action from the card, close from its chip.
stack.report({notice(5,'Acts',nil,1,1,{actions={{key='default',label='Open'},{key='r',label='Reply'},{key='m',label='Mark Read'}}})})
local acts,arow=stack.slot('Acts');assert(arow.acts==2 and arow.opens and arow.a1=='Reply' and arow.a2=='Mark Read')
stack.event('toast_action',acts*4+2)
assert(stack.calls[#stack.calls][1]=='notifications.invoke' and stack.calls[#stack.calls][2]==5 and stack.calls[#stack.calls][3]=='m')
assert(not stack.slot('Acts'))
stack.report({notice(6,'Default',nil,1,1,{actions={{key='default',label='Open'}}})})
stack.event('toast_open',(stack.slot('Default')))
assert(stack.calls[#stack.calls][3]=='default' and not stack.slot('Default'))
stack.report({notice(7,'Closed')});stack.event('toast_close',(stack.slot('Closed')))
assert(stack.calls[#stack.calls][1]=='notifications.dismiss' and stack.calls[#stack.calls][2]==7 and not stack.slot('Closed'))

-- Expiry: the capillary drains with the server's time, stops while held, and runs on when let go.
local expiry=boot(fresh())
expiry.report({notice(1,'Timed',nil,1,1,{timeout=6000,left=6000})})
local index,timed=expiry.slot('Timed');assert(timed.counting and near(timed.left,1))
expiry.advance(3000);_,timed=expiry.slot('Timed');assert(near(timed.left,0.5))
expiry.event('toast_enter',index)
assert(expiry.calls[#expiry.calls][1]=='notifications.hold' and expiry.calls[#expiry.calls][3]==true)
_,timed=expiry.slot('Timed');assert(timed.held and near(timed.left,0.5))
expiry.advance(10000);_,timed=expiry.slot('Timed');assert(near(timed.left,0.5),'held time stands still')
expiry.event('toast_enter',index);assert(#expiry.calls==1)
expiry.event('toast_leave',index)
assert(expiry.calls[#expiry.calls][3]==false)
expiry.advance(1500);_,timed=expiry.slot('Timed');assert(not timed.held and near(timed.left,0.25))
-- A report brings the line back to the server's time.
expiry.report({notice(1,'Timed',nil,1,1,{timeout=6000,left=600})})
_,timed=expiry.slot('Timed');assert(near(timed.left,0.1))
expiry.report({});assert(not expiry.slot('Timed'))
expiry.report({notice(2,'Forever',nil,1,1,{timeout=0,left=0})})
local forever_index,forever=expiry.slot('Forever');assert(not forever.counting and forever.left==0)
expiry.event('toast_enter',forever_index);assert(#expiry.calls==2,'nothing to hold')

-- A bar popup open over the top right hides the toasts and holds their lines until it closes.
local cover=boot(fresh())
cover.report({notice(1,'Covered',nil,1,1,{timeout=6000,left=6000})})
local covered_index=cover.slot('Covered')
cover.advance(1000);cover.event('notices_cover',1);assert(cover.env.fact.covered)
assert(cover.calls[#cover.calls][1]=='notifications.hold' and cover.calls[#cover.calls][3]==true)
cover.advance(20000);local _,held=cover.slot('Covered');assert(held.held and near(held.left,5/6))
assert(not cover.env.fact.toasts_open,'the glass closes under the popup, so it takes no clicks')
cover.report({notice(2,'Arrived'),notice(1,'Covered',nil,1,1,{timeout=6000,left=5000})})
local _,arrived=cover.slot('Arrived');assert(arrived.held,'a toast that arrives under a popup waits for it')
cover.event('toast_enter',covered_index);cover.event('notices_cover',0)
assert(cover.env.fact.toasts_open)
_,held=cover.slot('Covered');assert(held.held,'the pointer still holds it');_,arrived=cover.slot('Arrived');assert(not arrived.held)
cover.event('toast_leave',covered_index);_,held=cover.slot('Covered');assert(not held.held)
cover.event('notices_cover',1);cover.event('notices_position',1)
_,held=cover.slot('Covered');assert(not held.held,'bar popups do not reach the bottom right')
cover.event('notices_cover',3);assert(cover.env.fact.covered)
local power_cover=boot(fresh())
power_cover.event('notices_cover',1);power_cover.event('power_warning_low',0.2)
power_cover.advance(60000);assert(power_cover.slot('Battery low'),'a covered power warning waits')
power_cover.event('notices_cover',0);power_cover.advance(5999);assert(power_cover.slot('Battery low'))
power_cover.advance(1);assert(not power_cover.slot('Battery low'))

-- Critical: no capillary, the numeral is the clock, and it stays until closed.
local critical=boot(fresh())
critical.report({notice(1,'Critical',nil,0,2,{timeout=0,left=0})})
local _,crit=critical.slot('Critical');assert(crit.critical and not crit.counting and crit.left==0)
local plain
critical.report({notice(2,'Plain'),notice(1,'Critical',nil,0,2,{timeout=0,left=0})})
_,plain=critical.slot('Plain');_,crit=critical.slot('Critical')
assert(crit.h==plain.h+8,'the critical numeral takes 8 px more')
critical.advance(60000);assert(critical.slot('Critical'))

local power=boot(fresh())
power.report({notice(1,'Application')})
power.event('power_warning_low',0.2)
local _,low=power.slot('Battery low');assert(low and low.clock=='20%' and low.counting and not low.critical and #power.tones==2)
power.event('power_warning_low',0.19);assert(#power.tones==2)
power.report({notice(1,'Application')});assert(#power.live()==2)
power.event('power_warning_critical',0.05);assert(#power.tones==3)
local _,battery=power.slot('Battery critical');assert(battery.critical and battery.clock=='5%' and battery.body=='5% battery remaining')
power.advance(60000);assert(power.slot('Battery critical'),'a critical power warning does not expire')
power.event('power_warning_empty',0.02);assert(#power.tones==4)
power.event('power_warning_clear',0.8)
assert(#power.live()==1 and power.live()[1].title=='Application')
power.event('power_warning_low',0/0);power.event('power_warning_low',math.huge)
power.event('power_warning_low',-1);power.event('power_warning_low',2)
assert(#power.tones==4)
power.event('power_warning_low',0.15);assert(power.slot('Battery low'))
power.advance(5999);assert(power.slot('Battery low'))
power.advance(1);assert(not power.slot('Battery low'),'a low power warning expires after six seconds')
power.event('power_warning_low',0.1);assert(not power.slot('Battery low'),'the same level is not warned twice')
power.event('power_warning_clear',0.5);power.event('power_warning_low',0.1);local power_index=power.slot('Battery low')
power.advance(2000);power.event('toast_enter',power_index);power.advance(30000)
assert(power.slot('Battery low'),'held under the pointer');power.event('toast_leave',power_index)
power.advance(3999);assert(power.slot('Battery low'));power.advance(1);assert(not power.slot('Battery low'))

-- Unread: set by a new ID that is not low urgency, kept across restarts, cleared only when seen.
local bell=boot(fresh())
assert(bell.said[1]=='emit notices_state 0' and not bell.env.fact.unread)
assert(bell.said[#bell.said]=='emit notices_ask','a daemon that starts asks the bar where to stack')
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

-- Do Not Disturb: no toast or tone, still unread; on hides what shows and shows the corner pill for a moment;
-- power warnings bypass it.
local quiet=boot(fresh())
quiet.report({notice(1)});assert(#quiet.live()==1)
quiet.event('notices_dnd',2);assert(quiet.env.fact.dnd and #quiet.live()==0)
assert(quiet.env.fact.dnd_pill and quiet.env.fact.toasts_open)
assert(quiet.said[#quiet.said]=='emit notices_state 3')
quiet.event('notices_seen',0);assert(quiet.said[#quiet.said]=='emit notices_state 2')
quiet.report({notice(2),notice(1)})
assert(#quiet.live()==0 and #quiet.tones==1 and quiet.env.fact.unread)
quiet.event('power_warning_low',0.2);assert(#quiet.live()==1 and #quiet.tones==2)
assert(quiet.live()[1].y==6+46+9,'a toast sits below the pill')
quiet.event('notices_position',1);assert(quiet.live()[1].y+quiet.live()[1].h==608-46-9,'at the bottom right it sits above it')
quiet.event('notices_position',0)
quiet.advance(2599);assert(quiet.env.fact.dnd_pill)
quiet.advance(1);assert(not quiet.env.fact.dnd_pill and quiet.live()[1].y==6)
quiet.event('notices_dnd',1);quiet.event('notices_dnd',7);assert(quiet.env.fact.dnd and not quiet.env.fact.dnd_pill)
quiet.event('notices_dnd',2);assert(not quiet.env.fact.dnd and not quiet.env.fact.dnd_pill and #quiet.live()==1)
quiet.report({notice(3),notice(2),notice(1)});assert(#quiet.live()==2 and #quiet.tones==3)
quiet.event('notices_dnd',1);assert(quiet.env.fact.dnd_pill);quiet.event('notices_dnd',0)
assert(not quiet.env.fact.dnd and not quiet.env.fact.dnd_pill,'turning it off takes the pill away')
print('native notification entry logic: passed')
