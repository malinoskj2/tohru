-- Navigation belongs to this window; all values belong to the existing process owner.
return function(applied, wallpaper)
    local pages = {"Bar", "Appearance", "System", "Wallpaper", "Capture"}
    local history, position, rows = {1}, 1, {}
    local snapshot = tohru_settings.snapshot()
    local function refresh(settings)
        snapshot = settings or snapshot
        local page = history[position]
        fact.settings_page = page
        fact.settings_can_back = position > 1
        fact.settings_can_forward = position < #history
        local query = string.sub(text.settings_search or "", 1, 256):lower()
        fact.settings_wallpaper_page = page==4 and query==""
        fact.settings_capture_page = page==5 and query==""
        text.settings_heading = query ~= "" and "Search results" or pages[page]
        local categories = {}
        for id, title in ipairs(pages) do categories[id] = {id=id, title=title, selected=id==page} end
        model.settings_categories = categories
        local mooncoil = snapshot["bar.mooncoil"]
        local managed_detail = mooncoil.managed and "Nix managed · runtime edits last until shell exits" or "Saved in writable config.toml"
        if mooncoil.runtime_override then managed_detail = "Nix managed · temporary runtime override is active" end
        local catalog = {
            {page=1, key="bar.mooncoil", title="Mooncoil mark", detail=managed_detail,
             value=mooncoil.value, editable=true, managed=mooncoil.managed, temporary=mooncoil.runtime_override},
            {page=1, title="Outputs", detail="Native bar is shown on every output", editable=false},
            {page=2, key="appearance.corners", title="Screen corners", detail="Native corner overlays on every output",
             value=snapshot["appearance.corners"].value, editable=true, managed=snapshot["appearance.corners"].managed,
             temporary=snapshot["appearance.corners"].runtime_override},
            {page=2, title="Corner size", detail=tostring(snapshot["appearance.corner_size"].value) .. " logical pixels", editable=false},
            {page=2, key="wallpaper.enabled", title="Wallpaper enabled", detail="Native static backgrounds; video integration pending",
             value=snapshot["wallpaper.enabled"].value, editable=true, managed=snapshot["wallpaper.enabled"].managed,
             temporary=snapshot["wallpaper.enabled"].runtime_override},
            {page=2, key="wallpaper.fit", title="Wallpaper fit", choice=true,
             detail=snapshot["wallpaper.fit"].value .. " · Activate to cycle Fill, Fit, Centre, Tile (static images)",
             editable=true, managed=snapshot["wallpaper.fit"].managed, temporary=snapshot["wallpaper.fit"].runtime_override},
            {page=5,key="capture.save",title="Save screenshots",detail="Native PNG export after a completed region selection",
             value=snapshot["capture.save"].value,editable=true,managed=snapshot["capture.save"].managed,temporary=snapshot["capture.save"].runtime_override},
            {page=5,key="capture.copy",title="Copy screenshots",detail="Native image/png clipboard after a completed selection",
             value=snapshot["capture.copy"].value,editable=true,managed=snapshot["capture.copy"].managed,temporary=snapshot["capture.copy"].runtime_override},
            {page=3, title="Shell", detail="Tohru · pinned native Pleamar 0.7.27", editable=false},
        }
        rows = {}
        for _, row in ipairs(catalog) do
            local matches = (row.title .. " " .. row.detail .. " " .. (row.key or "")):lower():find(query, 1, true)
            if query ~= "" and matches or query == "" and row.page == page then rows[#rows+1]=row end
        end
        model.settings_rows = rows
        fact.settings_selected = math.clamp(fact.settings_selected or 0, 0, math.max(#rows-1, 0))
        text.settings_notice = "Nix-managed edits apply immediately in memory and survive reload; a new shell process restores declared values."
    end
    local function present()
        if not wallpaper then return end
        if fact.settings_open and fact.settings_wallpaper_page then wallpaper.open()
        else wallpaper.close() end
    end
    local function navigate(page)
        if not pages[page] then return end
        if history[position] ~= page then
            while #history > position do table.remove(history) end
            if #history == 64 then table.remove(history,1); position=position-1 end
            history[#history+1]=page; position=#history
        end
        text.settings_search=""
        fact.settings_selected=0
        refresh()
        present()
    end
    local function activate(index)
        if type(index)~="number" or index~=math.floor(index) then return end
        local row=rows[index+1]
        if not row or not row.editable then return end
        local value
        if row.key == "wallpaper.fit" then
            local next_mode = {fill="fit", fit="centre", centre="tile", tile="fill"}
            value = next_mode[snapshot[row.key].value]
        else
            value = not snapshot[row.key].value
        end
        local ok, settings, warning=pcall(tohru_settings.set,row.key,value)
        if ok then
            text.settings_ui_error=warning or ""
            refresh(settings)
            applied(settings,warning)
        else text.settings_ui_error=tostring(settings) end
    end
    on("settings_open_request",function()
        refresh(tohru_settings.snapshot())
        fact.settings_open=true
        present()
    end)
    on("settings_close",function() fact.settings_open=false; if wallpaper then wallpaper.close() end end)
    on("fact:settings_open",function() if not fact.settings_open and wallpaper then wallpaper.close() end end)
    on("settings_wallpaper_open",function() fact.settings_open=true; navigate(4) end)
    on("wallpaper_picker_close",function() navigate(2) end)
    on("settings_category",navigate)
    on("settings_back",function() if position>1 then position=position-1; text.settings_search=""; refresh(); present() end end)
    on("settings_forward",function() if position<#history then position=position+1; text.settings_search=""; refresh(); present() end end)
    on("text:settings_search",function() fact.settings_sidebar=false; fact.settings_selected=0; refresh() end)
    on("settings_row",activate)
    on("submit:settings_search",function() activate(fact.settings_selected) end)
    on("settings_move",function(delta)
        if fact.settings_sidebar then navigate(math.clamp(history[position]+delta,1,#pages))
        else fact.settings_selected=math.clamp(fact.settings_selected+delta,0,math.max(#rows-1,0)) end
    end)
    on("settings_activate",function() if not fact.settings_sidebar then activate(fact.settings_selected) end end)
    refresh()
    present()
    return refresh
end
