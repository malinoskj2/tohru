-- Executes the actual Lua-compatible feature entry scripts with controlled native boundaries.
-- This CPU check does not establish compositor or Luau VM parity.
local function boot(path, facts, texts)
    local h, timers, calls, watches, runs = {}, {}, {}, {}, {}
    local now, failure, exit_code = 0, false, 0
    local env = setmetatable({ fact = facts, text = texts }, { __index = _G })
    env.on = function(name, fn) h[name] = fn end
    env.focus = function() end
    env.every = function(ms, fn) local t = {due=now+ms, step=ms, fn=fn}; timers[#timers+1]=t; return t end
    env.after = function(ms, fn) local t = {due=now+ms, fn=fn}; timers[#timers+1]=t; return t end
    env.cancel = function(t) t.cancelled=true end
    env.run = function(program, args, done)
        if failure then error("native request rejected") end
        runs[#runs+1] = {program, table.unpack(args)}
        if done then done("", exit_code) end
    end
    env.sys = {
        call = function(name, ...)
            if failure then error("native request rejected") end
            calls[#calls+1] = {name, ...}
        end,
        watch = function(name, fn) watches[name]=fn end,
    }
    assert(loadfile(path, "t", env))()
    return {
        fact=facts, text=texts, calls=calls, runs=runs,
        event=function(name, ...) assert(h[name], name)(...) end,
        report=function(name, value) watches[name](value) end,
        fail=function(value) failure=value end,
        exit=function(value) exit_code=value end,
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
assert(#m.calls==1 and #m.runs==1 and m.runs[1][1]=="loginctl" and m.runs[1][2]=="lock-session" and #m.runs[1]==2)
assert(not m.fact.open)
m.event("menu_open"); m.event("choose",2)
assert(#m.calls==1 and #m.runs==3)
assert(m.runs[2][1]=="loginctl" and m.runs[2][2]=="lock-session")
assert(m.runs[3][1]=="systemctl" and m.runs[3][2]=="suspend" and #m.runs[3]==2)
m.exit(1); m.event("menu_open"); m.event("choose",2)
assert(#m.runs==4 and m.runs[4][1]=="loginctl" and m.text.error:find("not suspending",1,true))
m.exit(0)
m.event("menu_open"); m.fail(true); m.event("choose",1)
assert(m.fact.open and m.text.error:find("rejected"))
m.event("choose",2); assert(m.fact.open and #m.runs==4)
m.fail(false); m.event("choose",4); m.fact.open=false; m.event("fact:open",false); m.event("accept_choice")
assert(#m.calls==1)
for index,command in pairs({[4]="session.poweroff",[5]="session.logout"}) do
    m.event("menu_open");m.event("choose",index);m.event("accept_choice")
    assert(m.calls[#m.calls][1]==command)
end
assert(#m.runs==4)
print("session menu entry logic: passed")
