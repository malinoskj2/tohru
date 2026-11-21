-- Unit tests for shell/tray/controller.luau (SNI filtering, DBusMenu, submenus, fallback, overflow, keyboard).
local function boot()
    local handlers, calls, menus, watched, opened = {}, {}, {}, nil, 0
    local fail_activate = {}
    local env = setmetatable({
        fact = {
            tray_open = false, tray_mode = 0, tray_has_back = false, tray_focus = 0,
            tray_keyed = false, tray_failed = false, tray_panel_h = 140,
            tray_has_icon = false, tray_has_subtitle = false,
            tray_page = 0, tray_pages = 1, tray_anchor_x = -1,
        },
        text = { tray_title = "", tray_subtitle = "", tray_icon = "", tray_error = "" },
        model = {},
    }, { __index = _G })
    env.on = function(name, fn) handlers[name] = fn end
    env.sys = {
        watch = function(name, fn)
            assert(name == "tray")
            watched = fn
            return true
        end,
        ask = function(name, key)
            assert(name == "tray.menu")
            if menus[key] == "ERR" then error("menu unavailable") end
            return menus[key]
        end,
        call = function(name, key, extra)
            if name == "tray.activate" and fail_activate[key] then error("no Activate") end
            calls[#calls + 1] = { name, key, extra }
        end,
    }
    local make = assert(loadfile("shell/tray/controller.luau", "t", env))()
    local controller = make(function() opened = opened + 1 end)
    return {
        env = env, handlers = handlers, calls = calls, menus = menus,
        fail_activate = fail_activate, controller = controller,
        opened = function() return opened end,
        report = function(list) watched(list) end,
    }
end

local t = boot()
local sample = {
    { key = ":1.10/StatusNotifierItem", id = "steam", title = "Steam", status = "Active", icon = "steam", menu = true, tooltip = "Steam", tooltip_body = "Online" },
    { key = ":1.11/StatusNotifierItem", id = "hidden", title = "Hidden", status = "Passive", icon = "x", menu = false, tooltip = "", tooltip_body = "" },
    { key = ":1.12/StatusNotifierItem", id = "discord", title = "Discord", status = "NeedsAttention", icon = "/tmp/d.png", menu = true, tooltip = "3 unread", tooltip_body = "" },
    { key = ":1.13/StatusNotifierItem", id = "obs", title = "OBS", status = "Active", icon = "", menu = false, tooltip = "", tooltip_body = "" },
}
t.report(sample)
assert(#t.env.model.tray_items == 3, "Passive items are filtered out")
assert(t.env.model.tray_items[1].id == "steam" and t.env.model.tray_items[1].tip == "Steam · Online")
assert(t.env.model.tray_items[2].id == "discord" and t.env.model.tray_items[2].attention)
assert(t.env.model.tray_items[3].id == "obs" and not t.env.model.tray_items[3].has_icon)

-- Left click activates; middle click calls secondary; scroll forwards signed notches.
t.handlers.tray_activate(0)
assert(t.calls[#t.calls][1] == "tray.activate" and t.calls[#t.calls][2] == ":1.10/StatusNotifierItem")
t.handlers.tray_secondary(1)
assert(t.calls[#t.calls][1] == "tray.secondary" and t.calls[#t.calls][2] == ":1.12/StatusNotifierItem")
t.handlers.tray_scroll(0 * 256 + (128 - 2))
assert(t.calls[#t.calls][1] == "tray.scroll" and t.calls[#t.calls][3] == -2)

-- Menu-only app (Activate errors): left click falls back to opening DBusMenu.
t.fail_activate[":1.12/StatusNotifierItem"] = true
t.menus[":1.12/StatusNotifierItem"] = {
    { id = 1, label = "Open Discord", enabled = true, separator = false, children = {} },
    { id = 2, label = "", enabled = true, separator = true, children = {} },
    { id = 3, label = "Mute", enabled = true, separator = false, checked = true, children = {} },
    { id = 4, label = "Status", enabled = true, separator = false, children = {
        { id = 41, label = "Online", enabled = true, separator = false, checked = true, children = {} },
        { id = 42, label = "Do Not Disturb", enabled = true, separator = false, checked = false, children = {} },
    }},
}
t.handlers.tray_activate(1)
assert(t.env.fact.tray_open and t.env.fact.tray_mode == 0 and t.env.text.tray_title == "Discord")
assert(t.opened() == 1 and #t.env.model.tray_menu_rows == 4)
assert(t.env.model.tray_menu_rows[2].separator and t.env.model.tray_menu_rows[3].has_check and t.env.model.tray_menu_rows[3].checked)
assert(t.env.model.tray_menu_rows[4].has_sub)

-- Navigating into a submenu, back, and clicking a submenu item.
t.handlers.tray_menu_pick(3)
assert(t.env.fact.tray_has_back and #t.env.model.tray_menu_rows == 2 and t.env.model.tray_menu_rows[2].label == "Do Not Disturb")
t.handlers.tray_menu_back()
assert(not t.env.fact.tray_has_back and #t.env.model.tray_menu_rows == 4)
-- Keyboard navigation skips separators.
t.handlers.tray_focus_move(1); t.handlers.tray_focus_move(1)
assert(t.env.fact.tray_focus == 2)
t.handlers.tray_focus_activate()
assert(t.env.fact.tray_has_back, "Return opens submenu")
t.handlers.tray_focus_move(1); t.handlers.tray_focus_activate()
assert(t.calls[#t.calls][1] == "tray.menu_click" and t.calls[#t.calls][2] == ":1.12/StatusNotifierItem" and t.calls[#t.calls][3] == 42)
assert(not t.env.fact.tray_open, "clicking a leaf item closes the menu")

-- Right-click on an app with no DBusMenu calls tray.context directly.
t.handlers.tray_menu_open(2)
assert(t.calls[#t.calls][1] == "tray.context" and t.calls[#t.calls][2] == ":1.13/StatusNotifierItem")

-- Active app vanishes while its menu is open: menu closes cleanly.
t.handlers.tray_menu_open(1)
assert(t.env.fact.tray_open)
t.report({ sample[1], sample[4] })
assert(not t.env.fact.tray_open)

-- Overflow list paging (10 items -> 2 pages of 8).
local many = {}
for i = 1, 10 do
    many[i] = { key = ":1." .. i .. "/SNI", id = "app" .. i, title = "App " .. i, status = "Active", icon = "app", menu = false, tooltip = "", tooltip_body = "" }
end
t.report(many)
t.handlers.tray_overflow_toggle()
assert(t.env.fact.tray_open and t.env.fact.tray_mode == 1 and t.env.fact.tray_pages == 2 and t.env.fact.tray_page == 0)
t.handlers.tray_page_step(1)
assert(t.env.fact.tray_page == 1)
t.handlers.tray_overflow_toggle()
assert(not t.env.fact.tray_open)

print("System tray controller: SNI filtering, DBusMenu, submenus, fallback, overflow and keyboard passed")
