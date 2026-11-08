-- One native whole-report audio watch is reused across logic reloads.
-- Commands only publish acknowledgements/errors; reported levels remain authoritative.
return function(observed)
    local pending, timer = nil, nil
    local last_error = ""
    local function row(list, id, maximum)
        if type(id) ~= "number" or id ~= id or id < 0 or id > 4294967295 or id ~= math.floor(id) then return nil end
        for i=0, math.min(maximum, fact[list .. ".count"] or 0)-1 do
            local prefix=list .. "." .. i .. "."
            if fact[prefix .. "id"] == id then return prefix, i end
        end
    end
    local function active_list()
        if fact.sound_tab == 0 then return "sound_outputs",32 end
        if fact.sound_tab == 1 then return "sound_inputs",32 end
        return "sound_apps",64
    end
    local function reconcile()
        local list, maximum=active_list()
        local count=math.min(maximum,fact[list .. ".count"] or 0)
        if count == 0 then fact.sound_row_focus=-1;fact.sound_row_id=-1;fact.sound_list_page=0;return end
        local _, index=row(list,fact.sound_row_id,maximum)
        if not index then index=math.clamp(fact.sound_row_focus or 0,0,count-1) end
        fact.sound_row_focus=index
        fact.sound_row_id=fact[list .. "." .. index .. ".id"] or -1
        fact.sound_list_page=math.floor(index/4)
    end
    local function label(value)
        local pieces, bytes = {}, 0
        for _, cp in utf8.codes(value or "") do
            local character=utf8.char(cp)
            if bytes+#character > 256 then break end
            pieces[#pieces+1], bytes=character,bytes+#character
        end
        return table.concat(pieces)
    end
    local function reported(list, maximum, apps)
        local entries={}
        for i=0, math.min(maximum, fact[list .. ".count"] or 0)-1 do
            local prefix=list .. "." .. i .. "."
            local entry={id=fact[prefix .. "id"], name=label(text[prefix .. "name"])}
            if apps then
                entry.title=label(text[prefix .. "title"])
                entry.volume=fact[prefix .. "volume"]
                entry.muted=fact[prefix .. "muted"] == true
                entry.playing=fact[prefix .. "playing"] == true
            else entry.default=fact[prefix .. "default"] == true end
            entries[#entries+1]=entry
        end
        return entries
    end
    local function publish()
        local observed = fact["audio.volume"] >= 0
        local output_present = fact["sound_outputs.count"] > 0
        local muted = nil
        if observed then muted = fact["audio.muted"] end
        fact.sound_input_ready = false
        for i=0, math.min(32, fact["sound_inputs.count"] or 0)-1 do
            if fact["sound_inputs." .. i .. ".default"] then fact.sound_input_ready = true; break end
        end
        reconcile()
        fact.audio_failed = last_error ~= ""
        text.audio_error = last_error
        local input_volume=fact["audio.input"]
        local input_observed=fact.sound_input_ready and type(input_volume) == "number" and input_volume == input_volume and input_volume >= 0 and input_volume < math.huge
        local input_muted = nil
        if input_observed then input_muted = fact["audio.input_muted"] == true end
        text.audio_status = json.encode({
            schema = 2, observation = observed and "last_report" or "awaiting_report",
            volume = observed and fact["audio.volume"] or nil,
            muted = muted,
            output_present = output_present, panel_open = fact.sound_open,
            input_available = fact.sound_input_ready,
            input_volume = input_observed and input_volume or nil,
            input_muted = input_muted,
            app_count = fact["sound_apps.count"] or 0,
            outputs = reported("sound_outputs", 32, false),
            inputs = reported("sound_inputs", 32, false),
            apps = reported("sound_apps", 64, true),
            input_selection = "unavailable_native_id_collision",
            capacities = {outputs=32, inputs=32, apps=64},
            error = last_error,
        })
    end
    local function call(command, value, extra)
        local ok, error
        if extra ~= nil then ok, error = pcall(sys.call, command, value, extra)
        elseif value == nil then ok, error = pcall(sys.call, command)
        else ok, error = pcall(sys.call, command, value) end
        last_error = ok and "" or tostring(error)
        if not ok then fact.sound_dragging = false; fact.sound_input_dragging=false end
        publish()
    end
    local function flush()
        if timer then cancel(timer); timer = nil end
        if pending ~= nil then
            local request = pending
            pending = nil
            call(request.command, request.value)
        end
    end
    local function queue_volume(command, value)
        if type(value) ~= "number" or value ~= value or value < 0 or value > 1 then
            last_error = "Volume must be between 0 and 1"
            publish()
            return
        end
        if pending and pending.command ~= command then flush() end
        pending = {command=command,value=value}
        if not timer then timer = after(50, function() timer = nil; flush() end) end
    end
    on("audio_set", function(value) queue_volume("audio.volume", value) end)
    on("audio_input_set", function(value)
        if not fact.sound_input_ready then last_error="No native default microphone is reported";publish();return end
        queue_volume("audio.input",value)
    end)
    on("audio_flush", flush)
    on("audio_step", function(notches)
        if type(notches) ~= "number" or notches ~= notches then return end
        flush()
        call("audio.step", math.clamp(notches, -1, 1) * 0.05)
    end)
    on("audio_mute", function() flush(); call("audio.mute") end)
    local function reject(message) last_error=message; publish() end
    local function select_output(id)
        if not row("sound_outputs", id, 32) then reject("Selected output is no longer reported"); return end
        flush(); call("audio.default", id)
    end
    on("audio_output_select", select_output)
    local function input_step(direction)
        if not fact.sound_input_ready then reject("No native default microphone is reported"); return end
        local volume=fact["audio.input"]
        if type(volume) ~= "number" or volume ~= volume or volume < 0 or volume == math.huge then reject("Input volume is unavailable"); return end
        flush(); call("audio.input", math.clamp(volume + direction * 0.05, 0, 1))
    end
    on("audio_input_up", function() input_step(1) end)
    on("audio_input_down", function() input_step(-1) end)
    on("audio_input_mute", function()
        if not fact.sound_input_ready then reject("No native default microphone is reported"); return end
        flush(); call("audio.input_mute")
    end)
    local function app_step(id, direction)
        local prefix=row("sound_apps", id, 64)
        if not prefix then reject("Application stream is no longer reported"); return end
        local volume=fact[prefix .. "volume"]
        if type(volume) ~= "number" or volume ~= volume or volume < 0 or volume == math.huge then reject("Application volume is unavailable"); return end
        flush(); call("audio.app_volume", id, math.clamp(volume + direction * 0.05, 0, 1))
    end
    on("audio_app_up", function(id) app_step(id, 1) end)
    on("audio_app_down", function(id) app_step(id, -1) end)
    local function mute_app(id)
        if not row("sound_apps", id, 64) then reject("Application stream is no longer reported"); return end
        flush(); call("audio.app_mute", id)
    end
    on("audio_app_mute", mute_app)
    on("sound_details_toggle", function() fact.sound_details=not fact.sound_details end)
    local function focus_row(index)
        local list=fact.sound_tab == 0 and "sound_outputs" or fact.sound_tab == 1 and "sound_inputs" or "sound_apps"
        local count=fact[list .. ".count"] or 0
        if count == 0 then fact.sound_row_focus=-1; fact.sound_row_id=-1; return end
        fact.sound_row_focus=math.clamp(index,0,count-1)
        fact.sound_list_page=math.floor(fact.sound_row_focus/4)
        fact.sound_row_id=fact[list .. "." .. fact.sound_row_focus .. ".id"] or -1
    end
    local function tab(tab)
        if tab == 0 or tab == 1 or tab == 2 then fact.sound_tab=tab; fact.sound_list_page=0; focus_row(0) end
    end
    on("sound_tab", tab)
    on("sound_tab_next", function() tab(((fact.sound_tab or 0)+1)%3) end)
    on("sound_row_move", function(delta)
        if delta == -1 or delta == 1 then reconcile();focus_row((fact.sound_row_focus or -1)+delta) end
    end)
    on("sound_details_step", function(direction)
        reconcile()
        if direction ~= -1 and direction ~= 1 then return end
        if fact.sound_tab == 1 then input_step(direction)
        elseif fact.sound_tab == 2 then app_step(fact.sound_row_id or -1,direction)
        else flush(); call("audio.step",direction*0.05) end
    end)
    on("sound_details_activate", function()
        reconcile()
        if fact.sound_tab == 0 then select_output(fact.sound_row_id or -1)
        elseif fact.sound_tab == 2 then mute_app(fact.sound_row_id or -1)
        elseif fact.sound_input_ready then flush(); call("audio.input_mute") end
    end)
    on("sound_list_page", function(delta)
        if delta ~= -1 and delta ~= 1 then return end
        local list=fact.sound_tab == 0 and "sound_outputs" or fact.sound_tab == 1 and "sound_inputs" or "sound_apps"
        local count=fact[list .. ".count"] or 0
        fact.sound_list_page=math.clamp((fact.sound_list_page or 0)+delta,0,math.max(0,math.ceil(count/4)-1))
        focus_row(fact.sound_list_page*4)
    end)
    local function cleanup()
        flush()
        if fact.sound_dragging then fact.sound_dragging = false end
        if fact.sound_input_dragging then fact.sound_input_dragging=false end
    end
    local function close()
        cleanup()
        if fact.sound_open then fact.sound_open = false end
        publish()
    end
    on("sound_toggle", function()
        if fact.sound_open then close(); return end
        cleanup()
        fact.sound_open = true
        fact.sound_focus = 1
        publish()
    end)
    on("sound_open_request", function()
        cleanup()
        fact.sound_open = true
        fact.sound_focus = 1
        publish()
    end)
    on("panel_close", close)
    on("panel_close_sound", close)
    on("fact:sound_open", function()
        -- popup_done has already changed the fact: do not write it recursively.
        if not fact.sound_open then cleanup() end
        publish()
    end)
    -- Replacing the VM cancels its native timers and drops unissued drag work.
    -- Never let a preserved presentation fact override the native observation.
    if fact.sound_dragging then fact.sound_dragging = false end
    if fact.sound_input_dragging then fact.sound_input_dragging=false end
    on("audio_report", function()
        publish()
        if observed then observed() end
    end)
    publish()
    -- Native HUBS retains this one owner/name/watch subscription and replays its
    -- last report on VM replacement. Do not also declare service audio.
    sys.watch("audio", function(value)
        if type(value) ~= "table" then reject("Invalid native audio report"); return end
        model.sound_outputs=value.outputs or {}
        model.sound_inputs=value.inputs or {}
        model.sound_apps=value.apps or {}
        fact["audio.volume"]=type(value.volume) == "number" and value.volume or -1
        fact["audio.muted"]=value.muted == true
        fact["audio.input"]=type(value.input) == "number" and value.input or -1
        fact["audio.input_muted"]=value.input_muted == true
        publish()
        if observed then observed() end
    end)
    return {close = close}
end
