local previous = {}
local shown = {}
local sequence = 0
local POWER_ID = -1
local tone = { wave = "sine", freq = 880, ms = 80, volume = 0.07, release = 60 }
local SEEN = "notices.json"
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
local function publish()
    local rows = {}
    for _, entry in ipairs(shown) do rows[#rows + 1] = { title = entry.title, body = entry.body } end
    model.banners = rows
end
local function remove(id, revision)
    for index = #shown, 1, -1 do
        local entry = shown[index]
        if entry.id == id and (revision == nil or entry.revision == revision) then
            cancel(entry.timer)
            table.remove(shown, index)
        end
    end
end
-- Under Do Not Disturb only power warnings show and sound.
local function incoming(notice, title, body)
    remove(notice.id)
    if fact.dnd and notice.id ~= POWER_ID then return end
    sequence = sequence + 1
    local revision = sequence
    local entry = { id = notice.id, revision = revision, title = title, body = body }
    entry.timer = after(6000, function() remove(notice.id, revision); publish() end)
    table.insert(shown, 1, entry)
    if #shown > 4 then remove(shown[#shown].id, shown[#shown].revision) end
    -- The same native placeholder tone regardless of app/content/urgency.
    sound.tone(tone)
end
model.banners = {}
local saved = sys.ask("files.read", SEEN, "json")
fact.unread = type(saved) == "table" and saved.unread == true
tell()
sys.watch("notifications", function(notices)
    local current = {}
    local watermark = fact.last_id
    local known_after = false
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
        if notice.id > watermark or moved_replacement or (previous[notice.id] and previous[notice.id] ~= fingerprint) then incoming(notice, title, body) end
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
    incoming({ id = POWER_ID }, title, tostring(math.floor(charge * 100 + 0.5)) .. "% battery remaining")
    publish()
end
on("power_warning_clear", function(charge) power_warning(0, "", charge) end)
on("power_warning_low", function(charge) power_warning(1, "Battery low", charge) end)
on("power_warning_critical", function(charge) power_warning(2, "Battery critical", charge) end)
on("power_warning_empty", function(charge) power_warning(3, "Battery nearly empty", charge) end)
on("notices_seen", function() set_unread(false) end)
-- 0 off, 1 on, 2 toggle.
on("notices_dnd", function(mode)
    if mode ~= 0 and mode ~= 1 and mode ~= 2 then return end
    local dnd = mode == 1 or (mode == 2 and not fact.dnd)
    if dnd == fact.dnd then return end
    fact.dnd = dnd
    if dnd then
        for index = #shown, 1, -1 do if shown[index].id ~= POWER_ID then remove(shown[index].id) end end
        publish()
    end
    tell()
end)
on("notices_report", tell)
