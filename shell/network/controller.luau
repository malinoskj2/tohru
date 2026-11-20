-- Native NetworkManager/iwd owns status, credentials and asynchronous work.
return function()
    local function set_request(message)
        text.net_request = message
        fact.net_has_request = message ~= ""
    end
    local function request(command, ...)
        local ok, err = pcall(sys.call, "network." .. command, ...)
        set_request(ok and "Request sent — check observed status" or tostring(err))
        return ok
    end
    local function close()
        fact.net_open = false
        fact.net_unfolded = false
        fact.net_joining = false
        text.net_password = ""
    end
    local function open()
        fact.net_open = true
        fact.net_unfolded = false
        fact.net_page = 0
        fact.net_selected = -1
        fact.net_joining = false
        text.net_password = ""
        set_request("")
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
        if fact.net_joining then fact.net_unfolded = true end
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
    on("network_fold_toggle", function()
        fact.net_unfolded = not fact.net_unfolded
        if not fact.net_unfolded then fact.net_joining = false; text.net_password = "" end
    end)
    on("network_select", select); on("network_join", join); on("submit:net_password", join)
    on("network_cancel", function() fact.net_joining = false; text.net_password = "" end)
    on("network_scan", function() request("scan") end)
    on("network_power", function() request("wifi", not fact["net.wifi"]) end)
    on("network_disconnect", function() if fact["net.online"] and fact["net.kind"] == "wifi" and text["net.name"] == text.net_target then request("disconnect") end end)
    on("network_disconnect_active", function() if fact["net.online"] and fact["net.kind"] == "wifi" then request("disconnect") end end)
    on("network_forget", function() if fact.net_target_known and text.net_target ~= "" then request("forget", text.net_target) end end)
    on("network_forget_row", forget_row)
    on("network_move", function(delta)
        local count = fact["net_rows.count"] or 0
        if count == 0 then return end
        fact.net_unfolded = true
        fact.net_selected = math.max(0, math.min(count - 1, fact.net_selected + delta))
        fact.net_page = math.floor(fact.net_selected / 7)
    end)
    on("network_activate", function() select(fact.net_selected) end)
    on("network_page", function(delta) fact.net_page = math.max(0, math.min(math.max(0, math.ceil((fact["net_rows.count"] or 0) / 7) - 1), fact.net_page + delta)) end)
    on("fact:net_open", function(value) if not value then close() end end)
    return {open = open, close = close, toggle = toggle}
end
