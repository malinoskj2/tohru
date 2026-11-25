local scenes=assert(loadfile('shell/assistant/afk-scenes.luau'))()
local motion=assert(loadfile('shell/assistant/afk-motion.luau'))()(scenes)
local cases={
    {w=1920,h=1080,scale=1,camera=true,glass=true},
    {w=1536,h=864,scale=1.25,camera=true,glass=true},
    {w=864,h=1536,scale=1.25,camera=false,glass=true},
    {w=1920,h=1080,scale=2,camera=false,glass=false},
}
for _,s in ipairs(cases) do
    local eligible,reasons=motion.availability(s)
    assert(eligible.screen_window==s.camera)
    assert(eligible.glass_inspection==s.glass)
    assert(eligible.corner_nap and eligible.wing_grooming)
    if not s.camera then assert(reasons.screen_window) end
    s.target={id='verified',x=100,y=120,w=600,h=400};s.output_id=1
    for _,scene in ipairs(scenes.catalog) do
        for i=0,100 do
            local sample=motion.sample(scene.id,s,scene.duration*i/100)
            assert(type(sample.pose)=='string')
            local reach=256+75*sample.scale
            assert(sample.x>=-reach and sample.x<=s.w+reach and sample.y>=-reach and sample.y<=s.h+reach)
            assert(math.abs(sample.turn)>=.05 and math.abs(sample.turn)<=1.2)
            assert(sample.scale>0 and sample.scale<=12)
            for _,k in ipairs({'skin_frame','skin_next'}) do
                if sample[k] then assert(sample[k]>=0 and sample[k]<=223 and sample[k]==math.floor(sample[k])) end
            end
            if sample.skin_mix then assert(sample.skin_next and sample.skin_mix>=0 and sample.skin_mix<=1) end
            for _,k in ipairs({'closeup_frame','closeup_next','body_frame','body_next'}) do
                if sample[k] then assert(sample[k]>=0 and sample[k]<=15 and sample[k]==math.floor(sample[k])) end
            end
            assert(sample.generated==nil and sample.facing==nil)
            if scene.id=='quiet_rest' then
                assert(sample.airborne==0 and sample.bank==0 and sample.rotation==0)
            end
        end
    end
    local normal=motion.sample('wing_grooming',s,4)
    assert(150*normal.scale*s.scale<=236.001)
end
-- Contact actions have local authored frames and are not root-translated gags.
local s=cases[1]
local unknown=motion.availability({w=1920,h=1080})
assert(not unknown.screen_window and not unknown.quiet_rest)
local a=motion.sample('wing_grooming',s,5)
local b=motion.sample('wing_grooming',s,5.3)
assert(a.pose=='skin' and b.pose=='skin' and a.action==4 and b.action==4 and a.phase~=b.phase and a.x==b.x and a.y==b.y)
print('AFK native motion, physical source caps, rotated/fractional eligibility and authored contact frames passed')
for i=0,100 do
    local sample=motion.sample('glass_inspection',cases[1],2.6*i/100)
    if sample.bust_mix>0 then assert(sample.y+75*sample.scale>=cases[1].h) end
    if sample.bust_mix<0.9999 then assert(150*sample.scale*1.924510<=1254.01) end
end
-- Generated in-betweens drive the turns, nap, perch, peeks, climbs and closeups.
do
    local ranges={turn=96,arrive_right=112,arrive_left=128,curl=144,perch=160,bank=176,peek=192,wave=208}
    local wide={w=1920,h=1080,scale=1,target={id='v',x=500,y=300,w=700,h=450},output_id=1}
    local function uses(id,name)
        local scene=scenes.get(id)
        for i=0,600 do
            local x=motion.sample(id,wide,scene.duration*i/600)
            if name=='closeup' then
                if x.closeup_next or x.body_next then return true end
            elseif x.skin_frame and x.skin_frame>=ranges[name] and x.skin_frame<ranges[name]+16 then return true end
        end
    end
    for id,list in pairs({corner_nap={'curl','turn','arrive_right'},window_perch={'perch','bank'},
        hide_peek={'peek','wave'},distant_flight={'turn','bank'},tail_chase={'arrive_left','arrive_right'},
        screen_window={'closeup'},glass_inspection={'closeup'},upside_down_peek={'closeup'}}) do
        for _,name in ipairs(list) do assert(uses(id,name),id..' does not use '..name) end
    end
    -- A crossfade always blends toward the following generated frame.
    local nap=motion.sample('corner_nap',wide,2.8+1)
    assert(nap.skin_frame>=144 and nap.skin_frame<159 and nap.skin_next==nap.skin_frame+1 and nap.turn==-1)
end
