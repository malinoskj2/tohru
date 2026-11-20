-- Gregorian date arithmetic is interaction policy; the native clock supplies today.
return function(rail)
    -- Inject from the actual entry: native require resolves relative to that entry.
    local pitch = 21
    local today, selected, year, month, hover
    local scrubbing, wheel = false, 0
    if (fact.cal_year or 0) > 0 then
        year, month = fact.cal_year, fact.cal_month
        selected = {year = fact.cal_selected_year, month = fact.cal_selected_month, day = fact.cal_selected_day}
    end
    local months = {"January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"}
    local short = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"}
    local weekdays = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}
    local long = {"Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"}
    local function length(y, m)
        if m == 2 then return (y % 4 == 0 and (y % 100 ~= 0 or y % 400 == 0)) and 29 or 28 end
        return ({31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31})[m]
    end
    local function weekday(y, m, d)
        local offsets = {0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4}
        if m < 3 then y = y - 1 end
        return (y + math.floor(y / 4) - math.floor(y / 100) + math.floor(y / 400) + offsets[m] + d) % 7
    end
    -- Days since 1970-01-01.
    local function serial(date)
        local y, m = date.year, date.month
        if m <= 2 then y = y - 1 end
        local era = math.floor(y / 400)
        local yoe = y - era * 400
        local doy = math.floor((153 * ((m + 9) % 12) + 2) / 5) + date.day - 1
        return era * 146097 + yoe * 365 + math.floor(yoe / 4) - math.floor(yoe / 100) + doy - 719468
    end
    -- ISO 8601: weeks start on Monday and belong to the year that holds their Thursday.
    local function iso_week(date)
        local thursday = serial(date) - (weekday(date.year, date.month, date.day) + 6) % 7 + 3
        local y = date.year
        if thursday < serial({year = y, month = 1, day = 1}) then
            y = y - 1
        elseif thursday >= serial({year = y + 1, month = 1, day = 1}) then
            y = y + 1
        end
        return math.floor((thursday - serial({year = y, month = 1, day = 1})) / 7) + 1
    end
    local function same(a, b) return a and b and a.year == b.year and a.month == b.month and a.day == b.day end
    local function normalize(y, m, d)
        if m == 0 then y, m = y - 1, 12 elseif m == 13 then y, m = y + 1, 1 end
        if d < 1 then
            m = m - 1
            if m == 0 then y, m = y - 1, 12 end
            d = length(y, m) + d
        elseif d > length(y, m) then
            d, m = d - length(y, m), m + 1
            if m == 13 then y, m = y + 1, 1 end
        end
        return {year = y, month = m, day = d}
    end
    -- The shown month's own days count from today; anything else is named with its ISO week.
    local function label(date)
        local name = weekdays[weekday(date.year, date.month, date.day) + 1] .. " " .. date.day
        if year == today.year and month == today.month then
            local offset = serial(date) - serial(today)
            local relative = offset == 0 and "Today" or string.format("%s%dd", offset > 0 and "+" or "−", math.abs(offset))
            if date.year ~= today.year or date.month ~= today.month then name = name .. " " .. short[date.month] end
            return name .. " · " .. relative
        end
        return string.format("%s %s · W%d", name, short[date.month], iso_week(date))
    end
    local function note()
        if scrubbing then return "Scrubbing " .. months[month] end
        local day = hover and model.cal_days[hover + 1]
        if day then return label({year = day.year, month = day.month, day = day.number}) end
        if not same(selected, today) then return label(selected) end
        if year == today.year and month == today.month then return "Today" end
        return "Viewing " .. months[month]
    end
    local function refresh()
        text.cal_note = note()
        fact.cal_hovering = hover ~= nil and not scrubbing
    end
    local function publish(turn)
        if not today or not year then return end
        local lead, days = weekday(year, month, 1), {}
        local weeks = math.ceil((lead + length(year, month)) / 7)
        for i = 0, weeks * 7 - 1 do
            local date = normalize(year, month, i - lead + 1)
            days[#days + 1] = {day = tostring(date.day), year = date.year, month = date.month, number = date.day,
                row = math.floor(i / 7), col = i % 7, outside = date.month ~= month, today = same(date, today), selected = same(date, selected)}
        end
        model.cal_days = days
        if hover and hover >= #days then hover = nil end
        fact.cal_year, fact.cal_month, fact.cal_weeks = year, month, weeks
        -- The horizon moves with the grid's height; while scrubbed it only grows, so it stays under the pointer.
        fact.cal_rows = scrubbing and math.max(fact.cal_rows or weeks, weeks) or weeks
        fact.cal_selected_year, fact.cal_selected_month, fact.cal_selected_day = selected.year, selected.month, selected.day
        fact.cal_scrubbing = scrubbing
        -- While dragged, the bead follows the pointer; the release switches it back into the month.
        if rail and fact.cal_open and not scrubbing then rail.observe(12, month) end
        if turn then
            fact.cal_dir = turn
            fact.cal_turn = (fact.cal_turn or 0) + 1
        end
        fact.cal_current = year == today.year and month == today.month
        text.cal_title = months[month] .. " " .. tostring(year)
        text.cal_month_name = months[month]
        text.cal_year_text = tostring(year)
        text.cal_today_day = tostring(today.day)
        text.cal_today_line = string.format("%s · W%d", long[weekday(today.year, today.month, today.day) + 1], iso_week(today))
        text.cal_today_chip = string.format("Today · %s %d", short[today.month], today.day)
        refresh()
    end
    local function show(y, m, extra)
        if y < 1 or y > 9999 or (y == year and m == month) then return false end
        local turn = (y * 12 + m > year * 12 + month) and 1 or -1
        year, month = y, m
        if extra then extra() end
        publish(turn)
        return true
    end
    local function reset()
        if not today then return end
        selected = {year = today.year, month = today.month, day = today.day}
        if not year or not show(today.year, today.month) then
            year, month = today.year, today.month
            publish()
        end
    end
    sys.watch("clock", function(value)
        today = {year = value.year, month = value.month, day = value.day}
        if not year then reset() else publish() end
    end)
    local function open()
        if rail then rail.reset() end
        hover, scrubbing, wheel = nil, false, 0
        fact.cal_open = true
        reset()
    end
    local function close()
        hover, scrubbing = nil, false
        fact.cal_open = false
        fact.cal_scrubbing = false
        fact.cal_hovering = false
    end
    local function toggle() if fact.cal_open then close() else open() end end
    on("fact:cal_open", function(visible) if visible then open() else close() end end)
    on("calendar_toggle", toggle)
    on("calendar_open", open)
    on("calendar_close", close)
    on("calendar_today", reset)
    local function shift(delta)
        if not year then return end
        local date = normalize(year, month + delta, 1)
        show(date.year, date.month)
    end
    on("calendar_previous", function() shift(-1) end)
    on("calendar_next", function() shift(1) end)
    -- Wheel notches, positive upwards, go back in time; a touchpad's fractions add up to whole months.
    on("calendar_scroll", function(notches)
        if type(notches) ~= "number" or notches ~= notches or notches == 0 then return end
        if (wheel > 0) ~= (notches > 0) then wheel = 0 end
        wheel = math.max(-3, math.min(3, wheel + notches))
        while math.abs(wheel) >= 1 do
            local step = wheel > 0 and 1 or -1
            wheel = wheel - step
            shift(-step)
        end
    end)
    on("calendar_month", function(value)
        if year and value == math.floor(value) and value >= 1 and value <= 12 then show(year, value) end
    end)
    on("calendar_day", function(index)
        if index ~= math.floor(index) then return end
        local day = model.cal_days[index + 1]
        if not day then return end
        selected = {year = day.year, month = day.month, day = day.number}
        if not show(day.year, day.month) then publish() end
    end)
    on("calendar_hover", function(index)
        if type(index) ~= "number" or index ~= math.floor(index) or not model.cal_days[index + 1] then return end
        hover = index
        refresh()
    end)
    -- Leaving one day only clears the hover if no other day was entered first.
    on("calendar_unhover", function(index)
        if hover ~= index then return end
        hover = nil
        refresh()
    end)
    -- The horizon sends pixels from the left end of its rail; each month owns `pitch` of them.
    local function slot(x)
        if type(x) ~= "number" or x ~= x then return nil end
        return math.max(1, math.min(12, math.floor(x / pitch) + 1))
    end
    on("calendar_scrub_press", function(x)
        local m = slot(x)
        if year and m then show(year, m) end
    end)
    on("calendar_scrub", function(x)
        local m = slot(x)
        if not year or not m then return end
        scrubbing = true
        if rail then rail.chase(math.max(0, math.min(12 * pitch, x))) end
        if not show(year, m) then publish() end
    end)
    on("calendar_scrub_release", function()
        if not scrubbing then return end
        scrubbing = false
        publish()
    end)
    if fact.cal_open and rail and month then rail.observe(12, month) end
    return {open = open, close = close, toggle = toggle, pitch = pitch}
end
