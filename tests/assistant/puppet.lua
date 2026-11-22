_G.fact = {}
_G.text = {}
local tick = nil
_G.every = function(ms, f) tick = f; return 1 end
_G.cancel = function() tick = nil end
_G.after = function() end
math.randomseed(7)
local puppet = assert(loadfile("shell/assistant/puppet.luau"))()()
local function run(n) for _ = 1, n do tick() end end
local function settle(limit)
    for _ = 1, limit or 200 do
        local pose, target = puppet.pose()
        if pose == target then return pose end
        tick()
    end
    error("never settled on " .. select(2, puppet.pose()))
end

-- Hidden, she changes pose at once; shown, she idles on the loop sheet with the eyes following the breath.
puppet.thinking()
assert(puppet.pose() == "thinking")
puppet.calm()
puppet.visible(true)
assert(tick and fact.ai_dg_sheet == 0 and fact.ai_dg_mouth == -1)
run(11)
assert(fact.ai_dg_sheet == 0 and fact.ai_dg_cell == 12 and math.abs(fact.ai_dg_ey + 1.47) < 0.01)

-- Hearing her name: listening, swapped under a squash. A turn: thinking, through neutral on the drawn move.
puppet.hearing(true)
tick()
assert(fact.ai_dg_sq > 0)
assert(settle() == "listening" and fact.ai_dg_sheet == 2 and fact.ai_dg_cell == 6)
puppet.thinking()
local sheets = {}
for _ = 1, 40 do tick(); sheets[fact.ai_dg_sheet] = true end
assert(puppet.pose() == "thinking" and sheets[1] and fact.ai_dg_cell == 5)

-- Speaking: back through neutral to explaining, with the viseme mouth on the mouthless pose.
puppet.speaking(true)
assert(settle() == "explain")
puppet.mouth(0.7, 5)
tick()
assert(fact.ai_dg_sheet == 2 and fact.ai_dg_cell == 13 and fact.ai_dg_frame == 178 and fact.ai_dg_mouth == 5 and fact.ai_dg_my > 3)
puppet.mouth(0.5)
tick()
assert(fact.ai_dg_mouth == 3)

-- A new pose waits for a dip in the voice and a few seconds since the last change.
puppet.sentence("That is wonderful!")
run(100)
assert(puppet.pose() == "explain")
for _ = 1, 6 do puppet.mouth(0.02) end
tick()
local _, target = puppet.pose()
assert(target == "cheer")
local hearts = false
for _ = 1, 40 do tick(); hearts = hearts or (fact.ai_dg_h1_a or 0) > 0 end
assert(puppet.pose() == "cheer" and hearts and fact.ai_dg_cell == 16)

-- Done: pleased for a moment, then neutral and the idle breath again.
puppet.speaking(false)
assert(settle() == "pleased")
run(80)
assert(puppet.pose() == "neutral" and fact.ai_dg_sheet == 0)

-- Sentence content picks the pose.
assert(puppet.pick("Haha, that's funny.") == "laugh" and puppet.pick("Then the head goes, first, second.") == "scroll")
assert(puppet.pick("Glad to help.") == "happy" and puppet.pick("It is 4 pm.") == "explain" and puppet.pick("Hey there!") == "cheer")

puppet.visible(false)
assert(tick == nil)
print("Dragon puppet poses, transitions, dips, visemes and hearts passed")
