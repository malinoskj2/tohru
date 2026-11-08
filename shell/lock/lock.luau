-- Dedicated locker only. Reload never writes engaged=false.
local sleep_pending = false
local readiness = nil
local function cancel_sleep()
    sleep_pending = false
    if readiness then cancel(readiness); readiness = nil end
end
local function suspend_when_held()
    if not sleep_pending or not fact.acknowledged then return end
    cancel_sleep()
    local ok, err = pcall(function() sys.call("session.suspend") end)
    text.status = ok and "Sleep request accepted; completion is not reported" or ("Sleep request rejected: " .. tostring(err))
end
local function lock(sleep)
    if fact.checking then return end
    cancel_sleep()
    text.status = ""
    sleep_pending = sleep
    fact.engaged = true
    if sleep then
        if fact.acknowledged then suspend_when_held(); return end
        readiness = after(5000, function()
            cancel_sleep()
            text.status = "Lock was not acknowledged; sleep cancelled"
        end)
    end
end
-- No destructive intent or password survives a logic reload.
cancel_sleep()
text.password = ""
fact.checking = false
on("lock_request", function() lock(false) end)
on("lock_and_suspend", function() lock(true) end)
on("fact:acknowledged", function(held)
    if held then focus("password"); suspend_when_held() else cancel_sleep() end
end)
on("submit:password", function(password)
    if not fact.engaged or not fact.acknowledged or fact.checking then text.password = ""; return end
    cancel_sleep()
    fact.checking = true
    text.password = ""
    text.status = "Checking…"
    local ok, accepted = pcall(function() return sys.ask("auth.check", password) end)
    fact.checking = false
    if ok and accepted == true then
        text.status = ""
        fact.engaged = false
    else
        text.status = ok and "Password not accepted" or ("Authentication unavailable: " .. tostring(accepted))
        focus("password")
    end
end)
on("key", function(key)
    if key == "Escape" then cancel_sleep(); text.password = ""; text.status = "" end
end)
