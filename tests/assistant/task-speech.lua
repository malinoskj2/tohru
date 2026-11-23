local create = dofile("shell/assistant/task-speech.luau")
local function fixture(seed)
    local f = {now = 0, spoken = {}, hushed = 0}
    f.owner = create(function() return f.now end, function(text, chat, kind)
        local generation = #f.spoken + 1
        f.spoken[generation] = {text = text, chat = chat, kind = kind, at = f.now}
        return generation
    end, function() f.hushed = f.hushed + 1 end)
    f.owner.configure(true, seed or 1)
    f.owner.begin("chat", "request")
    function f.update(kind, sequence, text, task)
        return f.owner.update({chat="chat", request="request", task=task or "worktree", kind=kind, sequence=sequence, text=text or kind})
    end
    function f.tick(ms, visible, busy)
        f.now = f.now + (ms or 0)
        f.owner.tick(visible ~= false, busy == true)
    end
    function f.finish() f.owner.finished(#f.spoken) end
    return f
end

local f = fixture()
assert(not f.update("progress", 2)) -- acknowledgement is the admission boundary
assert(f.update("ack", 1, "I'll create that workspace now."))
f.tick()
assert(#f.spoken == 1 and f.spoken[1].at == 0) -- task is still incomplete
assert(f.update("progress", 2, "The workspace exists. I'm starting the agent."))
assert(not f.update("progress", 2, "duplicate"))
assert(not f.update("progress", 1, "old"))
f.finish(); f.tick(1000)
assert(f.spoken[2].text:find("workspace exists", 1, true))
assert(f.update("done", 4, "The agent session is attached and verified."))
f.tick()
assert(f.hushed == 1 and f.spoken[3].kind == "done")
assert(not f.update("failed", 5) and not f.update("ack", 6))
f.owner.finished(2); f.tick()
assert(#f.spoken == 3) -- stale audio completion cannot drain the active generation

-- Seeded timing and content are reproducible; silence is a real outcome.
local function trace(seed)
    local x = fixture(seed)
    x.update("ack", 1); x.tick(); x.finish()
    local result, previous = {}, nil
    for _ = 1, 600 do
        x.tick(1000)
        local item = x.spoken[#x.spoken]
        if item.kind == "chatter" and item ~= previous then
            result[#result+1] = tostring(item.at) .. ":" .. item.text
            previous = item
            x.finish()
        end
    end
    assert(#result <= 2)
    if #result >= 1 then assert(x.spoken[2].at >= 45000) end
    if #result == 2 then
        assert(x.spoken[2].text ~= x.spoken[3].text)
        assert(x.spoken[3].at - x.spoken[2].at >= 45000)
    end
    return table.concat(result, "\n"), x
end
assert(trace(12345) == trace(12345))
local silence = false
for seed = 1, 20 do
    local x = fixture(seed)
    x.update("ack", 1); x.tick(); x.finish(); x.tick(76000)
    if #x.spoken == 1 then silence = true end
end
assert(silence)

f = fixture(12345)
f.update("ack", 1); f.tick(); f.finish()
for _ = 1, 300 do
    f.tick(1000)
    if f.spoken[#f.spoken].kind == "chatter" then break end
end
assert(f.spoken[#f.spoken].kind == "chatter")
f.update("done", 2, "Verified success."); f.tick()
assert(f.hushed == 1 and f.spoken[#f.spoken].kind == "done")

f = fixture(12345)
f.update("ack", 1); f.tick(); f.finish()
for _ = 1, 300 do
    f.tick(1000)
    if f.spoken[#f.spoken].kind == "chatter" then break end
end
local before_cancel = #f.spoken
f.update("cancelled", 2); f.tick(100000)
assert(f.hushed == 1 and #f.spoken == before_cancel)

for _, action in ipairs({"interrupt", "hide", "supersede", "cancel"}) do
    f = fixture()
    f.update("ack", 1); f.tick()
    f.update("progress", 2)
    if action == "interrupt" then f.owner.interrupt()
    elseif action == "hide" then f.tick(0, false)
    elseif action == "supersede" then f.owner.begin("other", "new")
    else f.update("cancelled", 3) end
    f.tick(100000)
    assert(f.hushed == 1 and #f.spoken == 1)
    assert(not f.update("done", 4))
end

f = fixture()
f.update("ack", 1); f.tick(); f.finish()
f.owner.configure(false, 1); f.tick(120000)
assert(#f.spoken == 1)
f.update("failed", 2, "The agent could not start. The workspace remains available."); f.tick()
assert(f.spoken[2].kind == "failed")

f = fixture()
for task = 1, 8 do
    assert(f.update("ack", 1, "Starting task " .. task, tostring(task)))
    for sequence = 2, 100 do assert(f.update("progress", sequence, "Progress " .. sequence, tostring(task))) end
end
assert(not f.update("ack", 1, "Overflow", "9"))
assert(f.update("done", 101, "Task two finished", "2"))
f.tick()
assert(f.spoken[1].text == "Task two finished")
f.finish(); f.tick(31000)
assert(#f.spoken == 1) -- expired queue entries do not return later
assert(not f.update("done", 102, "\n", "2"))

f = fixture()
f.update("ack", 1); f.tick()
f.tick(30000, true, true)
assert(f.hushed == 1) -- missing audio completion still has a deadline
print("Task speech: acknowledgement, progress, seeded silence/chatter, priority, generations, lifecycle and bounds passed")
