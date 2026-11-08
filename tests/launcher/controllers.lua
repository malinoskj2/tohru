local make_usage = assert(loadfile("shell/launcher/usage.luau"))()
local search = assert(loadfile("shell/launcher/search.luau"))()
local geometry = assert(loadfile("shell/launcher/geometry.luau"))()
local usage = make_usage({counts = {a = 2, bad = -1}, pins = {"", "a", "a", "b"}, recent = {"a", "a", "b"}})
assert(usage.state.counts.a == 2 and not usage.state.counts.bad)
assert(#usage.state.pins == 2 and #usage.state.recent == 2)
usage.toggle("a"); usage.toggle("a")
assert(usage.state.pins[1] == "b" and usage.state.pins[2] == "a")
for _, id in ipairs({"c", "d", "e", "f", "g", "a"}) do usage.record(id) end
assert(#usage.state.recent == 6 and usage.state.recent[1] == "a" and usage.state.counts.a == 3)
local counts = {}
for i = 1, 2005 do counts[string.format("app%04d", i)] = 2 end
usage = make_usage({counts = counts})
assert(usage.state.counts.app2000 == 2 and usage.state.counts.app2001 == nil)
usage.record("new")
assert(usage.state.counts.app2000 == nil and usage.state.counts.new == 1)
for i = 1, 65 do usage.toggle(tostring(i)) end
assert(#usage.state.pins == 64)

local handlers, watches, calls = {}, {}, {}
_G.text, _G.fact, _G.model = {}, {}, {}
_G.on = function(name, callback) handlers[name] = callback end
local saved
_G.sys = {
    ask = function(...) return saved end,
    call = function(name, ...)
        calls[#calls + 1] = {name, ...}
        if name == "files.write" then saved = select(2, ...) end
        if name == "apps.launch" and select(1, ...) == "refused" then error("Launch refused") end
    end,
    watch = function(name, callback) watches[name] = callback; return true end,
}
assert(loadfile("shell/launcher/controller.luau"))()(make_usage, search, geometry)
local actions = {}
for i = 1, 100 do actions[i] = {name = "Action " .. i, exec = "action" .. i} end
watches.apps({
    {id = "a", name = "Art", local_name = "", icon = "art", categories = "Graphics;Development;", exec = "art", actions = actions},
    {id = "b", name = "Code", local_name = "", icon = "code", categories = "Development;", exec = "code"},
    {id = "c", name = "Music", local_name = "", icon = "music", categories = "AudioVideo;", exec = "refused"},
})
handlers.launcher_open()
assert(fact.launcher_visible and fact.launcher_total == 3)
handlers.launcher_tab(1)
assert(fact.launcher_total == 1 and model.launcher_tiles[1].id == "a")
handlers.launcher_tab(2)
assert(fact.launcher_total == 1 and model.launcher_tiles[1].id == "b")
handlers.launcher_tab(0)
handlers.launcher_context(1)
assert(model.launcher_choices[2].label == "Pin")
handlers.launcher_choice(1)
assert(model.launcher_tiles[1].id == "b" and fact.launcher_pins == 1 and saved.pins[1] == "b")
handlers.launcher_launch(0)
assert(not fact.launcher_visible and saved.recent[1] == "b" and saved.counts.b == 1)
handlers.launcher_open()
handlers.launcher_launch(2)
assert(fact.launcher_visible and text.launcher_error:find("Launch refused", 1, true))
handlers.launcher_context(1)
assert(#model.launcher_choices <= 14 and fact.launcher_menu_height == 360)
handlers.launcher_menu_scroll(-200)
assert(model.launcher_choices[#model.launcher_choices].label == "Pin")
handlers.launcher_choice(100)
assert(saved.counts.a == 1 and saved.recent[1] == "a")
assert(calls[#calls - 1][1] == "apps.launch" and calls[#calls - 1][2] == "action100")
-- Catalog virtualization covers entries beyond native model capacity.
local apps = {}
for i = 1, 1200 do apps[i] = {id = "x" .. i, name = "X" .. i, exec = "true", icon = "", categories = "Utility;"} end
watches.apps(apps)
handlers.launcher_open()
assert(fact.launcher_total == 1200 and #model.launcher_tiles <= 96)
handlers.launcher_scroll(-10000)
assert(model.launcher_tiles[#model.launcher_tiles].position == 1199)
print("Production launcher grid/category/pin/action/history fixtures passed")
-- Menu keyboard traversal reaches every action without truncation.
watches.apps({{id = "many", name = "Many", exec = "main", icon = "", categories = "Utility;", actions = actions}})
handlers.launcher_open()
handlers.launcher_context(0)
for _ = 1, 100 do handlers.launcher_menu_move(1) end
assert(fact.launcher_menu_selected == 100)
assert(model.launcher_choices[#model.launcher_choices].position == 100)
handlers.launcher_choice(100)
assert(calls[#calls - 1][1] == "apps.launch" and calls[#calls - 1][2] == "action100")
print("Menu keyboard virtualization passed")
-- Native input text callback rebuilds results; Escape clears before closing.
handlers.launcher_open()
handlers["text:launcher_query"]("many")
assert(fact.launcher_searching and fact.launcher_total == 1 and fact.launcher_selected == 0)
handlers["text:launcher_query"]("absent")
assert(fact.launcher_total == 0)
handlers.launcher_launch(0)
assert(fact.launcher_visible)
handlers.launcher_escape()
assert(not fact.launcher_searching and text.launcher_query == "" and fact.launcher_visible)
handlers.launcher_escape()
assert(not fact.launcher_visible)
handlers.launcher_open()
handlers["text:launcher_query"]("many")
handlers.launcher_tab(6)
assert(not fact.launcher_searching and text.launcher_query == "")
print("Native search-field callback/clear/tab/refusal lifecycle fixtures passed")

-- Q-L1: native raw Categories may omit a final semicolon; precedence stays source order.
watches.apps({
    {id = "dev", name = "Developer", exec = "true", icon = "", categories = "Development"},
    {id = "art", name = "Artist", exec = "true", icon = "", categories = "Development; Graphics"},
})
handlers.launcher_tab(1)
assert(fact.launcher_total == 1 and model.launcher_tiles[1].id == "art")
handlers.launcher_tab(2)
assert(fact.launcher_total == 1 and model.launcher_tiles[1].id == "dev")
handlers.launcher_tab(0)
handlers.launcher_select(1)
assert(fact.launcher_selected_x == 72 and fact.launcher_selected_y == 0)
handlers.launcher_context(fact.launcher_selected)
assert(fact.launcher_menu and model.launcher_choices[1].label == "Open")
handlers.launcher_menu_move(1)
handlers.launcher_choice(fact.launcher_menu_selected)
assert(saved.pins[#saved.pins] == "art")
print("Q-L1 token/precedence and Q-L2 selected context/keyboard pin fixtures passed")
