-- Gregorian date arithmetic is interaction policy; the native clock supplies today.
return function(rail)
    -- Inject from the actual entry: native require resolves relative to that entry.
    local rail_ready = false
    local today, selected, year, month
    local months = {"January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"}
    local short = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"}
    local weekdays = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}
    local function length(y, m)
        if m == 2 then return (y % 4 == 0 and (y % 100 ~= 0 or y % 400 == 0)) and 29 or 28 end
        return ({31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31})[m]
    end
    local function weekday(y, m, d)
        local offsets = {0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4}
        if m < 3 then y = y - 1 end
        return (y + math.floor(y / 4) - math.floor(y / 100) + math.floor(y / 400) + offsets[m] + d) % 7
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
    local function publish()
        if not today or not year then return end
        local lead, days = weekday(year, month, 1), {}
        local weeks = math.ceil((lead + length(year, month)) / 7)
        for i = 0, weeks * 7 - 1 do
            local date = normalize(year, month, i - lead + 1)
            days[#days + 1] = {day = tostring(date.day), year = date.year, month = date.month, number = date.day,
                row = math.floor(i / 7), col = i % 7, outside = date.month ~= month, today = same(date, today), selected = same(date, selected)}
        end
        model.cal_days = days
        fact.cal_month, fact.cal_weeks = month, weeks
        if rail_ready and fact.cal_open then rail.observe(12, month) end
        fact.cal_current = year == today.year and month == today.month
        text.cal_title = months[month] .. " " .. tostring(year)
        text.cal_footer = string.format("%s %d %s", weekdays[weekday(selected.year, selected.month, selected.day) + 1], selected.day, short[selected.month])
        local ordinal = selected.day - 1
        for m = 1, selected.month - 1 do ordinal = ordinal + length(selected.year, m) end
        text.cal_week = "Week " .. tostring(math.ceil((ordinal + weekday(selected.year, 1, 1) + 1) / 7))
    end
    local function reset()
        if not today then return end
        year, month = today.year, today.month
        selected = {year = year, month = month, day = today.day}
        publish()
    end
    sys.watch("clock", function(value)
        today = {year = value.year, month = value.month, day = value.day}
        if not year then reset() else publish() end
    end)
    local function open()
        rail_ready = rail and rail.reset() == true or false
        fact.cal_open = true
        reset()
    end
    local function close() fact.cal_open = false; if rail then rail.stop() end end
    local function toggle() if fact.cal_open then close() else open() end end
    on("fact:cal_open", function(visible) if visible then open() elseif rail then rail.stop() end end)
    on("calendar_toggle", toggle)
    on("calendar_open", open)
    on("calendar_close", close)
    on("calendar_today", reset)
    local function shift(delta)
        if not year then return end
        local date = normalize(year, month + delta, 1)
        if date.year < 1 or date.year > 9999 then return end
        year, month = date.year, date.month
        publish()
    end
    on("calendar_previous", function() shift(-1) end)
    on("calendar_next", function() shift(1) end)
    on("calendar_month", function(value)
        if year and value == math.floor(value) and value >= 1 and value <= 12 then month = value; publish() end
    end)
    on("calendar_day", function(index)
        if index ~= math.floor(index) then return end
        local day = model.cal_days[index + 1]
        if not day then return end
        selected = {year = day.year, month = day.month, day = day.number}
        year, month = day.year, day.month
        publish()
    end)
    return {open = open, close = close, toggle = toggle}
end
