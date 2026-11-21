math.clamp=math.clamp or function(v,lo,hi)return math.max(lo,math.min(hi,v))end
math.sign=math.sign or function(v)return v>0 and 1 or (v<0 and -1 or 0)end
local now=100
os.clock=function()return now end
fact={media_open=false,media_unfolded=false,media_seeking=false,media_focus=0,media_keyed=false,
      media_vol_dragging=false,media_vol_requested=0,media_volume=-1,media_length=0,media_seed_n=0,
      ['rail.target.3']=8,['rail.land.3']=0,['rail.serial.3']=0,['rail.crest.3']=0}
text,handlers,calls,timers={},{},{},{}
model=setmetatable({},{__newindex=function(_,name,value)
    fact[name..'.count']=math.min(#value,8)
    for i,row in ipairs(value)do for k,v in pairs(row)do
        local path=name..'.'..(i-1)..'.'..k
        if type(v)=='string' then text[path]=v else fact[path]=v end
    end end
end})
on=function(name,callback)handlers[name]=callback end
local failure
sys={call=function(...)if failure then error(failure)end;calls[#calls+1]=table.pack(...)end,
     watch=function()error('the media popup must not add a second watch')end}
local next_timer=0
after=function(ms,callback)next_timer=next_timer+1;timers[next_timer]={ms=ms,callback=callback};return next_timer end
cancel=function(id)timers[id]=nil end
local function run(ms)for id,t in pairs(timers)do if t.ms==ms then timers[id]=nil;t.callback()end end end
function require(name)assert(name=='bar/rail');return assert(loadfile('shell/bar/rail.luau'))()end
local media=assert(loadfile('shell/media/controller.luau'))()()
local function last()return calls[#calls]end
local function player(id,name,title,artist,playing,extra)
    local p={id=id,name=name,title=title,artist=artist,playing=playing,chosen=false,art=''}
    for k,v in pairs(extra or {})do p[k]=v end
    return p
end
local three={
    player('firefox.instance_1','Firefox','Says','Nils Frahm',false,{art='/art/frahm.png'}),
    player('mpv','mpv','Space Song','Beach House',false,{art='https://example.org/c.jpg'}),
    player('spotify','Spotify','Midnight City','M83',true,{art='/art/m83.png'}),
}
local function report(extra)
    local r={playing=true,title='Midnight City',artist='M83',album='',length=243,position=134,rate=1,can_seek=true,
             art='/art/m83.png',volume=0.72,player='spotify',players=three}
    for k,v in pairs(extra or {})do r[k]=v end
    return r
end

-- No players: nothing to show, nothing to command.
media.observe({playing=false,title='',artist='',length=0,position=0,rate=1,can_seek=false,art='',volume=-1,player='',players={}})
assert(not fact.media_has and not fact.media_playing and fact['media_players.count']==0 and fact.media_volume==-1)
media.open(); assert(fact.media_open and not fact.media_unfolded)
handlers.media_play();handlers.media_next();handlers.media_previous();handlers.media_step(5);handlers.media_fold_toggle()
assert(#calls==0 and not fact.media_unfolded,'nothing is commanded with no player')
media.close(); assert(not fact.media_open)

-- Players appear: the reported one heads the rows, the rest keep the bus's order.
media.observe(report())
assert(fact.media_has and fact.media_playing and fact.media_can_seek and fact.media_length==243)
assert(text.media_title=='Midnight City' and text.media_name=='Spotify' and text.media_art=='/art/m83.png' and fact.media_has_art)
assert(fact.media_volume==0.72 and fact['media_players.count']==3)
assert(text['media_players.0.id']=='spotify' and fact['media_players.0.active'] and text['media_players.0.sub']=='Spotify · M83')
assert(text['media_players.1.id']=='firefox.instance_1' and not fact['media_players.1.active'])
assert(text['media_players.2.art']=='' and not fact['media_players.2.has_art'],'remote covers are not painted')
assert(fact.media_seed==134,'the clock starts where the player is')

-- Opening carries the position on by the time since the report, while it plays.
now=110; media.open()
assert(fact.media_seed==144 and math.abs(fact['rail.goal.3']-(8+188*144/243))<1e-9)

-- Play/pause, next and previous go to the reported player.
handlers.media_play(); assert(last()[1]=='media.toggle')
handlers.media_next(); assert(last()[1]=='media.next')
handlers.media_previous(); assert(last()[1]=='media.previous')
handlers.media_step(-5); assert(last()[1]=='media.step' and last()[2]==-5)

-- A pause is shown from the report, not from the command.
media.observe(report({playing=false,position=144}))
assert(not fact.media_playing and fact.media_seed==144)
now=200; media.close(); media.open(); assert(fact.media_seed==144,'paused, the clock does not move')

-- Seeking: the drag shows the requested place and seeks once, on release.
media.observe(report({position=144}))
local before=#calls
handlers.media_seek_press(0.5); assert(fact.media_seeking and fact.media_requested==121.5)
handlers.media_seek_drag(160/243); assert(math.abs(fact.media_requested-160)<1e-9 and #calls==before)
media.observe(report({position=150})); assert(fact.media_seed==144,'a report mid-drag does not move the clock')
handlers.media_seek_release()
assert(not fact.media_seeking and last()[1]=='media.seek' and math.abs(last()[2]-160)<1e-9 and math.abs(fact.media_seed-160)<1e-9)
-- A player that cannot seek is not dragged.
media.observe(report({can_seek=false})); before=#calls
handlers.media_seek_press(0.3); assert(not fact.media_seeking); handlers.media_seek_release(); assert(#calls==before)
-- A failed seek is reported and the clock goes back to the player's place.
media.observe(report({position=10})); failure='Spotify cannot seek'
handlers.media_seek_press(0.9); handlers.media_seek_release()
assert(fact.media_failed and text.media_error:find('cannot seek') and fact.media_seed==10 and not fact.media_seeking)
failure=nil

-- Switching players: a row chooses its player; its play button chooses it and toggles it.
handlers.media_choose(1); assert(last()[1]=='media.choose' and last()[2]=='firefox.instance_1' and not fact.media_failed)
handlers.media_row_play(2); assert(calls[#calls-1][2]=='mpv' and last()[1]=='media.toggle')
before=#calls; handlers.media_choose(7); handlers.media_choose(-1); handlers.media_choose(0.5); assert(#calls==before)
local chosen={}
for i,p in ipairs(three)do chosen[i]=player(p.id,p.name,p.title,p.artist,p.playing,{art=p.art,chosen=p.id=='firefox.instance_1'})end
media.observe(report({player='firefox',title='Says',artist='Nils Frahm',art='/art/frahm.png',volume=-1,playing=false,players=chosen}))
assert(text.media_name=='Firefox' and text['media_players.0.id']=='firefox.instance_1' and fact['media_players.0.active'])
assert(fact.media_volume==-1 and not fact.media_playing)

-- Volume: a player without its own volume is left alone; one with it gets at most one write per 50 ms.
before=#calls; handlers.media_volume_press(0.4); handlers.media_volume_step(1); assert(#calls==before and not fact.media_vol_dragging)
media.observe(report())
handlers.media_volume_press(0.4); handlers.media_volume_drag(0.5); assert(#calls==before and fact.media_vol_requested==0.5)
run(50); assert(last()[1]=='media.volume' and last()[2]==0.5 and #calls==before+1)
handlers.media_volume_drag(0.6); handlers.media_volume_release(); assert(last()[2]==0.6 and fact.media_vol_dragging)
media.observe(report({volume=0.6})); assert(not fact.media_vol_dragging)
handlers.media_volume_step(-1); assert(math.abs(last()[2]-0.55)<1e-9)

-- Folding and the keyboard's rows: header, three players, volume.
fact.media_unfolded=false; handlers.media_fold_toggle(); assert(fact.media_unfolded and fact.media_focus==0)
handlers.media_focus_move(1); handlers.media_focus_move(1); assert(fact.media_focus==2 and fact.media_keyed)
handlers.media_activate(); assert(last()[1]=='media.choose' and last()[2]=='firefox.instance_1')
handlers.media_focus_move(9); assert(fact.media_focus==4)
handlers.media_focus_step(1); assert(last()[1]=='media.volume')
handlers.media_focus_move(-9); handlers.media_activate(); assert(last()[1]=='media.toggle')

-- A player vanishes while unfolded on the last row: focus stays on a row that exists.
media.observe(report({players={three[3]}}))
assert(fact['media_players.count']==1 and fact.media_focus<=2)
-- All vanish: the popup folds and shows nothing playing.
fact.media_focus=2
media.observe({playing=false,title='',artist='',length=0,position=0,rate=1,can_seek=false,art='',volume=-1,player='',players={}})
assert(not fact.media_has and not fact.media_unfolded and fact['media_players.count']==0 and fact.media_focus==1)
-- And come back.
media.observe(report()); assert(fact.media_has and fact['media_players.count']==3)

-- A close from the native popup (an outside click) closes the controller too.
fact.media_open=true; fact.media_unfolded=true; handlers['fact:media_open'](false)
assert(not fact.media_unfolded and not fact.media_keyed)
print('Media controller: players, switching, seek, play/pause, volume, keyboard and vanishing players passed')
