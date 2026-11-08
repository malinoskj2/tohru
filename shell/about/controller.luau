return function(snapshot, more)
    local function value(item) if item == nil or item == "" then return "Unknown" end; return tostring(item) end
    local function bytes(number) return type(number)=="number" and string.format("%.1f GiB",number/1073741824) or "Unknown" end
    local function refresh()
        local ok,data=pcall(snapshot or function()error("System information binding unavailable")end)
        if not ok or type(data)~="table" then text.about_error=ok and "System information unavailable" or tostring(data);data={}
        else text.about_error=#(data.issues or {})>0 and "Some system facts are unavailable" or "" end
        local system=data.system or {}
        text.about_name=value(system.os_name);text.about_version=value(system.os_version)
        local startup
        for _,mount in ipairs(data.mounts or {}) do
            if mount.path=="/" then
                for _,disk in ipairs(data.disks or {}) do if mount.source=="/dev/"..disk.name then startup=value(disk.model or disk.name).." · "..bytes(disk.capacity_bytes);break end end
                break
            end
        end
        model.about_specs={
            {label="Name",value=value(system.hostname)},
            {label="Processor",value=value(system.processor)},
            {label="Graphics",value=#(system.graphics or {})>0 and table.concat(system.graphics,", ") or "Unknown"},
            {label="Memory",value=bytes(system.memory_total_bytes)},
            {label="Startup disk",value=value(startup)},
            {label="Configuration",value=value(system.configuration_revision)},
        }
    end
    local function open() fact.about_visible=true;refresh() end
    local function close() fact.about_visible=false end
    local function toggle() if fact.about_visible then close() else open() end end
    on("about_open",open);on("about_close",close);on("about_toggle",toggle);on("about_refresh",refresh)
    on("about_more",function()
        local ok,result=pcall(more or function()error("Information page unavailable")end,0)
        if ok and result~=false then close() else text.about_error=ok and "Information page unavailable" or tostring(result) end
    end)
    on("about_repository",function()
        local ok,err=pcall(sys.call,"apps.launch","xdg-open https://github.com/malinoskj2/nix")
        text.about_error=ok and "Open request sent" or tostring(err)
    end)
    return {open=open,close=close,toggle=toggle,refresh=refresh}
end
