-- The notification daemon owns unread and Do Not Disturb; the bar shows what it reports.
-- The bar tells it where to stack (notifications.position) and when its popups cover the top right.
return function()
    local position, cover = nil, 0
    local function ask(request)
        pcall(sys.call, "say", "tohru-notifications", "emit " .. request)
    end
    on("notices_state", function(state)
        state = math.floor(tonumber(state) or 0)
        fact.notices_unread = state % 2 == 1
        fact.notices_dnd = state >= 2
    end)
    on("bell_seen", function() ask("notices_seen 0") end)
    on("bell_dnd", function() ask("notices_dnd 2") end)
    on("bar_popups", function(open)
        cover = open == 1 and 1 or 0
        ask("notices_cover " .. cover)
    end)
    on("notices_ask", function()
        ask("notices_position " .. (position or 0))
        ask("notices_cover " .. cover)
    end)
    ask("notices_report 0")
    return {
        -- 0 top right, 1 bottom right.
        place = function(value)
            if value == position then return end
            position = value
            ask("notices_position " .. value)
        end,
    }
end
