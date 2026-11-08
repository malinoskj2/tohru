-- Native monotonic timers only; root owns accepted settings and the rotate action.
-- Initial login rotates once after 1s. Later settings acceptance in login mode
-- cancels that initial timer, matching retained settings.apply behavior.
return function(rotate, notify, initial_change)
    if type(rotate) ~= "function" then error("Wallpaper rotation integration is unavailable") end
    local intervals = {never = 0, hourly = 3600000, daily = 86400000, login = 0}
    local timer, generation, closed = nil, 0, false
    local function clear()
        generation = generation + 1
        if timer then cancel(timer); timer = nil end
    end
    local function configure(change, initial)
        if closed then error("Wallpaper rotation owner is stopped") end
        if type(change) ~= "string" or intervals[change] == nil then error("Invalid wallpaper change policy") end
        clear()
        local own = generation
        local interval = intervals[change]
        local delay = initial and change == "login" and 1000 or interval
        local function schedule(ms)
            timer = after(ms, function()
                if closed or own ~= generation then return end
                timer = nil
                local ok, err = pcall(rotate)
                local failure
                if not ok then failure = tostring(err) end
                if notify then pcall(notify, failure) end
                -- The rotate/result callback may accept another configuration
                -- or stop this owner; it must not revive this old schedule.
                if not closed and own == generation and interval > 0 then schedule(interval) end
            end)
        end
        if delay > 0 then schedule(delay) end
    end
    configure(initial_change or "never", true)
    return {
        configure = function(change) configure(change, false) end,
        stop = function() clear(); closed = true end,
    }
end
