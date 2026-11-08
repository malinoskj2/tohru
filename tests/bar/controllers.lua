-- Executes production Luau modules (also Lua-compatible) with native boundary fixtures.
local function environment()
    local callbacks, watches, values = {}, {}, {}
    _G.text, _G.model = {}, {}
    _G.fact = setmetatable({}, {
        __index = values,
        __newindex = function(_, name, value)
            local changed = values[name] ~= value
            values[name] = value
            if changed and callbacks["fact:" .. name] then callbacks["fact:" .. name](value) end
        end,
    })
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
assert(text.bar_media == "▶ Track - Artist" and fact.bar_playing == true)
watches.media({artist = "Artist", playing = false})
assert(text.bar_media == "▶ Artist" and fact.bar_playing == false)
watches.clock({year = 2026, month = 11, day = 7, weekday = 6, hour = 0, minute = 3})
assert(text.bar_date == "Sat 7 Nov" and text.bar_time == "12:03 AM")
watches.clock({year = 2026, month = 11, day = 7, weekday = 6, hour = 12, minute = 59})
assert(text.bar_time == "12:59 PM")

 events, watches = environment()
assert(loadfile("shell/calendar/controller.luau"))()()
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
-- Leap-year adjacency and century rule from the same production code.
for _, case in ipairs({{2024, "29"}, {2100, "28"}, {2000, "29"}}) do
    watches.clock({year = case[1], month = 2, day = 1})
    events.calendar_today()
    local last
    for _, day in ipairs(model.cal_days) do if day.month == 2 then last = day.day end end
    assert(last == case[2])
end
print("Production bar/calendar controller fixtures passed")
