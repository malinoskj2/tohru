-- Navigation belongs to this window; all values belong to the existing process owner.
return function(applied, publish_corner)
    local pages = {"Bar", "Appearance", "System", "Shortcuts", "Assistant"}
    local providers = {"codex", "claude", "gemini", "agy", "opencode"}
    local selected_provider, editing = 1, nil
    local function cancel_edit() editing=nil;fact.settings_model_editing=false end
    local history, position, rows = {1}, 1, {}
    local snapshot = tohru_settings.snapshot()
    for index, provider in ipairs(providers) do
        if snapshot['assistant.provider'] and snapshot['assistant.provider'].value == provider then selected_provider=index end
    end
    local devices, device_generation = nil, 0
    local query_shortcuts
    local shortcut_offset = 0
    local shortcut_report = {owner="Unknown", state="Checking compositor", bindings={}}
    local source_status = "Unknown · effectiveness unmeasured"
    local choices = {
        ["shortcuts.mode"]={"compositor","portal","disabled"},
        ["appearance.font_mono"]={"DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono"},
        ["appearance.font_text"]={"Noto Sans", "DejaVu Sans", "Liberation Sans"},
        ["appearance.font_display"]={"Noto Sans", "DejaVu Sans", "Liberation Sans"},
        ["assistant.voice_source"]={""},
        ["appearance.palette"]={"mocha","macchiato","rose_pine","nord","gruvbox","tokyo_night","dracula","everforest"},
        ["lock.style"]={"still-glass","ribbon-key","flooded-monolith","drifting-frost-veil","unfolding-glass-capsule",
            "edge-glass-drawer","horizon-meniscus","refractive-typographic-lens","corner-anchor","bar-drop-curtain"},
        ["notifications.position"]={"top-right","bottom-right"},
    }
    local function choice_detail(setting)
        local provenance
        if setting.runtime_override then
            provenance=setting.managed and "Managed · temporary runtime override" or "Temporary until shell exit"
        else provenance=setting.managed and "Managed · edits are temporary" or "Saved in config.toml" end
        return setting.value .. " · " .. provenance .. " · Activate to cycle"
    end
    local function refresh(settings)
        if settings then
            if settings["assistant.voice_source"] and snapshot["assistant.voice_source"]
                and settings["assistant.voice_source"].value ~= snapshot["assistant.voice_source"].value then
                source_status = "Unknown · effectiveness unmeasured"
            end
            snapshot = settings
            if publish_corner then publish_corner(settings) end
        end
        local pval = snapshot["appearance.palette"] and snapshot["appearance.palette"].value
        fact.palette_id = 0
        for idx, name in ipairs(choices["appearance.palette"]) do
            if name == pval then fact.palette_id = idx - 1; break end
        end
        local page = history[position]
        fact.settings_page = page
        fact.settings_can_back = position > 1
        fact.settings_can_forward = position < #history
        local query = string.sub(text.settings_search or "", 1, 256):lower()
        fact.settings_appearance_page = page==2 and query==""
        text.settings_heading = query ~= "" and "Search results" or pages[page]
        local categories = {}
        for id, title in ipairs(pages) do categories[id] = {id=id, title=title, selected=id==page} end
        model.settings_categories = categories
        local function switch(page, key, title)
            local setting = snapshot[key]
            local detail = setting.managed and "Managed · runtime edits last until shell exits" or "Saved in writable config.toml"
            if setting.runtime_override then detail = "Managed · temporary runtime override is active" end
            return {page=page, key=key, title=title, detail=detail,
                    value=setting.value, editable=true, managed=setting.managed, temporary=setting.runtime_override}
        end
        local output = snapshot["bar.preferred_output"].value
        local catalog = {
            switch(1, "bar.mooncoil", "Mooncoil mark"),
            switch(1, "bar.info_button", "System information button"),
            switch(1, "bar.about_button", "About button"),
            {page=1, title="Outputs", detail=output ~= "" and "Bar is shown on " .. output or "Bar is shown on every output", editable=false},
            {page=2, key="appearance.corners", title="Screen corners", detail="Native corner overlays on every output",
             value=snapshot["appearance.corners"].value, editable=true, managed=snapshot["appearance.corners"].managed,
             temporary=snapshot["appearance.corners"].runtime_override},
            {page=2, title="Corner size", detail=tostring(snapshot["appearance.corner_size"].value) .. " logical pixels", editable=false},
            {page=2, key="appearance.palette", title="Palette", choice=true,
             detail=choice_detail(snapshot["appearance.palette"]), editable=true,
             managed=snapshot["appearance.palette"].managed, temporary=snapshot["appearance.palette"].runtime_override},
            {page=2, key="lock.style", title="Lock screen style", choice=true,
             detail=choice_detail(snapshot["lock.style"]), editable=true,
             managed=snapshot["lock.style"].managed, temporary=snapshot["lock.style"].runtime_override},
            {page=2, key="notifications.position", title="Notification position", choice=true,
             detail=choice_detail(snapshot["notifications.position"]), editable=true,
             managed=snapshot["notifications.position"].managed, temporary=snapshot["notifications.position"].runtime_override},
            switch(2, "assistant.reduced_motion", "Reduce dragon motion"),
            {page=3, title="Microphone", key="assistant.voice_source", choice=true, editable=true,
             detail=(snapshot["assistant.voice_source"] and snapshot["assistant.voice_source"].value ~= "" and snapshot["assistant.voice_source"].value or "Automatic") .. " · Activate to cycle sources"},
            {page=3, title="Echo cancellation", detail=source_status, editable=false},
            {page=2, title="UI text font", key="appearance.font_text", choice=true,
             detail=(snapshot["appearance.font_text"] and snapshot["appearance.font_text"].value or "Noto Sans") .. " · restart components after change",
             editable=snapshot["appearance.font_text"] and not snapshot["appearance.font_text"].managed},
            {page=2, title="UI display font", key="appearance.font_display", choice=true,
             detail=(snapshot["appearance.font_display"] and snapshot["appearance.font_display"].value or "Noto Sans") .. " · managed values are read-only",
             editable=snapshot["appearance.font_display"] and not snapshot["appearance.font_display"].managed},
            {page=2, title="UI monospace font", key="appearance.font_mono", choice=true,
             detail=(snapshot["appearance.font_mono"] and snapshot["appearance.font_mono"].value or "DejaVu Sans Mono") .. " · restart components after change",
             editable=snapshot["appearance.font_mono"] and not snapshot["appearance.font_mono"].managed},
            {page=4, title="Shortcut ownership mode", key="shortcuts.mode", choice=true,
             detail=(snapshot["shortcuts.mode"] and snapshot["shortcuts.mode"].value or "compositor") .. " · portal owner requires explicit service start/restart",
             editable=snapshot["shortcuts.mode"] and not snapshot["shortcuts.mode"].managed},
            {page=4, title="Shortcuts", detail=shortcut_report.owner .. " · " .. shortcut_report.state .. " · read-only", editable=false},
            {page=4, title="Refresh shortcut snapshot", detail="Read current compositor state; no configuration writes", editable=true, choice=true, operation="refresh_shortcuts"},
            {page=3, title="Shell", detail="Tohru · pinned native Pleamar 0.7.27", editable=false},
        }
        if shortcut_report.registration == "portal-session" then
            catalog[#catalog+1] = {page=4, title="Set up Tohru action shortcuts", detail="Request desktop grants; preferred keys are hints", editable=shortcut_report.setup_available, choice=true, operation="shortcut_setup"}
            catalog[#catalog+1] = {page=4, title="Configure granted shortcuts", detail=(shortcut_report.portal_version or 0) >= 2 and "Open the desktop's shortcut configuration dialog" or "Requires portal v2; v1 supports initial setup only", editable=shortcut_report.configure_available, choice=true, operation="shortcut_configure"}
        end
        local bindings = shortcut_report.bindings or {}
        for i=shortcut_offset+1,math.min(shortcut_offset+10,#bindings) do
            local binding = bindings[i]
            catalog[#catalog+1] = {page=4, title=binding.shortcut,
                detail=(binding.submap ~= "" and binding.submap .. " · " or "") .. binding.action .. " " .. (binding.arguments or "") .. ((binding.flags or "") ~= "" and " · " .. binding.flags or "") .. (binding.conflict and " · possible conflict" or ""), editable=false}
        end
        if #bindings > 10 then
            catalog[#catalog+1] = {page=4, title="Next shortcuts page", detail=tostring(shortcut_offset+1) .. "–" .. tostring(math.min(shortcut_offset+10,#bindings)) .. " of " .. tostring(#bindings),
                editable=true, choice=true, operation="shortcut_page"}
        end
        catalog[#catalog+1] = {page=5, title="Model settings for "..providers[selected_provider], detail="Activate to choose a provider; does not switch the active assistant", editable=true, choice=true, provider_picker=true}
        for _, group in ipairs({{"models", "Regular replies and tools"}, {"voice_models", "Tool-free spoken replies"}, {"semantic_models", "Turn decisions and draft answers"}}) do
            for _, field in ipairs({"model", "effort"}) do
                local key = "assistant."..group[1].."."..providers[selected_provider].."."..field
                local entry = snapshot[key] or {value=""}
                if field == "effort" then choices[key] = {"", "low", "medium", "high", "xhigh", "max"} end
                catalog[#catalog+1] = {page=5, key=key, title=group[2].." · "..field,
                    detail=(entry.value ~= "" and entry.value or "Default / inherit").." · "..(entry.managed and "Nix managed; edits are temporary" or "Saved in config.toml").." · "..(field=="model" and "Activate to edit" or "Activate to cycle"),
                    editable=true, choice=true, text_edit=field=="model"}
            end
        end
        catalog[#catalog+1] = {page=5, title="Local provider models", detail="Configure Orca bot models in Orca. Turn settings do not enable the feature.", editable=false}
        rows = {}
        for _, row in ipairs(catalog) do
            local matches = (row.title .. " " .. row.detail .. " " .. (row.key or "")):lower():find(query, 1, true)
            if query ~= "" and matches or query == "" and row.page == page then rows[#rows+1]=row end
        end
        model.settings_rows = rows
        fact.settings_selected = math.clamp(fact.settings_selected or 0, 0, math.max(#rows-1, 0))
        text.settings_notice = "Managed edits apply immediately in memory and survive reload; a new shell process restores declared values."
    end
    local function navigate(page)
        if not pages[page] then return end
        if history[position] ~= page then
            while #history > position do table.remove(history) end
            if #history == 64 then table.remove(history,1); position=position-1 end
            history[#history+1]=page; position=#history
        end
        text.settings_search=""
        editing=nil
        fact.settings_model_editing=false
        fact.settings_selected=0
        refresh()
    end
    local function activate(index)
        if type(index)~="number" or index~=math.floor(index) then return end
        local row=rows[index+1]
        if not row or not row.editable then return end
        if row.operation == "shortcut_setup" then query_shortcuts("setup"); return end
        if row.operation == "shortcut_configure" then query_shortcuts("configure"); return end
        if row.operation == "refresh_shortcuts" then if query_shortcuts then query_shortcuts() end; return end
        if row.operation == "shortcut_page" then
            shortcut_offset = shortcut_offset + 10
            if shortcut_offset >= #(shortcut_report.bindings or {}) then shortcut_offset = 0 end
            refresh(); return
        end
        if row.provider_picker then
            selected_provider=selected_provider%#providers+1
            editing=nil
            fact.settings_model_editing=false
            refresh()
            return
        end
        if row.text_edit then
            editing=row.key
            text.settings_model_edit=(snapshot[row.key] and snapshot[row.key].value) or ""
            text.settings_model_label=row.title.." (empty restores inheritance)"
            fact.settings_model_editing=true
            return
        end
        local value
        local cycle=choices[row.key]
        if cycle then
            value = cycle[1]
            for i,current in ipairs(cycle)do
                if current==((snapshot[row.key] and snapshot[row.key].value) or "") then value=cycle[i%#cycle+1];break end
            end
        else
            value = not snapshot[row.key].value
        end
        local ok, settings, warning=pcall(tohru_settings.set,row.key,value)
        if ok then
            text.settings_ui_error=warning or ""
            refresh(settings)
            applied(settings,warning)
            if row.key == "assistant.voice_source" and devices then devices() end
        else text.settings_ui_error=tostring(settings) end
    end
    on("settings_open_request",function()
        cancel_edit()
        fact.settings_open = true
        local ok, settings = pcall(tohru_settings.reload)
        if not ok then
            text.settings_ui_error = tostring(settings)
            return
        end
        text.settings_ui_error = ""
        refresh(settings)
        devices()
        query_shortcuts()
    end)
    on("settings_close",function() cancel_edit();fact.settings_open=false end)
    on("settings_category",navigate)
    on("settings_back",function() cancel_edit();if position>1 then position=position-1; text.settings_search=""; refresh() end end)
    on("settings_forward",function() cancel_edit();if position<#history then position=position+1; text.settings_search=""; refresh() end end)
    on("text:settings_search",function() cancel_edit();fact.settings_sidebar=false; fact.settings_selected=0; refresh() end)
    on("settings_row",activate)
    on("settings_model_cancel",cancel_edit)
    on("submit:settings_model_edit",function()
        if not editing then return end
        local ok, settings, warning=pcall(tohru_settings.set,editing,text.settings_model_edit or "")
        if not ok then text.settings_ui_error=tostring(settings); return end
        editing=nil
        fact.settings_model_editing=false
        text.settings_ui_error=warning or ""
        refresh(settings)
        applied(settings,warning)
    end)
    on("submit:settings_search",function() activate(fact.settings_selected) end)
    on("settings_move",function(delta)
        if fact.settings_sidebar then navigate(math.clamp(history[position]+delta,1,#pages))
        else fact.settings_selected=math.clamp(fact.settings_selected+delta,0,math.max(#rows-1,0)) end
    end)
    on("settings_activate",function() if not fact.settings_sidebar then activate(fact.settings_selected) end end)
    devices = function()
        device_generation = device_generation + 1
        local mine = device_generation
        if not run then return end
        local selected = snapshot["assistant.voice_source"] and snapshot["assistant.voice_source"].value or ""
        run("tohru-voice", {"--devices", "--source", selected}, function(out, code)
            local ok, info = pcall(json.decode, out)
            if mine ~= device_generation or code ~= 0 or not ok or type(info) ~= "table" then return end
            local sources = {""}
            for _, name in ipairs(info.sources or {}) do sources[#sources + 1] = name end
            if selected ~= "" then
                local found = false
                for _, name in ipairs(sources) do if name == selected then found = true end end
                if not found then sources[#sources + 1] = selected end
            end
            choices["assistant.voice_source"] = sources
            source_status = tostring(info.aec or "unknown") .. " · " .. tostring(info.selected or "unavailable")
            refresh()
        end)
    end
    query_shortcuts = function(operation)
        if not run then return end
        run("tohru", {"shortcuts", operation or "status"}, function(out,code)
            local ok, info = pcall(json.decode,out)
            if code == 0 and ok and type(info) == "table" and type(info.bindings) == "table" then
                shortcut_report = info; if operation then shortcut_offset = 0 end; refresh()
            elseif operation then text.settings_ui_error="Shortcut operation unavailable; inspect owner/portal state"
            end
        end)
    end
    refresh(snapshot)
    query_shortcuts()
    if every then every(2000,function() if fact.settings_open and history[position]==4 and shortcut_report.registration=="portal-session" then query_shortcuts() end end) end
    devices()
    return refresh
end
