-- The notification daemon owns unread and Do Not Disturb; the bar shows what it reports.
return function()
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
    ask("notices_report 0")
end
