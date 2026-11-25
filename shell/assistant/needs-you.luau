return function(value, busy)
    local helper = "tohru-assistant"
    local FIRST_POLL = 20000
    local MAX_BACKOFF = 3600000
    local clock = 0
    local due_at = FIRST_POLL
    local failures = 0
    local polling, waking = false, false
    local queue = {}
    local signature = ""

    local function setting(key, default)
        local v = value(key)
        if v == nil then return default end
        return v
    end
    local function interval()
        local minutes = tonumber(setting("assistant.orca_needs_you_minutes", 5)) or 5
        return math.floor(minutes) * 60000
    end
    local function enabled()
        return setting("assistant.backend", "local") == "orca" and interval() > 0
    end
    local function backend_args(args)
        args[#args + 1] = "--backend"
        args[#args + 1] = "orca"
        args[#args + 1] = "--orca-bot"
        args[#args + 1] = tostring(setting("assistant.orca_bot", "Tohru"))
        return args
    end
    local function decode(out)
        local ok, data = pcall(json.decode, out or "")
        return ok and type(data) == "table" and data or nil
    end
    local function backoff()
        failures = failures + 1
        local wait = interval() * 2 ^ math.min(failures, 6)
        due_at = clock + math.min(wait, math.max(MAX_BACKOFF, interval()))
    end
    local function idle()
        return not (fact.notices_dnd or fact.ai_voice_paused or fact.ai_voice_conversation or fact.ai_speaking
            or (busy and busy()))
    end
    local function clear()
        queue = {}
    end
    local function drop(sent)
        local keep = {}
        for _, item in ipairs(queue) do
            if not sent[item.uid .. ":" .. item.event] then keep[#keep + 1] = item end
        end
        queue = keep
    end

    local function wake()
        if waking or #queue == 0 or not enabled() or not idle() then return end
        waking = true
        local args, sent = {"needs-you", "wake"}, {}
        for _, item in ipairs(queue) do
            local key = item.uid .. ":" .. item.event
            args[#args + 1] = key
            sent[key] = true
        end
        backend_args(args)
        local mine = signature
        run(helper, args, function(out, code)
            waking = false
            if mine ~= signature then return end
            local data = code == 0 and decode(out) or nil
            drop(sent)
            if not (data and data.ok) then backoff() end
        end)
    end

    local function poll()
        if polling or not enabled() then return end
        polling = true
        due_at = clock + interval()
        local mine = signature
        run(helper, backend_args({"needs-you", "poll"}), function(out, code)
            polling = false
            if mine ~= signature then return end
            local data = code == 0 and decode(out) or nil
            if not data or not data.ok then backoff(); return end
            failures = 0
            local fresh, seen = {}, {}
            for _, item in ipairs(data.pending or {}) do
                if type(item) == "table" and type(item.uid) == "string" and type(item.event) == "string"
                    and item.uid:match("^[%w%-]+$") and item.event:match("^[%w%-]+$") then
                    local key = item.uid .. ":" .. item.event
                    if not seen[key] then
                        seen[key] = true
                        fresh[#fresh + 1] = {uid = item.uid, event = item.event}
                    end
                end
            end
            queue = fresh
            wake()
        end)
    end

    every(1000, function()
        clock = clock + 1000
        if not enabled() then return end
        if clock >= due_at then poll() end
        wake()
    end)

    local function configure()
        local next_signature = table.concat({tostring(enabled()), tostring(interval()),
            tostring(setting("assistant.orca_bot", "Tohru"))}, "|")
        if next_signature == signature then return end
        local first = signature == ""
        signature = next_signature
        clear()
        failures = 0
        polling, waking = false, false
        due_at = clock + (first and FIRST_POLL or 1000)
    end
    configure()

    return {
        configure = configure,
        poll_now = function() due_at = clock end,
        status = function()
            return {enabled = enabled(), interval = interval(), due_in = math.max(0, due_at - clock),
                failures = failures, queued = #queue}
        end,
    }
end
