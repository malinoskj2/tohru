local scenes = assert(loadfile('shell/assistant/afk-scenes.luau'))()
local make = assert(loadfile('shell/assistant/afk-policy.luau'))()
local function safe()
    local supported={}
    for _,s in ipairs(scenes.catalog) do supported[s.id]=true end
    return {unlocked=true,session_active=true,output_ready=true,output='portrait-left',
        inhibited=false,fullscreen=false,window_ready=true,occlusion_ready=true,supported=supported}
end
local function configured(probability,force)
    local p=make(scenes.catalog)
    p.configure({enabled=true,probability=probability or 100,seed=17,force=force})
    p.observe(safe());p.source(101)
    return p
end

-- Unknown source never means desktop idle; duplicate notifications consume no roll.
local p=make(scenes.catalog)
p.configure({enabled=true,probability=100,seed=17})
p.observe(safe())
assert(p.idle(101)==nil and p.state().episode==0)
p.source(101)
local first=p.idle(101)
assert(first.kind=='start' and first.output=='portrait-left')
for _=1,100 do assert(p.idle(101)==nil) end
assert(p.state().episode==1)
p.finished(101,first.episode)
assert(p.idle(101)==nil)
p.resume(101)
assert(p.idle(101).scene~=first.scene)

-- Chance failures do not get a second chance until physical input resumes.
p=configured(50)
local failed,success,last=0,0,nil
local sequence={}
for i=1,200 do
    p.resume(101)
    local event=p.idle(101)
    sequence[i]=event.scene or event.reason
    if event.kind=='skip' then failed=failed+1;assert(event.reason=='chance')
    else success=success+1;assert(event.scene~=last);last=event.scene end
    assert(p.idle(101)==nil and p.state().episode==i)
end
assert(failed>60 and success>60)
local replay=configured(50)
for i=1,200 do
    replay.resume(101);local event=replay.idle(101)
    assert((event.scene or event.reason)==sequence[i])
end

-- Eligibility is applied before scene selection and the probability draw.
for _,field in ipairs({'unlocked','session_active','output_ready','inhibited'}) do
    p=configured();local state=safe();state[field]=nil;p.observe(state)
    assert(p.idle(101).reason=='ineligible',field)
end
for _,field in ipairs({'listening','speaking','pending','text_entry','dnd','entrance','puppet_busy','inhibited'}) do
    p=configured();local state=safe();state[field]=true;p.observe(state)
    assert(p.idle(101).reason=='ineligible',field)
end
for _,fullscreen in ipairs({true,'unknown'}) do
    p=configured();local state=safe();state.fullscreen=fullscreen;p.observe(state)
    assert(p.idle(101).reason=='ineligible')
end
p=configured();local state=safe();state.supported={};p.observe(state)
assert(p.idle(101).reason=='ineligible')
p=configured(100,'hide_peek');state=safe();state.occlusion_ready=false;p.observe(state)
assert(p.idle(101).reason=='forced-ineligible')
p=configured(100,'window_perch');state=safe();state.window_ready=false;p.observe(state)
assert(p.idle(101).reason=='forced-ineligible')
p=configured(0,'screen_window');assert(p.idle(101).reason=='chance')
p=configured(100,'screen_window');state=safe();state.fullscreen=nil;p.observe(state)
assert(p.idle(101).reason=='ineligible')

-- Reduced motion defaults to no AFK; optional rest has no translation or zoom.
p=configured();state=safe();state.reduced=true;p.observe(state)
assert(p.idle(101).reason=='ineligible')
p.configure({enabled=true,seed=17,probability=100,quiet=true})
p.resume(101);assert(p.idle(101).scene=='quiet_rest')
local rest=scenes.sample('quiet_rest',5)
assert(not rest.moving and rest.from=='home' and rest.to=='home')

-- Mid-scene safety changes stop the current scene. Input rearms; a lock, helper
-- reconnect or config change alone cannot roll again in the same idle episode.
for _,field in ipairs({'unlocked','session_active','output_ready','inhibited','listening','speaking','pending','text_entry','dnd','entrance'}) do
    p=configured(100,'screen_window');p.idle(101)
    state=safe();state[field]=not state[field]
    local event=p.observe(state)
    assert(event and event.kind=='stop',field)
    assert(p.idle(101)==nil)
end
p=configured();p.idle(101);state=safe();state.output='other';assert(p.observe(state).immediate)
p=configured();p.idle(101)
assert(p.resume(999)==nil and p.state().idle)
assert(p.source(102).reason=='source')
assert(p.idle(101)==nil and p.idle(102)==nil)
p.resume(102);assert(p.idle(102).kind=='start')
p.configure({enabled=false,probability=100,seed=17});assert(p.idle(102)==nil)

-- Scene timelines are bounded and require their actual authored actions.
assert(#scenes.catalog==12)
for _,scene in ipairs(scenes.catalog) do
    assert(scene.duration >= 8 and scene.duration <= (scene.id=='corner_nap' and 25 or 20))
    local last=scenes.sample(scene.id,scene.duration)
    assert(last.finished and last.requires_pose_ready)
    assert(scenes.sample(scene.id,-1).progress==0)
    local supported=scenes.supported({})
    assert(supported[scene.id]==false)
    local complete={};for pose in pairs(scene.poses) do complete[pose]=true end
    assert(scenes.supported(complete)[scene.id])
end
assert(scenes.get('wing_grooming').poses.horn_polish)
assert(scenes.get('glass_inspection').poses.glass_paw)
print('AFK episode, deterministic replay, eligibility, cancellation and bounded choreography passed')
