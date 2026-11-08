-- One settings owner persists proposals; this module owns transient picker state only.
return function(wallpaper, read, commit, save_folder, connected_outputs)
    local PAGE = 8
    local files, filtered, outputs, page, selection, output_index = {}, {}, {}, 0, 0, 1
    local loaded_folder
    local function fail(message) text.wallpaper_picker_error = message or "" end
    local function settings()
        local value = read()
        if type(value) ~= "table" or type(value.assignments) ~= "table" or type(value.folder) ~= "string" then error("Wallpaper settings snapshot is unavailable") end
        local count, assignments = 0, {}
        for output, path in pairs(value.assignments) do
            wallpaper.validate(output, path)
            count = count + 1
            if count > 32 then error("Wallpaper assignment capacity is32 outputs") end
            assignments[output] = path
        end
        return {assignments=assignments,folder=value.folder,count=count}
    end
    local function target()
        local output = outputs[output_index]
        if not output then error("Choose an output before assigning wallpaper") end
        return output.name
    end
    local function rows()
        local value = settings()
        local query = (text.wallpaper_picker_search or ""):sub(1,256):lower()
        filtered = {}
        for _, path in ipairs(files) do
            local name = path:match("([^/]+)$") or path
            if name:lower():find(query, 1, true) then filtered[#filtered + 1] = path end
        end
        local pages = math.max(1, math.ceil(#filtered / PAGE))
        page = math.max(0, math.min(page, pages - 1))
        selection = math.max(0, math.min(selection, math.max(0, math.min(PAGE, #filtered-page*PAGE)-1)))
        fact.wallpaper_picker_selected = selection
        fact.wallpaper_picker_previous, fact.wallpaper_picker_next = page > 0, page + 1 < pages
        text.wallpaper_picker_count = tostring(#filtered) .. " files · page " .. (page+1) .. "/" .. pages
        local output = outputs[output_index]
        text.wallpaper_picker_output = output and (output.name .. (output.connected and "" or " · disconnected")) or "No output available"
        text.wallpaper_picker_current = output and (value.assignments[output.name] or "No wallpaper assigned") or ""
        local visible = {}
        for index = page*PAGE+1, math.min(#filtered, (page+1)*PAGE) do
            local path = filtered[index]
            visible[#visible+1] = {row_id=index-page*PAGE-1, name=path:match("([^/]+)$") or path,
                kind=wallpaper.validate("catalog",path), current=output and value.assignments[output.name] == path or false}
        end
        model.wallpaper_picker_files = visible
    end
    local function refresh_outputs(value)
        local previous = outputs[output_index] and outputs[output_index].name
        local seen, next_outputs = {}, {}
        local connected = connected_outputs()
        if type(connected) ~= "table" or #connected > 32 then error("Wallpaper output information is unavailable") end
        for _, output in ipairs(connected) do
            wallpaper.validate(output.name, "/validation.png")
            if output.routable == false then error("Output cannot be represented by native screen selectors") end
            if seen[output.name] then error("Duplicate native output name") end
            seen[output.name] = true
            next_outputs[#next_outputs+1] = {name=output.name, connected=true}
        end
        for output in pairs(value.assignments) do
            if not seen[output] then next_outputs[#next_outputs+1] = {name=output, connected=false} end
        end
        table.sort(next_outputs,function(a,b) return a.name < b.name end)
        outputs, output_index = next_outputs, 1
        local requested = text.wallpaper_request_output
        for index, output in ipairs(outputs) do
            if output.name == (requested ~= "" and requested or previous) then output_index=index; break end
        end
    end
    local function catalog(folder)
        if folder == "" then return {} end
        local value = sys.ask("wallpaper.catalog", folder)
        if type(value) ~= "table" or #value > 4096 then error("Wallpaper catalog exceeds4096 files or is unavailable") end
        local seen = {}
        for _, path in ipairs(value) do
            wallpaper.validate("catalog", path)
            if seen[path] then error("Duplicate canonical wallpaper catalog entry") end
            seen[path] = true
        end
        return value
    end
    local function attempt(action)
        local ok, err = pcall(action)
        fail(ok and "" or tostring(err))
        return ok
    end
    local function refresh()
        local value = settings()
        refresh_outputs(value)
        if loaded_folder ~= value.folder then files, loaded_folder = catalog(value.folder), value.folder; page, selection=0,0 end
        rows()
    end
    local function open()
        local ok = attempt(function()
            local value = settings()
            text.wallpaper_picker_folder = value.folder
            text.wallpaper_picker_search = ""
            refresh()
        end)
        fact.wallpaper_picker_open = true
        return ok
    end
    local function choose(index)
        return attempt(function()
            if type(index) ~= "number" or index ~= math.floor(index) or index < 0 or index >= PAGE then error("Invalid wallpaper row") end
            local path = filtered[page*PAGE+index+1]
            if not path then return end
            local value, output = settings(), target()
            wallpaper.validate(output,path)
            if value.assignments[output] == path then selection=index; rows(); return end
            if not value.assignments[output] and value.count >= 32 then error("Wallpaper assignment capacity is32 outputs; nothing was changed") end
            value.assignments[output] = path
            commit(value.assignments) -- Entire map is accepted/persisted once, or nothing changes.
            selection=index
            rows()
        end)
    end
    local function randomize(targets)
        return attempt(function()
            local value = settings()
            targets = targets or {}
            if type(targets) ~= "table" or #targets > 4096 then error("Wallpaper randomization requires at most4096 output targets") end
            for index, output in pairs(targets) do
                if type(index) ~= "number" or index ~= math.floor(index) or index < 1 or index > #targets then error("Wallpaper randomization targets must be an array") end
                wallpaper.validate(output, "/validation.png")
            end
            local current = catalog(value.folder)
            -- Fisher-Yates: no filesystem, output assignment or random choice inside sort.
            for i=#current,2,-1 do local j=math.random(i); current[i],current[j]=current[j],current[i] end
            local assigned, identities = {}, {}
            for _, path in pairs(value.assignments) do assigned[#assigned+1] = path end
            if #assigned > 0 and #current > 0 then
                local keys=sys.ask("wallpaper.identities",assigned)
                if type(keys) ~= "table" or #keys ~= #assigned then error("Canonical wallpaper identities are unavailable") end
                for i,path in ipairs(assigned) do identities[path]=keys[i] end
            end
            for _,path in ipairs(current) do if not identities[path] then identities[path]=path end end
            local proposal=wallpaper.randomize(value.assignments,targets or {},current,function(path) return identities[path] end)
            commit(proposal)
            files,loaded_folder=current,value.folder
            rows()
        end)
    end
    on("wallpaper_picker_open_request",open)
    on("wallpaper_picker_close",function() fact.wallpaper_picker_open=false end)
    on("wallpaper_picker_refresh",function() attempt(function() loaded_folder=nil; refresh() end) end)
    on("wallpaper_picker_folder_submit",function()
        attempt(function()
            local folder=text.wallpaper_picker_folder
            local next_files=catalog(folder)
            save_folder(folder)
            files,loaded_folder,page,selection=next_files,folder,0,0
            rows()
        end)
    end)
    on("wallpaper_picker_output",function(delta)
        attempt(function()
            if delta ~= -1 and delta ~= 1 then return end
            if #outputs > 0 then output_index=((output_index-1+delta)%#outputs)+1 end
            text.wallpaper_request_output=""
            rows()
        end)
    end)
    on("wallpaper_picker_page",function(delta)
        attempt(function() if delta == -1 or delta == 1 then page=page+delta; selection=0; rows() end end)
    end)
    on("wallpaper_picker_move",function(delta)
        attempt(function()
            if delta ~= -1 and delta ~= 1 then return end
            local index=page*PAGE+selection+delta
            if index >= 0 and index < #filtered then page=math.floor(index/PAGE); selection=index%PAGE; rows() end
        end)
    end)
    on("wallpaper_picker_choose",choose)
    on("wallpaper_picker_accept",function() choose(selection) end)
    on("wallpaper_picker_clear",function()
        attempt(function() local value=settings(); value.assignments[target()]=nil; commit(value.assignments); rows() end)
    end)
    on("wallpaper_picker_randomize",function()
        local ok, output=pcall(target)
        if ok then randomize({output}) else fail(tostring(output)) end
    end)
    on("wallpaper_randomize",function()
        attempt(function()
            local requested=text.wallpaper_randomize_outputs
            if requested ~= "" and not requested:match("^%s*%[") then error("Wallpaper randomization targets must be a JSON array") end
            local targets=requested == "" and {} or json.decode(requested)
            if type(targets) ~= "table" then error("Wallpaper randomization targets must be an array") end
            local ok=randomize(targets)
            if not ok then error(text.wallpaper_picker_error) end
        end)
    end)
    on("text:wallpaper_picker_search",function() page,selection=0,0; attempt(rows) end)
    return {open=open, refresh=function() return attempt(refresh) end, choose=choose, randomize=randomize}
end
