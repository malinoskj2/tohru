-- Executes production Luau modules (also Lua-compatible) with native boundary fixtures.
local function environment()
    local callbacks, watches = {}, {}
    _G.text, _G.model, _G.fact = {}, {}, {}
    _G.on = function(name, callback) callbacks[name] = callback end
    _G.sys = {watch = function(name, callback) watches[name] = callback; return true end}
    return callbacks, watches
end

local events, watches = environment()
assert(loadfile("shell/bar/controller.luau"))()()
watches.window({title = "Editor", class = "TestEditor"})
assert(text.bar_title == "Editor" and fact.bar_has_icon == false)
watches.apps({{id = "org.example.Editor", wmclass = "testeditor", icon = "editor-icon"}})
assert(text.bar_icon == "editor-icon" and fact.bar_has_icon == true)
watches.window({})
assert(text.bar_title == "" and text.bar_icon == "" and fact.bar_has_icon == false)
watches.media({title = "Track", artist = "Artist", playing = true})
assert(text.bar_media == "Track - Artist" and fact.bar_playing == true)
watches.media({artist = "Artist", playing = false})
assert(text.bar_media == "Artist" and fact.bar_playing == false)
watches.clock({year = 2026, month = 11, day = 7, weekday = 6, hour = 0, minute = 3})
assert(text.bar_date == "Sat 7 Nov" and text.bar_time == "12:03 AM")
watches.clock({year = 2026, month = 11, day = 7, weekday = 6, hour = 12, minute = 59})
assert(text.bar_time == "12:59 PM")

 events, watches = environment()
local calendar = assert(loadfile("shell/calendar/controller.luau"))()()
watches.clock({year = 2026, month = 10, day = 3})
assert(text.cal_title == "October 2026" and text.cal_week == "Week 40")
assert(fact.cal_weeks == 5 and #model.cal_days == 35)
events.calendar_month(2)
assert(fact.cal_weeks == 4 and #model.cal_days == 28)
events.calendar_month(8)
assert(fact.cal_weeks == 6 and #model.cal_days == 42)
events.calendar_day(41)
assert(text.cal_title == "September 2026" and text.cal_footer == "Sat 5 Sep")
events.calendar_today()
assert(text.cal_title == "October 2026" and text.cal_footer == "Sat 3 Oct")
events.calendar_month(1)
events.calendar_previous()
assert(text.cal_title == "December 2025")
events.calendar_next()
assert(text.cal_title == "January 2026")
events.calendar_day(-1)
events.calendar_day(1.5)
events.calendar_month(13)
assert(text.cal_title == "January 2026")
events.calendar_open()
assert(fact.cal_open and text.cal_title == "October 2026")
watches.clock({year = 2026, month = 11, day = 1})
assert(text.cal_title == "October 2026" and not fact.cal_current)
events.calendar_today()
assert(text.cal_title == "November 2026" and text.cal_footer == "Sun 1 Nov")
events.calendar_close()
events.calendar_open()
assert(fact.cal_open and fact.cal_current)
-- Native Lua writes do not redispatch fact handlers; toggling must reset itself.
events.calendar_month(2)
calendar.close()
calendar.toggle()
assert(text.cal_title == "November 2026" and fact.cal_current)
calendar.close()
assert(not fact.cal_open)
calendar.open()
assert(fact.cal_open)
-- Leap-year adjacency and century rule from the same production code.
for _, case in ipairs({{2024, "29"}, {2100, "28"}, {2000, "29"}}) do
    watches.clock({year = case[1], month = 2, day = 1})
    events.calendar_today()
    local last
    for _, day in ipairs(model.cal_days) do if day.month == 2 then last = day.day end end
    assert(last == case[2])
end
print("Production bar/calendar controller fixtures passed")

-- Q-B-1: source alias insertion and exact/lower/tail lookup precedence.
events, watches = environment()
assert(loadfile("shell/bar/controller.luau"))()()
watches.apps({
    {id = "org.gnome.Nautilus", wmclass = "Files", icon = "first"},
    {id = "Nautilus", wmclass = "FILES", icon = "later"},
    {id = "editor-bin", icon = "base"},
    {id = "tool-desktop", icon = "tool"},
})
for class, icon in pairs({Nautilus = "first", nautilus = "first", Files = "first", FILES = "later", ["/usr/bin/Nautilus"] = "first", editor = "base", tool = "tool"}) do
    watches.window({title = "Window", class = class})
    assert(text.bar_icon == icon, class .. ": " .. tostring(text.bar_icon))
end
watches.window({class = "unknown"})
assert(text.bar_icon == "" and not fact.bar_has_icon)
-- Q-B-2: use chosen Identity, or first playing native bus order, then status.
watches.media({playing = true, title = "", artist = "", player = "foo", players = {{id = "foo.instance1", name = "First phone", playing = true}, {id = "foo.instance2", name = "Second phone", playing = true}}})
assert(text.bar_media == "First phone")
watches.media({playing = true, players = {{id = "foo.instance1", name = "First phone", playing = true}, {id = "foo.instance2", name = "Second phone", playing = true, chosen = true}}})
assert(text.bar_media == "Second phone")
watches.media({playing = true, players = {{id = "foo", name = "", playing = true}}})
assert(text.bar_media == "Playing")
watches.media({playing = false, players = {}})
assert(not fact.bar_playing)
print("Q-B-1/Q-B-2 regression fixtures passed")

-- The real calendar controller drives the month rail only while visible.
events, watches = environment()
local rail_calls = {}
local rail = {}
for _, operation in ipairs({"observe", "reset"}) do
    rail[operation] = function(...) rail_calls[#rail_calls + 1] = {operation, ...} end
end
calendar = assert(loadfile("shell/calendar/controller.luau"))()(rail)
watches.clock({year = 2026, month = 11, day = 7})
assert(#rail_calls == 0)
calendar.open()
assert(rail_calls[1][1] == "reset" and rail_calls[2][1] == "observe" and rail_calls[2][2] == 12 and rail_calls[2][3] == 11)
events.calendar_month(12)
assert(rail_calls[#rail_calls][3] == 12)
calendar.close()
local count = #rail_calls
watches.clock({year = 2026, month = 12, day = 8})
assert(#rail_calls == count)
calendar.open()
assert(rail_calls[count + 1][1] == "reset" and rail_calls[count + 2][3] == 12)
print("Calendar rail lifecycle passed")

-- Visible VM reload retains the date facts and observes the month again without a reset.
events,watches=environment()
local resets,observations=0,{}
rail={reset=function()resets=resets+1 end,observe=function(_,active)observations[#observations+1]=active end}
calendar=assert(loadfile("shell/calendar/controller.luau"))()(rail)
watches.clock({year=2026,month=11,day=7});calendar.open();events.calendar_month(2)
local retained_year,retained_month=fact.cal_year,fact.cal_month
assert(retained_month==2 and resets==1)
events,watches={},{}
_G.on=function(name,callback)events[name]=callback end
_G.sys={watch=function(name,callback)watches[name]=callback;return true end}
calendar=assert(loadfile("shell/calendar/controller.luau"))()(rail)
assert(resets==1 and observations[#observations]==2 and fact.cal_open)
watches.clock({year=2026,month=11,day=8});assert(fact.cal_year==retained_year and fact.cal_month==2)
events.calendar_next();assert(observations[#observations]==3)
calendar.close();calendar.open();assert(resets==2 and fact.cal_month==11)
print("Visible calendar reload keeps the month and the rail; close/reopen works")

events = environment()
local said = {}
sys.call = function(name, role, request) assert(name == "say" and role == "tohru-notifications"); said[#said + 1] = request end
assert(loadfile("shell/bar/bell.luau"))()()
assert(said[1] == "emit notices_report 0")
events.notices_state(1); assert(fact.notices_unread == true and fact.notices_dnd == false)
events.notices_state(2); assert(fact.notices_unread == false and fact.notices_dnd == true)
events.notices_state("garbage"); assert(fact.notices_unread == false and fact.notices_dnd == false)
events.bell_seen(); events.bell_dnd()
assert(said[2] == "emit notices_seen 0" and said[3] == "emit notices_dnd 2")
sys.call = function() error("tohru-notifications is not running") end
events.bell_seen()
print("Bell shows what the notification daemon reports and survives it being away")

