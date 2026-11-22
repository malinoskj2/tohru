local now, tick=0,nil
_G.every=function(_,f) tick=f;return f end
_G.cancel=function() tick=nil end
local scenes=assert(loadfile('shell/assistant/afk-scenes.luau'))()
local policy=assert(loadfile('shell/assistant/afk-policy.luau'))()
local make=assert(loadfile('shell/assistant/afk-controller.luau'))()
local state={unlocked=true,session_active=true,output_ready=true,inhibited=false,fullscreen=false,output='left'}
local poses={};for _,scene in ipairs(scenes.catalog) do for pose in pairs(scene.poses) do poses[pose]=true end end
local callbacks, releases, played, prepared={}, {}, {}, {}
local actor={supported=function()return poses end,observe=function()return state end,
    prepare=function(sample,ready)prepared[#prepared+1]={sample=sample,ready=ready} end,
    play=function(sample)played[#played+1]=sample end,
    release=function(reason)releases[#releases+1]=reason end}
local stopped=0
local source={start=function(f)callbacks[#callbacks+1]=f;return #callbacks end,stop=function()stopped=stopped+1 end}
local c=make(scenes,policy,actor,source,function()return now end)
local settings={enabled=true,seed=17,force='screen_window',probability=100}
c.configure(settings)
local deliver=callbacks[1]
deliver({kind='ready',generation=1,seed=45})
assert(not tick)
deliver({kind='idle',generation=1})
assert(tick and #prepared==1 and #played==0)
now=1;tick();assert(#played==0) -- No root motion while cold.
prepared[1].ready(true)
now=1.016;tick();assert(#played==0) -- Staged frame even for warm/synchronous pose.
now=1.032;tick();assert(#played==1)
deliver({kind='resume',generation=1})
assert(not tick and releases[#releases].reason=='input')
prepared[1].ready(true);assert(not tick)
deliver({kind='idle',generation=1})
assert(#prepared==2)
now=7;tick();assert(not tick and releases[#releases].reason=='pose-timeout')
prepared[2].ready(true);assert(not tick)
deliver({kind='idle',generation=1});assert(not tick)
deliver({kind='resume',generation=1});deliver({kind='idle',generation=1})
assert(tick)
state.unlocked=false;c.changed();assert(not tick and releases[#releases].immediate)
state.unlocked=true
c.configure({enabled=false,probability=100,seed=17})
assert(stopped==1)
deliver({kind='ready',generation=2,seed=17});deliver({kind='resume',generation=2});deliver({kind='idle',generation=2})
assert(not tick) -- Disabled configuration fenced the previous source callback.
c.configure(settings)
local fresh=callbacks[2]
fresh({kind='ready',generation=3,seed=17});fresh({kind='resume',generation=3});fresh({kind='idle',generation=3})
assert(tick)
c.close();assert(not tick and stopped==2)
fresh({kind='resume',generation=3});fresh({kind='idle',generation=3});assert(not tick)
print('AFK controller cold-pose, source lifetime, lock/input teardown and stale callback tests passed')
