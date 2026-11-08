return function(make_usage, search, geometry)
    local ok, saved = pcall(sys.ask, "files.read", "launcher.json", "json")
    if not ok then text.launcher_error = tostring(saved) end
    local usage = make_usage(ok and saved or nil)
    local catalog, shown, context = {}, {}, nil
    local choices, menu_scroll, menu_selected = {}, 0, 0
    local columns, viewport, scroll = 8, 432, 0
    local tab, selected, query = 0, -1, ""
    local groups = {
        {"graphics", "photography", "2dgraphics", "3dgraphics"},
        {"development", "ide", "texteditor", "terminalemulator"},
        {"audiovideo", "audio", "video", "player"},
        {"network", "webbrowser", "email", "instantmessaging"},
        {"office", "wordprocessor", "spreadsheet", "calendar"},
    }
    local function category(app)
        local names = ";" .. (app.categories or ""):lower()
        for index, group in ipairs(groups) do
            for _, name in ipairs(group) do if names:find(";" .. name .. ";", 1, true) then return index end end
        end
        return 6
    end
    local function save()
        local success, err = pcall(sys.call, "files.write", "launcher.json", usage.state)
        text.launcher_error = success and "" or tostring(err)
    end
    local function close_menu() context = nil; choices = {}; model.launcher_choices = {}; fact.launcher_menu = false end
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
        fact.launcher_menu_selected = menu_selected
    end
    local function rebuild()
        local by_id = {}
        for _, app in ipairs(catalog) do by_id[app.id] = app end
        shown = {}
        local pinned_count = 0
        local searching = query:find("%S") ~= nil
        fact.launcher_searching = searching
        if searching then
            shown = search(query, catalog)
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
        fact.launcher_selected_x, fact.launcher_selected_y = sx * 72, sy
        local tiles = {}
        for index, app in ipairs(shown) do
            local x, y = position(index - 1)
            if y + 72 >= scroll - 72 and y <= scroll + viewport + 72 then
                tiles[#tiles + 1] = {position = index - 1, x = x * 72, y = y, name = app.local_name and app.local_name ~= "" and app.local_name or app.name,
                    id = app.id, icon = app.icon, pinned = usage.pinned(app.id)}
            end
        end
        model.launcher_tiles = tiles
    end
    local function show()
        tab, selected, scroll, query = 0, -1, 0, ""
        text.launcher_query = ""
        close_menu(); rebuild(); fact.launcher_visible = true
    end
    local function hide() fact.launcher_visible = false; close_menu() end
    on("launcher_open", show)
    on("launcher_close", hide)
    on("launcher_toggle", function() if fact.launcher_visible then hide() else show() end end)
    on("launcher_tab", function(value)
        if value ~= math.floor(value) then return end
        tab, selected, scroll, query = value % 7, -1, 0, ""
        text.launcher_query = ""
        close_menu(); rebuild()
    end)
    on("text:launcher_query", function(value)
        query, selected, scroll = value, 0, 0
        close_menu(); rebuild()
    end)
    on("launcher_escape", function()
        if fact.launcher_menu then close_menu()
        elseif query ~= "" then query = ""; text.launcher_query = ""; selected, scroll = -1, 0; rebuild()
        else hide() end
    end)
    on("launcher_layout", function(value)
        if value ~= math.floor(value) or value < 3 or value > 8 then return end
        columns = value; rebuild()
    end)
    on("launcher_viewport", function(value) viewport = math.max(72, math.min(value, 452)); rebuild() end)
    on("launcher_scroll", function(value) scroll = scroll - value * 72; close_menu(); rebuild() end)
    on("launcher_select", function(value)
        if value == math.floor(value) and value >= 0 and value < #shown then selected = value; fact.launcher_selected = selected end
    end)
    local function move(value, vertical)
        if #shown == 0 then return end
        local from = math.max(0, selected)
        selected = vertical and geometry.neighbour(columns, fact.launcher_pins, from, #shown, value) or math.max(0, math.min(from + value, #shown - 1))
        scroll = geometry.follow(columns, fact.launcher_pins, selected, scroll, viewport)
        close_menu(); rebuild()
    end
    on("launcher_move", function(value) move(value, false) end)
    on("launcher_vertical", function(value) if value == -1 or value == 1 then move(value, true) end end)
    on("launcher_page", function(value) scroll = scroll + value * viewport; selected = -1; close_menu(); rebuild() end)
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
        menu_scroll, menu_selected = 0, 0
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
        menu_selected = math.max(0, math.min(menu_selected + value, #choices - 1))
        local y = menu_selected * 30
        local viewport_height = fact.launcher_menu_height - 12
        if y < menu_scroll then menu_scroll = y elseif y + 30 > menu_scroll + viewport_height then menu_scroll = y + 30 - viewport_height end
        publish_menu()
    end)
    on("launcher_menu_close", close_menu)
    on("launcher_menu_scroll", function(value) menu_scroll = menu_scroll - value * 30; publish_menu() end)
    if not sys.watch("apps", function(value)
        catalog = value
        close_menu(); rebuild()
    end) then text.launcher_error = "Application service unavailable" end
    rebuild()
end
