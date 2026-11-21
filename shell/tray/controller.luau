-- Watches Pleamar's native StatusNotifierItem (tray) service and drives inline icons,
-- overflow listing, and adaptive-glass DBusMenu popups with nested submenu navigation.
return function(on_open)
    local items = {}
    local active_key = nil
    local menu_stack = {}
    local visible_rows = {}
    local MAX_ITEMS = 24
    local MAX_MENU_ROWS = 16
    local OVERFLOW_PAGE = 8

    local function fail(message)
        text.tray_error = message or ""
        fact.tray_failed = (message or "") ~= ""
    end

    local function find_item(index)
        index = math.floor(tonumber(index) or -1)
        if index < 0 or index >= #items then return nil end
        return items[index + 1]
    end

    local function find_by_key(key)
        for i, item in ipairs(items) do
            if item.key == key then return item, i - 1 end
        end
    end

    local function render_menu()
        local current = menu_stack[#menu_stack] or {}
        local rows = {}
        visible_rows = {}
        local y = 0
        local selectable = 0
        for _, node in ipairs(current) do
            if #rows >= MAX_MENU_ROWS then break end
            local is_sep = node.separator == true
            local label = type(node.label) == "string" and node.label or ""
            if is_sep or label ~= "" then
                local has_sub = type(node.children) == "table" and #node.children > 0
                local has_check = node.checked ~= nil
                local row_index = -1
                if not is_sep then
                    row_index = selectable
                    selectable = selectable + 1
                end
                rows[#rows + 1] = {
                    id = tonumber(node.id) or 0,
                    y = y,
                    label = label,
                    enabled = node.enabled ~= false,
                    separator = is_sep,
                    checked = node.checked == true,
                    has_check = has_check,
                    has_sub = has_sub,
                    row_index = row_index,
                }
                visible_rows[#visible_rows + 1] = node
                y = y + (is_sep and 10 or 26)
            end
        end
        model.tray_menu_rows = rows
        fact.tray_has_back = #menu_stack > 1
        local header_h = fact.tray_has_subtitle and 52 or 40
        local body_h = #rows > 0 and y or 44
        local back_h = fact.tray_has_back and 28 or 0
        local err_h = fact.tray_failed and 24 or 0
        fact.tray_panel_h = header_h + 6 + back_h + body_h + 10 + err_h
        fact.tray_focus = math.max(0, math.min(fact.tray_focus or 0, math.max(0, selectable - 1)))
    end

    local function render_overflow()
        local pages = math.max(1, math.ceil(#items / OVERFLOW_PAGE))
        local page = math.max(0, math.min(math.floor(fact.tray_page or 0), pages - 1))
        fact.tray_pages = pages
        fact.tray_page = page
        local visible = math.max(0, math.min(OVERFLOW_PAGE, #items - page * OVERFLOW_PAGE))
        local body_h = visible > 0 and (visible * 34) or 44
        local pager_h = pages > 1 and 34 or 0
        local err_h = fact.tray_failed and 24 or 0
        fact.tray_panel_h = 40 + 8 + body_h + pager_h + 8 + err_h
        fact.tray_focus = math.max(0, math.min(fact.tray_focus or 0, math.max(0, visible - 1)))
    end

    local function close()
        fact.tray_open = false
        fact.tray_keyed = false
        fact.tray_has_back = false
        active_key = nil
        menu_stack = {}
        visible_rows = {}
        fail("")
    end

    local function open_overflow()
        if on_open then on_open() end
        fail("")
        active_key = nil
        menu_stack = {}
        visible_rows = {}
        fact.tray_mode = 1
        fact.tray_has_icon = false
        fact.tray_has_subtitle = false
        fact.tray_has_back = false
        fact.tray_page = 0
        fact.tray_focus = 0
        fact.tray_keyed = false
        text.tray_title = "System Tray"
        text.tray_subtitle = ""
        text.tray_icon = ""
        render_overflow()
        fact.tray_open = true
    end

    local function toggle_overflow()
        if fact.tray_open and fact.tray_mode == 1 then close() else open_overflow() end
    end

    local function open_menu_for(item)
        if not item then return end
        if not item.has_menu then
            local ok, err = pcall(sys.call, "tray.context", item.key)
            if not ok then fail(tostring(err)) end
            return
        end
        local ok, tree = pcall(sys.ask, "tray.menu", item.key)
        if not ok or type(tree) ~= "table" then
            -- Fall back to app-owned ContextMenu if DBusMenu fails.
            local fallback_ok, fallback_err = pcall(sys.call, "tray.context", item.key)
            if not fallback_ok then
                if on_open then on_open() end
                active_key = item.key
                menu_stack = {{}}
                fact.tray_mode = 0
                text.tray_title = item.title
                text.tray_subtitle = item.tip
                fact.tray_has_subtitle = item.tip ~= ""
                text.tray_icon = item.icon
                fact.tray_has_icon = item.has_icon
                fail(tostring(ok and fallback_err or tree))
                render_menu()
                fact.tray_open = true
            end
            return
        end
        if on_open then on_open() end
        fail("")
        active_key = item.key
        menu_stack = {tree}
        fact.tray_mode = 0
        fact.tray_focus = 0
        fact.tray_keyed = false
        text.tray_title = item.title
        text.tray_subtitle = item.tip
        fact.tray_has_subtitle = item.tip ~= ""
        text.tray_icon = item.icon
        fact.tray_has_icon = item.has_icon
        render_menu()
        fact.tray_open = true
    end

    local function activate_item(item)
        if not item then return end
        local ok = pcall(sys.call, "tray.activate", item.key)
        if not ok and item.has_menu then
            open_menu_for(item)
        elseif ok and fact.tray_open then
            close()
        end
    end

    local function pick_menu_row(row_index)
        row_index = math.floor(tonumber(row_index) or -1)
        local node = visible_rows[row_index + 1]
        if not node or node.separator == true or node.enabled == false then return end
        if type(node.children) == "table" and #node.children > 0 then
            menu_stack[#menu_stack + 1] = node.children
            fact.tray_focus = 0
            render_menu()
            return
        end
        if not active_key then close(); return end
        local ok, err = pcall(sys.call, "tray.menu_click", active_key, tonumber(node.id) or 0)
        if ok then
            close()
        else
            fail(tostring(err))
            render_menu()
        end
    end

    local function selectable_row_for(focus_idx)
        local count = 0
        for i, node in ipairs(visible_rows) do
            if node.separator ~= true then
                if count == focus_idx then return i - 1 end
                count = count + 1
            end
        end
        return -1
    end

    local function observe(raw)
        items = {}
        if type(raw) == "table" then
            for _, entry in ipairs(raw) do
                if #items >= MAX_ITEMS then break end
                if type(entry) == "table" and entry.status ~= "Passive" then
                    local key = type(entry.key) == "string" and entry.key or ""
                    local id = type(entry.id) == "string" and entry.id or ""
                    local title = type(entry.title) == "string" and entry.title ~= "" and entry.title or id
                    local tip = type(entry.tooltip) == "string" and entry.tooltip or ""
                    local tip_body = type(entry.tooltip_body) == "string" and entry.tooltip_body or ""
                    if tip == "" then tip = tip_body elseif tip_body ~= "" and tip_body ~= tip then tip = tip .. " · " .. tip_body end
                    local icon = type(entry.icon) == "string" and entry.icon or ""
                    if key ~= "" then
                        items[#items + 1] = {
                            key = key,
                            id = id,
                            title = title ~= "" and title or key,
                            tip = tip,
                            icon = icon,
                            has_icon = icon ~= "",
                            has_menu = entry.menu == true,
                            attention = entry.status == "NeedsAttention",
                        }
                    end
                end
            end
        end
        model.tray_items = items
        if fact.tray_open then
            if fact.tray_mode == 1 then
                render_overflow()
            elseif active_key and not find_by_key(active_key) then
                close()
            end
        end
    end

    on("tray_anchor", function(x)
        if type(x) == "number" and x == x then fact.tray_anchor_x = x end
    end)
    on("tray_activate", function(index) activate_item(find_item(index)) end)
    on("tray_secondary", function(index)
        local item = find_item(index)
        if item then pcall(sys.call, "tray.secondary", item.key) end
    end)
    on("tray_menu_open", function(index)
        local item = find_item(index)
        if not item then return end
        if fact.tray_open and fact.tray_mode == 0 and active_key == item.key then
            close()
        else
            open_menu_for(item)
        end
    end)
    on("tray_scroll", function(encoded)
        encoded = math.floor(tonumber(encoded) or -1)
        if encoded < 0 then return end
        local index = math.floor(encoded / 256)
        local delta = (encoded % 256) - 128
        local item = find_item(index)
        if item and delta ~= 0 then pcall(sys.call, "tray.scroll", item.key, delta) end
    end)
    on("tray_overflow_toggle", toggle_overflow)
    on("tray_open_request", open_overflow)
    on("tray_close", close)
    on("tray_menu_pick", pick_menu_row)
    on("tray_menu_back", function()
        if #menu_stack > 1 then
            table.remove(menu_stack)
            fact.tray_focus = 0
            render_menu()
        end
    end)
    on("tray_overflow_activate", function(index) activate_item(find_item(index)) end)
    on("tray_overflow_menu", function(index) open_menu_for(find_item(index)) end)
    on("tray_page_step", function(delta)
        if delta ~= -1 and delta ~= 1 then return end
        fact.tray_page = math.max(0, math.min((fact.tray_page or 0) + delta, math.max(0, (fact.tray_pages or 1) - 1)))
        fact.tray_focus = 0
        render_overflow()
    end)
    on("tray_focus_move", function(delta)
        if delta ~= -1 and delta ~= 1 then return end
        fact.tray_keyed = true
        if fact.tray_mode == 0 then
            local count = 0
            for _, node in ipairs(visible_rows) do if node.separator ~= true then count = count + 1 end end
            if count > 0 then fact.tray_focus = math.max(0, math.min((fact.tray_focus or 0) + delta, count - 1)) end
        else
            local visible = math.max(0, math.min(OVERFLOW_PAGE, #items - (fact.tray_page or 0) * OVERFLOW_PAGE))
            if visible > 0 then fact.tray_focus = math.max(0, math.min((fact.tray_focus or 0) + delta, visible - 1)) end
        end
    end)
    on("tray_focus_activate", function()
        if fact.tray_mode == 0 then
            local row_idx = selectable_row_for(fact.tray_focus or 0)
            if row_idx >= 0 then pick_menu_row(row_idx) end
        else
            local item = find_item((fact.tray_page or 0) * OVERFLOW_PAGE + (fact.tray_focus or 0))
            if item then activate_item(item) end
        end
    end)
    on("fact:tray_open", function(open) if not open then close() end end)

    pcall(sys.watch, "tray", observe)
    return { open = open_overflow, close = close, toggle = toggle_overflow, observe = observe }
end
