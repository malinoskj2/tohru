-- The declarative native Bluetooth service explicitly offers the pairing agent.
return function()
    local shown_id, shown_kind = 0, "none"
    local function request(command, ...)
        local ok, err = pcall(sys.call, "bluetooth." .. command, ...)
        text.bt_request = ok and "Request sent — check observed status" or tostring(err)
        return ok
    end
    local function question() return fact["bt.pairing_id"] or 0, fact["bt.pairing"] or "none" end
    local function cancel_question()
        local id, kind = question()
        if id > 0 and kind ~= "none" then request("cancel", id) end
        text.bt_answer = ""
    end
    local function close()
        fact.bt_open = false
        cancel_question()
        if fact.bt_scan_owned and request("scan", false) then fact.bt_scan_owned = false end
    end
    local function open() fact.bt_open = true; fact.bt_page = 0; fact.bt_selected = -1; fact.bt_has_target = false; text.bt_target = "" end
    local function toggle() if fact.bt_open then close() else open() end end
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
        request(fact[prefix .. "connected"] and "disconnect" or "connect", text.bt_target)
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
    on("bluetooth_open", open); on("bluetooth_close", close)
    on("bluetooth_toggle", toggle); on("bluetooth_select", select)
    on("bluetooth_connect", connect)
    on("bluetooth_activate", connect)
    on("bluetooth_forget", function() if text.bt_target ~= "" then request("forget", text.bt_target) end end)
    on("bluetooth_power", function() request("power", not fact["bt.powered"]) end)
    on("bluetooth_scan", function()
        if fact.bt_scan_owned then
            if request("scan", false) then fact.bt_scan_owned = false end
        elseif fact["bt.discovering"] then text.bt_request = "Discovery is already active"
        elseif request("scan", true) then fact.bt_scan_owned = true end
    end)
    on("bluetooth_answer", answer); on("submit:bt_answer", answer)
    on("bluetooth_cancel", cancel_question)
    local function reconcile_question()
        text.bt_answer = ""
        local id, kind = question()
        shown_id, shown_kind = id, kind
        if id > 0 and kind ~= "none" then fact.bt_open = true end
    end
    on("bluetooth_question", reconcile_question)
    on("bluetooth_move", function(delta)
        local count = fact["bt_devices.count"] or 0
        if count == 0 then return end
        select(math.max(0, math.min(count - 1, fact.bt_selected + delta)))
        fact.bt_page = math.floor(fact.bt_selected / 6)
    end)
    on("bluetooth_page", function(delta) fact.bt_page = math.max(0, math.min(math.max(0, math.ceil((fact["bt_devices.count"] or 0) / 6) - 1), fact.bt_page + delta)) end)
    on("fact:bt_open", function(value) if not value then close() end end)
    reconcile_question()
    if not fact.bt_open and fact.bt_scan_owned and request("scan", false) then fact.bt_scan_owned = false end
    return {open = open, close = close, toggle = toggle}
end
