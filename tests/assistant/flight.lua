local paths = dofile("shell/assistant/flight-path.luau")
local scene = { w = 1920, h = 1080, target = { x = 160, y = 145, w = 940, h = 570, id = "0xaa" } }
local function replay(seed)
    local last, series = nil, {}
    for i = 1, 100 do
        local pick = paths.choose(scene, seed, i, last)
        assert(pick and pick ~= last)
        series[#series + 1] = pick
        last = pick
    end
    return table.concat(series, ",")
end
assert(replay(1234) == replay(1234) and replay(1) ~= replay(2))
local function wrap(r)
    return paths.choose({ w = 1920, h = 1080, target = r }, 1, 1, nil, "wrap")
end
assert(wrap(scene.target) and not wrap(nil))
for _, r in ipairs({
    { x = 0, y = 0, w = 1920, h = 1080 },
    { x = 5, y = 5, w = 1910, h = 1070 },
    { x = 120, y = 180, w = 160, h = 480 },
    { x = 120, y = 250, w = 1740, h = 640 },
    { x = 15, y = 100, w = 700, h = 480 },
    { x = 406, y = 500, w = 1394, h = 460 },
}) do
    assert(not wrap(r))
end
assert(not paths.choose({ w = 1920, h = 1080, blocked = true }, 1, 1))
assert(not paths.choose({ w = 320, h = 300 }, 1, 1))
for _, kind in ipairs({ "rise", "right", "sweep", "wrap" }) do
    local p = paths.plan(scene, kind)
    local prev = paths.sample(p, 0)
    local speed = 0
    for i = 1, 1000 do
        local dt = p.duration / 1000
        local pos = paths.sample(p, dt * i)
        local dx, dy = pos.x - prev.x, pos.y - prev.y
        local v = math.sqrt(dx * dx + dy * dy) / dt
        assert(v < 4000 and math.abs(v - speed) / dt < 16000 and math.abs(pos.bank) <= 24)
        assert(pos.heading == pos.heading and pos.phase >= prev.phase)
        prev, speed = pos, v
    end
    local pos = paths.sample(p, p.duration + 10)
    assert(pos.x == 1797 and pos.y == 947 and pos.bank == 0 and pos.heading == 0 and pos.altitude == 0)
end

local now = 0
os.clock = function()
    return now
end
local handlers, tick, receive = {}, nil, nil
_G.on = function(k, f)
    handlers[k] = f
end
_G.every = function(_, f)
    tick = f
    return 1
end
_G.cancel = function()
    tick = nil
end
_G.after = function() end
_G.spawn = function(_, _, f)
    receive = f
    return 42
end
_G.kill = function(id)
    assert(id == 42)
end
_G.fact = {}
_G.text = {}
_G.json = {
    decode = function(x)
        return x
    end,
    encode = function(x)
        return x
    end,
}
local f = dofile("shell/assistant/flight.luau")(paths, {
    end_entrance = function() end,
    land = function() return true end,
    begin_entrance = function()
        return true
    end,
    wingbeat = function() end,
})
local function configure(k, default)
    if k == "assistant.contextual_entrances" then
        return true
    end
    if k == "assistant.entrance_variant" then
        return "wrap"
    end
    return default
end
f.outputs({ "test" })
scene.output = "test"
f.configure(configure)
handlers.dragon_view_width(1920)
handlers.dragon_view_height(1080)
local function start()
    now = now + 31
    f.show()
    receive(scene)
    assert(fact.ai_flying and fact.ai_flight_behind)
end
for _, change in ipairs({
    function()
        f.hearing(true)
    end,
    function()
        f.window({ fullscreen = true })
    end,
    function()
        receive({ blocked = true })
    end,
    function()
        receive({})
    end,
    function()
        receive({ target = { id = "0xaa", x = 270 } })
    end,
    function()
        handlers.dragon_view_width(1600)
    end,
    function()
        f.interrupt()
    end,
    function()
        fact.ai_open = true
    end,
    function()
        fact.notices_dnd = true
    end,
}) do
    start()
    change()
    tick()
    now = now + 0.09
    tick()
    assert(not fact.ai_flying)
    now = now + 0.2
    tick()
    assert(not tick and fact.ai_flight_alpha == 1)
    fact.ai_open = false
    fact.notices_dnd = false
    f.hearing(false)
    f.window({})
    handlers.dragon_view_width(1920)
    receive = nil
    f.show()
    assert(not receive, "cooldown")
end
fact.ai_dg_reduced = true
now = now + 31
f.show()
assert(not receive)
fact.ai_dg_reduced = false
f.configure(function(key, default)
    if key == "assistant.contextual_entrances" then return false end
    return default
end)
f.show()
assert(not receive and not fact.ai_use_flight_surface, "explicit opt-out keeps the compact appearance")
print("PASS flight selection, geometry, trajectory, interruption, reduced motion and cooldown")
