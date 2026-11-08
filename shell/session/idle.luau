-- Private ordinary-root entry. Native process owner survives this VM's reload.
local function request(fn)
    local ok, message = pcall(fn)
    if not ok then text.idle_status = "Idle unavailable: " .. tostring(message) end
    return ok
end
on("idle_lock_state", function(held)
    if held == 0 or held == 1 then
        request(function() tohru_idle.held(held == 1) end)
    end
end)
on("idle_resume", function() request(function() tohru_idle.resume() end) end)
on("idle_caffeine_toggle", function()
    local enabled = not fact.idle_caffeine
    if request(function() tohru_idle.caffeine(enabled) end) then fact.idle_caffeine = enabled end
end)
return {
    configure = function(config)
        return request(function() tohru_idle.start(config) end)
    end,
}
