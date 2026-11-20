local previous = {}
local shown = {}
local slots = {}
local sequence = 0
local serial = 0
local ticking = nil
local pill_timer = nil
local linger = nil
local LINGER_MS = 450
local POWER_ID = -1
local SLOTS = 4
local VISIBLE = 3
local DEFAULT_MS = 6000
local TICK_MS = 100
local PILL_MS = 2600
local WIDTH = 374
local BOTTOM = 608
local GAP = 9
local PILL_H = 46
local tone = { wave = "sine", freq = 880, ms = 80, volume = 0.07, release = 60 }
local SEEN = "notices.json"
local clock = os.clock

local function tell()
    local state = (fact.unread and 1 or 0) + (fact.dnd and 2 or 0)
    pcall(sys.call, "say", "tohru", "emit notices_state " .. state)
end
local function set_unread(unread)
    if fact.unread == unread then return end
    fact.unread = unread
    pcall(sys.call, "files.write", SEEN, { unread = unread })
    tell()
end
local function bounded(value, count)
    local boundary = utf8.offset(value, count + 1)
    return boundary and string.sub(value, 1, boundary - 1) or value
end

-- SF Pro Text advance widths at 12 px, rounded up, so a body is never cut short of a line it needs.
local function advance(code)
    if code == 32 then return 3.6 end
    if code > 127 then return code > 0x2000 and 13 or 7.4 end
    local c = string.char(code)
    if c:find("[iljI%.,:;'|!`]") then return 3.4 end
    if c:find("[frt%(%)%[%]{}\"%-/]") then return 4.6 end
    if c:find("[mwMW@]") then return 10.2 end
    if c:find("%u") then return 8.6 end
    if c:find("%d") then return 7.4 end
    return 6.9
end
local function body_lines(body, width)
    if body == "" then return 0 end
    local lines, used = 1, 0
    for paragraph in (body .. "\n"):gmatch("(.-)\n") do
        if used > 0 then lines = lines + 1; used = 0 end
        for word, space in paragraph:gmatch("(%S+)(%s*)") do
            local w = 0
            for _, code in utf8.codes(word) do w = w + advance(code) end
            if used > 0 and used + w > width then lines = lines + 1; used = 0 end
            while w > width do lines = lines + 1; w = w - width end
            used = used + w + #space * 3.6
        end
        if lines >= 3 then return 3 end
    end
    return math.min(lines, 3)
end

local function height(entry)
    local top = entry.critical and 49 or 41
    local bottom = top + 17
    if entry.lines > 0 then bottom = bottom + 2 + entry.lines * 16.5 end
    if entry.pic ~= "" then bottom = math.max(bottom, top + 47) end
    return math.ceil(bottom + 11 + (#entry.buttons > 0 and 30 or 0))
end

local function counting(entry)
    return entry.timeout > 0 and not entry.critical
end
local function left_ms(entry, now)
    if not counting(entry) then return 0 end
    if entry.held then return entry.left end
    return math.max(0, entry.left - (now - entry.stamp) * 1000)
end

local function row(slot, now)
    local entry = slot.entry
    if not entry then return { live = false, serial = slot.serial } end
    local buttons = entry.buttons
    return {
        live = slot.live, serial = slot.serial, y = slot.y, h = entry.h, depth = slot.depth,
        app = entry.app, title = entry.title, body = entry.body, lines = entry.lines,
        clock = entry.clock, icon = entry.icon, has_icon = entry.icon ~= "",
        pic = entry.pic, has_pic = entry.pic ~= "", critical = entry.critical,
        counting = counting(entry), held = entry.held,
        left = counting(entry) and left_ms(entry, now) / entry.timeout or 0,
        acts = #buttons, a1 = buttons[1] and buttons[1].label or "",
        a2 = buttons[2] and buttons[2].label or "", a3 = buttons[3] and buttons[3].label or "",
        opens = entry.default ~= nil,
    }
end

local schedule
local function publish()
    local now = clock()
    local bottom = BOTTOM - (fact.dnd_pill and PILL_H + GAP or 0)
    for depth, entry in ipairs(shown) do
        local slot = slots[entry.slot]
        slot.y = bottom - entry.h
        slot.depth = depth - 1
        bottom = slot.y - GAP
    end
    local rows = {}
    for index = 1, SLOTS do rows[index] = row(slots[index], now) end
    model.toasts = rows
    -- The popup stays open while the last toast or the pill fades out.
    if #shown > 0 or fact.dnd_pill then
        if linger then cancel(linger); linger = nil end
        fact.toasts_open = true
    elseif fact.toasts_open and not linger then
        linger = after(LINGER_MS, function()
            linger = nil
            if #shown == 0 and not fact.dnd_pill then fact.toasts_open = false end
        end)
    end
    schedule()
end

local function tick()
    ticking = nil
    publish()
end
function schedule()
    local now = clock()
    local needed = false
    for _, entry in ipairs(shown) do
        if counting(entry) and not entry.held and left_ms(entry, now) > 0 then needed = true end
    end
    if needed and not ticking then ticking = after(TICK_MS, tick) end
    if not needed and ticking then cancel(ticking); ticking = nil end
end

local function free_slot()
    local best
    for index = 1, SLOTS do
        local slot = slots[index]
        if not slot.live and (not best or slot.freed < slots[best].freed) then best = index end
    end
    return best
end

local function release(entry)
    local slot = slots[entry.slot]
    slot.live = false
    sequence = sequence + 1
    slot.freed = sequence
    if entry.timer then cancel(entry.timer) end
end

local function remove(id)
    for index = #shown, 1, -1 do
        if shown[index].id == id then
            release(shown[index])
            table.remove(shown, index)
        end
    end
end

local function find(id)
    for _, entry in ipairs(shown) do if entry.id == id then return entry end end
end

local function timing(entry, notice, now)
    entry.timeout = tonumber(notice.timeout) or DEFAULT_MS
    entry.left = tonumber(notice.left) or entry.timeout
    entry.stamp = now
end

local function fill(entry, notice, title, body)
    entry.app = bounded(notice.app or "", 64)
    entry.title = title
    entry.body = body
    entry.icon = notice.icon or ""
    entry.pic = notice.image or ""
    entry.critical = (notice.urgency or 1) >= 2
    entry.clock = notice.clock or os.date("%H:%M", notice.time)
    entry.default = nil
    entry.buttons = {}
    for _, action in ipairs(notice.actions or {}) do
        if action.key == "default" then entry.default = action.key
        elseif #entry.buttons < 3 and action.label ~= "" then
            entry.buttons[#entry.buttons + 1] = { key = action.key, label = bounded(action.label, 32) }
        end
    end
    entry.lines = body_lines(body, (entry.pic ~= "" and 287 or 344) - 8)
    entry.h = height(entry)
end

-- Under Do Not Disturb only power warnings show and sound. A replacement keeps its place in the stack.
local function incoming(notice, title, body)
    local now = clock()
    local entry = find(notice.id)
    if fact.dnd and notice.id ~= POWER_ID then
        if entry then remove(notice.id) end
        return
    end
    if not entry then
        entry = { id = notice.id }
        local index = free_slot()
        local slot = slots[index]
        serial = serial + 1
        slot.serial = serial
        slot.live = true
        slot.entry = entry
        entry.slot = index
        table.insert(shown, 1, entry)
        while #shown > VISIBLE do
            release(shown[#shown])
            table.remove(shown)
        end
    elseif entry.held then
        entry.held = false
    end
    fill(entry, notice, title, body)
    timing(entry, notice, now)
    if notice.id == POWER_ID then
        if entry.timer then cancel(entry.timer) end
        entry.timer = counting(entry) and after(entry.timeout, function()
            remove(POWER_ID)
            publish()
        end) or nil
    end
    sound.tone(tone)
end

for index = 1, SLOTS do slots[index] = { live = false, serial = 0, freed = 0, y = BOTTOM, depth = 0 } end
fact.dnd_pill = false
publish()
local saved = sys.ask("files.read", SEEN, "json")
fact.unread = type(saved) == "table" and saved.unread == true
tell()
sys.watch("notifications", function(notices)
    local current = {}
    local watermark = fact.last_id
    local known_after = false
    local now = clock()
    -- Bound UI work/state; native protocol ownership/history stays in Pleamar.
    for index = math.min(#notices, 64), 1, -1 do
        local notice = notices[index]
        local title = bounded(notice.title, 256)
        local body = bounded(notice.body, 768)
        local fingerprint = title .. "\0" .. body .. "\0" .. tostring(notice.time)
        current[notice.id] = fingerprint
        -- Native Notify moves a replacement to the front. Pruning alone
        -- cannot move an unknown old ID ahead of an entry from the old window.
        local moved_replacement = previous[notice.id] == nil and known_after
        -- Only a new ID is unread: a replacement keeps whether it was seen, and low urgency is never kept.
        if notice.id > watermark and (notice.urgency or 1) > 0 then set_unread(true) end
        if notice.id > watermark or moved_replacement or (previous[notice.id] and previous[notice.id] ~= fingerprint) then
            incoming(notice, title, body)
        else
            local entry = find(notice.id)
            if entry and not entry.held then timing(entry, notice, now) end
        end
        if previous[notice.id] ~= nil then known_after = true end
        fact.last_id = math.max(fact.last_id, notice.id)
    end
    previous = current
    for index = #shown, 1, -1 do if shown[index].id ~= POWER_ID and current[shown[index].id] == nil then remove(shown[index].id) end end
    publish()
end)

local function power_warning(level, title, charge)
    if type(charge) ~= "number" or charge ~= charge or charge < 0 or charge > 1 then return end
    if level == 0 then
        remove(POWER_ID)
        fact.power_level = 0
        publish()
        return
    end
    if fact.power_level == level then return end
    fact.power_level = level
    local percent = tostring(math.floor(charge * 100 + 0.5)) .. "%"
    incoming({
        id = POWER_ID, app = "Power", urgency = level >= 2 and 2 or 1, clock = percent,
        timeout = level >= 2 and 0 or DEFAULT_MS,
    }, title, percent .. " battery remaining")
    publish()
end

local function slot_entry(index)
    local slot = slots[math.floor(tonumber(index) or 0) + 1]
    return slot and slot.live and slot.entry or nil
end

local function hold(index, yes)
    local entry = slot_entry(index)
    if not entry or entry.held == yes or not counting(entry) then return end
    local now = clock()
    entry.left = left_ms(entry, now)
    entry.stamp = now
    entry.held = yes
    if entry.id == POWER_ID then
        if entry.timer then cancel(entry.timer); entry.timer = nil end
        if not yes then entry.timer = after(entry.left, function() remove(POWER_ID); publish() end) end
    else
        pcall(sys.call, "notifications.hold", entry.id, yes)
    end
    publish()
end

local function close(entry)
    if entry.id == POWER_ID then
        remove(POWER_ID)
    else
        pcall(sys.call, "notifications.dismiss", entry.id)
        remove(entry.id)
    end
    publish()
end

on("toast_enter", function(index) hold(index, true) end)
on("toast_leave", function(index) hold(index, false) end)
on("toast_close", function(index)
    local entry = slot_entry(index)
    if entry then close(entry) end
end)
on("toast_open", function(index)
    local entry = slot_entry(index)
    if not entry or not entry.default then return end
    pcall(sys.call, "notifications.invoke", entry.id, entry.default)
    remove(entry.id)
    publish()
end)
-- slot * 4 + button, buttons from 1.
on("toast_action", function(code)
    code = math.floor(tonumber(code) or -1)
    local entry = slot_entry(code // 4)
    local button = entry and entry.buttons[code % 4]
    if not button then return end
    if entry.id == POWER_ID then return close(entry) end
    pcall(sys.call, "notifications.invoke", entry.id, button.key)
    remove(entry.id)
    publish()
end)

on("power_warning_clear", function(charge) power_warning(0, "", charge) end)
on("power_warning_low", function(charge) power_warning(1, "Battery low", charge) end)
on("power_warning_critical", function(charge) power_warning(2, "Battery critical", charge) end)
on("power_warning_empty", function(charge) power_warning(3, "Battery nearly empty", charge) end)
on("notices_seen", function() set_unread(false) end)

local function pill(show)
    if pill_timer then cancel(pill_timer); pill_timer = nil end
    fact.dnd_pill = show
    if show then
        pill_timer = after(PILL_MS, function()
            pill_timer = nil
            fact.dnd_pill = false
            publish()
        end)
    end
end
-- 0 off, 1 on, 2 toggle.
on("notices_dnd", function(mode)
    if mode ~= 0 and mode ~= 1 and mode ~= 2 then return end
    local dnd = mode == 1 or (mode == 2 and not fact.dnd)
    if dnd == fact.dnd then return end
    fact.dnd = dnd
    if dnd then
        for index = #shown, 1, -1 do if shown[index].id ~= POWER_ID then remove(shown[index].id) end end
    end
    pill(dnd)
    publish()
    tell()
end)
on("notices_report", tell)
