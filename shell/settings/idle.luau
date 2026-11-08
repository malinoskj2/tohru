-- Project the single accepted snapshot into the retained native owner.
-- Native start must itself be idempotent across replacement of this VM.
return function(idle)
    local accepted
    return function(settings)
        local config, changed = {}, accepted == nil
        for _, key in ipairs({"lock_seconds", "screen_off_unlocked_seconds", "screen_off_locked_seconds",
            "suspend_unlocked_seconds", "suspend_locked_seconds", "fade_milliseconds"}) do
            config[key] = settings["idle." .. key].value
            if accepted and accepted[key] ~= config[key] then changed = true end
        end
        if not changed then return true end
        if not idle.configure(config) then return false end
        accepted = config
        return true
    end
end
