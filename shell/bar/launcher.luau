-- tohru-launcher reports its state: 0 closed, 1 open, 2 open over its mapped layers, 3 closing.
-- Asking it again each second while it is not closed is what clears a launcher that died open.
return function()
    local function ask()
        if not pcall(sys.call, "say", "tohru-launcher", "emit launcher_report 0") then fact.bar_launcher = 0 end
    end
    on("launcher_bar", function(state)
        state = math.floor(tonumber(state) or 0)
        fact.bar_launcher = (state >= 0 and state <= 3) and state or 0
    end)
    on("launcher_check", ask)
    -- A level asked for before the bar's surface exists is lost, so a bar that starts under an open
    -- launcher asks once its surface is up.
    if after then after(1000, ask) else ask() end
end
