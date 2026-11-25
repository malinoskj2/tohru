_G.json = {decode = function(value) assert(type(value) == "table", "fixture answers are tables"); return value end}
_G.fact = {}
local ticks = {}
_G.every = function(_, f) ticks[#ticks + 1] = f end
local runs, replies = {}, {}
_G.run = function(command, args, done)
    runs[#runs + 1] = {command = command, args = args, done = done}
end
local function tick(seconds)
    for _ = 1, seconds do for _, f in ipairs(ticks) do f() end end
end
local function last() return runs[#runs] end
local function has(list, a, b)
    for i = 1, #list - 1 do if list[i] == a and list[i + 1] == b then return true end end
    return false
end

local values = {["assistant.backend"] = "local"}
local streaming = false
local watcher = assert(loadfile("shell/assistant/needs-you.luau"))()(function(key) return values[key] end,
    function() return streaming end)

-- Off unless the Orca backend is on and the interval is above zero.
tick(600)
assert(#runs == 0 and not watcher.status().enabled)
values["assistant.backend"] = "orca"
values["assistant.orca_needs_you_minutes"] = 0
watcher.configure()
tick(600)
assert(#runs == 0 and not watcher.status().enabled)

-- On: the first poll waits a second after the change, then every five minutes by default.
values["assistant.orca_needs_you_minutes"] = nil
watcher.configure()
assert(watcher.status().interval == 300000)
tick(1)
assert(#runs == 1 and last().command == "tohru-assistant" and last().args[1] == "needs-you" and last().args[2] == "poll")
assert(has(last().args, "--backend", "orca") and has(last().args, "--orca-bot", "Tohru"))
last().done({ok = true, pending = {}}, 0)
tick(299)
assert(#runs == 1)
tick(1)
assert(#runs == 2)

-- A pending item wakes the bot once, with every pending item in one call.
last().done({ok = true, pending = {
    {uid = "f418a981-0d34", event = "e549"}, {uid = "f418a981-0d34", event = "e549"}, {uid = "aa", event = "e7"},
    {uid = "bad uid; rm", event = "e1"},
}}, 0)
assert(#runs == 3 and last().args[2] == "wake" and last().args[3] == "f418a981-0d34:e549" and last().args[4] == "aa:e7"
    and last().args[5] == "--backend")
last().done({ok = true, woke = true}, 0)
tick(10)
assert(#runs == 3 and watcher.status().queued == 0)

-- Waking waits while a voice turn, speech, a streaming answer, Do Not Disturb or the lock is active.
for _, hold in ipairs({"ai_voice_conversation", "ai_speaking", "notices_dnd", "ai_voice_paused", "streaming"}) do
    watcher.poll_now()
    tick(1)
    local polls = #runs
    if hold == "streaming" then streaming = true else fact[hold] = true end
    last().done({ok = true, pending = {{uid = "bb", event = "e9"}}}, 0)
    tick(30)
    assert(#runs == polls and watcher.status().queued == 1, hold)
    if hold == "streaming" then streaming = false else fact[hold] = false end
    tick(1)
    assert(#runs == polls + 1 and last().args[2] == "wake" and last().args[3] == "bb:e9", hold)
    last().done({ok = true, woke = true}, 0)
end

-- Orca being unavailable backs off quietly, doubling up to an hour, and recovers on success.
watcher.poll_now()
tick(1)
local before = #runs
last().done({ok = false, error = "Orca is not running"}, 0)
assert(watcher.status().failures == 1 and watcher.status().due_in == 600000)
tick(599)
assert(#runs == before)
tick(1)
assert(#runs == before + 1)
last().done(nil, 1)
assert(watcher.status().failures == 2 and watcher.status().due_in == 1200000)
for _ = 1, 6 do
    watcher.poll_now(); tick(1); last().done({ok = false}, 0)
end
assert(watcher.status().due_in == 3600000)
watcher.poll_now(); tick(1); last().done({ok = true, pending = {}}, 0)
assert(watcher.status().failures == 0 and watcher.status().due_in == 300000)

-- A failed wake keeps nothing queued and backs off; the next poll offers the item again.
watcher.poll_now(); tick(1)
last().done({ok = true, pending = {{uid = "cc", event = "e11"}}}, 0)
assert(last().args[2] == "wake")
last().done({ok = false, error = "Could not reach the bot"}, 0)
assert(watcher.status().queued == 0 and watcher.status().failures == 1)

-- A configured interval is honoured, and a reply from before a settings change is ignored.
values["assistant.orca_needs_you_minutes"] = 15
watcher.poll_now(); tick(1)
local stale = last()
watcher.configure()
assert(watcher.status().interval == 900000 and watcher.status().failures == 0)
stale.done({ok = true, pending = {{uid = "dd", event = "e12"}}}, 0)
assert(watcher.status().queued == 0)
tick(1)
local polled = #runs
last().done({ok = true, pending = {}}, 0)
tick(899)
assert(#runs == polled)
tick(1)
assert(#runs == polled + 1)

print("Needs-you watcher gating, interval, single wake, quiet hours and backoff passed")
