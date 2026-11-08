-- Cached read-only presentation. Snapshot only when opening a data page/refresh.
return function(snapshot, outputs)
    local rows = {}
    local function value(item)
        if item == nil or item == "" then return "Unknown" end
        if type(item) == "boolean" then return item and "Yes" or "No" end
        return tostring(item)
    end
    local function bytes(number)
        if type(number) ~= "number" or number < 0 or number ~= number then return "Unknown" end
        return string.format("%.1f GiB",number / 1073741824)
    end
    local function add(label,item) if #rows < 256 then rows[#rows + 1] = {label=label,value=value(item)} end end
    local function visible()
        local view = {}
        fact.info_offset = math.max(0,math.min(fact.info_offset or 0,math.max(0,#rows - 8)))
        for index = fact.info_offset + 1,math.min(#rows,fact.info_offset + 8) do view[#view+1]=rows[index] end
        model.info_rows = view
        fact.info_total = #rows
    end
    local function cache()
        model.info_cache = rows
        fact.info_cached_page = fact.info_page
        fact.info_cache_valid = true
        visible()
    end
    local function refresh()
        rows = {}; fact.info_offset = 0; text.info_error = ""
        local page = fact.info_page
        if page == 1 then
            local ok,list = pcall(outputs or function() return {} end)
            if not ok then text.info_error = tostring(list); list = {} end
            for index,output in ipairs(list or {}) do
                if index > 8 then break end
                add("Output",output.name)
                add("Logical size",output.width and output.height and (value(output.width).." × "..value(output.height)) or nil)
                add("Current mode",nil);add("Scale",nil);add("Refresh rate",nil)
            end
            if #rows == 0 then add("Output",nil);add("Logical size",nil);add("Current mode",nil);add("Scale",nil);add("Refresh rate",nil) end
        elseif page == 2 then
            add("Timezone",nil)
        elseif page == 6 then
            -- Live native battery facts are drawn directly by the scene.
        else
            local ok,data = pcall(snapshot or function() error("System information binding unavailable") end)
            if not ok or type(data) ~= "table" then text.info_error = ok and "System information unavailable" or tostring(data); data = {} end
            local system = data.system or {}
            if page == 0 then
                add("System",system.os_name);add("Version",system.os_version);add("Name",system.hostname)
                add("Kernel",system.kernel);add("Processor",system.processor)
                add("Graphics",#(system.graphics or {}) > 0 and table.concat(system.graphics,", ") or nil)
                add("Memory",bytes(system.memory_total_bytes));add("Available memory",bytes(system.memory_available_bytes))
                add("Uptime",system.uptime_seconds and (value(system.uptime_seconds).." seconds") or nil)
                add("Active generation",system.current_generation);add("Configuration",system.configuration_revision)
            elseif page == 3 then
                local current
                for _,account in ipairs(data.accounts or {}) do if account.uid == system.current_uid then current = account; break end end
                local function account_row(account)
                    add(account.name,(account.description ~= "" and value(account.description).." · " or "").."UID "..value(account.uid).." · "..value(account.home).." · "..value(account.shell))
                end
                if current then account_row(current) end
                for _,account in ipairs(data.accounts or {}) do if account ~= current then account_row(account) end end
                if #(data.accounts or {}) == 0 then add("Accounts",nil) end
                text.info_current_user = current and value(current.name).." · UID "..value(current.uid) or "Unknown"
            elseif page == 4 then
                for _,disk in ipairs(data.disks or {}) do
                    add(disk.name,bytes(disk.capacity_bytes).." · "..value(disk.model).." · removable: "..value(disk.removable))
                end
                for _,mount in ipairs(data.mounts or {}) do
                    add(mount.path,value(mount.source).." · "..value(mount.filesystem)..(mount.read_only and " · read only" or ""))
                end
                if #(data.disks or {}) == 0 and #(data.mounts or {}) == 0 then add("Storage",nil) end
                add("Filesystem free space",nil)
            elseif page == 5 then
                text.info_generation = value(system.current_generation)
                for _,generation in ipairs(data.generations or {}) do
                    add("Generation "..value(generation.number)..(generation.current and " · active" or ""),generation.target)
                end
                if #(data.generations or {}) == 0 then add("Generations",nil) end
            end
            if #(data.issues or {}) > 0 then text.info_error = "Some facts are unavailable or partial — "..#data.issues.." source reports" end
        end
        cache()
    end
    local function open(page)
        if type(page) ~= "number" or page ~= math.floor(page) or page < 0 or page > 6 then page = 0 end
        fact.info_visible = true;fact.info_page = page;refresh()
    end
    local function close() fact.info_visible = false end
    local function toggle() if fact.info_visible then close() else open(fact.info_page) end end
    on("info_open",open);on("info_close",close);on("info_toggle",toggle)
    on("info_page",open);on("info_refresh",refresh)
    on("info_scroll",function(delta) fact.info_offset = fact.info_offset + delta;visible() end)
    -- Native facts/texts outlive VM reload: restore the cached page without I/O.
    if fact.info_cache_valid and fact.info_cached_page == fact.info_page then
        for index=0,math.min(255,(fact["info_cache.count"] or 0)-1) do
            rows[#rows+1]={label=text["info_cache."..index..".label"] or "",value=text["info_cache."..index..".value"] or "Unknown"}
        end
        visible()
    end
    return {open=open,close=close,toggle=toggle,refresh=refresh}
end
