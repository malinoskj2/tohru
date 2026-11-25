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
            if sample.skin_frame then assert(sample.skin_frame>=0 and sample.skin_frame<=95) end
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
