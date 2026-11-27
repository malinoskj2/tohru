-- Rolling monitor captures exist only while she is on screen: the summoned dragon or the popup.
-- tohru-screens deletes them when stopped, when the bar's pipe closes, and on the next start.
return function()
    local id, generation = nil, 0
    local failures, wait = 0, 0
    pcall(spawn, "tohru-screens", {"clear"}, function() end, function() end)

    local function present()
        return fact.ai_dragon_open == true or fact.ai_open == true
    end

    local function stop()
        generation = generation + 1
        if id then kill(id, "term") end
        id = nil
    end

    local function start()
        generation = generation + 1
        local mine = generation
        local ok, pid = pcall(spawn, "tohru-screens", {"run"}, function() end, function()
            if mine ~= generation then return end
            id = nil
            failures = math.min(failures + 1, 6)
            wait = 2 ^ failures
        end, {stdin = "open"})
        if ok then id = pid else failures, wait = 6, 64 end
    end

    local function sync()
        local on = present()
        if not on then
            failures, wait = 0, 0
            if id then stop() end
        elseif not id and wait <= 0 then
            start()
        end
    end

    every(250, function()
        if wait > 0 then wait = wait - 0.25 end
        sync()
    end)

    return {sync = sync, running = function() return id ~= nil end}
end
