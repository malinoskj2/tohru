-- Dedicated native notification owner; no history subscription, DND or app policy.
local previous = {}
local shown = {}
local sequence = 0
local POWER_ID = -1
local CAPTURE_ID = -2
local capture_titles = {
    saved = "Screenshot saved",
    copied = "Screenshot copied",
    saved_copied = "Screenshot saved and copied",
    partial = "Screenshot partly completed",
    error = "Screenshot failed",
}
local tone = { wave = "sine", freq = 880, ms = 80, volume = 0.07, release = 60 }
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
            if id == CAPTURE_ID then text.capture_notice = "" end
            table.remove(shown, index)
        end
    end
end
local function incoming(notice, title, body, milliseconds, quiet)
    remove(notice.id)
    sequence = sequence + 1
    local revision = sequence
    local entry = { id = notice.id, revision = revision, title = title, body = body }
    entry.timer = after(milliseconds or 6000, function() remove(notice.id, revision); publish() end)
    table.insert(shown, 1, entry)
    if #shown > 4 then remove(shown[#shown].id, shown[#shown].revision) end
    -- The same native placeholder tone regardless of app/content/urgency.
    if not quiet then sound.tone(tone) end
end
model.banners = {}
-- Native emit carries only a number. One submit carries its string directly,
-- avoiding a separate text-write/event race between capture result producers.
local function capture_value(payload, retained)
    if type(payload) ~= "string" or #payload == 0 or #payload > 3072 then return nil end
    local ok, value = pcall(function() return json.decode(payload) end)
    if not ok or type(value) ~= "table" then return nil end
    if type(value.status) ~= "string" or capture_titles[value.status] == nil then return nil end
    if type(value.message) ~= "string" or #value.message > 2048 then return nil end
    if string.find(value.message, "[%z\1-\8\11\12\14-\31\127]") then return nil end
    local valid, length = pcall(function() return utf8.len(value.message) end)
    if not valid or length == nil or length > 512 then return nil end
    for key in pairs(value) do
        if key ~= "status" and key ~= "message" and not (retained and key == "expires") then return nil end
    end
    return value
end
local function restore_capture()
    local value = capture_value(text.capture_notice, true)
    local now = os.clock()
    if not value or type(value.expires) ~= "number" or value.expires ~= value.expires
        or value.expires <= now or value.expires > now + 6 then
        text.capture_notice = ""
        return
    end
    local retained = text.capture_notice
    incoming({ id = CAPTURE_ID }, capture_titles[value.status], value.message, (value.expires - now) * 1000, true)
    text.capture_notice = retained
    publish()
end
restore_capture()
on("submit:capture_result", function(payload)
    text.capture_result = ""
    local value = capture_value(payload, false)
    if not value then return end
    incoming({ id = CAPTURE_ID }, capture_titles[value.status], value.message)
    text.capture_notice = json.encode({ status = value.status, message = value.message, expires = os.clock() + 6 })
    publish()
end)
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
        if notice.id > watermark or moved_replacement or (previous[notice.id] and previous[notice.id] ~= fingerprint) then incoming(notice, title, body) end
        if previous[notice.id] ~= nil then known_after = true end
        fact.last_id = math.max(fact.last_id, notice.id)
    end
    previous = current
    for index = #shown, 1, -1 do if shown[index].id ~= POWER_ID and shown[index].id ~= CAPTURE_ID and current[shown[index].id] == nil then remove(shown[index].id) end end
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
