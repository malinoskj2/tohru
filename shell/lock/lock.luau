-- Requires the reviewed native lock boundary patch; production validation pending.
-- Dedicated locker. Native compositor acknowledgments belong to a lock cycle.
local sleep_pending = false
local readiness = nil
local ready = fact.engaged and fact.acknowledged
local releasing = not fact.engaged and fact.acknowledged
local queued_lock = false
local queued_sleep = false
local function cancel_sleep()
    sleep_pending = false
    if readiness then cancel(readiness); readiness = nil end
end
local function native_held()
    local ok, held = pcall(function() return sys.ask("lock.held") end)
    return ok and held == true
end
local function suspend_when_held()
    if not sleep_pending or not ready or releasing or fact.sleep_submitted or not native_held() then return end
    cancel_sleep()
    -- Retained across logic reload; reset only by successful authentication or
    -- an explicit `system_resumed` event.
    fact.sleep_submitted = true
    local ok, err = pcall(function() sys.call("session.suspend") end)
    if not ok then fact.sleep_submitted = false end
    text.status = ok and "Sleep request accepted; completion is not reported" or ("Sleep request rejected: " .. tostring(err))
end
local function lock(sleep)
    if fact.checking or fact.sleep_submitted then return end
    cancel_sleep()
    text.status = ""
    if releasing then
        -- Do not reassert open until the render reports the old lock released.
        queued_lock = true
        queued_sleep = sleep
        return
    end
    -- A public engaged=false write while the compositor still holds this cycle
    -- must not strand it unready: acknowledged would never change to restore it.
    if not fact.engaged and not native_held() then ready = false end
    sleep_pending = sleep
    fact.engaged = true
    if sleep then
        if ready then suspend_when_held(); return end
        readiness = after(5000, function()
            cancel_sleep()
            text.status = "Lock was not acknowledged; sleep cancelled"
        end)
    end
end
-- Reload cancels intent/secrets, preserving engaged and submitted-request state.
cancel_sleep()
text.password = ""
fact.checking = false
on("lock_request", function() lock(false) end)
on("lock_and_suspend", function() lock(true) end)
on("system_resumed", function()
    cancel_sleep()
    fact.sleep_submitted = false
end)
on("fact:acknowledged", function(held)
    if not held then
        ready = false
        cancel_sleep()
        if releasing then
            releasing = false
            local requested, sleep = queued_lock, queued_sleep
            queued_lock, queued_sleep = false, false
            if requested then lock(sleep) end
        end
        return
    end
    if releasing or not fact.engaged then return end
    ready = true
    focus("password")
    suspend_when_held()
end)
on("submit:password", function(password)
    if not native_held() or not ready or releasing or fact.checking then text.password = ""; return end
    cancel_sleep()
    fact.checking = true
    text.password = ""
    text.status = "Checking…"
    local ok, accepted = pcall(function() return sys.ask("auth.unlock", password) end)
    fact.checking = false
    if ok and accepted == true then
        -- Invalidate readiness BEFORE requesting release. A sleep request
        -- queued behind PAM must wait for release then a new held event.
        ready = false
        releasing = true
        fact.sleep_submitted = false
        text.status = ""
        fact.engaged = false
    else
        text.status = ok and "Password not accepted" or ("Authentication unavailable: " .. tostring(accepted))
        focus("password")
    end
end)
on("key", function(key)
    if key == "Escape" then
        cancel_sleep()
        queued_lock, queued_sleep = false, false
        text.password = ""
        text.status = ""
    end
end)
