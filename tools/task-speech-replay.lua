-- Synthetic replay: production policy, fake task and speech adapters, no devices.
local create = dofile("shell/assistant/task-speech.luau")
local now, generation, current = 0, 0, nil
local owner = create(function() return now end, function(text, chat, kind)
    generation = generation + 1
    current = generation
    print(string.format("%6.1fs  %-8s %s", now / 1000, kind, text))
    return generation
end, function()
    print(string.format("%6.1fs  hush", now / 1000))
    current = nil
end)
owner.configure(true, tonumber(arg[1]) or 12345)
owner.begin("synthetic-chat", "synthetic-request")
local function update(kind, sequence, text)
    assert(owner.update({chat="synthetic-chat", request="synthetic-request", task="workspace",
        kind=kind, sequence=sequence, text=text}))
end
update("ack", 1, "I'll create that workspace now.")
for second = 0, 150 do
    now = second * 1000
    if second == 5 then update("progress", 2, "The workspace exists. The agent is starting.") end
    if second == 90 then update("done", 3, "The agent session is attached and verified.") end
    owner.tick(true, false)
    if current then owner.finished(current); current = nil end
end
