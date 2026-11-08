-- Private native session-menu entry. No authentication/service owner lives here.
local pending = nil
local countdown = nil
local actions = { "lock", "sleep", "reboot", "poweroff", "logout" }
local labels = { "Lock Screen", "Sleep", "Restart", "Shut Down", "Log Out" }
local function clear()
    if countdown then cancel(countdown); countdown = nil end
    pending = nil
    fact.confirming = 0
    fact.remaining = 0
end
local function close()
    clear()
    fact.open = false
end
local function execute(action)
    clear()
    text.error = ""
    local ok, err = pcall(function()
        if action == "lock" or action == "sleep" then
            sys.call("say", "tohru-lock", "emit " .. (action == "sleep" and "lock_and_suspend" or "lock_request"))
        else
            sys.call("session." .. action)
        end
    end)
    if ok then
        text.status = "Request accepted; completion is not reported"
        fact.open = false
    else
        text.status = "Request rejected"
        text.error = tostring(err)
    end
end
local function choose(index)
    if not fact.open or not actions[index] then return end
    clear()
    fact.selected = index
    if index < 3 then execute(actions[index]); return end
    pending = actions[index]
    fact.confirming = index
    fact.remaining = 60
    text.prompt = labels[index] .. " now?"
    text.accept = labels[index]
    countdown = every(1000, function()
        fact.remaining = fact.remaining - 1
        if fact.remaining <= 0 then clear() end
    end)
end
-- Reload cancels an unfinished confirmation; never resumes destructive intent.
clear()
on("menu_open", function() clear(); text.error = ""; fact.selected = 1; fact.open = true end)
on("menu_close", close)
on("choose", choose)
on("cancel_choice", clear)
on("accept_choice", function() if pending then execute(pending) end end)
on("fact:open", function(open) if not open then clear() end end)
on("key", function(key)
    if not fact.open then return end
    if key == "Escape" then
        if pending then clear() else close() end
    elseif key == "Down" then fact.selected = fact.selected % 5 + 1
    elseif key == "Up" then fact.selected = (fact.selected + 3) % 5 + 1
    elseif key == "Return" then
        if pending then execute(pending) else choose(fact.selected) end
    end
end)
