-- Dedicated native notification owner; no history subscription, DND or app policy.
local previous = {}
local shown = {}
local sequence = 0
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
            table.remove(shown, index)
        end
    end
end
local function incoming(notice, title, body)
    remove(notice.id)
    sequence = sequence + 1
    local revision = sequence
    local entry = { id = notice.id, revision = revision, title = title, body = body }
    entry.timer = after(6000, function() remove(notice.id, revision); publish() end)
    table.insert(shown, 1, entry)
    if #shown > 4 then cancel(shown[#shown].timer); table.remove(shown) end
    -- The same native placeholder tone regardless of app/content/urgency.
    sound.tone(tone)
end
model.banners = {}
sys.watch("notifications", function(notices)
    local current = {}
    local watermark = fact.last_id
    -- Bound UI work/state; native protocol ownership/history stays in Pleamar.
    for index = math.min(#notices, 64), 1, -1 do
        local notice = notices[index]
        local title = bounded(notice.title, 256)
        local body = bounded(notice.body, 768)
        local fingerprint = title .. "\0" .. body .. "\0" .. tostring(notice.time)
        current[notice.id] = fingerprint
        if notice.id > watermark or (previous[notice.id] and previous[notice.id] ~= fingerprint) then incoming(notice, title, body) end
        fact.last_id = math.max(fact.last_id, notice.id)
    end
    previous = current
    for index = #shown, 1, -1 do if current[shown[index].id] == nil then remove(shown[index].id) end end
    publish()
end)
