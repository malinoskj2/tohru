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
    local function finite(volume)
        return type(volume) == "number" and volume == volume and volume >= 0 and volume < math.huge
    end
    local function device(list, fallback)
        local first_id, first_name = -1, fallback
        for i=0, math.min(32, fact[list .. ".count"] or 0)-1 do
            local prefix=list .. "." .. i .. "."
            local id, name=fact[prefix .. "id"] or -1, text[prefix .. "name"] or fallback
            if i == 0 then first_id,first_name=id,name end
            if fact[prefix .. "default"] then return id,name end
        end
        return first_id,first_name
    end
    local function observe()
        local volume, input = fact["audio.volume"], fact["audio.input"]
        local available = finite(volume) and fact["sound_outputs.count"] > 0
        local input_available = finite(input) and fact.sound_input_ready == true
        local output_id, output_name=device("sound_outputs", "Output")
        local input_id, input_name=device("sound_inputs", "Microphone")
        local muted, input_muted=fact["audio.muted"] == true, fact["audio.input_muted"] == true
        local changed=available and fact.osd_known and (volume ~= fact.osd_last_volume or muted ~= fact.osd_last_muted or output_id ~= fact.osd_last_output_id)
        local input_changed=input_available and fact.osd_input_known and (input ~= fact.osd_last_input_volume or input_muted ~= fact.osd_last_input_muted or input_id ~= fact.osd_last_input_id)
        fact.osd_known, fact.osd_input_known=available,input_available
        if available then fact.osd_last_volume,fact.osd_last_muted,fact.osd_last_output_id=volume,muted,output_id end
        if input_available then fact.osd_last_input_volume,fact.osd_last_input_muted,fact.osd_last_input_id=input,input_muted,input_id end
        if fact.osd_open and (fact.osd_input and not input_available or not fact.osd_input and not available) then hide() end
        if not available and not input_available or fact.sound_open then hide(); return end
        if not changed and not input_changed then return end
        -- A combined native report can change both; output takes deterministic priority.
        fact.osd_input=input_changed and not changed
        local shown=fact.osd_input and input or volume
        local shown_muted=muted
        if fact.osd_input then shown_muted=input_muted end
        fact.osd_muted=shown_muted
        fact.osd_level=shown_muted and 0 or math.clamp(shown,0,1)
        text.osd_label=shown_muted and "Mute" or tostring(math.floor(shown*100+0.5))
        text.osd_device=fact.osd_input and input_name or output_name
        fact.osd_open=true
        hold()
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
