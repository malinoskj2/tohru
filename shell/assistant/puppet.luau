-- The voice dragon's animator. It plays the sheets in dragon/ at 24 frames a second, and only while she is on screen.
return function()
    local TICK = 42
    local POSES = {
        neutral = {cell = 0, talk = 12, mx = 0, my = 0},
        explain = {cell = 4, talk = 13, mx = 0.59, my = 3.16},
        happy = {cell = 1, talk = 14, mx = -0.23, my = 2.11},
        scroll = {cell = 10, talk = 15, mx = -0.23, my = 1.76},
        cheer = {cell = 11, talk = 16, mx = -0.12, my = 3.87},
        thinking = {cell = 5}, listening = {cell = 6}, laugh = {cell = 7}, pleased = {cell = 8}, surprised = {cell = 9},
    }
    -- Drawn transitions (first cell among the moves, frames), played backwards for the way back.
    local MOVES = {
        {"neutral", "explain", 0, 17}, {"explain", "cheer", 17, 17, heart = 9},
        {"neutral", "laugh", 34, 17}, {"neutral", "thinking", 51, 18},
    }
    -- Poses with no drawn in-between are swapped under a squash, from neutral or from the explaining pose.
    local POPS = {"listening", "pleased", "happy", "scroll", "surprised"}
    local edges = {}
    local function edge(a, b, step)
        edges[a] = edges[a] or {}
        edges[a][#edges[a] + 1] = {to = b, step = step}
    end
    for _, m in ipairs(MOVES) do
        edge(m[1], m[2], {kind = "move", first = m[3], count = m[4], back = false, to = m[2], heart = m.heart})
        edge(m[2], m[1], {kind = "move", first = m[3], count = m[4], back = true, to = m[1]})
    end
    for _, p in ipairs(POPS) do
        for _, hub in ipairs({"neutral", "explain"}) do
            edge(hub, p, {kind = "pop", to = p})
            edge(p, hub, {kind = "pop", to = hub})
        end
    end

    local function route(from, to)
        if from == to then return {} end
        local seen, queue = {[from] = true}, {{at = from, path = {}}}
        local head = 1
        while queue[head] do
            local node = queue[head]
            head = head + 1
            for _, e in ipairs(edges[node.at] or {}) do
                if not seen[e.to] then
                    local path = {table.unpack(node.path)}
                    path[#path + 1] = e.step
                    if e.to == to then return path end
                    seen[e.to] = true
                    queue[#queue + 1] = {at = e.to, path = path}
                end
            end
        end
        return {}
    end

    local shown = {}
    local function set(name, value)
        if shown[name] ~= value then
            shown[name] = value
            fact[name] = value
        end
    end

    local pose, target = "neutral", "neutral"
    local plan, step, frame = {}, nil, 0
    local phase = 0
    local talking, mouth_cell = false, 0
    local quiet_events, since_change, change_after = 0, 0, 72
    local pending, hold = nil, 0
    local blink_in, blink_at = 90, -1
    local flourish_in, flourish, flourish_at = 600, nil, 0
    local heart_t = -1
    local timer = nil

    local function rnd(a, b) return a + math.random() * (b - a) end
    local LOOK = {
        {eye = 2, tilt = -1.6, n = 26}, {eye = -1, tilt = 0, n = 8}, {eye = 3, tilt = 0.9, n = 22},
        {eye = -1, tilt = 0, n = 5}, {eye = 0, n = 1}, {eye = 1, n = 2}, {eye = 0, n = 1},
    }

    local function want(name)
        target = name
        if not timer then
            pose, plan, step = name, {}, nil
            phase = 0
        end
    end

    local function heart(t)
        for i, h in ipairs({{d = 0, x = 65.2, y = 65.8, s = 0.65, sway = 1}, {d = 0.2, x = 55.3, y = 75.2, s = 0.45, sway = -1}}) do
            local v = t - h.d
            if t < 0 or v < 0 or v > 1.4 then
                set("ai_dg_h" .. i .. "_a", 0)
            else
                local x = math.min(v / 0.28, 1) - 1
                local grow = 1 + 2.7 * x * x * x + 1.7 * x * x
                local u = 1 - math.min(v / 1.4, 1)
                set("ai_dg_h" .. i .. "_x", h.x + 7 * h.sway * math.sin(v * 5.5) * 150 / 640)
                set("ai_dg_h" .. i .. "_y", h.y - 70 * (1 - u * u) * 150 / 640)
                set("ai_dg_h" .. i .. "_s", h.s * grow)
                set("ai_dg_h" .. i .. "_a", math.min(v / 0.08, 1) * (1 - math.max(v - 0.9, 0) / 0.5))
            end
        end
    end

    -- body.webp holds the loop (0-95), the moves (96-164) and the poses (165-181).
    local FIRST = {[0] = 0, [1] = 96, [2] = 165}
    local function draw(sheet, cell, eye, ex, ey)
        set("ai_dg_sheet", sheet)
        set("ai_dg_cell", cell)
        set("ai_dg_frame", FIRST[sheet] + cell)
        set("ai_dg_eye", eye)
        set("ai_dg_ex", ex or 0)
        set("ai_dg_ey", ey or 0)
    end

    local function rest()
        local p = POSES[pose]
        local tilt, eye = 0, -1
        if flourish == "look" then
            local at = flourish_at
            for _, s in ipairs(LOOK) do
                if at < s.n then
                    eye = s.eye
                    tilt = s.tilt or 0
                    break
                end
                at = at - s.n
            end
        elseif blink_at >= 0 then
            eye = ({0, 1, 1, 0})[blink_at + 1]
        end
        set("ai_dg_tilt", tilt)
        if talking and p.talk then
            draw(2, p.talk, pose == "neutral" and eye or -1)
            set("ai_dg_mouth", mouth_cell)
            set("ai_dg_mx", p.mx)
            set("ai_dg_my", p.my)
        elseif pose == "neutral" and not talking then
            set("ai_dg_mouth", -1)
            if flourish == "flutter" then
                draw(0, 48 + flourish_at, -1)
            else
                local a = 2 * math.pi * phase / 48
                draw(0, phase, eye, 0.39 * math.sin(a - 2 * math.pi * 4 / 48), -1.47 * math.sin(a))
            end
        else
            set("ai_dg_mouth", -1)
            draw(2, p.cell, pose == "neutral" and eye or -1)
        end
    end

    local function begin_plan()
        plan = route(pose, target)
        step, frame = table.remove(plan, 1), 0
        flourish = nil
    end

    local function tick()
        since_change = since_change + 1
        if heart_t >= 0 then
            heart_t = heart_t + TICK / 1000
            if heart_t > 1.6 then heart_t = -1 end
            heart(heart_t)
        end
        if hold > 0 and not step then
            hold = hold - 1
            if hold == 0 and target == pose then
                if pose == "laugh" then target = talking and "explain" or "neutral"
                elseif pose == "pleased" or pose == "listening" or pose == "surprised" then target = "neutral" end
            end
        end
        if talking and pending and pending ~= target and quiet_events >= 5 and since_change >= change_after and not step then
            target, pending = pending, nil
        end
        if not step and target ~= pose then
            local first = route(pose, target)[1]
            if flourish == "look" then flourish, flourish_at = nil, 0 end
            -- The idle breath and the flutter leave only where they match the neutral key, so a drawn move starts clean.
            local idling = pose == "neutral" and not talking
            local ready = not (idling and (flourish == "flutter" or first and first.kind == "move" and phase % 24 ~= 0))
            if first and ready then begin_plan() elseif not first then target = pose end
        end
        if step then
            if step.kind == "move" then
                local i = step.back and (step.count - 1 - frame) or frame
                set("ai_dg_mouth", -1)
                set("ai_dg_tilt", 0)
                draw(1, step.first + i, -1)
                if step.heart and frame == step.heart then heart_t = 0 end
                frame = frame + 1
                if frame >= step.count then
                    pose = step.to
                    step, frame = table.remove(plan, 1), 0
                end
            else
                local squash = ({0.35, 0.7, 1, -0.8, -0.4})[frame + 1]
                set("ai_dg_sq", squash or 0)
                if frame == 3 then pose = step.to end
                frame = frame + 1
                rest()
                if frame >= 5 then
                    set("ai_dg_sq", 0)
                    step, frame = table.remove(plan, 1), 0
                end
            end
            if not step then
                since_change = 0
                change_after = math.floor(rnd(60, 96))
                phase = 0
                if pose == "laugh" then hold = 30
                elseif pose == "pleased" then hold = 15 end
            end
            return
        end
        if pose == "neutral" and not talking then
            if flourish then
                flourish_at = flourish_at + 1
                if (flourish == "flutter" and flourish_at >= 48) or (flourish == "look" and flourish_at >= 65) then
                    flourish, flourish_at = nil, 0
                    flourish_in = math.floor(rnd(480, 960))
                    phase = 0
                end
            else
                phase = (phase + 1) % 48
                flourish_in = flourish_in - 1
                if flourish_in <= 0 and phase == 0 then
                    flourish, flourish_at = math.random() < 0.5 and "flutter" or "look", 0
                end
            end
        end
        if blink_at >= 0 then
            blink_at = blink_at + 1
            if blink_at > 3 then blink_at = -1 end
        else
            blink_in = blink_in - 1
            if blink_in <= 0 then
                blink_at, blink_in = 0, math.floor(rnd(72, 144))
            end
        end
        rest()
    end

    local function pick(sentence)
        local s = string.lower(sentence or "")
        if s:find("%f[%a]ha%s?ha") or s:find("%f[%a]he%s?he%f[%A]") or s:find("%f[%a]hah%f[%A]") then return "laugh" end
        if s:find("!") then return "cheer" end
        if s:find(":") or s:find(";") or s:find("first") or s:find("second") or s:find("then") and select(2, s:gsub(",", "")) >= 2 then return "scroll" end
        for _, w in ipairs({"glad", "love", "happy", "lovely", "wonderful", "great", "thank", "sweet", "nice", "welcome"}) do
            if s:find(w) then return "happy" end
        end
        return "explain"
    end

    return {
        load = function(dir)
            if (dir or "") == "" then
                for _, n in ipairs({"body", "eyes", "mouths", "heart"}) do text["ai_dg_" .. n .. "_src"] = "" end
                fact.ai_dg_ready = false
                return
            end
            for _, n in ipairs({"body", "eyes", "mouths"}) do
                text["ai_dg_" .. n .. "_src"] = dir .. "/" .. n .. ".webp"
            end
            text.ai_dg_heart_src = dir .. "/heart.png"
            fact.ai_dg_ready = true
            fact.ai_dg_warm = true
            after(3000, function() fact.ai_dg_warm = false end)
        end,
        visible = function(on)
            if on and not timer then
                timer = every(TICK, tick)
                tick()
            elseif not on and timer then
                cancel(timer)
                timer = nil
                heart_t = -1
                heart(-1)
                if step then
                    pose, plan, step = target, {}, nil
                end
            end
        end,
        hearing = function(on)
            if on and not talking then
                want("listening")
            elseif not on and target == "listening" then
                hold = 36
            end
        end,
        thinking = function()
            if not talking then want("thinking") end
        end,
        speaking = function(on, state)
            talking = on
            pending, quiet_events = nil, 0
            if on then
                hold = 0
                if not POSES[target].talk then want("explain") end
                since_change = 0
            else
                mouth_cell = 0
                if state == "paused" then
                    want("listening")
                elseif state == "interrupted" then
                    if target ~= "listening" and target ~= "thinking" then want("neutral") end
                else
                    want("pleased")
                end
            end
        end,
        sentence = function(text)
            if talking then pending = pick(text) end
        end,
        mouth = function(level, viseme)
            if level < 0.08 then quiet_events = quiet_events + 1 else quiet_events = 0 end
            if viseme then
                mouth_cell = viseme
            elseif level < 0.08 then mouth_cell = 0
            elseif level < 0.3 then mouth_cell = 4
            elseif level < 0.55 then mouth_cell = 3
            else mouth_cell = 2 end
            if timer and not step and talking and POSES[pose].talk then set("ai_dg_mouth", mouth_cell) end
        end,
        calm = function()
            talking, pending = false, nil
            want("neutral")
        end,
        pose = function() return pose, target end,
        pick = pick,
    }
end
