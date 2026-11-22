-- Cached read-only presentation. Snapshot only when opening the overview or a data page, or on refresh.
return function(snapshot, outputs, rail)
    local rows = {}
    local function value(item)
        if item == nil or item == "" then return "Unknown" end
        if type(item) == "boolean" then return item and "Yes" or "No" end
        return tostring(item)
    end
    local function number(item) return type(item) == "number" and item >= 0 and item == item and item < math.huge end
    local function bytes(item)
        if not number(item) then return "Unknown" end
        return string.format("%.1f GiB",item / 1073741824)
    end
    local function thin(item) if item >= 10 or item == math.floor(item) then return string.format("%d",math.floor(item + .5)) end; return string.format("%.1f",item) end
    local function hero(page,big,unit,note)
        text["info_big."..page] = big or "—"
        text["info_unit."..page] = unit or ""
        text["info_note."..page] = note or ""
    end
    local function processor(name)
        if type(name) ~= "string" or name == "" then return nil end
        local short = name:gsub("%(R%)",""):gsub("%(TM%)",""):gsub("%d+%-Core",""):gsub("Processor",""):gsub("CPU",""):gsub("^%s*AMD%s+",""):gsub("^%s*Intel%s+","")
        local words = {}
        for word in short:gmatch("%S+") do if #words < 2 then words[#words+1] = word end end
        return #words > 0 and table.concat(words," ") or nil
    end
    local function summarise(data)
        local system = data.system or {}
        local total,available = system.memory_total_bytes,system.memory_available_bytes
        local kernel = type(system.kernel) == "string" and system.kernel:match("^(%d+%.%d+)") or nil
        local caption = {}
        if processor(system.processor) then caption[#caption+1] = processor(system.processor) end
        if kernel then caption[#caption+1] = kernel end
        text.info_caption = #caption > 0 and table.concat(caption," · ") or "Unknown"
        local note = {}
        if number(available) then note[#note+1] = bytes(available).." available" end
        if system.os_name or system.os_version then note[#note+1] = table.concat({system.os_name,system.os_version}," ") end
        hero(0,number(total) and thin(total / 1073741824) or nil,number(total) and "GiB" or nil,#note > 0 and table.concat(note," · ") or "Memory unknown")
        local accounts,current = data.accounts or {},nil
        for _,account in ipairs(accounts) do if account.uid == system.current_uid then current = account; break end end
        hero(3,#accounts > 0 and tostring(#accounts) or nil,#accounts == 1 and "account" or (#accounts > 0 and "accounts" or nil),current and "Current "..value(current.name).." · UID "..value(current.uid) or "Current account unknown")
        local capacity,disks,mounts = 0,#(data.disks or {}),#(data.mounts or {})
        for _,disk in ipairs(data.disks or {}) do if number(disk.capacity_bytes) then capacity = capacity + disk.capacity_bytes end end
        local shown = capacity >= 1e12 and {thin(capacity / 1e12),"TB"} or (capacity > 0 and {thin(capacity / 1e9),"GB"} or {})
        local devices = disks.." block device"..(disks == 1 and "" or "s").." · "..mounts.." mount"..(mounts == 1 and "" or "s")
        hero(4,shown[1],shown[2],devices)
        text.info_storage_caption = disks.." disk"..(disks == 1 and "" or "s").." · "..mounts.." mount"..(mounts == 1 and "" or "s")
        local nix = system.system_target ~= nil
        text.info_platform_title = nix and "Generations" or "Platform"
        text.info_platform_short = nix and "Gen" or "OS"
        text.info_platform_opening = nix and "Unfolding Generations…" or "Unfolding Platform…"
        local generations = #(data.generations or {})
        hero(5,system.current_generation and value(system.current_generation) or nil,nil,generations.." generation"..(generations == 1 and "" or "s")..(system.current_generation and " · active" or ""))
        text.info_generation_caption = nix and generations.." generation"..(generations == 1 and "" or "s") or value(system.os_name)
        if not nix then hero(5, system.os_name or "Linux", nil, system.os_version or "System information") end
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
    local function read()
        local ok,data = pcall(snapshot or function() error("System information binding unavailable") end)
        if not ok or type(data) ~= "table" then text.info_error = ok and "System information unavailable" or tostring(data); data = {} end
        summarise(data)
        if #(data.issues or {}) > 0 then text.info_error = "Some facts are unavailable or partial — "..#data.issues.." source reports" end
        fact.info_partial = text.info_error ~= ""
        return data
    end
    local function refresh()
        rows = {}; fact.info_offset = 0; text.info_error = ""
        local page = fact.info_page
        -- The overview's tiles need a snapshot whichever page is chosen; pages 1, 2 and 6 alone do not.
        local data = (not fact.info_unfolded or page == 0 or page >= 3 and page <= 5) and read() or nil
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
            local first = (list or {})[1]
            hero(1,#(list or {}) > 0 and tostring(#list) or nil,#(list or {}) == 1 and "output" or (#(list or {}) > 0 and "outputs" or nil),
                first and value(first.name)..(first.width and first.height and " · "..value(first.width).." × "..value(first.height) or "") or "No outputs reported")
        elseif page == 2 then
            add("Timezone",nil)
        elseif page == 6 then
            -- Live native battery facts are drawn directly by the scene.
        else
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
            elseif page == 5 and not system.system_target then
                add("Operating system", system.os_name); add("Version", system.os_version)
                add("Kernel", system.kernel)
            elseif page == 5 then
                text.info_generation = value(system.current_generation)
                for _,generation in ipairs(data.generations or {}) do
                    add("Generation "..value(generation.number)..(generation.current and " · active" or ""),generation.target)
                end
                if #(data.generations or {}) == 0 then add("Generations",nil) end
            end
        end
        cache()
    end
    local function valid(page) return type(page) == "number" and page == math.floor(page) and page >= 0 and page <= 6 end
    local function track()
        if rail and fact.info_visible then rail.observe(7,fact.info_page + 1) end
    end
    -- A page argument opens that page unfolded; none opens the overview with the last page chosen.
    local function open(page)
        local unfolded = valid(page)
        fact.info_visible = true
        fact.info_unfolded = unfolded
        if unfolded then fact.info_page = page elseif not valid(fact.info_page) then fact.info_page = 0 end
        refresh()
        track()
    end
    -- The overview with a page already chosen on the rail, as About's More Info opens it.
    local function overview(page)
        if valid(page) then fact.info_page = page end
        open()
    end
    local function close()
        fact.info_visible = false
        fact.info_unfolded = false
        if rail then rail.reset() end
    end
    local function toggle() if fact.info_visible then close() else open() end end
    local function unfold(page)
        if not valid(page) then page = fact.info_page end
        if not valid(page) then return end
        if not fact.info_visible then open(page); return end
        local same = page == fact.info_page and fact.info_cache_valid and fact.info_cached_page == page and page ~= 6
        fact.info_page = page
        fact.info_unfolded = true
        if not same then refresh() end
        track()
    end
    local function fold()
        if not fact.info_unfolded then return end
        fact.info_unfolded = false
        track()
    end
    -- Folded, the arrows choose a page on the rail; unfolded, they turn to it.
    local function step(delta)
        if not fact.info_visible or type(delta) ~= "number" then return end
        local page = math.max(0,math.min(6,(valid(fact.info_page) and fact.info_page or 0) + (delta > 0 and 1 or (delta < 0 and -1 or 0))))
        if page == fact.info_page then return end
        if fact.info_unfolded then unfold(page) else fact.info_page = page; track() end
    end
    on("info_open",open);on("info_close",close);on("info_toggle",toggle)
    on("info_page",unfold);on("info_unfold",function() unfold() end);on("info_fold",fold)
    on("info_fold_toggle",function() if fact.info_unfolded then fold() else unfold() end end)
    on("info_step",step);on("info_refresh",refresh)
    on("info_scroll",function(delta) fact.info_offset = fact.info_offset + delta;visible() end)
    -- Native facts/texts outlive VM reload: restore the cached page without I/O.
    if fact.info_cache_valid and fact.info_cached_page == fact.info_page then
        for index=0,math.min(255,(fact["info_cache.count"] or 0)-1) do
            rows[#rows+1]={label=text["info_cache."..index..".label"] or "",value=text["info_cache."..index..".value"] or "Unknown"}
        end
        visible()
    end
    return {open=open,overview=overview,close=close,toggle=toggle,refresh=refresh,unfold=unfold,fold=fold,step=step}
end
