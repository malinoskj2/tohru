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
local function dip()
    for _ = 1, 6 do puppet.mouth(0.02) end
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

-- Pacing: a new pose waits for both the 2.5-4 s minimum (60-96 ticks) and a voice dip (5 quiet events).
puppet.sentence("That is wonderful!")
run(50)
dip()
tick()
assert(puppet.pose() == "explain")
puppet.mouth(0.6)
run(55)
assert(puppet.pose() == "explain")
dip()
tick()
local _, target = puppet.pose()
assert(target == "cheer")
local hearts = false
for _ = 1, 40 do tick(); hearts = hearts or (fact.ai_dg_h1_a or 0) > 0 end
assert(puppet.pose() == "cheer" and hearts and fact.ai_dg_cell == 16)

-- Pausing speech drops the mouth and switches to listening; resuming returns to explaining.
puppet.mouth(0.6, 5)
tick()
assert(fact.ai_dg_mouth == 5)
puppet.speaking(false, "paused")
assert(settle() == "listening" and fact.ai_dg_mouth == -1)
puppet.speaking(true)
assert(settle() == "explain")
-- An interrupted speech end does not overwrite listening or thinking with pleased.
puppet.speaking(false, "paused")
assert(settle() == "listening")
puppet.speaking(false, "interrupted")
assert(select(2, puppet.pose()) == "listening")
puppet.thinking()
puppet.speaking(false, "interrupted")
assert(select(2, puppet.pose()) == "thinking")
puppet.speaking(true)
assert(settle() == "explain")

-- Moving-head talking poses apply their per-pose mouth and eye offsets while speaking.
puppet.sentence("By the way, here is a handy tip.")
run(100)
dip()
tick()
assert(settle() == "lean_in")
puppet.mouth(0.6, 2)
tick()
assert(fact.ai_dg_frame == 165 + 28 and fact.ai_dg_mouth == 2)
assert(math.abs(fact.ai_dg_mx + 10.35) < 0.01 and math.abs(fact.ai_dg_my - 6.1) < 0.01)
assert(math.abs(fact.ai_dg_ex + 10.1) < 0.01 and math.abs(fact.ai_dg_ey - 5.7) < 0.01)

puppet.sentence("Does that sound good?")
run(100)
dip()
tick()
assert(settle() == "head_tilt")
puppet.mouth(0.6, 4)
tick()
assert(fact.ai_dg_frame == 165 + 27 and fact.ai_dg_eye == -1)
assert(math.abs(fact.ai_dg_mx + 0.3) < 0.01 and math.abs(fact.ai_dg_my + 2.5) < 0.01)

-- Done: pleased for a moment, then neutral and the idle breath again.
puppet.speaking(false)
assert(settle() == "pleased")
run(80)
assert(puppet.pose() == "neutral" and fact.ai_dg_sheet == 0)

-- Sentence cues pick the matching pose.
assert(puppet.pick("Haha, that's funny.") == "laugh")
assert(puppet.pick("Something failed!") ~= "cheer", "punctuation alone must not celebrate a failure")
assert(puppet.pick("Does that make sense?") == "head_tilt")
assert(puppet.pick("I wonder how that works.") == "head_tilt")
assert(puppet.pick("Then the head goes, first, second.") == "scroll")
assert(puppet.pick("Steps: open settings, pick voice.") == "scroll")
assert(puppet.pick("First, check the logs.") == "count_one")
assert(puppet.pick("Second, restart the service.") == "count_one")
assert(puppet.pick("One thing to note is the timeout.") == "count_one")
assert(puppet.pick("It depends on the driver.") == "shrug")
assert(puppet.pick("Either way works.") == "shrug")
assert(puppet.pick("Maybe try again.") == "shrug")
assert(puppet.pick("Yes, that makes sense.") == "soft_nod")
assert(puppet.pick("Right, exactly.") == "soft_nod")
assert(puppet.pick("That's right.") == "soft_nod")
assert(puppet.pick("The icon sits on the right.") ~= "soft_nod")
assert(puppet.pick("Let's see, it is in the config.") == "recall_glance")
assert(puppet.pick("Check /etc/nixos/flake.nix.") == "recall_glance")
assert(puppet.pick("Overall, the pieces fit together.") == "open_palms")
assert(puppet.pick("In short, it runs at 24 fps.") == "open_palms")
assert(puppet.pick("On the other hand, we can keep it small.") == "right_gesture")
assert(puppet.pick("However, that takes longer.") == "right_gesture")
assert(puppet.pick("Honestly, I'd recommend the smaller sheet.") == "chest_paw")
assert(puppet.pick("Don't worry about it.") == "chest_paw")
assert(puppet.pick("By the way, here is a handy tip.") == "lean_in")
assert(puppet.pick("Good news, the build is green.") == "wing_lift")
assert(puppet.pick("Speaking of that, the audio is ready.") == "wing_lift")
assert(puppet.pick("Glad to help.") == "happy")

-- Uncued sentences vary across talking poses with no immediate repeats.
local seen_variety = {}
local prev = nil
for _ = 1, 40 do
    local p = puppet.pick("It is 4 pm.")
    assert(puppet.poses[p] and puppet.poses[p].talk, "uncued pick must be a talking pose: " .. tostring(p))
    assert(p ~= prev, "uncued pick repeated immediately: " .. tostring(p))
    seen_variety[p] = true
    prev = p
end
local variety_count = 0
for _ in pairs(seen_variety) do variety_count = variety_count + 1 end
assert(variety_count >= 4, "expected variety across talking poses, got " .. variety_count)

-- Transition sources follow the registered hubs and every pose is reachable.
local function direct(a, b)
    local path = puppet.route(a, b)
    return #path == 1 and path[1].kind == "pop" and path[1].to == b
end
assert(direct("neutral", "head_tilt"))
assert(direct("neutral", "lean_in") and direct("listening", "lean_in"))
assert(direct("neutral", "right_gesture") and direct("explain", "right_gesture"))
assert(direct("neutral", "shrug") and direct("explain", "shrug"))
assert(direct("neutral", "wing_lift") and direct("pleased", "wing_lift"))
assert(direct("explain", "count_one") and direct("neutral", "count_one"))
assert(direct("neutral", "soft_nod") and direct("pleased", "soft_nod"))
assert(direct("neutral", "recall_glance") and direct("thinking", "recall_glance"))
assert(direct("explain", "open_palms") and direct("scroll", "open_palms"))
assert(direct("neutral", "chest_paw") and direct("happy", "chest_paw"))
for name in pairs(puppet.poses) do
    if name ~= "neutral" then
        assert(#puppet.route("neutral", name) > 0, "no route neutral -> " .. name)
        assert(#puppet.route(name, "neutral") > 0, "no route " .. name .. " -> neutral")
    end
end

-- Every pose's cell and mouthless talk cell exists inside body.webp and matches the scene declarations.
local function slurp(path)
    local f = assert(io.open(path, "rb"))
    local data = f:read("*a")
    f:close()
    return data
end
local webp = slurp("shell/assistant/dragon/body.webp")
assert(webp:sub(1, 4) == "RIFF" and webp:sub(9, 12) == "WEBP" and webp:sub(13, 16) == "VP8X")
local b1, b2, b3, b4, b5, b6 = webp:byte(25, 30)
local atlas_w = b1 + b2 * 256 + b3 * 65536 + 1
local atlas_h = b4 + b5 * 256 + b6 * 65536 + 1
local cols, rows = assert(slurp("shell/assistant/dragon.plm"):match("cell:%s*ai_dg_frame,%s*(%d+),%s*(%d+)"))
cols, rows = tonumber(cols), tonumber(rows)
local lw, lh = assert(slurp("shell/assistant/state.plm"):match("image%s+ai_dg_body%s*=%s*from%s+ai_dg_body_src,%s*(%d+),%s*(%d+)"))
lw, lh = tonumber(lw), tonumber(lh)
assert(atlas_w == cols * 240 and atlas_h == rows * 240, string.format("atlas %dx%d vs %dx%d grid", atlas_w, atlas_h, cols, rows))
assert(lw == cols * 150 and lh == rows * 150, string.format("logical %dx%d vs %dx%d grid", lw, lh, cols, rows))
local total_cells = cols * rows
local used_frames = {}
for name, p in pairs(puppet.poses) do
    local frame = 165 + p.cell
    assert(frame >= 0 and frame < total_cells, name .. " cell out of atlas: " .. frame)
    assert(not used_frames[frame], "duplicate frame " .. frame .. " for " .. name)
    used_frames[frame] = name
    if p.talk then
        local tframe = 165 + p.talk
        assert(tframe >= 0 and tframe < total_cells, name .. " talk cell out of atlas: " .. tframe)
        assert(not used_frames[tframe], "duplicate talk frame " .. tframe .. " for " .. name)
        used_frames[tframe] = name .. ":talk"
        assert(type(p.mx) == "number" and type(p.my) == "number", name .. " missing mouth offset")
    end
end

puppet.visible(false)
assert(tick == nil)
print("Dragon puppet poses, cues, variety, pacing, transitions, atlas and hearts passed")
