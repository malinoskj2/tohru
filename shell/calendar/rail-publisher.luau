-- One native timer per moving rail. Physics stays in the process-owned Rust binding.
return function(binding, role, prefix)
    local timer
    local generation = 0
    local function stop()
        generation = generation + 1
        if timer then cancel(timer); timer = nil end
        fact[prefix .. "moving"] = false
    end
    local tick
    local function publish(sample)
        for index = 1, 16 do fact[prefix .. "v." .. (index - 1)] = sample.shape[index] end
        fact[prefix .. "r"], fact[prefix .. "g"], fact[prefix .. "b"] = sample.color[1] * 255, sample.color[2] * 255, sample.color[3] * 255
        fact[prefix .. "present"] = sample.color[4] > 0
        fact[prefix .. "width"] = sample.width
        fact[prefix .. "moving"] = sample.moving
        if sample.moving then
            if not timer then
                local owner = generation
                timer = every(1000 / 60, function()
                    if timer and owner == generation then tick() end
                end)
            end
        else stop() end
    end
    local function invoke(method, ...)
        local ok, sample = pcall(method, ...)
        if not ok then text[prefix .. "error"] = tostring(sample); stop(); return false end
        text[prefix .. "error"] = ""
        publish(sample)
        return true
    end
    tick = function() invoke(binding.tick, role) end
    return {
        observe = function(highest, active) return invoke(binding.observe, role, highest, active) end,
        reset = function() stop(); return invoke(binding.reset, role) end,
        stop = stop,
        active = function() return timer ~= nil end,
    }
end
