-- All positions are sprite centers in output-local logical pixels. The desktop
-- idle center is the center of the original registered 150px body cell.
local M = {}
local function length(x, y)
    return math.sqrt(x * x + y * y)
end
local function wrap(s)
    local r = s.target
    if not r then
        return false
    end
    local right, bottom = r.x + r.w, r.y + r.h
    return r.w >= s.w * 0.35
        and r.h >= s.h * 0.35
        and r.w >= 240
        and r.h >= 220
        and r.x >= 24
        and r.y >= 24
        and (r.x >= 96 or r.y >= 96)
        and right <= s.w - 96
        and bottom <= s.h - 96
        and (right <= s.w - 220 or bottom <= s.h - 230)
end
function M.choose(s, seed, event, last, force)
    if s.blocked or s.w < 380 or s.h < 340 or s.w > 8192 or s.h > 8192 then
        return
    end
    local pool = { { "rise", 3 }, { "right", 2 }, { "sweep", 1 } }
    if wrap(s) then
        pool[#pool + 1] = { "wrap", 4 }
    end
    local total = 0
    for _, v in ipairs(pool) do
        if force == v[1] then
            return force
        end
        if last ~= v[1] then
            total = total + v[2]
        end
    end
    if force and force ~= "auto" then
        return
    end
    -- Park-Miller with a per-event initial state. Products fit exact doubles.
    local n = (seed + event * 65537) % 2147483646 + 1
    for _ = 1, 3 do
        n = n * 48271 % 2147483647
    end
    local ticket = n % total
    for _, v in ipairs(pool) do
        if last ~= v[1] then
            if ticket < v[2] then
                return v[1]
            end
            ticket = ticket - v[2]
        end
    end
end
function M.plan(s, kind)
    local w, h = s.w, s.h
    local x, y = w - 123, h - 133
    local p
    if kind == "wrap" then
        local r = s.target
        local a, b = r.x + r.w, r.y + r.h
        local dx, dy = math.min(r.w * 0.22, 92), math.min(r.h * 0.22, 80)
        p = {
            { math.max(80, r.x - 96), math.max(80, r.y - 96) },
            { r.x + 64, r.y - 40 },
            { a - dx, b - dy },
            { a, b },
            { a + dx, b + dy },
            { x - 35, y - 14 },
            { x, y },
        }
    elseif kind == "rise" then
        p = { { x - 60, h + 84 }, { x - 130, h - 96 }, { x + 30, y - 74 }, { x, y } }
    elseif kind == "right" then
        p = { { w + 84, h * 0.57 }, { w - 115, h * 0.48 }, { x - 100, y - 12 }, { x, y } }
    else
        p = { { -84, h * 0.78 }, { w * 0.27, h * 0.43 }, { w * 0.61, y + 36 }, { x, y } }
    end
    local distance = 0
    for i = 2, #p do
        distance = distance + length(p[i][1] - p[i - 1][1], p[i][2] - p[i - 1][2])
    end
    return { points = p, kind = kind, duration = 0.65 + distance / math.max(720, math.max(w, h) * 0.42) }
end
function M.sample(path, t)
    local u = math.min(1, math.max(0, t / path.duration))
    local ease = u * u * u * (10 + u * (-15 + 6 * u))
    local p = path.points
    local n = (#p - 1) / 3
    local seg = math.min(n - 1, math.floor(ease * n))
    local a, b, c, d = table.unpack(p, seg * 3 + 1, seg * 3 + 4)
    local v = ease * n - seg
    local q = 1 - v
    local function axis(i)
        return q ^ 3 * a[i] + 3 * q * q * v * b[i] + 3 * q * v * v * c[i] + v ^ 3 * d[i],
            3 * (q * q * (b[i] - a[i]) + 2 * q * v * (c[i] - b[i]) + v * v * (d[i] - c[i]))
    end
    local x, dx = axis(1)
    local y, dy = axis(2)
    -- Ease orientation into the desktop bind-independent idle transform.
    local landing = math.min(1, (1 - u) * 5)
    local facing = math.atan2 and math.atan2(dy, dx) or math.atan(dy, dx)
    local bank = u == 1 and 0 or math.sin(math.pi * u) * 14 * dx / math.max(1, length(dx, dy))
    return {
        x = x,
        y = y,
        heading = facing * landing * landing * (3 - 2 * landing),
        bank = bank,
        phase = u,
        altitude = landing,
    }
end
return M
