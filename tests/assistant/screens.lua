_G.fact = {}
local ticks, processes, kills = {}, {}, {}
_G.every = function(_, f) ticks[#ticks + 1] = f end
_G.spawn = function(command, args, _, exited, options)
    local p = {command = command, args = args, exited = exited, options = options, id = #processes + 1}
    processes[#processes + 1] = p
    return p.id
end
_G.kill = function(id, signal) kills[#kills + 1] = {id = id, signal = signal} end
local function tick(n) for _ = 1, n or 1 do for _, f in ipairs(ticks) do f() end end end
local function runs()
    local n = 0
    for _, p in ipairs(processes) do if p.args[1] == "run" then n = n + 1 end end
    return n
end

local screens = assert(loadfile("shell/assistant/screens.luau"))()()

-- Leftovers from a previous bar go at once; nothing captures while she is hidden.
assert(#processes == 1 and processes[1].command == "tohru-screens" and processes[1].args[1] == "clear")
tick(8)
assert(runs() == 0 and not screens.running())

-- The dragon appearing starts the runner; hiding sends it SIGTERM, which deletes every capture.
fact.ai_dragon_open = true
tick()
assert(runs() == 1 and screens.running())
assert(processes[#processes].options.stdin == "open")
local runner = processes[#processes]
tick(8)
assert(runs() == 1)
fact.ai_dragon_open = false
tick()
assert(#kills == 1 and kills[1].id == runner.id and kills[1].signal == "term" and not screens.running())
runner.exited(runner.id, 0)
tick(8)
assert(runs() == 1)

-- The chat popup counts as on screen too.
fact.ai_open = true
tick()
assert(runs() == 2)

-- A runner that dies while she is shown restarts after a back-off, not in a tight loop.
local second = processes[#processes]
second.exited(second.id, 1)
assert(not screens.running())
tick(4)
assert(runs() == 2)
tick(5)
assert(runs() == 3)

-- A stale exit of an older runner does not forget the current one.
second.exited(second.id, 0)
assert(screens.running())
fact.ai_open = false
tick()
assert(not screens.running() and kills[#kills].id == processes[#processes].id)

-- A missing executable backs off too.
_G.spawn = function() error("not permitted") end
fact.ai_dragon_open = true
tick(40)
assert(not screens.running())
print("screens: ok")
