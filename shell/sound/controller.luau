-- One native whole-report audio watch is reused across logic reloads.
-- Commands only publish acknowledgements/errors; reported levels remain authoritative.
return function(observed)
    local pending, timer = nil, nil
    local last_error = ""
    local rail = require("bar/rail")(2)
    local fold_serial, rail_live = 0, false
    local focus_id = -1
    local function rail_x(volume) return 8 + math.clamp(volume, 0, 1) * 178 end
    local function row(list, id, maximum)
        if type(id) ~= "number" or id ~= id or id < 0 or id > 4294967295 or id ~= math.floor(id) then return nil end
        for i=0, math.min(maximum, fact[list .. ".count"] or 0)-1 do
            local prefix=list .. "." .. i .. "."
            if fact[prefix .. "id"] == id then return prefix, i end
        end
    end
    local function outputs() return math.min(32, fact["sound_outputs.count"] or 0) end
    local function inputs() return math.min(32, fact["sound_inputs.count"] or 0) end
    local function apps() return math.min(64, fact["sound_apps.count"] or 0) end
    local function default_row()
        for i=0, outputs()-1 do
            if fact["sound_outputs." .. i .. ".default"] then return i end
        end
        return -1
    end
    local function default_in_row()
        for i=0, inputs()-1 do
            if fact["sound_inputs." .. i .. ".default"] then return i end
        end
        return -1
    end
    local function others() return outputs() - (default_row() >= 0 and 1 or 0) end
    local function in_others() return inputs() - (default_in_row() >= 0 and 1 or 0) end
    -- Rows put the default output first: slot 0 is the default, the rest keep their order.
    local function output_at(slot)
        local d = default_row()
        if d < 0 then return slot end
        if slot == 0 then return d end
        if slot <= d then return slot - 1 end
        return slot
    end
    local function slot_of(index)
        local d = default_row()
        if d < 0 then return index end
        if index == d then return 0 end
        if index < d then return index + 1 end
        return index
    end
    local function input_at(slot)
        local d = default_in_row()
        if d < 0 then return slot end
        if slot == 0 then return d end
        if slot <= d then return slot - 1 end
        return slot
    end
    local function in_slot_of(index)
        local d = default_in_row()
        if d < 0 then return index end
        if index == d then return 0 end
        if index < d then return index + 1 end
        return index
    end
    -- The keyboard walks the rows in order: master, outputs, microphone, input devices (when > 1), apps.
    local function rows()
        local list = {{kind=0, index=0}}
        for slot=0, outputs()-1 do list[#list+1] = {kind=1, index=slot} end
        if fact.sound_input_ready then list[#list+1] = {kind=2, index=0} end
        if inputs() > 1 then
            for slot=0, inputs()-1 do list[#list+1] = {kind=4, index=slot} end
        end
        for i=0, apps()-1 do list[#list+1] = {kind=3, index=i} end
        return list
    end
    local function focus_set(entry)
        fact.sound_focus_kind = entry.kind
        fact.sound_focus_index = entry.index
        focus_id = -1
        if entry.kind == 1 then
            focus_id = fact["sound_outputs." .. output_at(entry.index) .. ".id"] or -1
            local other = entry.index - (default_row() >= 0 and 1 or 0)
            if other >= 0 then fact.sound_out_page = math.floor(other / 3) end
        elseif entry.kind == 4 then
            focus_id = fact["sound_inputs." .. input_at(entry.index) .. ".id"] or -1
            local other = entry.index - (default_in_row() >= 0 and 1 or 0)
            if other >= 0 then fact.sound_in_page = math.floor(other / 3) end
        elseif entry.kind == 3 then
            focus_id = fact["sound_apps." .. entry.index .. ".id"] or -1
            fact.sound_app_page = math.floor(entry.index / 4)
        end
    end
    local function reconcile()
        fact.sound_default_row = default_row()
        fact.sound_default_in_row = default_in_row()
        fact.sound_out_page = math.clamp(fact.sound_out_page or 0, 0, math.max(0, math.ceil(others() / 3) - 1))
        fact.sound_in_page = math.clamp(fact.sound_in_page or 0, 0, math.max(0, math.ceil(in_others() / 3) - 1))
        fact.sound_app_page = math.clamp(fact.sound_app_page or 0, 0, math.max(0, math.ceil(apps() / 4) - 1))
        local kind = fact.sound_focus_kind or 0
        if kind == 1 then
            local _, index = row("sound_outputs", focus_id, 32)
            if index then focus_set({kind=1, index=slot_of(index)})
            elseif outputs() > 0 then focus_set({kind=1, index=math.clamp(fact.sound_focus_index or 0, 0, outputs()-1)})
            else focus_set({kind=0, index=0}) end
        elseif kind == 4 then
            local _, index = row("sound_inputs", focus_id, 32)
            if inputs() <= 1 then
                if fact.sound_input_ready then focus_set({kind=2, index=0}) else focus_set({kind=0, index=0}) end
            elseif index then focus_set({kind=4, index=in_slot_of(index)})
            else focus_set({kind=4, index=math.clamp(fact.sound_focus_index or 0, 0, inputs()-1)}) end
        elseif kind == 3 then
            local _, index = row("sound_apps", focus_id, 64)
            if index then focus_set({kind=3, index=index})
            elseif apps() > 0 then focus_set({kind=3, index=math.clamp(fact.sound_focus_index or 0, 0, apps()-1)})
            elseif fact.sound_input_ready then focus_set({kind=2, index=0})
            else focus_set({kind=0, index=0}) end
        elseif kind == 2 and not fact.sound_input_ready then
            focus_set({kind=0, index=0})
        end
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
    local function reported(list, maximum, with_levels)
        local entries={}
        for i=0, math.min(maximum, fact[list .. ".count"] or 0)-1 do
            local prefix=list .. "." .. i .. "."
            local entry={id=fact[prefix .. "id"], name=label(text[prefix .. "name"])}
            if with_levels then
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
        local seen = fact["audio.volume"] >= 0
        local output_present = fact["sound_outputs.count"] > 0
        local muted = nil
        if seen then muted = fact["audio.muted"] end
        if outputs() == 0 then text.sound_device = seen and "No output reported" or "Waiting for audio"
        else
            local d = default_row()
            text.sound_device = d >= 0 and label(text["sound_outputs." .. d .. ".name"]) or "No default output"
        end
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
            schema = 2, observation = seen and "last_report" or "awaiting_report",
            volume = seen and fact["audio.volume"] or nil,
            muted = muted,
            output_present = output_present, panel_open = fact.sound_open,
            panel_unfolded = fact.sound_unfolded == true,
            input_available = fact.sound_input_ready,
            input_volume = input_observed and input_volume or nil,
            input_muted = input_muted,
            app_count = fact["sound_apps.count"] or 0,
            outputs = reported("sound_outputs", 32, false),
            inputs = reported("sound_inputs", 32, false),
            apps = reported("sound_apps", 64, true),
            input_selection = "available",
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
        if not ok then fact.sound_dragging = false; fact.sound_input_dragging=false; fact.sound_app_drag=-1 end
        publish()
    end
    local function flush()
        if timer then cancel(timer); timer = nil end
        if pending ~= nil then
            local request = pending
            pending = nil
            if request.id ~= nil then call(request.command, request.id, request.value)
            else call(request.command, request.value) end
        end
    end
    local function queue_volume(command, value, id)
        if type(value) ~= "number" or value ~= value or value < 0 or value > 1 then
            last_error = "Volume must be between 0 and 1"
            publish()
            return
        end
        if pending and (pending.command ~= command or pending.id ~= id) then flush() end
        pending = {command=command,value=value,id=id}
        if not timer then timer = after(50, function() timer = nil; flush() end) end
    end
    local function reject(message) last_error=message; publish() end
    -- A released slider keeps showing the requested level until a report agrees with it (or 500 ms
    -- pass), so its label never steps back to the level from before the drag.
    local holds = {}
    local function agrees(kind)
        local h = holds[kind]
        local reported, requested
        if kind == "master" then reported, requested = fact["audio.volume"], fact.sound_requested
        elseif kind == "mic" then reported, requested = fact["audio.input"], fact.sound_input_requested
        else
            local index = fact.sound_app_drag
            if type(index) ~= "number" or index < 0 or fact["sound_apps." .. index .. ".id"] ~= h.id then return true end
            reported, requested = fact["sound_apps." .. index .. ".volume"], fact.sound_app_requested
        end
        return type(reported) ~= "number" or reported < 0 or math.abs(reported - requested) < 0.006
    end
    local rail_follow
    local function unhold(kind)
        local h = holds[kind]
        holds[kind] = nil
        if h and h.timer then cancel(h.timer) end
        if kind == "master" then fact.sound_dragging = false; rail_follow()
        elseif kind == "mic" then fact.sound_input_dragging = false
        else fact.sound_app_drag = -1 end
    end
    local function release(kind, id)
        flush()
        local h = {id = id}
        holds[kind] = h
        if agrees(kind) then unhold(kind); return end
        h.timer = after(500, function() if holds[kind] == h then h.timer = nil; unhold(kind) end end)
    end
    local function grab(kind)
        local h = holds[kind]
        holds[kind] = nil
        if h and h.timer then cancel(h.timer) end
    end
    local function settle_holds()
        for _, kind in ipairs({"master", "mic", "app"}) do
            if holds[kind] and agrees(kind) then unhold(kind) end
        end
    end
    on("audio_set", function(value) queue_volume("audio.volume", value) end)
    on("audio_input_set", function(value)
        if not fact.sound_input_ready then reject("No native default microphone is reported"); return end
        grab("mic")
        fact.sound_input_dragging = true
        fact.sound_input_requested = value
        queue_volume("audio.input",value)
    end)
    on("sound_input_drag", function(value)
        if not fact.sound_input_ready or type(value) ~= "number" or value ~= value then return end
        value = math.clamp(value, 0, 1)
        fact.sound_input_requested = value
        queue_volume("audio.input", value)
    end)
    -- The scene sends the app's row and level together: row * 2 + level.
    local function app_set(payload)
        if type(payload) ~= "number" or payload ~= payload or payload < 0 then return end
        local index = math.floor(payload / 2)
        local id = fact["sound_apps." .. index .. ".id"]
        if index >= apps() or type(id) ~= "number" then reject("Application stream is no longer reported"); return end
        fact.sound_app_requested = math.clamp(payload - index * 2, 0, 1)
        queue_volume("audio.app_volume", fact.sound_app_requested, id)
    end
    on("audio_app_set", function(payload)
        grab("app")
        if type(payload) == "number" and payload >= 0 and payload == payload then fact.sound_app_drag = math.floor(payload / 2) end
        app_set(payload)
    end)
    on("sound_app_drag_to", app_set)
    on("sound_rail_release", function() release("master") end)
    on("sound_input_release", function() release("mic") end)
    on("sound_app_release", function()
        local index = fact.sound_app_drag
        if type(index) ~= "number" or index < 0 then return end
        release("app", fact["sound_apps." .. index .. ".id"])
    end)
    on("audio_flush", flush)
    on("audio_step", function(notches)
        if type(notches) ~= "number" or notches ~= notches then return end
        flush()
        call("audio.step", math.clamp(notches, -1, 1) * 0.05)
    end)
    on("audio_mute", function() flush(); call("audio.mute") end)
    local function select_output(id)
        if not row("sound_outputs", id, 32) then reject("Selected output is no longer reported"); return end
        flush(); call("audio.default", id)
    end
    on("audio_output_select", select_output)
    local function select_input(id)
        if not row("sound_inputs", id, 32) then reject("Selected input is no longer reported"); return end
        flush(); call("audio.default_input", id)
    end
    on("audio_input_select", select_input)
    local function input_step(direction)
        if not fact.sound_input_ready then reject("No native default microphone is reported"); return end
        local volume=fact["audio.input"]
        if type(volume) ~= "number" or volume ~= volume or volume < 0 or volume == math.huge then reject("Input volume is unavailable"); return end
        flush(); call("audio.input", math.clamp(volume + direction * 0.05, 0, 1))
    end
    on("audio_input_up", function() input_step(1) end)
    on("audio_input_down", function() input_step(-1) end)
    local function mute_input()
        if not fact.sound_input_ready then reject("No native default microphone is reported"); return end
        flush(); call("audio.input_mute")
    end
    on("audio_input_mute", mute_input)
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

    -- The master rail: parked at its left end while folded, then switched to the volume on unfold.
    function rail_follow()
        if not rail_live or fact.sound_dragging or not fact.sound_unfolded then return end
        local volume = fact["audio.volume"]
        if type(volume) ~= "number" or volume < 0 then return end
        local goal = rail_x(volume)
        if math.abs(goal - rail.target()) > 0.5 then rail.glide(goal) end
    end
    local function fold(unfolded)
        if fact.sound_unfolded == unfolded then return end
        fold_serial = fold_serial + 1
        local serial = fold_serial
        fact.sound_unfolded = unfolded
        fact.sound_out_page = 0
        fact.sound_in_page = 0
        fact.sound_app_page = 0
        rail_live = false
        if unfolded then
            -- The rail is still invisible here; the switch from its left end starts at 180 ms, as in the mock.
            after(120, function()
                if serial == fold_serial then rail.place(8) end
            end)
            after(180, function()
                if serial ~= fold_serial then return end
                rail_live = true
                local volume = fact["audio.volume"]
                rail.glide(rail_x(type(volume) == "number" and volume >= 0 and volume or 0))
            end)
        else
            focus_set({kind=0, index=0})
        end
        publish()
    end
    on("sound_fold_toggle", function() fold(not fact.sound_unfolded) end)
    on("sound_rail_press", function(value)
        if type(value) ~= "number" or value ~= value or outputs() == 0 then return end
        grab("master")
        value = math.clamp(value, 0, 1)
        fact.sound_dragging = true
        fact.sound_big_held = true
        fact.sound_requested = value
        rail_live = true
        rail.glide(rail_x(value))
        queue_volume("audio.volume", value)
    end)
    on("sound_rail_drag", function(value)
        if type(value) ~= "number" or value ~= value or outputs() == 0 then return end
        value = math.clamp(value, 0, 1)
        fact.sound_requested = value
        rail.chase(rail_x(value))
        queue_volume("audio.volume", value)
    end)

    -- Dragging in the water sets the level to the pointer's height once it has moved 3 px;
    -- a press on the number that never moved toggles mute. The scene adds 2 for the number and
    -- sends drags as drag.dy.
    local water = nil
    on("sound_water_press", function(value)
        if type(value) ~= "number" or value ~= value then return end
        grab("master")
        water = {number = value >= 2, from = value >= 2 and value - 2 or value, moved = false}
    end)
    on("sound_water_drag", function(dy)
        if not water or type(dy) ~= "number" or dy ~= dy or outputs() == 0 then return end
        if not water.moved and math.abs(dy) < 3 then return end
        water.moved = true
        local value = math.clamp(water.from - dy / 190, 0, 1)
        fact.sound_dragging = true
        fact.sound_big_held = true
        fact.sound_requested = value
        queue_volume("audio.volume", value)
    end)
    on("sound_water_release", function()
        local pressed = water
        water = nil
        if not pressed then return end
        if pressed.moved then
            release("master")
        elseif pressed.number then
            flush(); call("audio.mute")
        end
    end)
    local function focused()
        reconcile()
        local kind, index = fact.sound_focus_kind or 0, fact.sound_focus_index or 0
        if kind == 1 then return kind, fact["sound_outputs." .. output_at(index) .. ".id"] or -1 end
        if kind == 4 then return kind, fact["sound_inputs." .. input_at(index) .. ".id"] or -1 end
        if kind == 3 then return kind, fact["sound_apps." .. index .. ".id"] or -1 end
        return kind, -1
    end
    on("sound_focus_move", function(delta)
        if delta ~= -1 and delta ~= 1 then return end
        reconcile()
        local list = rows()
        local at = 1
        for i, entry in ipairs(list) do
            if entry.kind == fact.sound_focus_kind and (entry.kind == 0 or entry.kind == 2 or entry.index == fact.sound_focus_index) then at = i end
        end
        fact.sound_keyed = true
        focus_set(list[math.clamp(at + delta, 1, #list)])
    end)
    on("sound_focus_step", function(direction)
        if direction ~= -1 and direction ~= 1 then return end
        local kind, id = focused()
        fact.sound_keyed = true
        if kind == 2 or kind == 4 then input_step(direction)
        elseif kind == 3 then app_step(id, direction)
        else flush(); call("audio.step", direction * 0.05) end
    end)
    on("sound_focus_activate", function()
        local kind, id = focused()
        fact.sound_keyed = true
        if kind == 1 then select_output(id)
        elseif kind == 4 then select_input(id)
        elseif kind == 2 then mute_input()
        elseif kind == 3 then mute_app(id)
        else flush(); call("audio.mute") end
    end)
    on("sound_page", function(delta)
        if delta ~= -1 and delta ~= 1 then return end
        reconcile()
        local kind = fact.sound_focus_kind or 0
        local section = kind == 3 and 3 or kind == 4 and 4 or kind == 1 and 1 or (others() > 3 and 1 or (in_others() > 3 and 4 or 3))
        if section == 1 and others() > 0 then
            local page = math.clamp((fact.sound_out_page or 0) + delta, 0, math.ceil(others() / 3) - 1)
            focus_set({kind=1, index=page * 3 + (default_row() >= 0 and 1 or 0)})
        elseif section == 4 and in_others() > 0 then
            local page = math.clamp((fact.sound_in_page or 0) + delta, 0, math.ceil(in_others() / 3) - 1)
            focus_set({kind=4, index=page * 3 + (default_in_row() >= 0 and 1 or 0)})
        elseif section == 3 and apps() > 0 then
            local page = math.clamp((fact.sound_app_page or 0) + delta, 0, math.ceil(apps() / 4) - 1)
            focus_set({kind=3, index=page * 4})
        end
        fact.sound_keyed = true
    end)

    local function cleanup()
        flush()
        for kind in pairs(holds) do grab(kind) end
        if fact.sound_big_held then fact.sound_big_held = false end
        if fact.sound_dragging then fact.sound_dragging = false end
        if fact.sound_input_dragging then fact.sound_input_dragging=false end
        if fact.sound_app_drag ~= -1 then fact.sound_app_drag=-1 end
    end
    local function reset()
        fold_serial = fold_serial + 1
        rail_live = false
        if fact.sound_unfolded then fact.sound_unfolded = false end
        fact.sound_out_page = 0
        fact.sound_in_page = 0
        fact.sound_app_page = 0
        fact.sound_keyed = false
        focus_set({kind=0, index=0})
        rail.place(8)
    end
    local function close()
        cleanup()
        if fact.sound_open then fact.sound_open = false end
        reset()
        publish()
    end
    local function open()
        cleanup()
        reset()
        fact.sound_open = true
        publish()
    end
    on("sound_toggle", function()
        if fact.sound_open then close(); return end
        open()
    end)
    on("sound_open_request", open)
    on("panel_close", close)
    on("panel_close_sound", close)
    on("fact:sound_open", function()
        -- popup_done has already changed the fact: do not write it recursively.
        if not fact.sound_open then cleanup(); reset() end
        publish()
    end)
    -- Replacing the VM cancels its native timers and drops unissued drag work.
    -- Never let a preserved presentation fact override the native observation.
    if fact.sound_dragging then fact.sound_dragging = false end
    if fact.sound_input_dragging then fact.sound_input_dragging=false end
    if fact.sound_app_drag ~= -1 then fact.sound_app_drag=-1 end
    rail_live = fact.sound_unfolded == true
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
        settle_holds()
        rail_follow()
        if observed then observed() end
    end)
    return {close = close}
end
