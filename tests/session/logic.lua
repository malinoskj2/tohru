-- Executes the actual Lua-compatible feature entry scripts with controlled native boundaries.
-- This CPU check does not establish compositor, PAM, polkit-helper or Luau VM parity.
local function boot(path, facts, texts)
    local h, timers, calls, watches, hints = {}, {}, {}, {}, {}
    local now, auth, failure = 0, false, false
    local native_held = facts.acknowledged or false
    local env = setmetatable({ fact = facts, text = texts }, { __index = _G })
    env.on = function(name, fn) h[name] = fn end
    env.focus = function() end
    env.every = function(ms, fn) local t = {due=now+ms, step=ms, fn=fn}; timers[#timers+1]=t; return t end
    env.after = function(ms, fn) local t = {due=now+ms, fn=fn}; timers[#timers+1]=t; return t end
    env.cancel = function(t) t.cancelled=true end
    env.sys = {
        call = function(name, ...)
            if failure then error("native request rejected") end
            local args = {...}
            if name == "say" and args[1] == "tohru" then
                hints[#hints+1] = args[2]
            else calls[#calls+1] = {name, ...} end
        end,
        ask = function(name, password)
            if name == "lock.held" then return native_held end
            assert(name == "auth.unlock")
            assert(texts.password == "", "secret must be cleared before PAM call")
            if failure then error("PAM unavailable") end
            return auth
        end,
        watch = function(name, fn) watches[name]=fn end,
    }
    assert(loadfile(path, "t", env))()
    return {
        fact=facts, text=texts, calls=calls, hints=hints,
        event=function(name, ...) assert(h[name], name)(...) end,
        report=function(name, value) watches[name](value) end,
        auth=function(value) auth=value end,
        held=function(value) native_held=value end,
        fail=function(value) failure=value end,
        advance=function(ms)
            for _=1,ms do
                now=now+1
                for _,t in ipairs(timers) do
                    if not t.cancelled and t.due <= now then
                        if t.step then t.due=now+t.step else t.cancelled=true end
                        t.fn()
                    end
                end
            end
        end,
    }
end
local function menu()
    return boot("shell/session/menu.luau", {open=false, selected=1}, {})
end
local m=menu()
m.event("menu_toggle");m.event("choose",3);m.event("menu_toggle")
assert(not m.fact.open and m.fact.confirming==0)
m.event("accept_choice");assert(#m.calls==0)
m.event("choose", 4); assert(#m.calls==0)
m.event("menu_open"); m.event("choose", 4)
assert(m.fact.confirming==4 and #m.calls==0)
m.advance(59999); assert(m.fact.remaining==1)
m.advance(1); m.event("accept_choice"); assert(#m.calls==0)
m.event("choose", 3); m.event("key", "Escape"); m.event("accept_choice"); assert(#m.calls==0)
m.event("choose", 5); m.event("menu_close"); m.event("accept_choice"); assert(#m.calls==0)
m.event("menu_open"); m.event("choose", 3); m.event("accept_choice")
assert(m.calls[1][1]=="session.reboot" and not m.fact.open)
assert(m.text.status:find("completion is not reported",1,true))
m.event("menu_open"); m.event("choose",1)
assert(m.calls[2][1]=="say" and m.calls[2][2]=="tohru-lock" and m.calls[2][3]=="emit lock_request")
m.event("menu_open"); m.event("choose",2)
assert(m.calls[3][3]=="emit lock_and_suspend")
m.event("menu_open"); m.fail(true); m.event("choose",1)
assert(m.fact.open and m.text.error:find("rejected"))
m.fail(false); m.event("choose",4); m.fact.open=false; m.event("fact:open",false); m.event("accept_choice")
assert(#m.calls==3)
for index,command in pairs({[4]="session.poweroff",[5]="session.logout"}) do
    m.event("menu_open");m.event("choose",index);m.event("accept_choice")
    assert(m.calls[#m.calls][1]==command)
end
local function locker(held)
    return boot("shell/lock/lock.luau", {engaged=held, acknowledged=held, checking=false, sleep_submitted=false}, {password="old secret"})
end
local l=locker(false)
l.event("lock_and_suspend"); assert(l.fact.engaged and #l.calls==0)
l.advance(5000); l.held(true);l.fact.acknowledged=true; l.event("fact:acknowledged",true)
assert(#l.calls==0 and l.text.status:find("cancelled"))
l.event("lock_and_suspend"); assert(#l.calls==1 and l.calls[1][1]=="session.suspend")
assert(l.fact.engaged)
l.event("submit:password","bad"); assert(l.fact.engaged and l.text.password=="")
l.fail(true); l.event("submit:password","bad"); assert(l.fact.engaged and l.text.status:find("unavailable"))
l.fail(false); l.auth(true); l.event("submit:password","good"); assert(not l.fact.engaged)
l=locker(false);l.event("lock_and_suspend");l.event("key","Escape");l.held(true);l.fact.acknowledged=true;l.event("fact:acknowledged",true)
assert(l.fact.engaged and #l.calls==0)
l=locker(true);assert(l.fact.engaged and l.text.password=="") -- logic reload preserves lock
l.event("lock_and_suspend");assert(l.fact.engaged and #l.calls==1)
l=locker(false);l.auth(true);l.event("submit:password","good");assert(not l.fact.engaged and #l.calls==0)
l.event("lock_and_suspend");l.event("lock_request");l.held(true);l.fact.acknowledged=true;l.event("fact:acknowledged",true);assert(#l.calls==0)
-- Queued sleep behind PAM cannot reuse the released cycle's acknowledgment.
l=locker(true);l.auth(true);l.event("submit:password","good")
l.event("lock_and_suspend");assert(#l.calls==0 and not l.fact.engaged)
l.event("fact:acknowledged",true);assert(#l.calls==0)
l.held(false);l.fact.acknowledged=false;l.event("fact:acknowledged",false)
assert(l.fact.engaged and #l.calls==0)
l.held(true);l.fact.acknowledged=true;l.event("fact:acknowledged",true)
assert(#l.calls==1 and l.fact.sleep_submitted)
l.event("lock_and_suspend");l.event("lock_and_suspend");assert(#l.calls==1)
l.event("system_resumed");l.event("lock_and_suspend");assert(#l.calls==2)
-- Native spawn rejection permits an explicit retry without claiming completion.
l=locker(true);l.fail(true);l.event("lock_and_suspend");assert(not l.fact.sleep_submitted)
l.fail(false);l.event("lock_and_suspend");assert(#l.calls==1)
-- Reexecuting logic while submitted cannot issue another sleep.
-- A public engaged=false write during a held cycle must not leave it unable to unlock.
l=locker(true);l.fact.engaged=false;l.event("lock_request");assert(l.fact.engaged)
l.auth(true);l.event("submit:password","good");assert(not l.fact.engaged)
local retained=boot("shell/lock/lock.luau",{engaged=true,acknowledged=true,checking=false,sleep_submitted=true},{})
retained.event("lock_and_suspend");assert(#retained.calls==0)
local p=boot("shell/polkit/polkit.luau",{}, {answer="secret"})
p.event("submit:answer","bad");assert(#p.calls==0 and p.text.answer=="")
p.report("polkit",{pending=true,asking=true})
p.event("submit:answer","bad");assert(p.calls[1][1]=="polkit.answer" and p.calls[1][2]=="bad")
p.event("submit:answer","duplicate");assert(#p.calls==1)
p.report("polkit",{pending=true,asking=true,error="wrong"})
p.event("key","Escape");assert(p.calls[2][1]=="polkit.cancel" and p.text.answer=="")
p.report("polkit",{pending=false,asking=false});p.event("cancel_request");assert(#p.calls==2)
p.report("polkit",{pending=true,asking=true});p.fail(true);p.event("submit:answer","bad")
assert(p.text.status:find("rejected") and p.text.answer=="")
print("session, lock and polkit entry logic: passed")

-- Public mirror spoof cannot authorize native suspend or release.
l=locker(false);l.fact.engaged=true;l.fact.acknowledged=true
l.event("fact:acknowledged",true);l.event("lock_and_suspend")
assert(#l.calls==0)
l.auth(true);l.event("submit:password","irrelevant")
assert(l.fact.engaged and #l.calls==0)

l=locker(false);l.fact.acknowledged=true;l.event("state_request")
assert(l.hints[#l.hints]=="emit idle_lock_state 0")
l.held(true);l.event("state_request")
assert(l.hints[#l.hints]=="emit idle_lock_state 1")
