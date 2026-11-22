-- Native verification harness: real shipped puppet and renderer, synthetic voice events.
local p = require("assistant/puppet")(require("assistant/full-body"))
local speaking, elapsed = false, 0
local keys = require("assistant/full-body")
math.randomseed(41)
p.load("@ART@")
after(3500, function() fact.ai_dragon_open = true; p.visible(true) end)
on("review_listen", function() speaking = false; p.hearing(true) end)
on("review_think", function() speaking = false; p.thinking() end)
on("review_speak", function() speaking = true; p.speaking(true) end)
on("review_sentence", function() p.sentence(text.ai_draft) end)
on("review_calm", function() speaking = false; p.calm() end)
on("review_finish", function() speaking = false; p.speaking(false) end)
on("review_pause", function() speaking = false; p.speaking(false, "paused") end)
on("review_interrupt", function() speaking = false; p.speaking(false, "interrupted") end)
on("review_reduced", function(n) p.reduced(n ~= 0) end)
on("review_key", function(n)
    local k = keys[n + 1]
    if not k then return end
    speaking = false
    p.visible(false)
    fact.ai_dg_frame = 165 + k.cell
    fact.ai_dg_face = k.face * 24
    fact.ai_dg_fx, fact.ai_dg_fy = k.fx, k.fy
    fact.ai_dg_eye, fact.ai_dg_mouth = -1, -1
    fact.ai_dg_tilt, fact.ai_dg_sq = 0, 0
end)
on("review_live", function() p.calm(); p.visible(true) end)
every(100, function()
    text.review_pose = p.pose()
    elapsed = elapsed + 0.1
    local phase = math.floor(elapsed * 10) % 40
    local level = speaking and phase >= 8 and 0.5 or 0
    fact.ai_mouth = level
    p.mouth(level, level == 0 and 0 or math.floor(elapsed * 10) % 7 + 1)
end)
