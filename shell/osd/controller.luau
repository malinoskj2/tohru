-- One native audio observer is shared with sound; no polling or extra service.
return function()
    local timer
    local function stop()
        if timer then cancel(timer); timer = nil end
    end
    local function hide()
        stop()
        fact.osd_open = false
    end
    local function hold()
        stop()
        if fact.osd_open and not fact.osd_hover then
            timer = after(1600, function() timer = nil; fact.osd_open = false end)
        end
    end
    local function observe()
        local volume = fact["audio.volume"]
        local available = type(volume) == "number" and volume == volume
            and volume >= 0 and volume < math.huge and fact["sound_outputs.count"] > 0
        if not available then fact.osd_known = false; hide(); return end
        local muted = fact["audio.muted"] == true
        local changed = fact.osd_known and (volume ~= fact.osd_last_volume or muted ~= fact.osd_last_muted)
        fact.osd_last_volume, fact.osd_last_muted, fact.osd_known = volume, muted, true
        fact.osd_level = muted and 0 or math.clamp(volume, 0, 1)
        text.osd_label = muted and "Mute" or tostring(math.floor(volume * 100 + 0.5))
        if fact.sound_open then hide(); return end
        if changed then fact.osd_open = true; hold() end
    end
    on("osd_enter", function() fact.osd_hover = true; stop() end)
    on("osd_leave", function() fact.osd_hover = false; hold() end)
    on("fact:sound_open", function() if fact.sound_open then hide() end end)
    -- Old VM timers are cancelled natively. Never retain a stale pointer hold;
    -- a preserved visible vessel gets one new finite hold after replacement.
    fact.osd_hover = false
    hold()
    return {observe = observe, hide = hide}
end
