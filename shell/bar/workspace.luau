return function(make_rail, binding)
    local rail = make_rail(binding, "workspace", "ws_")
    fact.ws_available = sys.watch("workspaces", function(value)
        local highest = 5
        for _, workspace in ipairs(value.list or {}) do
            if type(workspace.id) == "number" and workspace.id == math.floor(workspace.id) and workspace.id > highest then highest = workspace.id end
        end
        local active = value.active
        if type(active) ~= "number" or active <= 0 or active ~= math.floor(active) then active = nil end
        rail.observe(highest, active)
    end)
    if not fact.ws_available then text.ws_error = "This compositor does not report workspaces" end
    on("workspace_select", function(value)
        if value ~= math.floor(value) or value < 1 or value > 10 then return end
        local ok, err = pcall(sys.call, "workspaces.focus", value)
        text.ws_error = ok and "" or tostring(err)
    end)
    return {stop = rail.stop}
end
