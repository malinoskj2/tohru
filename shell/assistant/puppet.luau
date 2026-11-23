-- The voice dragon's animator. It plays the sheets in dragon/ at 24 frames a second, and only while she is on screen.
return function(full_body, landing_frames)
    local TICK = 42
    -- Compact silhouettes need more room to match the original neutral.
    local POSE_SCALE = {
        loose_hover = 1.10, nested_hover = 1.25, reclined_hover = 1.25,
        high_knee = 1.10, crossed_feet = 1.25, lean_back = 1.20,
        side_float = 1.25, asymmetric_curl = 1.12,
        ponder_hover = 1.20, attentive_hover = 1.20, right_bank = 1.20,
        forward_bank = 1.18, hip_twist = 1.18, open_hover = 1.15,
        bright_hover = 1.20, belly_hover = 1.10, long_hover = 1.10,
    }
    local frame_scale = {}
    local POSES = {
        neutral = {cell = 0, talk = 12, mx = 0, my = 0, ex = 0, ey = 0},
        explain = {cell = 4, talk = 13, mx = 0.59, my = 3.16},
        happy = {cell = 1, talk = 14, mx = -0.23, my = 2.11},
        scroll = {cell = 10, talk = 15, mx = -0.23, my = 1.76},
        cheer = {cell = 11, talk = 16, mx = -0.12, my = 3.87},
        thinking = {cell = 5}, listening = {cell = 6}, laugh = {cell = 7}, pleased = {cell = 8}, surprised = {cell = 9},
        head_tilt = {cell = 17, talk = 27, mx = -0.3, my = -2.5},
        lean_in = {cell = 18, talk = 28, mx = -10.35, my = 6.1, ex = -10.1, ey = 5.7},
        right_gesture = {cell = 19, talk = 29, mx = 0, my = 0, ex = 0, ey = 0},
        shrug = {cell = 20, talk = 30, mx = 0, my = 0, ex = 0, ey = 0},
        wing_lift = {cell = 21, talk = 31, mx = 0, my = 0, ex = 0, ey = 0},
        count_one = {cell = 22, talk = 32, mx = 0, my = 0, ex = 0, ey = 0},
        soft_nod = {cell = 23, talk = 33, mx = 0.5, my = 7.9, ex = 0.25, ey = 7.8},
        recall_glance = {cell = 24, talk = 34, mx = -1.2, my = 0.9},
        open_palms = {cell = 25, talk = 35, mx = 0, my = 0, ex = 0, ey = 0},
        chest_paw = {cell = 26, talk = 36, mx = 0, my = 0, ex = 0, ey = 0},
    }
    for _, p in ipairs(full_body or {}) do
        POSES[p.name] = {cell = p.cell, talk = p.cell, full = true, face = p.face, fx = p.fx, fy = p.fy, shadow = p.shadow}
        local scale = POSE_SCALE[p.name]
        if scale then
            frame_scale[165 + p.cell] = scale
            for i = 0, p.count - 1 do
                local t = i / (p.count - 1)
                frame_scale[p.first + i] = 1 + (scale - 1) * t * t * (3 - 2 * t)
            end
        end
    end
    -- Drawn transitions (first cell among the moves, frames), played backwards for the way back.
    local MOVES = {
        {"neutral", "explain", 0, 17}, {"explain", "cheer", 17, 17, heart = 9},
        {"neutral", "laugh", 34, 17}, {"neutral", "thinking", 51, 18},
    }
    for _, p in ipairs(full_body or {}) do
        MOVES[#MOVES + 1] = {"neutral", p.name, p.first - 96, p.count, lip = p}
    end
    -- Poses with no drawn in-between swap under a squash from their registered hub poses.
    local POPS = {
        {"listening", {"neutral", "explain"}},
        {"pleased", {"neutral", "explain"}},
        {"happy", {"neutral", "explain"}},
        {"scroll", {"neutral", "explain"}},
        {"surprised", {"neutral", "explain"}},
        {"head_tilt", {"neutral"}},
        {"lean_in", {"neutral", "listening"}},
        {"right_gesture", {"neutral", "explain"}},
        {"shrug", {"neutral", "explain"}},
        {"wing_lift", {"neutral", "pleased"}},
        {"count_one", {"explain", "neutral"}},
        {"soft_nod", {"neutral", "pleased"}},
        {"recall_glance", {"neutral", "thinking"}},
        {"open_palms", {"explain", "scroll"}},
        {"chest_paw", {"neutral", "happy"}},
    }
    local edges = {}
    local function edge(a, b, step)
        edges[a] = edges[a] or {}
        edges[a][#edges[a] + 1] = {to = b, step = step}
    end
    for _, m in ipairs(MOVES) do
        edge(m[1], m[2], {kind = "move", first = m[3], count = m[4], back = false, to = m[2], heart = m.heart, lip = m.lip})
        edge(m[2], m[1], {kind = "move", first = m[3], count = m[4], back = true, to = m[1], lip = m.lip})
    end
    for _, item in ipairs(POPS) do
        local p, hubs = item[1], item[2]
        for _, hub in ipairs(hubs) do
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
    local load_generation = 0
    local reduced, mode = false, "neutral"
    local posture_age, posture_after = 0, 240
    local flight_row, held_pose, entrance, touchdown
    local PIXEL_SCALE = 150 / 240
    local LANDING = {ponder_hover=489, attentive_hover=507, neutral=0}
    local voice_art, leased_art, current_art = "", "", ""
    local FLIGHT_POSES = {left_glide = 0, right_glide = 1, rising_bank = 2}
    local recent = {}
    local function remember(name)
        if recent[#recent] == name then return end
        recent[#recent + 1] = name
        if #recent > 3 then table.remove(recent, 1) end
    end
    local REST = {"loose_hover", "nested_hover", "reclined_hover", "high_knee", "crossed_feet", "lean_back", "side_float", "asymmetric_curl"}
    local WAIT = {"attentive_hover", "right_bank", "forward_bank", "loose_hover", "crossed_feet"}
    local THINK = {"ponder_hover", "high_knee", "hip_twist", "nested_hover"}
    local DRIFT = {"left_glide", "rising_bank", "belly_hover", "long_hover"}

    local function next_posture(settling)
        local source = mode == "listening" and WAIT or mode == "thinking" and THINK or REST
        -- A spacious drift is occasional; it is followed by a longer quiet hold.
        if mode == "neutral" and not settling and math.random() < 0.18 then source = DRIFT end
        local pool = {}
        for _, name in ipairs(source) do
            local skip = name == pose
            for _, previous in ipairs(recent) do skip = skip or name == previous end
            if POSES[name] and not skip then pool[#pool + 1] = name end
        end
        if #pool == 0 then return end
        local name = pool[math.random(#pool)]
        remember(name)
        target = name
        posture_age = 0
        posture_after = math.random(mode == "neutral" and 288 or 216, mode == "neutral" and 600 or 432)
    end

    local function rnd(a, b) return a + math.random() * (b - a) end
    local LOOK = {
        {eye = 2, tilt = -1.6, n = 26}, {eye = -1, tilt = 0, n = 8}, {eye = 3, tilt = 0.9, n = 22},
        {eye = -1, tilt = 0, n = 5}, {eye = 0, n = 1}, {eye = 1, n = 2}, {eye = 0, n = 1},
    }

    local function want(name)
        target = reduced and "neutral" or name
        if not reduced and POSES[name].full then remember(name) end
        posture_age = 0
        if reduced then
            pose, target, plan, step = "neutral", "neutral", {}, nil
            return
        end
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

    -- body.webp retains the legacy cells (0-201); full-body metadata adds registered moves/keys after them.
    local FIRST = {[0] = 0, [1] = 96, [2] = 165}
    local function draw(sheet, cell, eye, ex, ey, face)
        set("ai_dg_sheet", sheet)
        set("ai_dg_face", face or -1)
        set("ai_dg_travel", -1)
        set("ai_dg_cell", cell)
        set("ai_dg_frame", FIRST[sheet] + cell)
        set("ai_dg_pose_scale", frame_scale[FIRST[sheet] + cell] or 1)
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
        set("ai_dg_tilt", reduced and 0 or tilt)
        set("ai_dg_shadow", p.shadow or 1)
        if p.full then
            local blink = blink_at >= 0 and ({1, 2, 2, 1})[blink_at + 1] or 0
            draw(2, p.cell, -1, 0, 0, p.face * 24 + blink * 8 + (talking and mouth_cell or 0))
            set("ai_dg_mouth", -1)
            set("ai_dg_fx", p.fx)
            set("ai_dg_fy", p.fy)
        elseif reduced then
            draw(2, talking and p.talk or p.cell, eye, p.ex, p.ey)
            set("ai_dg_mouth", talking and mouth_cell or -1)
            set("ai_dg_mx", 0)
            set("ai_dg_my", 0)
        elseif talking and p.talk then
            draw(2, p.talk, p.ex and eye or -1, p.ex, p.ey)
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
            draw(2, p.cell, p.ex and eye or -1, p.ex, p.ey)
        end
    end

    local function begin_plan()
        plan = route(pose, target)
        step, frame = table.remove(plan, 1), 0
        flourish = nil
    end

    local function tick()
        if flight_row or held_pose or touchdown then return end -- The spatial owner samples these frames directly.
        since_change = since_change + 1
        if not reduced and not talking and not step and hold == 0 and target == pose then
            posture_age = posture_age + 1
            if posture_age >= posture_after then next_posture() end
        end
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
        if not reduced and talking and pending and pending ~= target and quiet_events >= 5 and since_change >= change_after and not step then
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
                if talking and step.lip then
                    local anchor = step.lip.mouths[i + 1]
                    set("ai_dg_travel", (step.lip.face * step.count + i) * 8 + mouth_cell)
                    set("ai_dg_tx", anchor[1])
                    set("ai_dg_ty", anchor[2])
                end
                if step.heart and frame == step.heart then heart_t = 0 end
                frame = frame + 1
                if frame >= step.count then
                    pose = step.to
                    if #plan > 0 and plan[#plan].to ~= target then plan = route(pose, target) end
                    step, frame = table.remove(plan, 1), 0
                end
            else
                local squash = ({0.35, 0.7, 1, -0.8, -0.4})[frame + 1]
                set("ai_dg_sq", squash or 0)
                if frame == 3 then pose = step.to end
                frame = frame + 1
                rest()
                if frame >= 5 then
                    if #plan > 0 and plan[#plan].to ~= target then plan = route(pose, target) end
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
        if not reduced and pose == "neutral" and not talking then
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

    local VARIETY = {"hip_twist", "right_glide", "open_hover", "reclined_hover", "loose_hover", "crossed_feet", "asymmetric_curl", "explain", "right_gesture", "open_palms", "wing_lift", "lean_in", "chest_paw"}
    local last_pick = nil

    local function any(s, patterns)
        for _, p in ipairs(patterns) do
            if s:find(p) then return true end
        end
        return false
    end

    local function pick(sentence, avoid)
        local s = string.lower(sentence or "")
        local chosen = nil
        if s:find("%f[%a]ha%s?ha") or s:find("%f[%a]he%s?he%f[%A]") or s:find("%f[%a]hah%f[%A]") then
            chosen = "laugh"
        elseif s:find("!") and any(s, {"%f[%a]wonderful", "%f[%a]great", "%f[%a]yay", "%f[%a]lovely", "%f[%a]delighted"}) then
            chosen = POSES.bright_hover and "bright_hover" or "cheer"
        elseif s:find("?") or any(s, {"%f[%a]curious%f[%A]", "%f[%a]wonder"}) then
            chosen = "head_tilt"
        elseif s:find(":") or s:find(";") or (select(2, s:gsub(",", "")) >= 2 and any(s, {"%f[%a]first%f[%A]", "%f[%a]second%f[%A]", "%f[%a]third%f[%A]", "%f[%a]then%f[%A]", "%f[%a]finally%f[%A]"})) then
            chosen = "scroll"
        elseif any(s, {"%f[%a]first%f[%A]", "%f[%a]second%f[%A]", "%f[%a]third%f[%A]", "one thing", "one more", "step one", "%f[%a]specifically%f[%A]", "to note", "point is"}) then
            chosen = "count_one"
        elseif any(s, {"%f[%a]depends", "%f[%a]either%f[%A]", "%f[%a]maybe%f[%A]", "%f[%a]perhaps%f[%A]", "might be", "could be", "not sure", "hard to say", "%f[%a]probably%f[%A]"}) then
            chosen = "shrug"
        elseif any(s, {"%f[%a]yes%f[%A]", "%f[%a]yeah%f[%A]", "%f[%a]yep%f[%A]", "^%s*right%f[%A]", "you'?re right", "that'?s right", "%f[%a]right,", "%f[%a]exactly%f[%A]", "%f[%a]indeed%f[%A]", "%f[%a]agreed?%f[%A]", "makes sense", "%f[%a]correct%f[%A]", "of course", "got it"}) then
            chosen = "soft_nod"
        elseif any(s, {"let'?s see", "i remember", "i recall", "back in", "last time", "%f[%a]hmm%f[%A]", "~/", "/[%w_%-%.]+/[%w_%-%.]+"}) then
            chosen = "recall_glance"
        elseif any(s, {"in short", "%f[%a]overall%f[%A]", "%f[%a]summar", "to sum up", "%f[%a]basically%f[%A]", "%f[%a]overview%f[%A]", "in general", "big picture", "fits? together", "in other words", "%f[%a]altogether%f[%A]", "%f[%a]essentially%f[%A]"}) then
            chosen = "open_palms"
        elseif any(s, {"on the other hand", "%f[%a]however%f[%A]", "%f[%a]instead%f[%A]", "%f[%a]otherwise%f[%A]", "%f[%a]alternatively%f[%A]", "compared to", "%f[%a]whereas%f[%A]", "another option", "another way", "%f[%a]versus%f[%A]", "%f[%a]vs%f[%A]"}) then
            chosen = "right_gesture"
        elseif any(s, {"%f[%a]honestly%f[%A]", "%f[%a]recommend", "i'?d suggest", "%f[%a]personally%f[%A]", "don'?t worry", "no worries", "i promise", "%f[%a]sorry%f[%A]", "%f[%a]apologize"}) then
            chosen = "chest_paw"
        elseif any(s, {"%f[%a]tips?%f[%A]", "%f[%a]trick%f[%A]", "fun fact", "%f[%a]interesting%f[%A]", "worth noting", "by the way", "keep in mind", "heads up", "%f[%a]handy%f[%A]", "watch out"}) then
            chosen = "lean_in"
        elseif any(s, {"speaking of", "next up", "moving on", "on top of that", "even better", "%f[%a]bonus%f[%A]", "good news", "%f[%a]notably%f[%A]", "%f[%a]also%f[%A]"}) then
            chosen = "wing_lift"
        elseif any(s, {"%f[%a]glad", "%f[%a]love", "%f[%a]happy", "%f[%a]lovely", "%f[%a]wonderful", "%f[%a]great", "%f[%a]thank", "%f[%a]sweet", "%f[%a]nice", "%f[%a]welcome"}) then
            chosen = "happy"
        else
            local skip = avoid or last_pick
            local pool = {}
            for _, p in ipairs(VARIETY) do
                if p ~= skip and POSES[p] then pool[#pool + 1] = p end
            end
            chosen = pool[math.random(#pool)]
        end
        last_pick = chosen
        return chosen
    end

    local function sync_art()
        local dir = voice_art ~= "" and voice_art or leased_art
        if current_art == dir then return end
        current_art = dir
        load_generation = load_generation + 1
        local generation = load_generation
        if (dir or "") == "" then
            for _, n in ipairs({"body", "eyes", "mouths", "heart", "faces", "travel"}) do text["ai_dg_" .. n .. "_src"] = "" end
            fact.ai_dg_ready = false
            fact.ai_dg_warm = false
            return
        end
        for _, n in ipairs({"body", "eyes", "mouths", "faces", "travel"}) do
            text["ai_dg_" .. n .. "_src"] = dir .. "/" .. n .. ".webp"
        end
        text.ai_dg_heart_src = dir .. "/heart.png"
        fact.ai_dg_ready = true
        fact.ai_dg_warm = true
        after(3000, function() if generation == load_generation then fact.ai_dg_warm = false end end)
    end

    local function flight(kind, direction)
        if not kind then
            if flight_row then
                flight_row = nil
                set("ai_flight_art", false)
                rest()
            end
            return
        end
        local name = kind == "rise" and "rising_bank" or direction < 0 and "left_glide" or "right_glide"
        -- A cold or absent voice atlas cannot support the handoff back to
        -- the registered key. Give this appearance its ordinary idle pose.
        if reduced or talking or not fact.ai_dg_ready or fact.ai_dg_warm or not POSES[name] then return false end
        flight_row = FLIGHT_POSES[name]
        -- A spatial scene owns the displayed key while voice retains its requested pose.
        pose, plan, step, flourish = name, {}, nil, nil
        heart_t = -1
        heart(-1)
        rest()
        set("ai_flight_cell", flight_row * 12)
        set("ai_flight_art", true)
        return true
    end

    return {
        load = function(dir)
            voice_art = dir or ""
            sync_art()
        end,
        lease_art = function(dir)
            leased_art = dir or ""
            sync_art()
        end,
        hold_pose = function(name, quiet)
            if not name then
                held_pose = nil
                return true
            end
            if (reduced and not quiet) or talking or not POSES[name] or not fact.ai_dg_ready or fact.ai_dg_warm then return false end
            held_pose = name
            pose, plan, step, flourish = name, {}, nil, nil
            heart_t = -1
            heart(-1)
            rest()
            return true
        end,
        flight = flight,
        begin_entrance = function(kind, direction)
            if not landing_frames or not flight(kind, direction) then return false end
            -- Alternate rests share the neutral landing; they have no separate
            -- turn-in strip and must not suppress the next appearance's flight.
            if LANDING[target] == nil then target = "neutral" end
            entrance = landing_frames[pose]
            return true
        end,
        land = function(t)
            local strip = entrance and entrance[target]
            if not strip or (touchdown and touchdown ~= target) then return false end
            if not touchdown then
                touchdown = target
                set("ai_flight_cell", flight_row * 12)
                flight_row = nil
                -- Pre-stage the actual conversational key under its registered
                -- last drawing. The ordinary route graph has no part in landing.
                pose, plan, step, flourish, phase = target, {}, nil, nil, 0
                set("ai_landing_target", LANDING[target])
                rest()
            end
            local index = math.floor((#strip - 1) * t + 0.5)
            local rect = strip[index + 1]
            set("ai_landing_frame", strip.row * #strip + index)
            set("ai_landing_x", rect[1] * PIXEL_SCALE)
            set("ai_landing_y", rect[2] * PIXEL_SCALE)
            set("ai_landing_w", rect[3] * PIXEL_SCALE)
            set("ai_landing_h", rect[4] * PIXEL_SCALE)
            set("ai_landing_scale", 1 + ((frame_scale[LANDING[target]] or 1) - 1) * t * t * (3 - 2 * t))
            -- Use the live, full-resolution endpoint in each direction.
            set("ai_flight_art", index == 0)
            set("ai_landing_art", index > 0 and index < #strip - 1)
            set("ai_landing_progress", t)
            return true
        end,
        end_entrance = function()
            if not entrance then return end
            entrance, touchdown, flight_row = nil, nil, nil
            pose, plan, step, flourish, phase = target, {}, nil, nil, 0
            posture_age, hold = 0, 0
            set("ai_flight_art", false)
            set("ai_landing_art", false)
            set("ai_landing_progress", 0)
            rest()
        end,
        wingbeat = function(cycle)
            if flight_row then set("ai_flight_cell", flight_row * 12 + math.floor(cycle * 12 + 0.5) % 12) end
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
        reduced = function(on)
            if reduced == on then return end
            reduced = on
            fact.ai_dg_reduced = on
            flourish, pending, hold = nil, nil, 0
            if on then
                pose, target, plan, step = "neutral", "neutral", {}, nil
                heart_t = -1
                heart(-1)
                set("ai_dg_sq", 0)
            end
            posture_age, posture_after = 0, 288
            if timer then rest() end
        end,
        hearing = function(on)
            if on and not talking then
                mode, hold = "listening", 0
                want(POSES.attentive_hover and "attentive_hover" or "listening")
            elseif not on and mode == "listening" then
                -- A VAD silence is still quiet listening; it need not reset her whole body.
                mode = "neutral"
                posture_age, posture_after = 0, 288
                if target == "listening" then hold = 36 end
            end
        end,
        thinking = function()
            if not talking then
                mode, hold = "thinking", 0
                local name = POSES.ponder_hover and "ponder_hover" or "thinking"
                want(name)
            end
        end,
        speaking = function(on, state)
            talking = on
            pending, quiet_events = nil, 0
            if on then
                mode, hold = "speaking", 0
                if not POSES[target].talk then want("explain") end
                since_change = 0
            else
                mouth_cell = 0
                if state == "paused" then
                    mode = "listening"
                    want(POSES.attentive_hover and "attentive_hover" or "listening")
                elseif state == "interrupted" then
                    if mode ~= "listening" and mode ~= "thinking" then mode = "neutral"; want("neutral") end
                else
                    mode = "neutral"
                    if POSES.nested_hover then next_posture(true) else want("pleased") end
                end
            end
        end,
        sentence = function(text)
            if talking and not reduced then pending = pick(text, target) end
        end,
        mouth = function(level, viseme)
            if level < 0.08 then quiet_events = quiet_events + 1 else quiet_events = 0 end
            if viseme then
                mouth_cell = math.max(0, math.min(7, math.floor(viseme)))
            elseif level < 0.08 then mouth_cell = 0
            elseif level < 0.3 then mouth_cell = 4
            elseif level < 0.55 then mouth_cell = 3
            else mouth_cell = 2 end
            if timer and not step and talking and POSES[pose].talk then
                if POSES[pose].full then rest() else set("ai_dg_mouth", mouth_cell) end
            end
        end,
        calm = function()
            talking, pending, mode, hold = false, nil, "neutral", 0
            want("neutral")
        end,
        pose = function() return pose, target end,
        poses = POSES,
        route = route,
        pick = pick,
    }
end
