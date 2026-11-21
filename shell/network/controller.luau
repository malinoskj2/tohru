-- Native NetworkManager/iwd owns status, credentials and asynchronous work.
-- rail is the Wi-Fi view's power switch (bar/rail.luau), or nil where the scene has none.
return function(rail)
    -- A stop the user chose is held until the service reports it, or for a few seconds if it never does.
    local held, held_timer, dragging, slid = nil, nil, false, false
    local function set_request(message)
        text.net_request = message
        fact.net_has_request = message ~= ""
    end
    local function request(command, ...)
        local ok, err = pcall(sys.call, "network." .. command, ...)
        set_request(ok and "Request sent — check observed status" or tostring(err))
        return ok
    end
    -- Power switch stops: 0 Off at the rail's left end, 1 On at its right.
    local function observed_stop() return fact["net.wifi"] == true and 1 or 0 end
    local function rail_goal(stop) return stop == 1 and 28 or 8 end
    local function power_follow(place)
        local observed = observed_stop()
        if held == observed then held = nil end
        fact.net_power_stop = held or observed
        if not rail or dragging then return end
        local goal = rail_goal(fact.net_power_stop)
        if place then rail.place(goal)
        elseif math.abs(goal - rail.target()) > 0.5 then rail.switch(goal) end
    end
    local function release_hold()
        if held_timer and cancel then cancel(held_timer) end
        held, held_timer = nil, nil
    end
    local function power(on)
        release_hold()
        if request("wifi", on) and (on and 1 or 0) ~= observed_stop() then
            held = on and 1 or 0
            if after then held_timer = after(4000, function() held, held_timer = nil, nil; power_follow() end) end
        end
        power_follow()
    end
    local function stop_joining()
        fact.net_joining = false
        text.net_password = ""
    end
    local function fold()
        fact.net_unfolded = false
        stop_joining()
    end
    local function close()
        fact.net_open = false
        fact.net_unfolded = false
        fact.net_keyed = false
        dragging = false
        stop_joining()
    end
    local function open()
        fact.net_open = true
        fact.net_unfolded = false
        fact.net_view = 0
        fact.net_focus = 0
        fact.net_keyed = false
        fact.net_page = 0
        fact.net_selected = -1
        dragging = false
        stop_joining()
        set_request("")
        release_hold()
        power_follow(true)
    end
    local function set_view(view)
        if view ~= 0 and view ~= 1 then return end
        if view ~= 0 then stop_joining() end
        fact.net_view = view
    end
    local function open_view(view)
        if view ~= 0 and view ~= 1 then return end
        set_view(view)
        fact.net_focus = view
        fact.net_unfolded = true
    end
    local function toggle() if fact.net_open then close() else open() end end
    local function select(index)
        if type(index) ~= "number" or index ~= math.floor(index) or index < 0 or index >= (fact["net_rows.count"] or 0) then return end
        local prefix = "net_rows." .. index .. "."
        local ssid = text[prefix .. "ssid"]
        if not ssid or ssid == "" then return end
        fact.net_selected = index
        text.net_target = ssid
        fact.net_target_active = fact[prefix .. "active"] == true
        fact.net_target_known = fact[prefix .. "known"] == true
        fact.net_joining = fact[prefix .. "secure"] == true and not fact.net_target_known and not fact.net_target_active
        if fact.net_joining then fact.net_view = 0; fact.net_unfolded = true end
        text.net_password = ""
        set_request("")
        if not fact.net_target_active and not fact.net_joining then request("connect", ssid) end
    end
    local function join()
        if not fact.net_joining or text.net_target == "" then return end
        local password = text.net_password
        text.net_password = ""
        if password == "" then set_request("Enter the network password"); return end
        if request("connect", text.net_target, password) then fact.net_joining = false end
    end
    local function forget_row(index)
        if type(index) ~= "number" or index ~= math.floor(index) or index < 0 or index >= (fact["net_rows.count"] or 0) then return end
        local prefix = "net_rows." .. index .. "."
        local ssid = text[prefix .. "ssid"]
        if fact[prefix .. "known"] == true and ssid and ssid ~= "" then
            text.net_target = ssid
            fact.net_target_known = true
            request("forget", ssid)
        end
    end
    on("network_open", open); on("network_close", close); on("network_toggle", toggle)
    on("network_open_view", open_view)
    on("network_view", set_view)
    on("network_fold", fold)
    on("network_focus_move", function(delta)
        if type(delta) ~= "number" or fact.net_unfolded then return end
        fact.net_focus = (fact.net_focus + (delta > 0 and 1 or -1)) % 2
        fact.net_keyed = true
    end)
    on("network_select", select); on("network_join", join); on("submit:net_password", join)
    on("network_cancel", stop_joining)
    on("network_scan", function() request("scan") end)
    on("network_power", function() power(observed_stop() == 0) end)
    -- A click toggles; a slide picks the stop nearest where it ends.
    on("network_power_press", function(x)
        if type(x) ~= "number" then return end
        dragging, slid = true, false
    end)
    on("network_power_drag", function(x)
        if type(x) ~= "number" or not dragging then return end
        slid = true
        if rail then rail.chase(math.max(8, math.min(28, x))) end
    end)
    on("network_power_release", function()
        if not dragging then return end
        dragging = false
        if slid and rail then power(rail.target() > 18) else power(fact.net_power_stop == 0) end
    end)
    on("network_observed", function() power_follow() end)
    on("network_disconnect", function() if fact["net.online"] and fact["net.kind"] == "wifi" and text["net.name"] == text.net_target then request("disconnect") end end)
    on("network_disconnect_active", function() if fact["net.online"] and fact["net.kind"] == "wifi" then request("disconnect") end end)
    on("network_forget", function() if fact.net_target_known and text.net_target ~= "" then request("forget", text.net_target) end end)
    on("network_forget_row", forget_row)
    on("network_move", function(delta)
        local count = fact["net_rows.count"] or 0
        if count == 0 or (fact.net_unfolded and fact.net_view ~= 0) then return end
        fact.net_view = 0
        fact.net_unfolded = true
        fact.net_selected = math.max(0, math.min(count - 1, fact.net_selected + delta))
        fact.net_page = math.floor(fact.net_selected / 7)
    end)
    on("network_activate", function() if fact.net_view == 0 then select(fact.net_selected) end end)
    on("network_page", function(delta) fact.net_page = math.max(0, math.min(math.max(0, math.ceil((fact["net_rows.count"] or 0) / 7) - 1), fact.net_page + delta)) end)
    on("fact:net_open", function(value) if not value then close() end end)
    return {open = open, close = close, toggle = toggle}
end
