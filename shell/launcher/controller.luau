-- `rail` is the sidebar's ribbon (bar/rail.luau as rail 0, a slot per tab).
return function(make_usage, search, geometry, output, rail)
    local names = {"Home", "Create", "Code", "Media", "Web", "Office", "System"}
    local ok, saved = pcall(sys.ask, "files.read", "launcher.json", "json")
    if not ok then text.launcher_error = tostring(saved) end
    local usage = make_usage(ok and saved or nil)
    local catalog, shown, context = {}, {}, nil
    local choices, menu_scroll = {}, 0
    local columns, viewport, scroll = 7, 440, 0
    local tab, selected, query = 0, -1, ""
    local groups = {
        {"graphics", "photography", "2dgraphics", "3dgraphics"},
        {"development", "ide", "texteditor", "terminalemulator"},
        {"audiovideo", "audio", "video", "player"},
        {"network", "webbrowser", "email", "instantmessaging"},
        {"office", "wordprocessor", "spreadsheet", "calendar"},
    }
    local function category(app)
        local names = {}
        for name in ((app.categories or "") .. ";"):gmatch("([^;]+);") do
            names[name:match("^%s*(.-)%s*$"):lower()] = true
        end
        for index, group in ipairs(groups) do
            for _, name in ipairs(group) do if names[name] then return index end end
        end
        return 6
    end
    local function save()
        local success, err = pcall(sys.call, "files.write", "launcher.json", usage.state)
        text.launcher_error = success and "" or tostring(err)
    end
    local function close_menu() context = nil; choices = {}; model.launcher_choices = {}; fact.launcher_menu = false; fact.launcher_menu_selected = -1 end
    local function publish_menu()
        local height = math.min(360, #choices * 30 + 12)
        menu_scroll = math.max(0, math.min(menu_scroll, #choices * 30 + 12 - height))
        local rows = {}
        for index, choice in ipairs(choices) do
            local y = (index - 1) * 30 - menu_scroll
            if y + 30 >= 0 and y < height - 12 then
                rows[#rows + 1] = {position = index - 1, y = y, label = choice.label, action = choice.action}
            end
        end
        model.launcher_choices = rows
        fact.launcher_menu_height = height
        fact.launcher_menu_offset = menu_scroll
    end
    local function rebuild()
        local by_id = {}
        for _, app in ipairs(catalog) do by_id[app.id] = app end
        shown = {}
        local pinned_count = 0
        local searching = query:find("%S") ~= nil
        fact.launcher_searching = searching
        fact.launcher_typed = query ~= ""
        if searching then
            shown = search(query, catalog, usage.state.counts)
        elseif tab == 0 then
            for _, id in ipairs(usage.state.pins) do
                if by_id[id] then shown[#shown + 1] = by_id[id]; pinned_count = pinned_count + 1 end
            end
        end
        if not searching then
            for _, app in ipairs(catalog) do
                if (tab == 0 and not usage.pinned(app.id)) or (tab ~= 0 and category(app) == tab) then shown[#shown + 1] = app end
            end
        end
        local per_tab = {[0] = #catalog, 0, 0, 0, 0, 0, 0}
        for _, app in ipairs(catalog) do
            local index = category(app)
            per_tab[index] = per_tab[index] + 1
        end
        for index = 0, 6 do fact["launcher_tab_count." .. index] = per_tab[index] end
        -- Pins absent from the current catalog never consume visual space.
        fact.launcher_pins = pinned_count
        fact.launcher_tab = tab
        fact.launcher_total = #shown
        selected = selected >= 0 and math.min(selected, #shown - 1) or -1
        fact.launcher_selected = selected
        local pin_rows = math.ceil(pinned_count / columns)
        local function position(index) return geometry.position(columns, pinned_count, index) end
        local content = geometry.content(columns, pinned_count, #shown)
        scroll = math.max(0, math.min(scroll, content - viewport))
        fact.launcher_scroll = scroll
        fact.launcher_content = content
        fact.launcher_pin_rows = pin_rows
        local sx, sy = position(math.max(0, selected))
        fact.launcher_selected_x, fact.launcher_selected_y = sx * geometry.cell, sy
        local tiles = {}
        for index, app in ipairs(shown) do
            local x, y = position(index - 1)
            if y + geometry.row >= scroll - geometry.row and y <= scroll + viewport + geometry.row then
                tiles[#tiles + 1] = {position = index - 1, x = x * geometry.cell, y = y, name = app.local_name and app.local_name ~= "" and app.local_name or app.name,
                    id = app.id, icon = app.icon, iconless = not app.icon_found, pinned = usage.pinned(app.id)}
            end
        end
        model.launcher_tiles = tiles
        text.launcher_count = string.format("%02d", #shown)
        text.launcher_count_label = searching and (#shown > 0 and "Search results" or "No matches") or names[tab + 1] .. " \u{00b7} apps"
        if searching then text.launcher_heading = "All tabs \u{00b7} " .. #shown .. (#shown == 1 and " match" or " matches")
        elseif tab == 0 then text.launcher_heading = pinned_count > 0 and "Pinned" or "Applications"
        else text.launcher_heading = names[tab + 1] end
        text.launcher_hint = ""
        if #shown > 0 then text.launcher_message = ""
        elseif searching then
            text.launcher_message = "No applications match \u{201c}" .. query:match("^%s*(.-)%s*$") .. "\u{201d}"
            text.launcher_hint = "Clear the search with Esc or pick a tab on the rail"
        elseif #catalog == 0 then text.launcher_message = "No applications installed"
        else text.launcher_message = "No applications in this tab" end
        if rail then rail.observe(7, tab + 1) end
    end
    -- The bar raises itself over the scrim and tints sky from what this says (shell/bar/launcher.luau).
    local function bar(state)
        fact.launcher_bar_state = state
        pcall(sys.call, "say", "tohru", "emit launcher_bar " .. state)
    end
    local function show()
        -- Repeated open keeps the same focused native role: no fresh enter is
        -- promised. A closed-to-open cycle must establish ownership anew.
        if not fact.launcher_visible then fact.launcher_focus_armed = false end
        fact.launcher_pointer, fact.launcher_hovered = false, -1
        tab, selected, scroll, query = 0, -1, 0, ""
        text.launcher_query = ""
        local was = fact.launcher_visible
        if not was and rail then rail.reset() end
        close_menu(); rebuild(); if output then output.open() end; fact.launcher_visible = true
        if not was then bar(1) end
    end
    local function hide()
        fact.launcher_focus_armed = false; fact.launcher_visible = false; close_menu(); if output then output.close() end
        bar(3)
    end
    -- Native focus events preserve arrival order. A removed layer can report
    -- its leave after the next open; it must not dismiss an as-yet unfocused
    -- replacement. Reset on closed-to-open/hide; preserve repeated visible
    -- open and VM reload ownership because neither promises a fresh enter.
    on("focus", function()
        if fact.launcher_visible then fact.launcher_focus_armed = true; if fact.launcher_bar_state == 1 then bar(2) end end
    end)
    on("launcher_mapped", function() if fact.launcher_visible and fact.launcher_bar_state == 1 then bar(2) end end)
    if output then output.unmapped = function() bar(0) end end
    on("launcher_report", function() bar(fact.launcher_bar_state or 0) end)
    on("blur", function()
        local owned = fact.launcher_focus_armed
        fact.launcher_focus_armed = false
        if owned and fact.launcher_visible then hide() end
    end)
    on("launcher_open", show)
    on("launcher_close", hide)
    on("launcher_toggle", function() if fact.launcher_visible then hide() else show() end end)
    on("launcher_tab", function(value)
        if value ~= math.floor(value) then return end
        tab, selected, scroll, query = value % 7, -1, 0, ""
        text.launcher_query = ""
        fact.launcher_pointer = false
        close_menu(); rebuild()
    end)
    on("text:launcher_query", function(value)
        query, selected, scroll = value, value:find("%S") and 0 or -1, 0
        fact.launcher_pointer = false
        close_menu(); rebuild()
    end)
    on("launcher_escape", function()
        if fact.launcher_menu then close_menu()
        elseif query ~= "" then query = ""; text.launcher_query = ""; selected, scroll = -1, 0; rebuild()
        else hide() end
    end)
    on("launcher_layout", function(value)
        if value ~= math.floor(value) or value < 3 or value > 7 then return end
        columns = value; rebuild()
    end)
    on("launcher_viewport", function(value) viewport = math.max(geometry.row, math.min(value, 440)); rebuild() end)
    on("launcher_scroll", function(value) scroll = scroll - value * geometry.row; selected = -1; close_menu(); rebuild() end)
    on("launcher_select", function(value)
        if value == math.floor(value) and value >= 0 and value < #shown then
            selected = value
            fact.launcher_selected = selected
            local x, y = geometry.position(columns, fact.launcher_pins, selected)
            fact.launcher_selected_x, fact.launcher_selected_y = x * geometry.cell, y
        end
    end)
    local function move(value, vertical)
        if #shown == 0 then return end
        local from = math.max(0, selected)
        selected = vertical and geometry.neighbour(columns, fact.launcher_pins, from, #shown, value) or math.max(0, math.min(from + value, #shown - 1))
        scroll = geometry.follow(columns, fact.launcher_pins, selected, scroll, viewport)
        fact.launcher_pointer = false
        close_menu(); rebuild()
    end
    on("launcher_move", function(value) move(value, false) end)
    on("launcher_vertical", function(value) if value == -1 or value == 1 then move(value, true) end end)
    on("launcher_page", function(value) scroll = scroll + value * viewport; selected = -1; fact.launcher_pointer = false; close_menu(); rebuild() end)
    local function launch(app, action)
        if not app then return end
        local exec = action and action.exec or app.exec
        local success, err = pcall(sys.call, "apps.launch", exec)
        if not success then text.launcher_error = tostring(err); return end
        -- Native acceptance is not proof that the detached application reached a window.
        usage.record(app.id); save(); hide()
    end
    on("launcher_launch", function(value)
        if value == math.floor(value) and value >= 0 then launch(shown[value + 1]) end
    end)
    on("launcher_context", function(value)
        if value ~= math.floor(value) or value < 0 then return end
        local app = shown[value + 1]
        if not app then return end
        context = app
        choices = {{label = "Open", action = -1}}
        menu_scroll = 0
        fact.launcher_menu_selected = -1
        for index, action in ipairs(app.actions or {}) do
            choices[#choices + 1] = {label = action.local_name and action.local_name ~= "" and action.local_name or action.name, action = index}
        end
        choices[#choices + 1] = {label = usage.pinned(app.id) and "Unpin" or "Pin", action = -2}
        publish_menu()
        fact.launcher_menu = true
    end)
    on("launcher_choice", function(value)
        local row = choices[value + 1]
        if value ~= math.floor(value) or not row or not context then return end
        local app, action = context, row.action
        if action == -2 then usage.toggle(app.id); save(); close_menu(); rebuild()
        elseif action == -1 then launch(app)
        else launch(app, app.actions[action]) end
    end)
    on("launcher_menu_move", function(value)
        local menu_selected = math.max(0, math.min((fact.launcher_menu_selected or -1) + value, #choices - 1))
        fact.launcher_menu_selected = menu_selected
        local y = menu_selected * 30
        local viewport_height = fact.launcher_menu_height - 12
        if y < menu_scroll then menu_scroll = y elseif y + 30 > menu_scroll + viewport_height then menu_scroll = y + 30 - viewport_height end
        publish_menu()
    end)
    on("launcher_menu_close", close_menu)
    on("launcher_menu_scroll", function(value) menu_scroll = menu_scroll - value * 30; publish_menu() end)
    on("launcher_palette", function(id)
        id = math.floor(tonumber(id) or -1)
        if id >= 0 and id <= 7 then
            fact.palette_id = id
            fact.rail_palette = id
        end
    end)
    if not sys.watch("apps", function(value)
        -- j2bar's order: by name whatever its case, then as written, then by id.
        catalog = {}
        for index, app in ipairs(value) do catalog[index] = app end
        local function shown(app) return app.local_name and app.local_name ~= "" and app.local_name or app.name or "" end
        table.sort(catalog, function(a, b)
            local x, y = shown(a):lower(), shown(b):lower()
            if x ~= y then return x < y end
            if shown(a) ~= shown(b) then return shown(a) < shown(b) end
            return (a.id or "") < (b.id or "")
        end)
        close_menu(); rebuild()
    end) then text.launcher_error = "Application service unavailable" end
    rebuild()
    bar(fact.launcher_bar_state or 0)
end
