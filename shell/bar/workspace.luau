return function(rail)
    local available, report, preferred = {}, nil, ""
    local function publish()
        -- One preferred monitor shows only its own workspaces; otherwise the rail spans them all.
        local output = preferred:find(",", 1, true) and "" or preferred
        local monitors, seen = {}, {}
        local highest, active = 5, nil
        available = {}
        for slot = 1, 10 do fact["ws_advertised." .. slot] = false end
        for _, workspace in ipairs(report and report.list or {}) do
            local id, monitor = workspace.id, workspace.monitor
            if type(monitor) == "string" and monitor ~= "" and not monitor:find(",", 1, true) and not seen[monitor] then
                monitors[#monitors + 1] = monitor
                seen[monitor] = true
            end
            if type(id) == "number" and id == math.floor(id) and id >= 1 and (output == "" or monitor == output) then
                highest = math.max(highest, id)
                if id <= 10 then available[id] = true; fact["ws_advertised." .. id] = true end
                if output ~= "" and workspace.active then active = id end
            end
        end
        if output == "" and report then
            active = report.active
            if type(active) ~= "number" or active <= 0 or active ~= math.floor(active) then active = nil end
        end
        -- Unset means every monitor, and only the workspace report names them.
        table.sort(monitors)
        text.bar_outputs = preferred ~= "" and preferred or table.concat(monitors, ", ")
        if report then rail.observe(math.min(highest, 10), active) end
    end
    fact.ws_available = sys.watch("workspaces", function(value)
        report = value
        publish()
    end)
    if not fact.ws_available then text.ws_error = "This compositor does not report workspaces" end
    on("workspace_select", function(value)
        if type(value) ~= "number" or value ~= math.floor(value) or value < 1 or value > 10 then return end
        if not available[value] then text.ws_error = "Workspace is not advertised"; return end
        local ok, err = pcall(sys.call, "workspaces.focus", value)
        text.ws_error = ok and "" or tostring(err)
    end)
    return {
        prefer = function(value)
            preferred = type(value) == "string" and value or ""
            publish()
        end,
    }
end
