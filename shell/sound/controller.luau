-- One declarative native audio service owns observation across logic reloads.
-- Commands only publish acknowledgements/errors; reported levels remain authoritative.
return function()
    local pending, timer = nil, nil
    local last_error = ""
    local function publish()
        local observed = fact["audio.volume"] >= 0
        local output_present = fact["sound_outputs.count"] > 0
        local muted = nil
        if observed then muted = fact["audio.muted"] end
        text.audio_error = last_error
        text.audio_status = json.encode({
            schema = 1, observation = observed and "last_report" or "awaiting_report",
            volume = observed and fact["audio.volume"] or nil,
            muted = muted,
            output_present = output_present, panel_open = fact.sound_open,
            error = last_error,
        })
    end
    local function call(command, value)
        local ok, error
        if value == nil then ok, error = pcall(sys.call, command)
        else ok, error = pcall(sys.call, command, value) end
        last_error = ok and "" or tostring(error)
        if not ok then fact.sound_dragging = false end
        publish()
    end
    local function flush()
        if timer then cancel(timer); timer = nil end
        if pending ~= nil then
            local value = pending
            pending = nil
            call("audio.volume", value)
        end
    end
    on("audio_set", function(value)
        if type(value) ~= "number" or value ~= value or value < 0 or value > 1 then
            last_error = "Volume must be between 0 and 1"
            publish()
            return
        end
        pending = value
        if not timer then timer = after(50, function() timer = nil; flush() end) end
    end)
    on("audio_flush", flush)
    on("audio_step", function(notches)
        if type(notches) ~= "number" or notches ~= notches then return end
        flush()
        call("audio.step", math.clamp(notches, -1, 1) * 0.05)
    end)
    on("audio_mute", function() flush(); call("audio.mute") end)
    on("sound_toggle", function()
        flush()
        fact.sound_open = not fact.sound_open
        fact.sound_focus = 1
        publish()
    end)
    on("sound_open_request", function() fact.sound_open = true; fact.sound_focus = 1; publish() end)
    local function close()
        flush()
        fact.sound_dragging = false
        fact.sound_open = false
        publish()
    end
    on("panel_close", close)
    on("panel_close_sound", close)
    on("fact:sound_open", publish)
    on("audio_report", publish)
    publish()
end
