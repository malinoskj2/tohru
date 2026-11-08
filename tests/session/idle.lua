-- Actual ordinary-root module boundary; no display, bus or action backend.
local handlers, calls = {}, {}
local fail = false
local env = setmetatable({fact={idle_caffeine=false},text={}}, {__index=_G})
env.on=function(name,fn) handlers[name]=fn end
env.tohru_idle={}
for _,name in ipairs({'held','resume','caffeine','start'}) do
    env.tohru_idle[name]=function(value)
        if fail then error('native boundary unavailable') end
        calls[#calls+1]={name,value}
    end
end
local idle=assert(loadfile('shell/session/idle.luau','t',env))()
assert(idle.configure({lock_seconds=1800}))
assert(calls[1][1]=='start' and calls[1][2].lock_seconds==1800)
handlers.idle_lock_state(1);assert(calls[#calls][1]=='held' and calls[#calls][2]==true)
handlers.idle_lock_state(0);assert(calls[#calls][2]==false)
local count=#calls
handlers.idle_lock_state(2);handlers.idle_lock_state('1');handlers.idle_lock_state(0/0)
assert(#calls==count)
handlers.idle_caffeine_toggle();assert(env.fact.idle_caffeine and calls[#calls][2])
fail=true;handlers.idle_caffeine_toggle();assert(env.fact.idle_caffeine)
assert(env.text.idle_status:find('unavailable'))
fail=false;handlers.idle_caffeine_toggle();assert(not env.fact.idle_caffeine)
handlers.idle_resume();assert(calls[#calls][1]=='resume')
print('ordinary root native idle entry: passed')

-- Actual private native fixture uses the same module/configuration boundary;
-- no event, including off/lock tests, may ever arm a suspend timeout.
local probe_handlers,configs={},{}
local probe_env=setmetatable({
    require=function(name)
        assert(name=='idle')
        return {configure=function(config)configs[#configs+1]=config;return true end}
    end,
    on=function(name,fn)probe_handlers[name]=fn end,
    tohru_idle={stop=function()end},
},{__index=_G})
assert(loadfile('shell/session/idle_probe.luau','t',probe_env))()
probe_handlers.fixture_arm_lock();probe_handlers.fixture_arm_off();probe_handlers.fixture_disable()
for _,config in ipairs(configs) do
    assert(config.suspend_unlocked_seconds==0 and config.suspend_locked_seconds==0)
end
assert(configs[2].lock_seconds==1 and configs[3].screen_off_locked_seconds==1)
