-- The calendar's controller with the real month rail (bar/rail.luau), from the repository root.
local events, watches = {}, {}
_G.text, _G.model, _G.fact = {}, {}, {}
_G.on = function(name, callback) events[name] = callback end
_G.sys = {watch = function(name, callback) watches[name] = callback; return true end}
for key, value in pairs({present = false, goal = 8, serial = 0, land = 0, crest = 0}) do fact["rail." .. key .. ".1"] = value end
local rail = assert(loadfile("shell/bar/rail.luau"))()(1, 21)
local calendar = assert(loadfile("shell/calendar/controller.luau"))()(rail)
assert(calendar.pitch == 21)

local function index_of(year, month, day)
    for i, d in ipairs(model.cal_days) do
        if d.year == year and d.month == month and d.number == day then return i - 1 end
    end
    error(string.format("%d-%d-%d is not in the grid", year, month, day))
end

watches.clock({year = 2026, month = 11, day = 20})
calendar.open()
assert(fact.cal_open and fact.cal_current and fact.cal_month == 11 and fact.cal_weeks == 5)
assert(text.cal_month_name == "November" and text.cal_year_text == "2026" and text.cal_title == "November 2026")
assert(text.cal_today_day == "20" and text.cal_today_line == "Friday · W47" and text.cal_today_chip == "Today · Nov 20")
assert(text.cal_note == "Today" and not fact.cal_hovering)
assert(model.cal_days[1].number == 1 and model.cal_days[1].col == 0, "November 2026 starts on the first column, a Sunday")
assert(model.cal_days[index_of(2026, 11, 20) + 1].today)
assert(fact["rail.present.1"] and fact["rail.goal.1"] == 220.5 and fact["rail.land.1"] == 1, "the bead lands on November")
print("Opens on today's month with the bead on it")

-- Hover text: the current month counts days from today; adjacent days name their month.
events.calendar_hover(index_of(2026, 11, 25))
assert(text.cal_note == "Wed 25 · +5d" and fact.cal_hovering)
events.calendar_hover(index_of(2026, 11, 16))
assert(text.cal_note == "Mon 16 · −4d")
events.calendar_unhover(index_of(2026, 11, 25))
assert(text.cal_note == "Mon 16 · −4d", "leaving a day already left changes nothing")
events.calendar_hover(index_of(2026, 11, 20))
assert(text.cal_note == "Fri 20 · Today")
events.calendar_hover(index_of(2026, 12, 2))
assert(text.cal_note == "Wed 2 Dec · +12d")
events.calendar_unhover(index_of(2026, 12, 2))
assert(text.cal_note == "Today" and not fact.cal_hovering)
events.calendar_hover(99); events.calendar_hover(1.5); events.calendar_hover("x")
assert(text.cal_note == "Today" and not fact.cal_hovering)
print("Hover text in the current month")

-- Month stepping: buttons and the wheel move by a month, and the bead follows.
local serial, turn = fact["rail.serial.1"], fact.cal_turn or 0
events.calendar_next()
assert(text.cal_title == "December 2026" and not fact.cal_current and fact.cal_dir == 1 and fact.cal_turn == turn + 1)
assert(fact["rail.serial.1"] == serial + 1 and fact["rail.goal.1"] == 241.5)
assert(text.cal_note == "Viewing December")
events.calendar_previous(); events.calendar_previous()
assert(text.cal_title == "October 2026" and fact.cal_dir == -1 and fact.cal_turn == turn + 3)
events.calendar_scroll(1)
assert(text.cal_title == "September 2026", "a notch upwards goes back a month")
events.calendar_scroll(-1); events.calendar_scroll(-1)
assert(text.cal_title == "November 2026")
events.calendar_scroll(-0.4); events.calendar_scroll(-0.4)
assert(text.cal_title == "November 2026", "touchpad fractions wait for a whole notch")
events.calendar_scroll(-0.4)
assert(text.cal_title == "December 2026")
events.calendar_scroll(0.5); events.calendar_scroll(0.5)
assert(text.cal_title == "November 2026", "turning back starts the count again")
events.calendar_scroll(0); events.calendar_scroll(0 / 0); events.calendar_scroll("up")
assert(text.cal_title == "November 2026")
print("Month stepping with the buttons and the wheel")

-- Away from today's month, the note names the hovered day with its ISO week.
events.calendar_month(6)
assert(text.cal_title == "June 2026" and text.cal_note == "Viewing June" and text.cal_today_day == "20")
events.calendar_hover(index_of(2026, 6, 11))
assert(text.cal_note == "Thu 11 Jun · W24")
events.calendar_hover(index_of(2026, 5, 31))
assert(text.cal_note == "Sun 31 May · W22")
events.calendar_unhover(index_of(2026, 5, 31))
print("Hover text in another month")

-- Today returns to the current month and drops a selection.
events.calendar_day(index_of(2026, 6, 11))
assert(text.cal_note == "Thu 11 Jun · W24" and fact.cal_selected_day == 11 and fact.cal_month == 6)
turn = fact.cal_turn
events.calendar_today()
assert(text.cal_title == "November 2026" and fact.cal_current and text.cal_note == "Today")
assert(fact.cal_selected_month == 11 and fact.cal_selected_day == 20 and fact.cal_dir == 1 and fact.cal_turn == turn + 1)
assert(fact["rail.goal.1"] == 220.5)
turn = fact.cal_turn
events.calendar_today()
assert(fact.cal_turn == turn, "Today on today's month does not slide the grid")
print("Today")

-- Scrubbing: a press jumps to that month, a drag changes it live and the release settles the bead.
serial = fact["rail.serial.1"]
events.calendar_scrub_press(5 * 21 + 3)
assert(text.cal_title == "June 2026" and fact["rail.serial.1"] == serial + 1 and fact["rail.goal.1"] == 115.5)
assert(not fact.cal_scrubbing and text.cal_note == "Viewing June", "a press alone is not a scrub")
events.calendar_scrub(130)
assert(fact.cal_scrubbing and text.cal_title == "July 2026" and text.cal_note == "Scrubbing July")
assert(fact["rail.target.1"] == 130 and fact["rail.serial.1"] == serial + 1, "the bead follows the pointer, not the slot")
events.calendar_scrub(40)
assert(text.cal_title == "February 2026" and fact.cal_weeks == 4 and fact["rail.target.1"] == 40 and fact.cal_dir == -1)
assert(fact.cal_rows == 5, "a scrub keeps the taller layout, so the horizon stays under the pointer")
events.calendar_scrub(7 * 21 + 3)
assert(text.cal_title == "August 2026" and fact.cal_weeks == 6 and fact.cal_rows == 6)
events.calendar_scrub(40)
assert(fact.cal_weeks == 4 and fact.cal_rows == 6)
events.calendar_scrub(-80)
assert(text.cal_title == "January 2026" and fact["rail.target.1"] == 0, "the drag is held to the rail")
events.calendar_scrub(900)
assert(text.cal_title == "December 2026" and fact["rail.target.1"] == 252)
events.calendar_hover(index_of(2026, 12, 9))
assert(text.cal_note == "Scrubbing December" and not fact.cal_hovering, "a scrub outranks a hovered day")
events.calendar_scrub(5 * 21 + 10)
assert(text.cal_today_day == "20" and text.cal_today_chip == "Today · Nov 20", "the left column stays on today")
serial = fact["rail.serial.1"]
events.calendar_scrub_release()
assert(not fact.cal_scrubbing and text.cal_title == "June 2026")
assert(fact.cal_rows == 5, "the release settles the layout on the shown month")
assert(fact["rail.serial.1"] == serial + 1 and fact["rail.goal.1"] == 115.5, "the release switches the bead into June")
events.calendar_scrub_release()
assert(fact["rail.serial.1"] == serial + 1, "a second release changes nothing")
events.calendar_scrub(0 / 0); events.calendar_scrub_press("x")
assert(text.cal_title == "June 2026" and not fact.cal_scrubbing)
print("Scrubbing the horizon")

-- Year boundaries: stepping past December and January changes the year, and the bead crosses the rail.
events.calendar_month(12)
events.calendar_next()
assert(text.cal_title == "January 2027" and text.cal_year_text == "2027" and fact["rail.goal.1"] == 10.5)
events.calendar_hover(index_of(2027, 1, 1))
assert(text.cal_note == "Fri 1 Jan · W53", "1 January 2027 belongs to ISO week 53 of 2026")
events.calendar_hover(index_of(2027, 1, 4))
assert(text.cal_note == "Mon 4 Jan · W1")
events.calendar_previous(); events.calendar_previous()
assert(text.cal_title == "November 2026" and fact.cal_current)
events.calendar_month(1); events.calendar_previous()
assert(text.cal_title == "December 2025" and fact["rail.goal.1"] == 241.5)
events.calendar_hover(index_of(2025, 12, 29))
assert(text.cal_note == "Mon 29 Dec · W1", "29 December 2025 opens 2026's first ISO week")
events.calendar_scrub(0)
assert(text.cal_title == "January 2025", "the horizon scrubs the shown year")
events.calendar_scrub_release()
watches.clock({year = 9999, month = 12, day = 31})
events.calendar_today(); events.calendar_next()
assert(text.cal_title == "December 9999", "the calendar stops at year 9999")
watches.clock({year = 1, month = 1, day = 1})
events.calendar_today(); events.calendar_previous(); events.calendar_scroll(3)
assert(text.cal_title == "January 1")
print("Year boundaries")

-- Leap years and the date changing while open.
watches.clock({year = 2028, month = 2, day = 28})
events.calendar_today()
events.calendar_hover(index_of(2028, 2, 29))
assert(text.cal_note == "Tue 29 · +1d")
watches.clock({year = 2028, month = 2, day = 29})
assert(text.cal_today_day == "29" and text.cal_note == "Tue 29 · Today" and text.cal_today_line == "Tuesday · W9")
print("Leap day and a date change while open")

-- Closing drops hover and scrub, and reopening returns to today.
events.calendar_scrub(30)
calendar.close()
assert(not fact.cal_open and not fact.cal_scrubbing and not fact.cal_hovering)
calendar.open()
assert(fact.cal_current and text.cal_note == "Today" and fact["rail.land.1"] >= 2)
print("Calendar controller passed")
