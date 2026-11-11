-- One rail's goal and per-switch draws. The motion itself is the scene's springs (bar/rail-state.plm).
return function(index)
    local function key(name) return "rail." .. name .. "." .. index end
    local active = fact[key("present")] and math.floor((fact[key("goal")] - 8) / 16) + 1 or nil
    local function observe(slots, slot)
        fact[key("slots")] = slots
        if slot and (slot < 1 or slot > slots) then slot = nil end
        fact[key("present")] = slot ~= nil
        if slot == active then return end
        local from = active
        active = slot
        if not slot then return end
        fact[key("goal")] = (slot - 1) * 16 + 8
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
    return {
        observe = observe,
        reset = function()
            active = nil
            fact[key("present")] = false
        end,
    }
end
