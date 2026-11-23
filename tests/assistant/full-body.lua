_G.fact, _G.text = {}, {}
local ticking, ticks = nil, 0
_G.every = function(_, f) ticking = f; return 1 end
_G.cancel = function() ticking = nil end
_G.after = function() end
local art = assert(loadfile('shell/assistant/full-body.luau'))()
local create = assert(loadfile('shell/assistant/puppet.luau'))()
local function fresh(seed)
    math.randomseed(seed or 41)
    local p = create(art)
    p.visible(true)
    return p
end
local function run(n) for _=1,n do ticks=ticks+1; ticking() end end
local function settle(p)
    for _=1,120 do
        run(1)
        local a,b=p.pose()
        if a==b then run(1); return a end
    end
    error('transition did not finish')
end

assert(#art == 20)
local cells, faces = {}, {}
for _, a in ipairs(art) do
    assert(not cells[a.cell] and not faces[a.face], 'duplicate body or face')
    cells[a.cell], faces[a.face] = true, true
    assert(a.face >= 0 and a.face < 20)
    assert(a.fx >= 0 and a.fx + 50 <= 151 and a.fy >= 0 and a.fy + 34 <= 151)
    assert(a.count == 17 and a.first >= 202 and a.first+a.count == a.cell+165)
    assert(#a.mouths == 17)
    for _, anchor in ipairs(a.mouths) do assert(anchor[1]>=0 and anchor[1]+24<=150 and anchor[2]>=0 and anchor[2]+18<=150) end
end

-- Long quiet waits actually visit new lower-body poses, with quiet holds and no
-- repeats among the last three selected postures. Randomness is reproducible.
local p=fresh()
local recent, visited, old, age, changes={}, {}, 'neutral', 0, 0
for _=1,24000 do
    run(1); age=age+1
    local shown,target=p.pose()
    if shown==target and shown~=old then
        assert(age >= 216, 'posture changed without a quiet hold')
        for _,x in ipairs(recent) do assert(x~=shown, 'recent posture repeated') end
        recent[#recent+1]=shown; if #recent>3 then table.remove(recent,1) end
        visited[shown]=true; old=shown; age=0; changes=changes+1
        assert(fact.ai_dg_face>=0 and fact.ai_dg_face<480 or fact.ai_dg_sheet==1)
        assert(fact.ai_dg_eye==-1 and fact.ai_dg_mouth==-1, 'legacy stickers on a turned face')
    end
end
local count=0; for _ in pairs(visited) do count=count+1 end
assert(count>=10 and changes<65, 'waits need both body variety and long holds')
p.visible(false)
assert(not ticking)

-- A live listening or thinking state remains attentive across long silences.
p=fresh(27); p.hearing(true); settle(p); run(1500)
local a,b=p.pose(); assert(a==b and a~='neutral' and p.poses[a].full)
p.thinking(); settle(p); run(1500)
a,b=p.pose(); assert(a==b and a~='neutral')
-- Interrupting an unfinished body route recovers at a safe key and never runs
-- the remainder of an obsolete speech plan.
p.speaking(true); run(2); p.hearing(false); p.speaking(false,'paused')
assert(settle(p)=='attentive_hover')
p.thinking(); run(2); p.speaking(false,'interrupted')
assert(settle(p)=='ponder_hover')
p.calm(); assert(settle(p)=='neutral')

-- Speech on native faces keeps a per-view patch; blinking and every viseme are
-- independently addressable. Positive emotion gets an airborne lift.
p.speaking(true); settle(p); run(110)
p.sentence('That is wonderful!')
for _=1,6 do p.mouth(0) end
local moving_speech = 0
for _=1,100 do
    run(1)
    p.mouth(0.6,5)
    if fact.ai_dg_travel>=0 then
        moving_speech=moving_speech+1
        if moving_speech>1 then assert(fact.ai_dg_travel%8==5,'viseme was lost during a body turn') end
    end
    if p.pose()=='bright_hover' then break end
end
assert(moving_speech>=16, 'new body transitions need registered lip sync')
assert(settle(p)=='bright_hover')
for v=0,7 do
    p.mouth(0.6,v); run(1)
    assert(fact.ai_dg_face%8==v and fact.ai_dg_mouth==-1 and fact.ai_dg_eye==-1)
end
p.speaking(false)
local resting=settle(p)
assert(p.poses[resting].full and resting~='bright_hover','normal speech end must settle quietly')
run(100); assert(p.pose()==resting,'settling fidgeted immediately')
p.speaking(true)
p.speaking(false,'paused'); assert(settle(p)=='attentive_hover')
p.speaking(false,'interrupted'); assert(select(2,p.pose())=='attentive_hover')
p.calm(); assert(settle(p)=='neutral')

-- Reduced motion can be enabled mid-transition. It suppresses posture travel,
-- flourishes and hearts indefinitely, but preserves speech and blink output.
p.thinking(); run(2); p.reduced(true); run(1)
assert(p.pose()=='neutral' and fact.ai_dg_reduced)
local frame=fact.ai_dg_frame
run(4000)
assert(fact.ai_dg_frame==frame and fact.ai_dg_sq==0 and fact.ai_dg_h1_a==0)
p.hearing(true); p.thinking(); p.speaking(true); p.sentence('That is wonderful!')
for _=1,6 do p.mouth(0) end
run(200); p.mouth(0.6,5); run(1)
assert(p.pose()=='neutral' and fact.ai_dg_mouth==5 and fact.ai_dg_face==-1)
p.speaking(false,'interrupted'); p.calm(); p.reduced(false); run(650)
assert(p.pose()~='neutral', 'normal motion did not resume')
p.visible(false)

-- Re-entering thinking after three different thought holds must leave a valid
-- next choice; semantic entry poses count in the same bounded recent history.
p=fresh(9); p.thinking(); settle(p)
local history,previous,exercised={},nil,false
for _=1,16000 do
    run(1)
    local shown,target=p.pose()
    if shown==target and shown~=previous then
        previous=shown;history[#history+1]=shown
        if #history>3 then table.remove(history,1) end
        local other=#history==3
        for _,name in ipairs(history) do other=other and name~='ponder_hover' end
        if other then
            p.thinking(); assert(settle(p)=='ponder_hover');run(500)
            assert(p.pose()~='ponder_hover','thinking re-entry exhausted the choice pool')
            exercised=true;break
        end
    end
end
assert(exercised,'thought re-entry fixture did not reach its history boundary')
p.visible(false)

-- Every new key has an explicit reversible transition, rather than a squash swap.
for _, key in ipairs(art) do
    local path=p.route('neutral', key.name)
    assert(#path==1 and path[1].kind=='move' and path[1].count>=16)
    local back=p.route(key.name, 'neutral')
    assert(#back==1 and back[1].back)
end
print('Full-body waits, holds, recent history, interruption, native faces, reduced motion and recovery passed')

-- Disabling voice drops every art source and closes warm-up. A stale warm-up
-- callback from an earlier service generation cannot shorten a fresh preload.
local callbacks={}
_G.after=function(_,f) callbacks[#callbacks+1]=f end
local w=create(art)
w.load('/art')
assert(fact.ai_dg_ready and fact.ai_dg_warm and text.ai_dg_faces_src=='/art/faces.webp')
w.load('')
assert(not fact.ai_dg_ready and not fact.ai_dg_warm)
for _,name in ipairs({'body','eyes','mouths','faces','travel','heart'}) do assert(text['ai_dg_'..name..'_src']=='') end
w.load('/art')
callbacks[1]()
assert(fact.ai_dg_warm,'stale service callback ended a new preload')
callbacks[2]()
assert(not fact.ai_dg_warm and fact.ai_dg_ready)
assert(w.pick('Something failed!')~='bright_hover')
print('Art unload and warm-up generation ownership passed')

-- Landing owns only the displayed key. Test all registered quiet destinations,
-- including a target change during transit and preemption halfway into a turn.
local landing = assert(loadfile("shell/assistant/landing-frames.luau"))()
local actor = create(art, landing)
actor.load("/fixture")
fact.ai_dg_warm = false
actor.visible(true)
for index, destination in ipairs({"ponder_hover", "attentive_hover", "neutral"}) do
    actor.calm()
    assert(actor.begin_entrance("right", -1))
    if destination == "ponder_hover" then actor.thinking() end
    if destination == "attentive_hover" then actor.hearing(true) end
    assert(actor.land(0.5))
    assert(fact.ai_landing_art and fact.ai_landing_frame == (index - 1) * 15 + 7)
    if destination == "neutral" then
        assert(fact.ai_dg_pose_scale == 1 and fact.ai_landing_scale == 1)
    else
        assert(fact.ai_dg_pose_scale >= 1.15, "thinking and listening still shrink below neutral size")
        assert(fact.ai_landing_scale > 1 and fact.ai_landing_scale < fact.ai_dg_pose_scale,
            "landing must grow smoothly into the conversational pose")
    end
    assert(actor.land(1))
    assert(fact.ai_landing_scale == fact.ai_dg_pose_scale, "landing endpoint changes size")
    local cell = fact.ai_dg_frame
    assert(not fact.ai_landing_art and not fact.ai_flight_art and cell == ({489, 507, 0})[index])
    actor.end_entrance()
    assert(actor.pose() == destination and fact.ai_dg_frame == cell)
end
actor.thinking()
assert(actor.begin_entrance("rise", 1) and actor.land(0.35))
actor.hearing(true)
assert(not actor.land(0.4))
actor.end_entrance()
assert(actor.pose() == "attentive_hover" and fact.ai_dg_frame == 507 and not fact.ai_landing_art)
print("PASS direct landings to current targets and clean release after preemption")
