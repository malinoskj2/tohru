-- Event lifetime belongs to visibility, independent of the voice turn and the
-- local puppet timeline. Every entrance takes exactly one route decision.
return function(paths, puppet, make_actor)
    local enabled, seed, force = false, 1, "auto"
    local width, height = 0, 0
    local serial, event, last, next_at = 0, 0, nil, 0
    local timer, process, plan, interruption, pending
    local started, sceneKey, waiting, release_at, cycles, landing_time
    local PREPARE, LANDING = 0.20, 0.76
    local hearing, fullscreen = false, false
    local outputs, focused, selected = {}, "", ""
    local actor, stop_actor, actor_active
    local function place(keep)
        local next_output = outputs[1] or ""
        for _, name in ipairs(outputs) do
            if keep and name == selected then
                next_output = selected
                break
            end
            if name == focused then
                next_output = name
            end
        end
        selected = next_output
        -- Keep the current surface through a visible config change. At the next
        -- entrance, reduced/disabled mode requests only the original compact surface.
        local use = keep and fact.ai_use_flight_surface or not keep and enabled and not fact.ai_dg_reduced
        text.ai_flight_output = use and selected or ""
    end
    local function blocked(quiet)
        return (fact.ai_dg_reduced and not quiet)
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
            puppet.end_entrance()
        end
        pending, plan, interruption, sceneKey, waiting = nil, nil, nil, nil, nil
        fact.ai_flying = false
        fact.ai_flight_behind = false
        fact.ai_flight_alpha = 1
        fact.ai_flight_bank = 0
        fact.ai_flight_altitude = 0
        fact.ai_flight_phase = 0
    end
    local function interrupt()
        if stop_actor then
            stop_actor("activity")
        end
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
                puppet.end_entrance()
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
        local elapsed = math.max(0, now - started)
        local travel = math.max(0, elapsed - PREPARE)
        local p = paths.sample(plan, travel)
        -- The pose and first wing frame are published at the start position
        -- before translation. End on the same registered key for landing.
        local landing = math.min(1, math.max(0, (travel - release_at) / landing_time))
        if travel < release_at then
            puppet.wingbeat(cycles * travel / release_at)
        elseif not puppet.land(landing) then
            interrupt()
            return
        end
        fact.ai_flight_alpha = math.min(1, elapsed / PREPARE)
        fact.ai_flight_phase = p.phase
        fact.ai_flight_x = p.x
        fact.ai_flight_y = p.y
        fact.ai_flight_bank = p.bank
        fact.ai_flight_altitude = p.altitude
        if p.phase >= 1 and landing >= 1 then
            reset()
        end
    end
    local function begin(scene)
        if not pending or blocked() then
            reset()
            return
        end
        if fact.ai_dg_warm then
            local mine = serial
            after(50, function()
                if mine == serial and pending then begin(scene) end
            end)
            return
        end
        local s = { w = width, h = height, blocked = scene.blocked }
        if
            scene.output == selected
            and scene.w
            and math.abs(scene.w - width) <= 1
            and math.abs(scene.h - height) <= 1
        then
            s.target = scene.target
        end
        local kind = paths.choose(s, seed, event + 1, last, force)
        if not kind then
            reset()
            return
        end
        local candidate = paths.plan(s, kind)
        if not puppet.begin_entrance(kind, paths.sample(candidate, 0).direction) then
            reset()
            return
        end
        landing_time = puppet.landing_seconds and puppet.landing_seconds() or LANDING
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
        plan, started, pending = candidate, os.clock(), false
        release_at = plan.duration * 0.80
        cycles = math.max(1, math.floor(release_at / 0.42 + 0.5))
        fact.ai_flying = true
        fact.ai_flight_behind = kind == "wrap"
        fact.ai_flight_alpha = 1
        update()
        timer = every(16, update)
    end
    local function show()
        if stop_actor then
            stop_actor("voice")
        end
        fact.ai_actor_released = false
        place(false)
        fact.ai_use_flight_surface = enabled and not fact.ai_dg_reduced and selected ~= ""
        if selected == "" or not enabled or blocked() or os.clock() < next_at then
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
                sceneKey, waiting = raw, s
                if
                    width > 0
                    and height > 0
                    and (
                        s.output ~= selected
                        or (s.w and s.h and math.abs(s.w - width) <= 1 and math.abs(s.h - height) <= 1)
                    )
                then
                    begin(s)
                end
            elseif raw ~= sceneKey then
                interrupt()
            end
        end
        local ok, id = pcall(spawn, "tohru-flight-scene", {}, receive, function()
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
                begin(waiting or {})
            end
        end)
    end
    local function resized()
        if pending then
            if
                waiting
                and waiting.output == selected
                and waiting.w
                and waiting.h
                and math.abs(waiting.w - width) <= 1
                and math.abs(waiting.h - height) <= 1
            then
                begin(waiting)
            end
        elseif not actor_active or not actor_active() then
            interrupt()
        end
        -- External preparation may map the first output surface. Its lease
        -- verifies the configured bounds before readiness and on every tick.
    end
    on("dragon_view_width", function(n)
        if n ~= width then
            width = n
            resized()
        end
    end)
    on("dragon_view_height", function(n)
        if n ~= height then
            height = n
            resized()
        end
    end)

    if make_actor then
        actor, stop_actor, actor_active = make_actor(puppet, {
            busy = function(quiet)
                return blocked(quiet)
                    or pending
                    or plan
                    or fact.ai_dragon_open
                    or fact.ai_voice_conversation
                    or fact.ai_streaming
            end,
            dimensions = function()
                return width, height
            end,
            open = function()
                place(false)
                if selected ~= "" then
                    text.ai_flight_output = selected
                    fact.ai_use_flight_surface = true
                end
                return selected
            end,
            close = function()
                reset()
                place(false)
                fact.ai_use_flight_surface = enabled and not fact.ai_dg_reduced and selected ~= ""
            end,
        })
    end
    return {
        actor = actor,
        outputs = function(names)
            outputs = names
            local before = selected
            place(fact.ai_dragon_open or (actor_active and actor_active()))
            if before ~= selected then
                -- The old output is gone; none of its travel coordinates belong
                -- on the replacement. Settle there before accepting more samples.
                if stop_actor then
                    stop_actor("output")
                end
                reset()
                if selected == "" then
                    fact.ai_use_flight_surface = false
                end
            end
        end,
        show = show,
        hide = function()
            hearing = false
            if stop_actor then
                stop_actor("visibility")
            end
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
            focused = w.monitor or ""
            place(fact.ai_dragon_open or pending or plan or (actor_active and actor_active()))
            fullscreen = w.fullscreen == true and (focused == "" or focused == selected)
            if fullscreen then
                interrupt()
            end
        end,
        configure = function(value)
            enabled = value("assistant.contextual_entrances", true)
            seed = value("assistant.entrance_seed", 1)
            force = value("assistant.entrance_variant", "auto")
            if not fact.ai_dragon_open and not (actor_active and actor_active()) then
                place(false)
                fact.ai_use_flight_surface = enabled and not fact.ai_dg_reduced and selected ~= ""
            end
            if
                (fact.ai_dg_reduced and not (actor_active and actor_active(true)))
                or (not enabled and not (actor_active and actor_active()))
            then
                interrupt()
            end
        end,
    }
end
