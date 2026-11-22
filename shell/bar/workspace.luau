-- Output ownership is independent of the optional window/workspace protocols.
return function(rail, scene_changed)
    local available, report, preferred, outputs = {}, nil, "", {}
    local function publish_outputs()
        local names, present = {}, {}
        for _, output in ipairs(outputs) do
            local name = output.name
            if type(name) == "string" and name ~= "" and not name:find(",", 1, true) and not present[name] then
                names[#names + 1] = name; present[name] = true
            end
        end
        table.sort(names)
        local selected = {}
        for name in preferred:gmatch("[^,]+") do
            name = name:match("^%s*(.-)%s*$")
            if present[name] then selected[#selected+1] = name end
        end
        -- A temporarily disconnected preferred output falls back to the live outputs.
        text.bar_outputs = table.concat(#selected > 0 and selected or names, ", ")
    end
    local function publish()
        local output = preferred:find(",", 1, true) and "" or preferred
        local list, active = {}, nil
        available = {}
        for slot = 1, 10 do fact["ws_advertised." .. slot] = false; text["ws_name." .. slot] = "" end
        for _, workspace in ipairs(report and report.list or {}) do
            if workspace.id ~= nil and (output == "" or not workspace.monitor or workspace.monitor == "" or workspace.monitor == output) then
                list[#list+1] = workspace
            end
        end
        table.sort(list, function(a,b)
            if type(a.id) == "number" and type(b.id) == "number" then return a.id < b.id end
            return tostring(a.id) < tostring(b.id)
        end)
        -- Generic IDs are opaque. Keep an active workspace visible even beyond the first page.
        local start = 1
        for i,w in ipairs(list) do if w.active or report.active == w.id then start = math.floor((i-1)/10)*10+1; break end end
        local slots = math.min(10, #list - start + 1)
        for slot = 1, slots do
            local workspace = list[start + slot - 1]
            available[slot] = workspace.can_activate ~= false and workspace.id or nil
            fact["ws_advertised." .. slot] = available[slot] ~= nil
            text["ws_name." .. slot] = tostring(workspace.name or workspace.id)
            if workspace.active or report.active == workspace.id then active = slot end
        end
        fact.ws_available = slots > 0
        rail.observe(math.max(0, slots), active)
        text.ws_error = report and (#list > 10 and "Showing active workspace page; use compositor shortcuts for other pages" or "") or "Workspace protocol unavailable"
    end
    local topology, active
    sys.watch("outputs", function(value)
        outputs = value.list or {}
        local names = {}
        for _, output in ipairs(outputs) do names[#names+1] = output.name or "" end
        table.sort(names)
        local identity = table.concat(names, "\n")
        if topology and topology ~= identity and scene_changed then scene_changed() end
        topology = identity
        publish_outputs()
    end)
    sys.watch("workspaces", function(value)
        if active ~= nil and active ~= value.active and scene_changed then scene_changed() end
        active, report = value.active, value
        publish()
    end)
    on("workspace_select", function(value)
        if type(value) ~= "number" or value ~= math.floor(value) or not available[value] then
            text.ws_error = "Workspace activation unavailable"; return
        end
        local ok, err = pcall(sys.call, "workspaces.focus", available[value])
        text.ws_error = ok and "" or tostring(err)
    end)
    publish()
    return { prefer = function(value)
        preferred = type(value) == "string" and value or ""
        publish_outputs(); publish()
    end }
end
