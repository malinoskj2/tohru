-- Presentation only: the password never reaches this VM (it hears bullets), and nothing here
-- can release the lock without the native PAM permit.
local STYLES = {"still-glass", "ribbon-key", "flooded-monolith", "drifting-frost-veil", "unfolding-glass-capsule",
    "edge-glass-drawer", "horizon-meniscus", "refractive-typographic-lens", "corner-anchor", "bar-drop-curtain"}
local DEFAULT = 3
-- How long each style's unlock motion plays before the lock is handed back (index 0 is plain).
local UNLOCK_MS = {[0] = 260, 320, 420, 520, 440, 380, 340, 480, 420, 300, 380}

local function style_index(name)
    for index, style in ipairs(STYLES) do
        if style == name then return index end
    end
    return nil
end

local function chosen_style(settings, env_style)
    if env_style and env_style ~= "" then
        local index = style_index(env_style)
        if index then return index end
        log("lock: TOHRU_LOCK_STYLE '" .. env_style .. "' is not a lock style; using flooded-monolith")
        return DEFAULT
    end
    if not settings then
        log("lock: settings are unavailable; using flooded-monolith")
        return DEFAULT
    end
    local ok, snapshot = pcall(settings.snapshot)
    if not ok then
        log("lock: settings are unavailable (" .. tostring(snapshot) .. "); using flooded-monolith")
        return DEFAULT
    end
    local warned, warnings = pcall(settings.warnings)
    if warned then
        for _, warning in ipairs(warnings) do log("lock: warning: " .. warning) end
    end
    local row = snapshot["lock.style"]
    local index = row and style_index(row.value)
    if not index then
        log("lock: unknown lock.style '" .. tostring(row and row.value) .. "'; using flooded-monolith")
        return DEFAULT
    end
    return index
end

local function luma(hex)
    local r, g, b = string.match(hex or "", "^#(%x%x)(%x%x)(%x%x)")
    if not r then return nil end
    return (0.2126 * tonumber(r, 16) + 0.7152 * tonumber(g, 16) + 0.0722 * tonumber(b, 16)) / 255
end

return function(styled)
    local env = function(name)
        local ok, value = pcall(sys.ask, "env", name)
        return ok and value or nil
    end
    fact.style = styled and chosen_style(tohru_settings, env("TOHRU_LOCK_STYLE")) or 0
    text.user = env("USER") or ""
    text.host = env("HOSTNAME") or ""
    if styled and tohru_info and type(tohru_info.avatar) == "function" then
        local ok, avatar = pcall(tohru_info.avatar, text.user, env("HOME") or "")
        if ok and type(avatar) == "string" and avatar ~= "" then
            text.avatar_path = avatar
            fact.has_avatar = true
        end
    end
    if styled and text.host == "" then
        pcall(run, "uname", {"-n"}, function(out, code)
            if code == 0 then text.host = string.gsub(out or "", "%s+$", "") end
        end)
    end

    local DAYS = {"Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"}
    local MONTHS = {"January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December"}
    pcall(sys.watch, "clock", function(value)
        local day, month = DAYS[value.weekday + 1], MONTHS[value.month]
        text.clock_h = string.format("%02d", value.hour)
        text.clock_m = string.format("%02d", value.minute)
        text.clock_hm = string.format("%02d:%02d", value.hour, value.minute)
        text.date_long = string.format("%s, %s %d", day, month, value.day)
        text.date_short = string.format("%s, %s %d", string.sub(day, 1, 3), string.sub(month, 1, 3), value.day)
        text.date_caps = string.upper(string.format("%s · %s %d", day, month, value.day))
        text.date_comma_caps = string.upper(text.date_long)
        text.date_bar = string.format("%s %d %s", string.sub(day, 1, 3), value.day, string.sub(month, 1, 3))
    end)
    if styled then
        pcall(sys.watch, "media", function(value)
            local title = value.title or ""
            if value.artist and value.artist ~= "" then title = value.artist .. " — " .. title end
            text.media_title = title
            fact.media_known = (value.player or "") ~= "" and title ~= ""
            fact.media_playing = value.playing == true
        end)
    end

    -- The screen as it was, captured before the lock covers it; nothing is captured afterwards.
    local engaging = false
    local function engage()
        if engaging or fact.engaged or fact.released then return end
        engaging = true
        fact.engaged = true
    end
    local function capture(output)
        if not styled or output == "" then return end
        local ok, name = pcall(sys.ask, "capture.freeze", output)
        if ok and type(name) == "string" and name ~= "" then
            text.output_name = output
            text.shot = name
            fact.has_shot = true
            local sum, count = 0, 0
            for _, point in ipairs({{160, 140}, {640, 120}, {1120, 160}, {300, 520}, {900, 600}}) do
                local sampled, colour = pcall(sys.ask, "capture.pixel", point[1], point[2], output)
                local l = sampled and luma(colour)
                if l then sum, count = sum + l, count + 1 end
            end
            if count > 0 then fact.pk = (sum / count > 0.55) and 1 or 0 end
        else
            log("lock: no capture of " .. output .. ": " .. tostring(name))
        end
    end
    local function preferred()
        if not tohru_settings then return "" end
        local ok, snapshot = pcall(tohru_settings.snapshot)
        local row = ok and snapshot["bar.preferred_output"]
        return row and row.value or ""
    end
    local watched = false
    if styled then
        watched = pcall(sys.watch, "workspaces", function(value)
            local count, active = 0, 1
            for index, workspace in ipairs(value.list or {}) do
                count = index
                if workspace.id == value.active then active = index end
            end
            fact.ws_count = math.max(5, count)
            fact.ws_active = active
            if engaging then return end
            local output = preferred()
            if output == "" then
                for _, workspace in ipairs(value.list or {}) do
                    if workspace.id == value.active then output = workspace.monitor or "" end
                end
            end
            if output == "" and value.list and value.list[1] then output = value.list[1].monitor or "" end
            capture(output)
            engage()
        end)
    end
    if not watched then engage() end
    after(900, engage)

    -- What the field holds reaches this VM only as bullets while the lock is engaged.
    local before = 0
    local idle
    local function rest()
        if idle then cancel(idle) end
        idle = after(9000, function()
            if fact.typed == 0 and not fact.checking then fact.awake = false end
        end)
    end
    on("text:password", function(value)
        local length = utf8.len(value) or 0
        if length > 0 then
            fact.wrong = false
            text.status = ""
            fact.awake = true
        end
        before = fact.typed
        fact.typed = length
        rest()
    end)
    on("submit:password", function()
        if fact.unlocking then return end
        -- The field is emptied natively as it is submitted; keep showing what was typed.
        fact.typed = before
    end)
    on("key", function(name)
        fact.awake = true
        rest()
        if name == "Escape" and not fact.checking and not fact.unlocking then
            text.password = ""
            fact.typed = 0
            fact.awake = false
            fact.wrong = false
            text.status = ""
        end
    end)
    -- From the lock, as from any login screen: suspend, and restart after a second click.
    on("lock_sleep", function()
        pcall(run, "systemctl", {"suspend"})
    end)
    local disarm
    on("lock_restart", function()
        if fact.restart_armed then
            fact.restart_armed = false
            pcall(run, "systemctl", {"reboot"})
            return
        end
        fact.restart_armed = true
        if disarm then cancel(disarm) end
        disarm = after(4000, function() fact.restart_armed = false end)
    end)
    on("fact:held", function(held)
        if held then focus("password") end
    end)
    on("fact:lock_auth", function(state)
        fact.checking = state == 1
        if state == 2 then
            fact.wrong = false
            text.status = ""
            fact.unlocking = true
            -- The motion plays on the lock; then it is handed back.
            after(UNLOCK_MS[fact.style] or 400, function()
                fact.released = true
                fact.engaged = false
            end)
        elseif state == 3 then
            fact.wrong = true
            text.status = "Incorrect password"
            focus("password")
            rest()
        elseif state == 4 then
            fact.wrong = true
            text.status = "Authentication unavailable"
            focus("password")
        elseif state == 0 and not fact.unlocking then
            focus("password")
        end
    end)
end
