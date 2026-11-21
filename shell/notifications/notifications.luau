local previous = {}
local shown = {}
local slots = {}
local live_notices = {}
local server_history = {}
local persisted_history = {}
local center_all = {}
local sequence = 0
local serial = 0
local ticking = nil
local pill_timer = nil
local linger = nil
local LINGER_MS = 450
local POWER_ID = -1
local SLOTS = 4
local VISIBLE = 3
local CENTER_PAGE_SIZE = 5
local MAX_HISTORY = 50
local DEFAULT_MS = 6000
local TICK_MS = 100
local PILL_MS = 2600
local WIDTH = 374
local TOP = 6
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
local function tell_center()
    pcall(sys.call, "say", "tohru", "emit notices_center_state " .. (fact.center_open and 1 or 0))
end
local function save_state()
    pcall(sys.call, "files.write", SEEN, { unread = fact.unread == true, history = persisted_history })
end
local function set_unread(unread)
    if fact.unread == unread then return end
    fact.unread = unread
    save_state()
    tell()
end
local function bounded(value, count)
    if type(value) ~= "string" then return "" end
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

local function covered()
    return fact.covered == true and fact.corner == "top_right"
end
local function showing()
    return fact.center_open == true or ((#shown > 0 or fact.dnd_pill) and not covered())
end

local function history_key(item)
    return tostring(item.id or 0) .. "\0" .. tostring(item.time or 0) .. "\0" .. tostring(item.title or "")
end

local function normalize_history(notice)
    local title = bounded(notice.title or "", 256)
    local body = bounded(notice.body or "", 768)
    local time = tonumber(notice.time) or 0
    return {
        id = tonumber(notice.id) or 0,
        app = bounded(notice.app or "", 64),
        title = title,
        body = body,
        icon = type(notice.icon) == "string" and notice.icon or "",
        pic = type(notice.image) == "string" and notice.image or (type(notice.pic) == "string" and notice.pic or ""),
        urgency = tonumber(notice.urgency) or 1,
        time = time,
        clock = type(notice.clock) == "string" and notice.clock ~= "" and notice.clock or os.date("%H:%M", time),
    }
end

local function merge_persisted(items)
    local seen = {}
    local merged = {}
    local changed = false
    for _, item in ipairs(items) do
        local norm = normalize_history(item)
        local key = history_key(norm)
        if not seen[key] then
            seen[key] = true
            merged[#merged + 1] = norm
        end
    end
    for _, item in ipairs(persisted_history) do
        local key = history_key(item)
        if not seen[key] and #merged < MAX_HISTORY then
            seen[key] = true
            merged[#merged + 1] = item
        end
    end
    if #merged ~= #persisted_history then
        changed = true
    else
        for i = 1, #merged do
            if history_key(merged[i]) ~= history_key(persisted_history[i]) then changed = true; break end
        end
    end
    if changed then
        persisted_history = merged
        save_state()
    end
end

local function find_power()
    for _, entry in ipairs(shown) do if entry.id == POWER_ID then return entry end end
end

local function publish_center()
    local items = {}
    local live_ids = {}
    local live_count = 0

    local power = find_power()
    if power then
        live_count = live_count + 1
        items[#items + 1] = {
            id = POWER_ID, active = true, critical = power.critical, opens = false,
            app = power.app, title = power.title, body = power.body, clock = power.clock,
            icon = power.icon, pic = power.pic, buttons = {}, default = nil,
        }
    end

    for _, notice in ipairs(live_notices) do
        live_ids[notice.id] = true
        live_count = live_count + 1
        local buttons, def = {}, nil
        for _, action in ipairs(notice.actions or {}) do
            if action.key == "default" then def = action.key
            elseif #buttons < 3 and action.label ~= "" then
                buttons[#buttons + 1] = { key = action.key, label = bounded(action.label, 32) }
            end
        end
        items[#items + 1] = {
            id = notice.id, active = true, critical = (notice.urgency or 1) >= 2, opens = def ~= nil,
            app = bounded(notice.app or "", 64),
            title = bounded(notice.title or "", 256),
            body = bounded(notice.body or "", 768),
            clock = notice.clock or os.date("%H:%M", notice.time),
            icon = notice.icon or "", pic = notice.image or "",
            buttons = buttons, default = def,
        }
    end

    local hist_seen = {}
    local recent_count = 0
    local function push_hist(raw)
        local norm = normalize_history(raw)
        local key = history_key(norm)
        if live_ids[norm.id] or hist_seen[key] then return end
        hist_seen[key] = true
        recent_count = recent_count + 1
        items[#items + 1] = {
            id = norm.id, key = key, active = false, critical = norm.urgency >= 2, opens = false,
            app = norm.app, title = norm.title, body = norm.body, clock = norm.clock,
            icon = norm.icon, pic = norm.pic, buttons = {}, default = nil,
        }
    end
    for _, h in ipairs(server_history) do push_hist(h) end
    for _, h in ipairs(persisted_history) do push_hist(h) end

    center_all = items
    local total = #items
    local pages = math.max(1, math.ceil(total / CENTER_PAGE_SIZE))
    local page = math.max(0, math.min(math.floor(fact.center_page or 0), pages - 1))
    fact.center_count = total
    fact.center_pages = pages
    fact.center_page = page
    text.center_count_text = string.format("%02d", math.min(total, 99))
    if fact.dnd then
        text.center_status = total > 0 and ("Do Not Disturb · " .. total .. " stored") or "Do Not Disturb · On"
    elseif total == 0 then
        text.center_status = "All clear"
    elseif live_count > 0 and recent_count > 0 then
        text.center_status = live_count .. " live · " .. recent_count .. " recent"
    elseif live_count > 0 then
        text.center_status = live_count .. " live"
    else
        text.center_status = recent_count .. " recent"
    end

    local start_idx = page * CENTER_PAGE_SIZE
    local visible_count = math.max(0, math.min(CENTER_PAGE_SIZE, total - start_idx))
    fact.center_selected = math.max(0, math.min(math.floor(fact.center_selected or 0), math.max(0, visible_count - 1)))

    local rows = {}
    for i = 1, CENTER_PAGE_SIZE do
        local entry = items[start_idx + i]
        if entry then
            local btns = entry.buttons or {}
            rows[i] = {
                live = true, active = entry.active, critical = entry.critical, opens = entry.opens,
                app = entry.app, title = entry.title, body = entry.body, clock = entry.clock,
                icon = entry.icon, has_icon = entry.icon ~= "",
                pic = entry.pic, has_pic = entry.pic ~= "",
                acts = #btns,
                a1 = btns[1] and btns[1].label or "",
                a2 = btns[2] and btns[2].label or "",
                a3 = btns[3] and btns[3].label or "",
            }
        else
            rows[i] = {
                live = false, active = false, critical = false, opens = false,
                app = "", title = "", body = "", clock = "",
                icon = "", has_icon = false, pic = "", has_pic = false,
                acts = 0, a1 = "", a2 = "", a3 = "",
            }
        end
    end
    model.center_items = rows
end

local schedule
-- The newest toast is nearest the corner: under the bar at the top right, at the bottom edge at the bottom right.
local function publish()
    local now = clock()
    local pill = (fact.dnd_pill and not fact.center_open) and PILL_H + GAP or 0
    local top, bottom = TOP + pill, BOTTOM - pill
    for depth, entry in ipairs(shown) do
        local slot = slots[entry.slot]
        if fact.corner == "top_right" then
            slot.y = top
            top = top + entry.h + GAP
        else
            slot.y = bottom - entry.h
            bottom = slot.y - GAP
        end
        slot.depth = depth - 1
    end
    local rows = {}
    for index = 1, SLOTS do rows[index] = row(slots[index], now) end
    model.toasts = rows
    publish_center()
    -- The popup stays open while the last toast or the pill fades out, or while the notification center is open.
    if showing() then
        if linger then cancel(linger); linger = nil end
        fact.toasts_open = true
    elseif fact.toasts_open and not linger then
        linger = after(LINGER_MS, function()
            linger = nil
            if not showing() then fact.toasts_open = false end
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

-- A toast's line stops while it is hovered, covered by a bar popup, or while the notification center is open.
local function sync_hold(entry)
    local yes = entry.hovered == true or covered() or fact.center_open == true
    if (entry.held == true) == yes or not counting(entry) then return end
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
    sync_hold(entry)
    sound.tone(tone)
end

for index = 1, SLOTS do slots[index] = { live = false, serial = 0, freed = 0, y = BOTTOM, depth = 0 } end
fact.dnd_pill = false
fact.center_open = fact.center_open == true
local saved = sys.ask("files.read", SEEN, "json")
fact.unread = type(saved) == "table" and saved.unread == true
if type(saved) == "table" and type(saved.history) == "table" then
    for i = 1, math.min(#saved.history, MAX_HISTORY) do
        if type(saved.history[i]) == "table" then
            persisted_history[#persisted_history + 1] = normalize_history(saved.history[i])
        end
    end
end
publish()
tell()
tell_center()

sys.watch("notifications", function(notices)
    local current = {}
    local watermark = fact.last_id
    local known_after = false
    local now = clock()
    live_notices = {}
    -- Bound UI work/state; native protocol ownership/history stays in Pleamar.
    local limit = math.min(#notices, 64)
    for index = 1, limit do live_notices[index] = notices[index] end
    for index = limit, 1, -1 do
        local notice = notices[index]
        local title = bounded(notice.title, 256)
        local body = bounded(notice.body, 768)
        local fingerprint = title .. "\0" .. body .. "\0" .. tostring(notice.time)
        current[notice.id] = fingerprint
        -- Native Notify moves a replacement to the front. Pruning alone
        -- cannot move an unknown old ID ahead of an entry from the old window.
        local moved_replacement = previous[notice.id] == nil and known_after
        -- Only a new ID is unread: a replacement keeps whether it was seen, and low urgency is never kept.
        if notice.id > watermark and (notice.urgency or 1) > 0 and not fact.center_open then set_unread(true) end
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

pcall(sys.watch, "notification_history", function(items)
    server_history = {}
    if type(items) == "table" then
        for i = 1, math.min(#items, MAX_HISTORY) do
            server_history[i] = items[i]
        end
        if #server_history > 0 then merge_persisted(server_history) end
    end
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
    if not entry then return end
    entry.hovered = yes
    sync_hold(entry)
    publish()
end

local function close(entry)
    if entry.id == POWER_ID then
        remove(POWER_ID)
    else
        pcall(sys.call, "notifications.dismiss", entry.id)
        remove(entry.id)
        for i = #live_notices, 1, -1 do
            if live_notices[i].id == entry.id then table.remove(live_notices, i) end
        end
    end
    publish()
end

local function center_item(slot)
    slot = math.floor(tonumber(slot) or -1)
    if slot < 0 or slot >= CENTER_PAGE_SIZE then return nil end
    return center_all[(fact.center_page or 0) * CENTER_PAGE_SIZE + slot + 1]
end

local function dismiss_center_item(entry)
    if not entry then return end
    if entry.active then
        close(entry)
        return
    end
    if entry.id and entry.id > 0 then pcall(sys.call, "notifications.forget", entry.id) end
    for i = #server_history, 1, -1 do
        if history_key(normalize_history(server_history[i])) == entry.key then table.remove(server_history, i) end
    end
    local removed = false
    for i = #persisted_history, 1, -1 do
        if history_key(persisted_history[i]) == entry.key then
            table.remove(persisted_history, i)
            removed = true
        end
    end
    if removed then save_state() end
    publish()
end

local function open_center()
    if fact.center_open then return end
    fact.center_open = true
    fact.center_page = 0
    fact.center_selected = 0
    fact.center_keyed = false
    if pill_timer then cancel(pill_timer); pill_timer = nil end
    fact.dnd_pill = false
    set_unread(false)
    for _, entry in ipairs(shown) do sync_hold(entry) end
    publish()
    tell_center()
end

local function close_center()
    if not fact.center_open then return end
    fact.center_open = false
    fact.center_keyed = false
    for _, entry in ipairs(shown) do sync_hold(entry) end
    publish()
    tell_center()
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

on("notices_center_open", open_center)
on("notices_center_close", close_center)
on("notices_center_toggle", function()
    if fact.center_open then close_center() else open_center() end
end)
on("notices_clear_all", function()
    pcall(sys.call, "notifications.clear")
    pcall(sys.call, "notifications.clear_history")
    remove(POWER_ID)
    fact.power_level = 0
    live_notices = {}
    server_history = {}
    persisted_history = {}
    for index = #shown, 1, -1 do release(shown[index]); table.remove(shown, index) end
    fact.unread = false
    save_state()
    tell()
    publish()
end)
on("center_dismiss", function(slot)
    dismiss_center_item(center_item(slot))
end)
on("center_open_item", function(slot)
    local entry = center_item(slot)
    if not entry or not entry.active or not entry.default then return end
    pcall(sys.call, "notifications.invoke", entry.id, entry.default)
    remove(entry.id)
    for i = #live_notices, 1, -1 do
        if live_notices[i].id == entry.id then table.remove(live_notices, i) end
    end
    publish()
end)
on("center_action", function(code)
    code = math.floor(tonumber(code) or -1)
    local entry = center_item(code // 4)
    local button = entry and entry.active and entry.buttons[code % 4]
    if not button then return end
    pcall(sys.call, "notifications.invoke", entry.id, button.key)
    remove(entry.id)
    for i = #live_notices, 1, -1 do
        if live_notices[i].id == entry.id then table.remove(live_notices, i) end
    end
    publish()
end)
on("center_page_step", function(delta)
    if delta ~= -1 and delta ~= 1 then return end
    local pages = math.max(1, fact.center_pages or 1)
    local next_page = math.max(0, math.min((fact.center_page or 0) + delta, pages - 1))
    if next_page ~= fact.center_page then
        fact.center_page = next_page
        fact.center_selected = 0
        publish_center()
    end
end)
on("center_focus_move", function(delta)
    if delta ~= -1 and delta ~= 1 then return end
    if #center_all == 0 then return end
    fact.center_keyed = true
    local page = fact.center_page or 0
    local start_idx = page * CENTER_PAGE_SIZE
    local visible = math.max(0, math.min(CENTER_PAGE_SIZE, #center_all - start_idx))
    local next_sel = (fact.center_selected or 0) + delta
    if next_sel < 0 and page > 0 then
        fact.center_page = page - 1
        fact.center_selected = CENTER_PAGE_SIZE - 1
        publish_center()
    elseif next_sel >= visible and page + 1 < (fact.center_pages or 1) then
        fact.center_page = page + 1
        fact.center_selected = 0
        publish_center()
    else
        fact.center_selected = math.max(0, math.min(next_sel, math.max(0, visible - 1)))
    end
end)
on("center_activate", function()
    local entry = center_item(fact.center_selected or 0)
    if not entry or not entry.active then return end
    if entry.default then
        pcall(sys.call, "notifications.invoke", entry.id, entry.default)
        remove(entry.id)
        for i = #live_notices, 1, -1 do
            if live_notices[i].id == entry.id then table.remove(live_notices, i) end
        end
        publish()
    elseif entry.buttons[1] then
        pcall(sys.call, "notifications.invoke", entry.id, entry.buttons[1].key)
        remove(entry.id)
        for i = #live_notices, 1, -1 do
            if live_notices[i].id == entry.id then table.remove(live_notices, i) end
        end
        publish()
    end
end)
on("center_delete", function()
    dismiss_center_item(center_item(fact.center_selected or 0))
end)
on("fact:toasts_open", function(open)
    if not open and fact.center_open then close_center() end
end)

on("power_warning_clear", function(charge) power_warning(0, "", charge) end)
on("power_warning_low", function(charge) power_warning(1, "Battery low", charge) end)
on("power_warning_critical", function(charge) power_warning(2, "Battery critical", charge) end)
on("power_warning_empty", function(charge) power_warning(3, "Battery nearly empty", charge) end)
on("notices_seen", function() set_unread(false) end)

local function pill(show)
    if pill_timer then cancel(pill_timer); pill_timer = nil end
    fact.dnd_pill = show and not fact.center_open
    if fact.dnd_pill then
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
on("notices_report", function()
    tell()
    tell_center()
end)

local function placement()
    for _, entry in ipairs(shown) do sync_hold(entry) end
    publish()
end
-- From the bar: 0 top right, 1 bottom right (notifications.position).
on("notices_position", function(position)
    if position ~= 0 and position ~= 1 then return end
    fact.corner = position == 1 and "bottom_right" or "top_right"
    placement()
end)
-- From the bar: 1 while one of its popups is open, 0 when none is.
on("notices_cover", function(open)
    if open ~= 0 and open ~= 1 then return end
    fact.covered = open == 1
    if fact.covered and fact.center_open then close_center(); return end
    placement()
end)
-- From the bar: appearance.palette index (0..7).
on("notices_palette", function(id)
    id = math.floor(tonumber(id) or -1)
    if id >= 0 and id <= 7 then fact.palette_id = id end
end)
pcall(sys.call, "say", "tohru", "emit notices_ask")
