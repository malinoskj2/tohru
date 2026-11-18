-- j2bar's NeonField Fill track (src/kit/field.rs `Track`): the liquid springs to the caret and
-- every change of the text's length throws a crashing wave along the track. It steps at 240 Hz
-- while anything moves and stops at rest; each step's columns go to the scene in 3-column slices.
local INSET, AHEAD, AMP, WAVES, SPAN = 14, 8, 1.6, 10, 260
local STEP, SETTLE_ENERGY = 1 / 240, 6e-3
local function sq(v) return v * v end

return function(publish, clock, timer)
    local n, env = 0, {}
    local x, tx, vx, hh, hv, round, rv = 0, 0, 0, 2, 0, 0, 0
    local k, c = 170, math.sqrt(170)
    local waves, o, d = {}, {}, {}
    local running, owed, last, tick = false, 0, nil, nil
    local caret_px = 38

    local function roll(lo, hi) return lo + (hi - lo) * math.random() end
    local function fill_end()
        local s = math.max(0, math.min(38 - INSET - AHEAD, n))
        return s, math.max(s + 16, x + AHEAD + math.min(2, math.abs(vx) * 0.01))
    end
    local function emit(active)
        local s, e = fill_end()
        local slices = {}
        if active then
            for slice = 0, math.ceil(n / 3) - 1 do
                local row = {k = slice}
                for j = 0, 3 do
                    local i = slice * 3 + j
                    row["o" .. j], row["d" .. j] = o[i] or 0, d[i] or 0
                end
                slices[#slices + 1] = row
            end
        end
        publish({start = s, finish = e, swell = hh - 2, round = math.max(0, math.min(1, round)),
            glow = (9 + math.min(5, math.abs(vx) * 0.03)) * 0.5, span = math.min(n, SPAN), columns = n,
            tint = x / math.min(math.max(n, 1), SPAN), slices = slices})
    end
    local function settle()
        x, vx, hh, hv, round, rv = tx, 0, 2, 0, 0, 0
        waves = {}
        for i = 0, n do o[i], d[i] = 0, 0 end
        running, owed, last = false, 0, nil
        if tick and timer then timer.cancel(tick) end
        tick = nil
        emit(false)
    end

    local function step(dt)
        vx = vx + (k * (tx - x) - c * vx) * dt
        x = x + vx * dt
        local sp = math.abs(vx)
        hv = hv + (420 * (2 + math.min(sp * 0.02, 3) - hh) - 2 * math.sqrt(420) * 0.4 * hv) * dt
        hh = hh + hv * dt
        rv = rv + (260 * (math.min(sp / 50, 1) - round) - 2 * math.sqrt(260) * 0.55 * rv) * dt
        round = round + rv * dt
        local energy = sq(vx) * 1e-3 + sq(hv) * 1e-2 + math.abs(hh - 2) + math.abs(round) * 0.1
        local kept = {}
        for _, w in ipairs(waves) do
            w.t = w.t + dt
            local skew = math.min(w.t / w.life, 1)
            local live = w.t < w.life and 1 or 0
            local crest = w.follow and (x + w.dir * (3 + w.reach * skew)) or (w.c + w.dir * 2)
            w.v = w.v + (70 * (crest - w.c) - 11 * w.v) * dt
            w.c = w.c + w.v * dt
            local amp = w.follow and w.gain * (math.min(sp * 0.032, 3) + 0.9 * (1 - skew) * live) or 0
            local prev = math.abs(w.amp)
            w.av = w.av + (220 * (amp - w.amp) - 2 * math.sqrt(220) * 0.8 * w.av) * dt
            w.amp = w.amp + w.av * dt
            local lost = math.max(prev - math.abs(w.amp), 0)
            w.wash = w.wash + lost * 1.3 * (0.4 + skew) - w.wash * 4.2 * dt
            w.wash_c = w.wash_c + (w.c + w.dir * 4 - w.wash_c) * 6 * dt
            w.wash_w = w.wash_w + (18 + 10 * math.abs(w.gain)) * dt
            energy = energy + sq(w.amp) + sq(w.av) * 1e-3 + sq(w.wash)
            if w.t < w.life or math.abs(w.amp) > 0.01 or w.wash > 0.005 or math.abs(w.av) > 0.05 then kept[#kept + 1] = w end
        end
        waves = kept
        return energy
    end

    -- The columns are only drawn once per tick, so they are worked out from the last step only.
    local function columns()
        for i = 0, n do o[i], d[i] = 0, 0 end
        for _, w in ipairs(waves) do
            local skew = math.min(w.t / w.life, 1)
            local front, back = 9 - 6 * skew, 12 + 12 * skew
            local wash_w = math.max(w.wash_w, 3)
            for i = 0, n do
                local u = (i - w.c) * w.dir
                local shape
                if u > 0 then shape = math.exp(-sq(u / front))
                else shape = math.exp(-sq(u / back)) - 0.22 * math.exp(-sq((u + back * 1.3) / back)) end
                local chop = 0.12 * math.sin(i * 0.23 + w.t * 7 + w.phase) * w.amp
                o[i] = o[i] + AMP * env[i] * (-w.amp * shape + chop)
                d[i] = d[i] + env[i] * (math.abs(w.amp) * 0.3 * math.max(shape, 0) + w.wash * math.exp(-sq((i - w.wash_c) / wash_w)))
            end
        end
    end

    local function advance()
        if not running then return end
        local now = clock()
        owed = owed + math.min(now - (last or now), 0.1)
        last = now
        local energy
        while owed >= STEP do
            energy = step(STEP)
            owed = owed - STEP
        end
        if energy and energy < SETTLE_ENERGY and math.abs(x - tx) < 0.05 then settle(); return end
        columns()
        emit(true)
    end

    local function start()
        if running then return end
        running, owed, last = true, 0, clock()
        if timer then tick = timer.every(8, advance) end
    end

    local track = {}
    function track.resize(columns_wanted)
        local wanted = math.max(4, math.floor(columns_wanted + 0.5))
        if wanted == n then return end
        n = wanted
        for i = 0, n do env[i] = math.sqrt(math.sin(math.pi * i / n)) end
        tx = math.max(0, math.min(caret_px - INSET, n))
        settle()
    end
    -- The caret, `caret` pixels into the field, as the text's length changes.
    function track.retarget(caret)
        caret_px = caret
        local target = math.max(0, math.min(caret - INSET, n))
        if math.abs(target - tx) < 0.5 then return end
        tx = target
        local stiffness = roll(140, 210)
        k, c = stiffness, 2 * math.sqrt(stiffness) * roll(0.4, 0.6)
        local dir = tx > x and 1 or -1
        for _, w in ipairs(waves) do w.follow = false end
        local gain = roll(0.8, 1.25) * math.min(0.7 + math.abs(tx - x) / 40, 1.3)
        local at = x - dir * 5
        waves[#waves + 1] = {t = 0, dir = dir, c = at, v = 0, amp = 0, av = 0, wash = 0, wash_c = at, wash_w = 4,
            gain = gain, reach = roll(9, 16), life = roll(0.38, 0.6), phase = roll(0, 6.28), follow = true}
        if #waves > WAVES then table.remove(waves, 1) end
        start()
    end
    function track.snap(caret)
        caret_px = caret
        tx = math.max(0, math.min(caret - INSET, n))
        settle()
    end
    track.advance = advance
    return track
end
