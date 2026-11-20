-- The declarative native Bluetooth service explicitly offers the pairing agent.
-- rail is the popup's Off / On / Scan rail (bar/rail.luau), or nil where the scene has none.
return function(rail)
    local shown_id, shown_kind = 0, "none"
    -- The device being connected, until it reports connected, the attempt is over or it goes.
    local connecting, connect_timer = "", nil
    local scan_pending = false
    local devices, adapter = {}, nil
    -- A stop the user chose is held until the adapter reports it, or for a few seconds if it never does.
    local rail_serial, rail_dragging, held, held_timer = 0, false, nil, nil
    local pressed_at, slid = 8, false
    local CONNECT_PATIENCE = 30000

    local function request(command, ...)
        local ok, err = pcall(sys.call, "bluetooth." .. command, ...)
        text.bt_request = ok and "Request sent — check observed status" or tostring(err)
        fact.bt_failed = not ok
        return ok
    end
    local function question() return fact["bt.pairing_id"] or 0, fact["bt.pairing"] or "none" end
    -- The controller's own watch can run before the scene's facts change, so its report wins once there is one.
    local function powered() if adapter then return adapter.powered == true end return fact["bt.powered"] == true end
    local function discovering() if adapter then return adapter.discovering == true end return fact["bt.discovering"] == true end

    -- Rail stops: 0 Off, 1 On, 2 Scan. Unfolded they sit 78 px apart; the folded Off panel's rail has two.
    local function observed_stop()
        if not powered() then return 0 end
        return discovering() and 2 or 1
    end
    local function rail_goal(stop)
        if fact.bt_unfolded then return 8 + 78 * stop end
        return stop > 0 and 54 or 8
    end
    local function rail_follow()
        local observed = observed_stop()
        if held == observed then held = nil end
        fact.bt_stop = held or observed
        if not rail or rail_dragging then return end
        local goal = rail_goal(fact.bt_stop)
        if math.abs(goal - rail.target()) > 0.5 then rail.switch(goal) end
    end
    local function rail_reset()
        rail_serial = rail_serial + 1
        if not rail then return end
        rail_dragging = false
        local serial = rail_serial
        rail.place(8)
        -- Unfolding, the bead leaves its left end once the rail has faded in, as the sound popup's does.
        if fact.bt_unfolded and after then
            after(180, function() if serial == rail_serial then rail_follow() end end)
        else
            rail_follow()
        end
    end

    local function kind_of(icon)
        if icon == "audio-headset" or icon == "audio-headphones" then return 1 end
        if icon == "input-keyboard" then return 2 end
        if icon == "input-mouse" or icon == "input-tablet" then return 3 end
        if icon == "audio-card" or icon == "audio-speakers" then return 4 end
        if icon == "input-gaming" then return 5 end
        if icon == "phone" then return 6 end
        return 0
    end
    local function stop_connecting()
        connecting = ""
        fact.bt_connecting = -1
        if connect_timer and cancel then cancel(connect_timer) end
        connect_timer = nil
    end
    local function wait_for_connection()
        if connect_timer and cancel then cancel(connect_timer) end
        connect_timer = nil
        if not after then return end
        connect_timer = after(CONNECT_PATIENCE, function()
            connect_timer = nil
            -- A pairing question keeps the attempt alive until it is answered or called off.
            local _, kind = question()
            if kind ~= "none" then wait_for_connection() else stop_connecting() end
        end)
    end

    -- Everything the folded tank and the rows need that the scene cannot work out from the model alone.
    local function reconcile(report)
        if report then
            devices = report.devices or {}
            adapter = report
        end
        local mine, connected, active, connecting_row, selected = 0, 0, -1, -1, -1
        for index, device in ipairs(devices) do
            local row = index - 1
            if device.paired or device.connected then mine = mine + 1 end
            if device.connected and powered() then
                connected = connected + 1
                local battery = type(device.battery) == "number" and device.battery or -1
                if active < 0 or (battery >= 0 and (devices[active + 1].battery or -1) < 0) then active = row end
            end
            if connecting ~= "" and device.address == connecting then
                if device.connected then stop_connecting() else connecting_row = row end
            end
            if text.bt_target ~= "" and device.address == text.bt_target then selected = row end
            fact["bt_kind." .. row] = kind_of(device.icon)
        end
        -- The selection follows its device as the list reorders, and goes with it.
        if report and text.bt_target ~= "" then
            fact.bt_selected = selected
            if selected < 0 then text.bt_target = ""; fact.bt_has_target = false end
        end
        if connecting ~= "" and (connecting_row < 0 or not powered()) then stop_connecting() end
        fact.bt_connecting = connecting ~= "" and connecting_row or -1
        fact.bt_paired = mine
        fact.bt_connected = connected
        fact.bt_active = active
        local first = active >= 0 and devices[active + 1] or nil
        text.bt_active_name = first and first.name or "Bluetooth"
        fact.bt_active_battery = first and type(first.battery) == "number" and first.battery or -1
        -- The minis: the active device on the left, then up to three more of the user's own on the right.
        local minis = {active}
        for index = 1, mine do
            if #minis >= 4 then break end
            if index - 1 ~= active then minis[#minis + 1] = index - 1 end
        end
        for slot = 0, 3 do
            local row = minis[slot + 1] or -1
            local device = row >= 0 and devices[row + 1] or nil
            fact["bt_mini." .. slot] = row
            fact["bt_mini_kind." .. slot] = device and kind_of(device.icon) or 0
            fact["bt_mini_level." .. slot] = device and type(device.battery) == "number" and device.battery or -1
            fact["bt_mini_on." .. slot] = device ~= nil and device.connected == true
        end
        fact.bt_minis_right = math.max(0, #minis - 1)
        if report and not powered() then fact.bt_scan_owned = false end
        if scan_pending and powered() then
            scan_pending = false
            if not discovering() and request("scan", true) then fact.bt_scan_owned = true end
        end
        rail_follow()
    end

    local function cancel_question()
        local id, kind = question()
        if id > 0 and kind ~= "none" then request("cancel", id) end
        text.bt_answer = ""
    end
    local function close()
        fact.bt_open = false
        fact.bt_unfolded = false
        cancel_question()
        scan_pending = false
        if fact.bt_scan_owned and request("scan", false) then fact.bt_scan_owned = false end
    end
    local function open()
        fact.bt_open = true; fact.bt_unfolded = false; fact.bt_page = 0; fact.bt_selected = -1; fact.bt_has_target = false; text.bt_target = ""
        text.bt_request = ""; fact.bt_failed = false
        rail_reset()
    end
    local function toggle() if fact.bt_open then close() else open() end end
    local function set_unfolded(value)
        if fact.bt_unfolded == value then return end
        fact.bt_unfolded = value
        if not value then fact.bt_page = 0 end
        rail_reset()
    end
    local function row(index)
        if type(index) ~= "number" or index ~= math.floor(index) or index < 0 or index >= (fact["bt_devices.count"] or 0) then return end
        return "bt_devices." .. index .. "."
    end
    local function select(index)
        local prefix = row(index)
        if not prefix then return end
        fact.bt_selected = index
        text.bt_target = text[prefix .. "address"] or ""
        fact.bt_has_target = text.bt_target ~= ""
        text.bt_target_name = text[prefix .. "name"] or text.bt_target
    end
    local function connect()
        local prefix = row(fact.bt_selected)
        if not prefix or text[prefix .. "address"] ~= text.bt_target or text.bt_target == "" then return end
        if fact[prefix .. "connected"] then
            if connecting == text.bt_target then stop_connecting() end
            request("disconnect", text.bt_target)
        elseif connecting == text.bt_target then
            text.bt_request = "Already connecting — check observed status"
        elseif request("connect", text.bt_target) then
            connecting = text.bt_target
            fact.bt_connecting = fact.bt_selected
            wait_for_connection()
        end
    end
    local function forget(address)
        if address == "" then return end
        if connecting == address then stop_connecting() end
        request("forget", address)
    end
    local function answer()
        local id, kind = question()
        if id <= 0 or id ~= shown_id or kind ~= shown_kind then text.bt_request = "Pairing question changed — review the current prompt"; return end
        if kind == "pin" or kind == "passkey" then
            local code = text.bt_answer
            text.bt_answer = ""
            request("answer", id, code)
        elseif kind == "confirm" or kind == "authorize" then request("confirm", id) end
    end
    local function power(on)
        if on == powered() then return true end
        return request("power", on)
    end
    local function scan(on)
        if on then
            if fact.bt_scan_owned then return end
            if discovering() then text.bt_request = "Discovery is already active"; return end
            if request("scan", true) then fact.bt_scan_owned = true end
        elseif fact.bt_scan_owned then
            if request("scan", false) then fact.bt_scan_owned = false end
        elseif discovering() then text.bt_request = "Discovery was started elsewhere and keeps running"
        end
    end
    -- Moving the rail to a stop asks the adapter for that state; the bead then follows what it reports.
    local function choose(stop)
        local ok = true
        if stop == 0 then
            scan_pending = false
            ok = power(false)
        elseif stop == 1 then
            scan_pending = false
            if powered() then scan(false) else ok = power(true) end
        elseif powered() then
            scan(true)
        elseif power(true) then
            scan_pending = true
        else
            ok = false
        end
        if held_timer and cancel then cancel(held_timer) end
        held, held_timer = nil, nil
        if ok and stop ~= observed_stop() then
            held = stop
            if after then held_timer = after(4000, function() held, held_timer = nil, nil; rail_follow() end) end
        end
        rail_follow()
    end
    local function rail_stop(x)
        if fact.bt_unfolded then return math.max(0, math.min(2, math.floor((x - 8) / 78 + 0.5))) end
        return x > 31 and 1 or 0
    end
    local function rail_clamp(x)
        return math.max(8, math.min(fact.bt_unfolded and 164 or 54, x))
    end

    on("bluetooth_open", open); on("bluetooth_close", close)
    on("bluetooth_toggle", toggle); on("bluetooth_select", select)
    on("bluetooth_fold_toggle", function() set_unfolded(not fact.bt_unfolded) end)
    on("bluetooth_connect", connect)
    on("bluetooth_activate", connect)
    on("bluetooth_row", function(index) select(index); connect() end)
    on("bluetooth_forget", function() forget(text.bt_target) end)
    on("bluetooth_forget_row", function(index)
        local prefix = row(index)
        if prefix and (fact[prefix .. "paired"] or fact[prefix .. "connected"]) then forget(text[prefix .. "address"] or "") end
    end)
    on("bluetooth_power", function() request("power", not powered()) end)
    on("bluetooth_scan", function()
        if fact.bt_scan_owned then scan(false)
        else scan(true) end
    end)
    on("bluetooth_rail", function(stop) if type(stop) == "number" then choose(math.max(0, math.min(2, math.floor(stop + 0.5)))) end end)
    -- A click picks the stop under it (anywhere on the folded Off panel's rail, On); a slide picks where it ends.
    on("bluetooth_rail_press", function(x)
        if type(x) ~= "number" then return end
        rail_dragging, slid, pressed_at = true, false, x
        if rail then rail.glide(rail_clamp(x)) end
    end)
    on("bluetooth_rail_drag", function(x)
        if type(x) ~= "number" or not rail_dragging then return end
        slid = true
        if rail then rail.chase(rail_clamp(x)) end
    end)
    on("bluetooth_rail_release", function()
        if not rail_dragging then return end
        rail_dragging = false
        if slid and rail then choose(rail_stop(rail.target()))
        elseif fact.bt_unfolded then choose(rail_stop(rail_clamp(pressed_at)))
        else choose(1) end
    end)
    on("bluetooth_answer", answer); on("submit:bt_answer", answer)
    on("bluetooth_cancel", cancel_question)
    local function reconcile_question()
        text.bt_answer = ""
        local id, kind = question()
        shown_id, shown_kind = id, kind
        local code = text["bt.pairing_code"] or ""
        text.bt_code = #code == 6 and code:match("^%d+$") and code:sub(1, 3) .. " " .. code:sub(4) or code
        if id > 0 and kind ~= "none" then fact.bt_open = true end
    end
    on("bluetooth_question", reconcile_question)
    on("bluetooth_move", function(delta)
        local count = fact["bt_devices.count"] or 0
        if count == 0 then return end
        set_unfolded(true)
        select(math.max(0, math.min(count - 1, fact.bt_selected + delta)))
        fact.bt_page = math.floor(fact.bt_selected / 6)
    end)
    on("bluetooth_page", function(delta) fact.bt_page = math.max(0, math.min(math.max(0, math.ceil((fact["bt_devices.count"] or 0) / 6) - 1), fact.bt_page + delta)) end)
    on("fact:bt_open", function(value) if not value then close() end end)
    if sys.watch then sys.watch("bluetooth", reconcile) end
    reconcile_question()
    if not fact.bt_open and fact.bt_scan_owned and request("scan", false) then fact.bt_scan_owned = false end
    return {open = open, close = close, toggle = toggle, reconcile = reconcile}
end
