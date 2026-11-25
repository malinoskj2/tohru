-- A single VM drives production voice, entrance and registered puppet code.
-- Substitute only the Pleamar host, compositor report and voice service process.
package.path = "shell/?.luau;" .. package.path
local handlers, timers, processes = {}, {}, {}
local now, sequence = 0, 0
_G.fact, _G.text = {}, {}
os.clock = function()
    return now / 1000
end
_G.on = function(name, fn)
    handlers[name] = fn
end
local function schedule(ms, fn, repeat_ms)
    sequence = sequence + 1
    timers[sequence] = { due = now + ms, fn = fn, interval = repeat_ms }
    return sequence
end
_G.after = function(ms, fn)
    return schedule(ms, fn)
end
_G.every = function(ms, fn)
    return schedule(ms, fn, ms)
end
_G.cancel = function(id)
    timers[id] = nil
end
local function advance(ms)
    local until_ms = now + ms
    while true do
        local first
        for id, t in pairs(timers) do
            if t.due <= until_ms and (not first or t.due < timers[first].due) then
                first = id
            end
        end
        if not first then
            break
        end
        local timer = timers[first]
        now = timer.due
        if timer.interval then
            timer.due = now + timer.interval
        else
            timers[first] = nil
        end
        timer.fn()
    end
    now = until_ms
end
local sample = { output = "LEFT", w = 864, h = 1536 }
local selection, greetings = nil, 0
_G.json = {
    decode = function(raw)
        return raw
    end,
    encode = function(v)
        if v.cmd == "say" then
            greetings = greetings + 1
        end
        if v.kind then
            selection = v
        end
        return "{}"
    end,
}
_G.spawn = function(command, args, line, exit)
    sequence = sequence + 1
    processes[command] = { id = sequence, line = line, exit = exit }
    if command == "tohru-flight-scene" then
        after(12, function()
            line(sample)
        end)
    end
    return sequence
end
_G.kill = function() end
_G.write = function() end
local answer, asked, opened
local assistant = {
    on_answer = function(fn)
        answer = fn
    end,
    on_alert = function() end,
    ask = function(prompt)
        asked = prompt
        return true
    end,
    listening = function(v)
        fact.ai_listening = v
    end,
    remember = function() end,
    show = function(id)
        opened = id
    end,
}
local puppet = dofile("shell/assistant/puppet.luau")(dofile("shell/assistant/full-body.luau"), dofile("shell/assistant/landing-frames.luau"))
local flight = dofile("shell/assistant/flight.luau")(
    dofile("shell/assistant/flight-path.luau"),
    puppet,
    dofile("shell/assistant/flight-actor.luau")
)
local voice = dofile("shell/assistant/voice.luau")(assistant, puppet, flight)
local settings = {
    ["assistant.voice"] = { value = true },
    ["assistant.entrance_variant"] = { value = "rise" },
    ["assistant.entrance_seed"] = { value = 42 },
}
flight.outputs({ "LEFT", "RIGHT" })
flight.window({ monitor = "LEFT" })
handlers.dragon_view_width(864)
handlers.dragon_view_height(1536)
voice.configure(settings)
local service = processes["tohru-voice"]
local function deliver(event, fields)
    fields = fields or {}
    fields.event = event
    service.line(fields)
end
deliver("ready", { dragon = "/fixture" })
deliver("listening", { on = true })
-- A cold first appearance waits for warm sheets, then flies in.
deliver("conversation", { on = true })
advance(50)
assert(not fact.ai_flying and not fact.ai_flight_art)
advance(3100)
assert(fact.ai_flying and fact.ai_flight_art, "cold appearance skipped its entrance")
handlers.assistant_dragon()
advance(31000)
local function wake(expected)
    deliver("hearing", { on = true })
    deliver("hearing", { on = false })
    deliver("conversation", { on = true })
    deliver("turn", { text = "Hello" })
    advance(35)
    assert(fact.ai_dragon_open and fact.ai_flying and fact.ai_listening)
    assert(
        selection.seed == 42 and selection.kind == (expected or "rise") and text.ai_flight_output == "LEFT"
    )
    assert(asked == "Hello")
end
wake()
local ordinal = selection.event
voice.configure(settings)
deliver("conversation", { on = true })
flight.window({ monitor = "RIGHT" })
advance(4000)
assert(not fact.ai_flying and selection.event == ordinal and text.ai_flight_output == "LEFT")
local _, target = puppet.pose()
assert(target == "ponder_hover" and fact.ai_flight_alpha == 1)
answer("done", "chat", "Ready", true)
deliver("speaking", { on = true, conversation = "chat" })
deliver("mouth", { level = 0.7, viseme = 4 })
assert(fact.ai_speaking and fact.ai_mouth == 0.7)
deliver("speaking", { on = false, conversation = "chat" })
handlers.assistant_dragon()
assert(opened == "chat" and not fact.ai_dragon_open)

-- A wake-only greeting must wait for the entrance instead of interrupting it.
advance(31000)
flight.window({ monitor = "LEFT" })
deliver("conversation", { on = true })
local before_greeting = greetings
deliver("wake")
advance(200)
assert(fact.ai_flying and greetings == before_greeting, "greeting cut off the entrance")
advance(4000)
assert(not fact.ai_flying and greetings == before_greeting + 1, "greeting did not follow landing")
handlers.assistant_dragon()

-- Starting a question during the entrance cancels its deferred greeting.
advance(31000)
deliver("conversation", { on = true })
before_greeting = greetings
deliver("wake")
advance(50)
deliver("hearing", { on = true })
deliver("hearing", { on = false })
deliver("turn", { text = "A question" })
advance(4000)
assert(greetings == before_greeting, "stale greeting interrupted the question")
handlers.assistant_dragon()

for _, interruption in ipairs({ "hearing", "speaking", "disconnect" }) do
    advance(31000)
    flight.window({ monitor = "LEFT" })
    wake()
    if interruption == "disconnect" then
        service.exit(nil, 3)
        assert(not voice.running() and not fact.ai_flying and not fact.ai_dragon_open)
    else
        deliver(interruption, { on = true, conversation = "chat" })
        advance(280)
        local _, current = puppet.pose()
        assert(
            not fact.ai_flying
                and current ~= "rising_bank"
                and fact.ai_dragon_open
                and fact.ai_flight_alpha == 1
        )
        deliver(interruption, { on = false, conversation = "chat" })
        deliver("conversation", { on = false })
        advance(4900)
        assert(fact.ai_dragon_open)
        advance(200)
        assert(not fact.ai_dragon_open)
    end
end
-- A new service generation must not inherit a pending flight or a forced pose.
voice.configure(settings)
service = processes["tohru-voice"]
settings["assistant.reduced_motion"] = { value = true }
voice.configure(settings)
deliver("conversation", { on = true })
advance(300)
assert(
    fact.ai_dragon_open
        and not fact.ai_flying
        and not fact.ai_use_flight_surface
        and text.ai_flight_output == ""
)
settings["assistant.voice"] = { value = false }
voice.configure(settings)
assert(not fact.ai_dragon_open and not voice.running())
-- The actual puppet's displayed atlas row must already be direction-correct
-- while the root is stationary, must flap in transit, and give the conversation
-- its drawn body transition only after braking. Inspect the same facts PLM draws.
settings["assistant.voice"].value = true
settings["assistant.reduced_motion"].value = false
voice.configure(settings)
service = processes["tohru-voice"]
deliver("ready", { dragon = "/fixture" })
deliver("listening", { on = true })
sample.target = { id = "0x1", x = 136, y = 195, w = 432, h = 768 }
local rows = { rise = 2, right = 0, sweep = 1, wrap = 1 }
local keys = { rise = 327, right = 309, sweep = 417, wrap = 417 }
for _, kind in ipairs({ "rise", "right", "sweep", "wrap" }) do
    advance(31000)
    settings["assistant.entrance_variant"].value = kind
    voice.configure(settings)
    wake(kind)
    assert(fact.ai_flight_art and fact.ai_flight_phase == 0)
    local start_x, start_y = fact.ai_flight_x, fact.ai_flight_y
    advance(100)
    assert(fact.ai_flight_x == start_x and fact.ai_flight_y == start_y)
    local cells, previous, landing = {}, start_x, false
    local last_landing_frame = -1
    for _ = 1, 400 do
        advance(16)
        if not fact.ai_flying then
            break
        end
        if fact.ai_flight_phase < 0.8 then
            assert(fact.ai_flight_art and fact.ai_dg_frame == keys[kind])
            assert(math.floor(fact.ai_flight_cell / 12) == rows[kind])
            cells[fact.ai_flight_cell] = true
            local dx = fact.ai_flight_x - previous
            assert(kind == "right" and dx <= 0.001 or kind ~= "right" and dx >= -0.001)
        else
            landing = true
            local index = fact.ai_landing_frame % 15
            assert(fact.ai_flight_art == (index == 0))
            assert(fact.ai_landing_art == (index > 0 and index < 14))
            assert(fact.ai_landing_frame >= last_landing_frame and math.floor(fact.ai_landing_frame / 15) == rows[kind] * 3)
            last_landing_frame = fact.ai_landing_frame
            -- The visible turn goes directly to ponder_hover. A backing frame
            -- from either neutral-hub leg would be a second body transition.
            assert(fact.ai_dg_frame == 489 and fact.ai_landing_target == 489 and puppet.pose() == "ponder_hover")
        end
        previous = fact.ai_flight_x
    end
    local count = 0
    for _ in pairs(cells) do
        count = count + 1
    end
    assert(count == 12 and landing and last_landing_frame % 15 == 14 and not fact.ai_flying)
    assert(not fact.ai_landing_art)
    assert(fact.ai_dg_frame == 489 and fact.ai_dragon_open)
    advance(750)
    assert(fact.ai_dg_frame == 489 and puppet.pose()=="ponder_hover")
    handlers.assistant_dragon()
end
-- A new user utterance while the feet and wings are settling owns the puppet.
advance(31000)
wake("wrap")
for _ = 1, 320 do
    advance(16)
    if fact.ai_landing_art and fact.ai_landing_progress > 0.3 then break end
end
assert(fact.ai_landing_art)
deliver("hearing", { on = true })
advance(260)
assert(not fact.ai_landing_art and not fact.ai_flight_art and not fact.ai_flying and fact.ai_dragon_open)
local _, interrupted_target = puppet.pose()
assert(interrupted_target == "attentive_hover")
deliver("hearing", { on = false })
handlers.assistant_dragon()
-- Keep a visible actor on its selected output across focus changes; a
-- disconnected selected output must release ownership of the old coordinates.
advance(31000)
wake("wrap")
flight.window({ monitor = "RIGHT" })
assert(text.ai_flight_output == "LEFT")
flight.outputs({ "RIGHT" })
assert(
    text.ai_flight_output == "RIGHT" and not fact.ai_flying and not fact.ai_flight_art and fact.ai_dragon_open
)
flight.outputs({})
assert(text.ai_flight_output == "" and not fact.ai_use_flight_surface)
print(
    "PASS combined voice, puppet and flight lifecycle through completion, hearing, speech, linger, reopen and disconnect"
)
-- The same flight controller lends the actor to finite idle scenes, even when
-- voice and contextual entrances are disabled. Start with cold shared artwork.
settings["assistant.voice"].value = false
settings["assistant.contextual_entrances"] = { value = false }
voice.configure(settings)
flight.outputs({ "LEFT", "RIGHT" })
flight.window({ monitor = "LEFT" })
sample.art = "/fixture"
local actor = flight.actor
local scene, reason, observed
local function ready(t, s)
    observed, scene = t, s
end
local function stopped(_, why)
    reason = why
end
assert(actor.register_skin("/fixture/afk.webp", 12))
assert(actor.register_closeup("/fixture/bust.webp"))
assert((text.ai_actor_skin_src or "") == "" and (text.ai_actor_closeup_src or "") == "")
assert(not actor.register_skin("/fixture/afk.webp", 65))
local lease = assert(actor.acquire(ready, stopped))
assert(not actor.acquire(ready, stopped) and fact.ai_actor_open and not fact.ai_dragon_open)
advance(2100)
assert(not scene and fact.ai_dg_warm)
advance(1150)
assert(observed == lease and scene.target == sample.target and scene.output == "LEFT")
local frame = { x = 150, y = 240, scale = 0.4, pose = "fly-left", behind = true, airborne = 1 }
local ok, state = actor.play(lease, frame)
assert(ok and state == "preparing" and fact.ai_flight_cell == 0 and fact.ai_flight_alpha <= 0.002)
advance(250)
frame.x = 136
ok, state = actor.play(lease, frame)
assert(ok and state == "ready" and fact.ai_flying and fact.ai_flight_cell > 0 and fact.ai_flight_x == 136)
frame.pose, frame.skin_frame, frame.rotation = "skin", 95, 180
ok, state = actor.play(lease, frame)
assert(ok and state == "preparing")
advance(3100)
ok, state = actor.play(lease, frame)
assert(
    ok
        and state == "ready"
        and fact.ai_actor_skin_open
        and fact.ai_actor_skin_frame == 95
        and fact.ai_actor_rotation == 180
)
assert(text.ai_actor_skin_src == "/fixture/afk.webp" and text.ai_actor_closeup_src == "")
frame.pose = "closeup"
ok, state = actor.play(lease, frame)
assert(
    ok
        and state == "preparing"
        and text.ai_actor_skin_src == ""
        and text.ai_actor_closeup_src == "/fixture/bust.webp"
)
advance(3100)
assert(actor.play(lease, frame))
frame.pose = "nested_hover"
assert(actor.play(lease, frame))
assert(not fact.ai_flight_art and not fact.ai_actor_skin_open and puppet.pose() == "nested_hover")
assert(text.ai_actor_skin_src == "" and text.ai_actor_closeup_src == "")
frame.pose = "closeup"
assert(actor.play(lease, frame))
local stale = processes["tohru-flight-scene"]
flight.hearing(true)
assert(reason == "activity" and not fact.ai_flying and not fact.ai_actor_open and fact.ai_actor_scale == 1)
assert(not actor.play(lease, frame) and not actor.release(lease))
assert(text.ai_actor_closeup_src == "" and text.ai_actor_skin_src == "")
stale.line(sample)
stale.exit()
assert(not fact.ai_actor_open and not fact.ai_dg_ready)
flight.hearing(false)
for _, interrupt in ipairs({ "voice", "geometry", "lock", "input", "output", "DND", "expiry" }) do
    scene = nil
    local t = assert(actor.acquire(ready, stopped))
    advance(3250)
    assert(observed == t and scene)
    local watcher = processes["tohru-flight-scene"]
    if interrupt == "voice" then
        flight.show()
    elseif interrupt == "geometry" then
        watcher.line({ output = "LEFT", w = 864, h = 1536, art = "/fixture" })
    elseif interrupt == "lock" then
        watcher.line({ unlocked = false, blocked = true })
    elseif interrupt == "input" then
        assert(actor.release(t))
    elseif interrupt == "output" then
        flight.outputs({ "RIGHT" })
    elseif interrupt == "DND" then
        fact.notices_dnd = true
        advance(40)
        fact.notices_dnd = false
    else
        advance(56000)
    end
    assert(not actor.release(t) and not fact.ai_actor_open and not fact.ai_flight_art)
    assert(not actor.play(lease, frame))
    flight.outputs({ "LEFT", "RIGHT" })
end
print(
    "PASS finite actor ownership, cold assets without voice, skin/rotation/scale, preemption and stale generation rejection"
)

puppet.reduced(true)
assert(not actor.acquire(ready, stopped))
local quiet = assert(actor.acquire(ready, stopped, { quiet = true }))
assert(not actor.register_skin("/fixture/other.webp", 12, quiet))
advance(3250)
assert(actor.register_skin("/fixture/perch.webp", 12, quiet))
assert(not actor.register_closeup("/fixture/bust.webp", quiet - 1))
assert(actor.register_closeup("/fixture/bust.webp", quiet))
local rest = { x = 320, y = 440, pose = "nested_hover", scale = 0.8 }
assert(actor.play(quiet, rest))
assert(not actor.register_closeup("/fixture/replacement.webp", quiet))
advance(250)
ok, state = actor.play(quiet, rest)
assert(ok and state == "ready" and fact.ai_actor_quiet and not fact.ai_flying and not fact.ai_flight_art)
flight.configure(function(_, fallback)
    return fallback
end)
assert(actor.play(quiet, rest))
rest.x = 319
assert(not actor.play(quiet, rest) and reason == "quiet" and not fact.ai_actor_open)
assert(not actor.register_closeup("/fixture/stale.webp", quiet))
local denied = assert(actor.acquire(ready, stopped, { quiet = true }))
advance(3250)
assert(not actor.play(denied, { x = 320, y = 440, pose = "fly-left" }))
print("PASS token-bound registration and reduced-motion quiet rest without translation or wingbeats")
