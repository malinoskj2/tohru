-- One rail's goal and per-switch draws. The motion itself is the scene's springs (bar/rail-state.plm).
-- `pitch` is a slot's length along the rail: 16 px unless said.
return function(index, pitch)
    pitch = pitch or 16
    local function key(name) return "rail." .. name .. "." .. index end
    local active = fact[key("present")] and math.floor(fact[key("goal")] / pitch) + 1 or nil
    local function observe(slots, slot)
        fact[key("slots")] = slots
        if slot and (slot < 1 or slot > slots) then slot = nil end
        fact[key("present")] = slot ~= nil
        if slot == active then return end
        local from = active
        active = slot
        if not slot then return end
        fact[key("goal")] = (slot - 0.5) * pitch
        if not from then
            -- A bead that appears lands in place.
            fact[key("land")] = fact[key("land")] + 1
            return
        end
        fact[key("kind")] = math.random(0, 4)
        fact[key("draw")] = 0.8 + math.random() * 0.45
        fact[key("reach")] = 9 + math.random() * 7
        fact[key("life")] = 0.38 + math.random() * 0.22
        -- Written last: the scene's switch rules fire on it and read the facts above.
        fact[key("serial")] = fact[key("serial")] + 1
    end
    -- A rail with no slots (the sound popup's volume) moves in pixels, with the sound mock's fixed draws.
    local function bump(name) fact[key(name)] = (fact[key(name)] or 0) + 1 end
    local target, heading = fact[key("target")] or 8, 0
    local function place(goal)
        target, heading = goal, 0
        fact[key("goal")] = goal
        bump("land")
    end
    local function glide(goal)
        target, heading = goal, 0
        fact[key("goal")] = goal
        fact[key("kind")] = 2
        fact[key("draw")] = 1
        fact[key("reach")] = 12
        fact[key("life")] = 0.5
        bump("serial")
    end
    -- The bead chases a dragged goal through its own spring; turning back starts a fresh crest.
    local function chase(goal)
        local delta = goal - target
        if math.abs(delta) > 1e-4 then
            local direction = delta > 0 and 1 or -1
            if direction ~= heading then
                heading = direction
                fact[key("dir")] = direction
                fact[key("gain")] = 0.95
                bump("crest")
            end
        end
        target = goal
        fact[key("target")] = goal
    end
    return {
        observe = observe,
        place = place,
        glide = glide,
        chase = chase,
        target = function() return target end,
        reset = function()
            active = nil
            fact[key("present")] = false
        end,
    }
end
