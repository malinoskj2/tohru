-- The Sheet-B turn in dragon/turn-sheet-b.webp: cells 0-59 turn from profile right to the front,
-- 60-119 turn back. The sheet binds only while a turn may play: with assistant.sheet_b_turn on and the
-- voice art known, or during `tohru dragon turn`.
return function()
    local TICK, FRONT, LAST, DECODE = 42, 59, 119, 800
    local NAME = "turn-sheet-b.webp"
    local enabled, art = false, ""
    local timer, finished = nil, nil
    local standalone, waits = 0, {}

    local function show(frame)
        fact.ai_turn_frame = frame
        fact.ai_turn_art = true
    end

    local function halt()
        if timer then cancel(timer) end
        timer = nil
        fact.ai_turn_art = false
        local done = finished
        finished = nil
        return done
    end

    -- While bound, dragon.plm keeps one transparent pixel of the sheet drawn, so it is decoded before a turn needs it.
    local function bind(path)
        if text.ai_turn_src ~= path then text.ai_turn_src = path end
        fact.ai_turn_bound = path ~= ""
    end

    -- Losing the setting or the art finishes a pending turn-out at once, so a dismissal still hides her.
    local function refresh()
        if fact.ai_turn_open then return end
        local done
        if not (enabled and art ~= "") then done = halt() end
        bind(enabled and art or "")
        if done then done() end
    end

    local function wait(ms, mine, fn)
        local id
        id = after(ms, function()
            waits[id] = nil
            if mine == standalone then fn() end
        end)
        waits[id] = true
    end

    local function stop_standalone()
        if not (fact.ai_turn_open or next(waits)) then return end
        standalone = standalone + 1
        for id in pairs(waits) do cancel(id) end
        waits = {}
        fact.ai_turn_open, fact.ai_turn_shown, fact.ai_turn_warm = false, false, false
        fact.ai_turn_art = false
        refresh()
    end

    local function play(first, last, done)
        halt()
        show(first)
        finished = done
        local frame = first
        timer = every(TICK, function()
            if frame >= last then
                local callback = halt()
                if callback then callback() end
                return
            end
            frame = frame + 1
            fact.ai_turn_frame = frame
        end)
    end

    -- `tohru dragon turn`: the whole turn and back on its own, with no illustrated keys around it.
    on("assistant_dragon_turn", function()
        local path = text.ai_turn_request or ""
        if path == "" or timer or fact.ai_dragon_open or fact.ai_actor_open or fact.ai_turn_open then return end
        standalone = standalone + 1
        local mine = standalone
        -- A live image decodes once something draws it: draw it nearly transparent first, as the voice sheets do.
        bind(path)
        show(0)
        fact.ai_turn_open, fact.ai_turn_warm = true, true
        wait(DECODE, mine, function()
            fact.ai_turn_shown, fact.ai_turn_warm = true, false
        end)
        wait(DECODE + 500, mine, function()
            play(0, LAST, function()
                show(LAST)
                wait(400, mine, function()
                    fact.ai_turn_open, fact.ai_turn_shown = false, false
                    wait(600, mine, function()
                        if fact.ai_turn_open then return end
                        fact.ai_turn_art = false
                        refresh()
                    end)
                end)
            end)
        end)
    end)

    local M = {}
    M.FRONT = FRONT
    function M.configure(on)
        enabled = on == true
        refresh()
    end
    function M.art(dir)
        art = (dir or "") ~= "" and dir .. "/" .. NAME or ""
        refresh()
    end
    function M.ready() return enabled and art ~= "" and not fact.ai_turn_open end
    function M.frame(index) show(index) end
    function M.hide() fact.ai_turn_art = false end
    function M.playing() return timer ~= nil end
    -- Front to profile right, then `done`; false when the turn cannot play.
    function M.out(done)
        if not M.ready() or timer or fact.ai_flying or fact.ai_actor_open or fact.ai_dg_reduced then return false end
        play(FRONT + 1, LAST, done)
        return true
    end
    -- A voice appearance supersedes any turn: a pending turn-out's completion is dropped, not run.
    function M.cancel()
        halt()
        stop_standalone()
    end
    return M
end
