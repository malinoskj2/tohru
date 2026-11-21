-- The notification daemon owns unread and Do Not Disturb; the bar shows what it reports.
-- The bar tells it where to stack (notifications.position) and when its popups cover the top right.
return function(on_center_open)
    local position, cover, palette_id = nil, 0, nil
    local function ask(request)
        pcall(sys.call, "say", "tohru-notifications", "emit " .. request)
    end
    on("notices_state", function(state)
        state = math.floor(tonumber(state) or 0)
        fact.notices_unread = state % 2 == 1
        fact.notices_dnd = state >= 2
    end)
    on("notices_center_state", function(open)
        local is_open = math.floor(tonumber(open) or 0) == 1
        fact.notices_center_open = is_open
        if is_open and on_center_open then on_center_open() end
    end)
    on("bell_seen", function() ask("notices_seen 0") end)
    on("bell_toggle", function()
        if not fact.notices_center_open and on_center_open then on_center_open() end
        ask("notices_center_toggle 0")
    end)
    on("bell_dnd", function() ask("notices_dnd 2") end)
    on("bar_popups", function(open)
        cover = open == 1 and 1 or 0
        ask("notices_cover " .. cover)
    end)
    on("notices_ask", function()
        ask("notices_position " .. (position or 0))
        ask("notices_cover " .. cover)
        if palette_id ~= nil then ask("notices_palette " .. palette_id) end
    end)
    ask("notices_report 0")
    return {
        -- 0 top right, 1 bottom right.
        place = function(value)
            if value == position then return end
            position = value
            ask("notices_position " .. value)
        end,
        palette = function(value)
            if value == palette_id then return end
            palette_id = value
            ask("notices_palette " .. value)
        end,
        close_center = function()
            if fact.notices_center_open then
                fact.notices_center_open = false
                ask("notices_center_close 0")
            end
        end,
    }
end
