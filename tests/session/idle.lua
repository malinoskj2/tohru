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
