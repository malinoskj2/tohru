return function(make_rail, binding)
    local available = {}
    local rail = make_rail(binding, "workspace", "ws_")
    fact.ws_available = sys.watch("workspaces", function(value)
        local highest = 5
        available = {}
        for slot = 1, 10 do fact["ws_advertised." .. slot] = false end
        for _, workspace in ipairs(value.list or {}) do
            local id = workspace.id
            if type(id) == "number" and id == math.floor(id) and id >= 1 then
                highest = math.max(highest, id)
                if id <= 10 then available[id] = true; fact["ws_advertised." .. id] = true end
            end
        end
        local active = value.active
        if type(active) ~= "number" or active <= 0 or active ~= math.floor(active) then active = nil end
        rail.observe(highest, active)
    end)
    if not fact.ws_available then text.ws_error = "This compositor does not report workspaces" end
    on("workspace_select", function(value)
        if type(value) ~= "number" or value ~= math.floor(value) or value < 1 or value > 10 then return end
        if not available[value] then text.ws_error = "Workspace is not advertised"; return end
        local ok, err = pcall(sys.call, "workspaces.focus", value)
        text.ws_error = ok and "" or tostring(err)
    end)
    return {stop = rail.stop}
end
