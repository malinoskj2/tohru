-- Event lifetime belongs to visibility, independent of the voice turn and the
-- local puppet timeline. Every entrance takes exactly one route decision.
return function(paths, puppet)
    local enabled, seed, force = false, 1, "auto"
    local width, height = 0, 0
    local serial, event, last, next_at = 0, 0, nil, 0
    local timer, process, plan, interruption, pending
    local started, sceneKey
    local hearing, fullscreen = false, false
    local function blocked()
        return fact.ai_dg_reduced
            or fact.notices_dnd
            or fact.ai_open
            or fact.ai_menu_open
            or fact.ai_speaking
            or fact.ai_voice_paused
            or hearing
            or fullscreen
    end
    local function reset()
        serial = serial + 1
        if timer then
            cancel(timer)
            timer = nil
        end
        if process then
            kill(process, "term")
            process = nil
        end
        if plan then
            puppet.flight(nil)
        end
        pending, plan, interruption, sceneKey = nil, nil, nil, nil
        fact.ai_flying = false
        fact.ai_flight_behind = false
        fact.ai_flight_alpha = 1
        fact.ai_flight_bank = 0
        fact.ai_flight_altitude = 0
    end
    local function interrupt()
        if pending then
            reset()
        elseif plan and not interruption then
            interruption = os.clock()
        end
    end
    local function update()
        local now = os.clock()
        if blocked() then
            interrupt()
        end
        if not plan then
            return
        end
        if interruption then
            local elapsed = now - interruption
            if elapsed < 0.08 then
                fact.ai_flight_alpha = math.max(0, 1 - elapsed / 0.08)
            else
                fact.ai_flying = false
                fact.ai_flight_behind = false
                fact.ai_flight_bank = 0
                fact.ai_flight_altitude = 0
                fact.ai_flight_alpha = math.min(1, (elapsed - 0.08) / 0.14)
                if elapsed >= 0.22 then
                    reset()
                end
            end
            return
        end
        local p = paths.sample(plan, now - started)
        fact.ai_flight_x = p.x
        fact.ai_flight_y = p.y
        fact.ai_flight_bank = p.bank
        fact.ai_flight_altitude = p.altitude
        if p.phase >= 1 then
            reset()
        end
    end
    local function begin(scene)
        if not pending or blocked() then
            reset()
            return
        end
        local s = { w = width, h = height, blocked = scene.blocked }
        if scene.w and math.abs(scene.w - width) <= 1 and math.abs(scene.h - height) <= 1 then
            s.target = scene.target
        end
        local kind = paths.choose(s, seed, event + 1, last, force)
        if not kind then
            reset()
            return
        end
        text.ai_flight_trace = json.encode({
            event = event + 1,
            seed = seed,
            previous = last,
            kind = kind,
            scene = scene,
            width = width,
            height = height,
        })
        event, last, next_at = event + 1, kind, os.clock() + 30
        plan, started, pending = paths.plan(s, kind), os.clock(), false
        puppet.flight(kind)
        fact.ai_flying = true
        fact.ai_flight_behind = kind == "wrap"
        fact.ai_flight_alpha = 1
        update()
        timer = every(16, update)
    end
    local function show()
        fact.ai_use_flight_surface = enabled and not fact.ai_dg_reduced
        if not enabled or blocked() or os.clock() < next_at then
            return
        end
        reset()
        local mine = serial
        pending = true
        fact.ai_flight_alpha = 0
        local function receive(raw)
            if mine ~= serial then
                return
            end
            local ok, s = pcall(json.decode, raw)
            if not ok or type(s) ~= "table" then
                interrupt()
                return
            end
            if pending then
                sceneKey = raw
                begin(s)
            elseif raw ~= sceneKey then
                interrupt()
            end
        end
        local ok, id = pcall(spawn, "tohru_flight_scene.py", {}, receive, function()
            if mine ~= serial then
                return
            end
            process = nil
            if pending then
                begin({})
            elseif plan and plan.kind == "wrap" then
                interrupt()
            end
        end)
        if ok then
            process = id
        else
            begin({})
        end
        after(360, function()
            if mine == serial and pending then
                begin({})
            end
        end)
    end
    on("dragon_view_width", function(n)
        if n ~= width then
            width = n
            interrupt()
        end
    end)
    on("dragon_view_height", function(n)
        if n ~= height then
            height = n
            interrupt()
        end
    end)

    return {
        show = show,
        hide = function()
            hearing = false
            reset()
        end,
        interrupt = interrupt,
        hearing = function(on)
            hearing = on
            if on then
                interrupt()
            end
        end,
        window = function(w)
            fullscreen = w.fullscreen == true
            if fullscreen then
                interrupt()
            end
        end,
        configure = function(value)
            enabled = value("assistant.contextual_entrances", false)
            seed = value("assistant.entrance_seed", 1)
            force = value("assistant.entrance_variant", "auto")
            if not fact.ai_dragon_open then
                fact.ai_use_flight_surface = enabled and not fact.ai_dg_reduced
            end
            if not enabled or fact.ai_dg_reduced then
                interrupt()
            end
        end,
    }
end
