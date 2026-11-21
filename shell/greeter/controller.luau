-- Presentation and greeter orchestration: passwords never enter this VM (it receives
-- masked bullets only), and greetd IPC runs natively inside `tohru_greeter`.
local STYLES = {"still-glass", "ribbon-key", "flooded-monolith", "drifting-frost-veil", "unfolding-glass-capsule",
    "edge-glass-drawer", "horizon-meniscus", "refractive-typographic-lens", "corner-anchor", "bar-drop-curtain"}
local DEFAULT_STYLE = 3
local TRANSITION_MS = {[0] = 220, 280, 360, 440, 380, 340, 300, 420, 360, 260, 340}

local function style_index(name)
    if name == "plain" then return 0 end
    for index, style in ipairs(STYLES) do
        if style == name then return index end
    end
    return nil
end

local function chosen_style(configured, env_style)
    if env_style and env_style ~= "" then
        local index = style_index(env_style)
        if index ~= nil then return index end
        log("greeter: TOHRU_GREETER_STYLE '" .. env_style .. "' is not a greeter style; using flooded-monolith")
        return DEFAULT_STYLE
    end
    if configured and configured ~= "" then
        local index = style_index(configured)
        if index ~= nil then return index end
        log("greeter: unknown greeter.style '" .. tostring(configured) .. "'; using flooded-monolith")
        return DEFAULT_STYLE
    end
    return DEFAULT_STYLE
end

return function(styled)
    local function env(name)
        local ok, value = pcall(sys.ask, "env", name)
        return ok and value or nil
    end

    fact.is_greeter = true
    text.cta_idle = "Type password to sign in"
    text.cta_submit = "Press Return to sign in"
    text.cta_ok = "Signing in…"
    text.cta_ready = "Signed in"
    text.cta_locked = "Sign in"
    text.cta_session = "Sign in"
    text.cta_ribbon = "Type to sign in"
    text.cta_tide = "Type password to sign in"

    local init = nil
    if tohru_greeter and type(tohru_greeter.init) == "function" then
        local ok, data = pcall(tohru_greeter.init)
        if ok and type(data) == "table" then
            init = data
        else
            log("greeter: native init unavailable: " .. tostring(data))
        end
    end

    local cfg = (init and init.config) or {}
    for _, warning in ipairs(cfg.warnings or {}) do
        log("greeter: warning: " .. tostring(warning))
    end

    fact.style = styled and chosen_style(cfg.style, env("TOHRU_GREETER_STYLE")) or 0
    fact.allow_power = cfg.allow_power_actions ~= false
    text.host = (init and init.host and init.host ~= "" and init.host) or env("HOSTNAME") or ""

    if styled and type(cfg.wallpaper) == "string" and cfg.wallpaper ~= "" then
        text.shot = cfg.wallpaper
        fact.has_shot = true
        fact.pk = (init and init.wallpaper_light == true) and 1 or 0
    else
        fact.has_shot = false
        fact.pk = 0
    end

    local users = (init and init.users) or {}
    local sessions = (init and init.sessions) or {}
    fact.user_count = #users
    fact.session_count = #sessions

    local user_idx = 0
    local session_idx = 0

    local function apply_user(name, display_name, avatar)
        text.user = name or ""
        text.user_display = (display_name and display_name ~= "" and display_name) or (name ~= "" and name) or "Select user"
        if type(avatar) == "string" and avatar ~= "" then
            text.avatar_path = avatar
            fact.has_avatar = true
        else
            text.avatar_path = ""
            fact.has_avatar = false
        end
        if tohru_greeter and type(tohru_greeter.select_user) == "function" then
            pcall(tohru_greeter.select_user, text.user)
        end
    end

    local function select_user_by_index(index)
        if #users == 0 then
            user_idx = 0
            apply_user(init and init.selected_user or "", nil, nil)
            return
        end
        user_idx = ((index - 1) % #users) + 1
        local row = users[user_idx]
        apply_user(row.name, row.display_name, row.avatar)
    end

    local function select_session_by_index(index)
        if #sessions == 0 then
            session_idx = 0
            text.session_name = "No session"
            return
        end
        session_idx = ((index - 1) % #sessions) + 1
        local row = sessions[session_idx]
        text.session_name = row.name or row.id or "Session"
        if tohru_greeter and type(tohru_greeter.select_session) == "function" then
            pcall(tohru_greeter.select_session, row.id or "")
        end
    end

    local initial_user = init and init.selected_user or ""
    local matched_user = nil
    for i, row in ipairs(users) do
        if row.name == initial_user then
            matched_user = i
            break
        end
    end
    if matched_user then
        select_user_by_index(matched_user)
    elseif #users > 0 then
        select_user_by_index(1)
    else
        apply_user(initial_user, nil, nil)
    end

    local initial_session = init and init.selected_session or ""
    local matched_session = nil
    for i, row in ipairs(sessions) do
        if row.id == initial_session then
            matched_session = i
            break
        end
    end
    if matched_session then
        select_session_by_index(matched_session)
    elseif #sessions > 0 then
        select_session_by_index(1)
    else
        text.session_name = "No session"
    end

    -- Route connected monitors cleanly without compositor IPC: preferred_output gets the
    -- interactive login UI when connected; any other outputs get the calm secondary backdrop.
    -- If preferred_output is disconnected, the first connected output becomes primary so
    -- unplugging a monitor can never cause a lockout.
    local focus_password
    local preferred_output = cfg.preferred_output or ""
    local function route_outputs()
        local list, seen = {}, {}
        for index = 0, 7 do
            local name = text["screen." .. index .. ".name"]
            if type(name) == "string" and name ~= "" and not string.find(name, ",", 1, true) and not seen[name] then
                seen[name] = true
                list[#list + 1] = name
            end
        end
        if #list == 0 then
            return
        end
        local primary = seen[preferred_output] and preferred_output or list[1]
        local secondary = {}
        for _, name in ipairs(list) do
            if name ~= primary then
                secondary[#secondary + 1] = name
            end
        end
        text.greeter_primary_outputs = primary
        text.greeter_secondary_outputs = table.concat(secondary, ",")
        text.output_name = primary
        if focus_password then
            focus_password()
            after(150, focus_password)
        end
    end
    for index = 0, 7 do
        on("text:screen." .. index .. ".name", route_outputs)
    end
    route_outputs()

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

    on("text:kb_layout", function(value)
        fact.has_kb_layout = type(value) == "string" and value ~= ""
    end)

    local before = 0
    local idle
    local function rest()
        if idle then cancel(idle) end
        idle = after(9000, function()
            if fact.typed == 0 and not fact.checking and not fact.custom_user_open and not fact.prompt_visible and not fact.prompt_info then
                fact.awake = false
            end
        end)
    end

    focus_password = function()
        if not fact.unlocking and not fact.custom_user_open and not fact.prompt_visible and not fact.prompt_info then
            focus("password")
        end
    end

    focus_password()
    after(150, focus_password)

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
        fact.typed = before
    end)

    on("submit:visible_answer", function(value)
        if not fact.prompt_visible or fact.checking or fact.unlocking then return end
        if tohru_greeter and type(tohru_greeter.submit_visible) == "function" then
            pcall(tohru_greeter.submit_visible, value or "")
        end
        text.visible_answer = ""
    end)

    on("greeter_continue_info", function()
        if not fact.prompt_info or fact.checking or fact.unlocking then return end
        if tohru_greeter and type(tohru_greeter.submit_visible) == "function" then
            pcall(tohru_greeter.submit_visible, nil)
        end
    end)

    on("greeter_next_user", function()
        if fact.checking or fact.unlocking then return end
        fact.wrong = false
        text.status = ""
        text.password = ""
        fact.typed = 0
        if #users > 1 then
            select_user_by_index(user_idx + 1)
            focus_password()
        else
            fact.custom_user_open = true
            text.custom_user_input = text.user or ""
            focus("custom_user_input")
        end
    end)

    on("greeter_open_custom_user", function()
        if fact.checking or fact.unlocking then return end
        fact.custom_user_open = true
        text.custom_user_input = text.user or ""
        focus("custom_user_input")
    end)

    on("greeter_close_custom_user", function()
        fact.custom_user_open = false
        focus_password()
    end)

    on("submit:custom_user_input", function(value)
        if not fact.custom_user_open then return end
        local name = string.gsub(value or "", "^%s*(.-)%s*$", "%1")
        fact.custom_user_open = false
        if name ~= "" then
            local matched = nil
            for i, row in ipairs(users) do
                if row.name == name then
                    matched = i
                    break
                end
            end
            if matched then
                select_user_by_index(matched)
            else
                user_idx = 0
                apply_user(name, name, nil)
            end
            fact.wrong = false
            text.status = ""
            text.password = ""
            fact.typed = 0
        end
        focus_password()
    end)

    on("greeter_next_session", function()
        if fact.checking or fact.unlocking or #sessions <= 1 then return end
        select_session_by_index(session_idx + 1)
        focus_password()
    end)

    on("key", function(name)
        if name == "Escape" then
            if fact.custom_user_open then
                fact.custom_user_open = false
                focus_password()
                return
            end
            if fact.prompt_visible or fact.prompt_info then
                fact.prompt_visible = false
                fact.prompt_info = false
                if tohru_greeter and type(tohru_greeter.cancel) == "function" then
                    pcall(tohru_greeter.cancel)
                end
                focus_password()
                return
            end
            if not fact.checking and not fact.unlocking then
                text.password = ""
                fact.typed = 0
                fact.awake = false
                fact.wrong = false
                text.status = ""
            end
            return
        end
        if name == "Return" and fact.prompt_info and not fact.checking and not fact.unlocking then
            if tohru_greeter and type(tohru_greeter.submit_visible) == "function" then
                pcall(tohru_greeter.submit_visible, nil)
            end
            return
        end
        if name == "F1" and not fact.checking and not fact.unlocking and not fact.custom_user_open then
            if #users > 1 then
                select_user_by_index(user_idx + 1)
            else
                fact.custom_user_open = true
                text.custom_user_input = text.user or ""
                focus("custom_user_input")
            end
            return
        end
        if name == "F2" and not fact.checking and not fact.unlocking and not fact.custom_user_open then
            if #sessions > 1 then
                select_session_by_index(session_idx + 1)
            end
            return
        end
        if name == "Ctrl+u" and not fact.checking and not fact.unlocking then
            fact.custom_user_open = true
            text.custom_user_input = text.user or ""
            focus("custom_user_input")
            return
        end
        fact.awake = true
        rest()
    end)

    on("lock_sleep", function()
        if not fact.allow_power then return end
        pcall(run, "systemctl", {"suspend"})
    end)

    local disarm_restart
    on("lock_restart", function()
        if not fact.allow_power then return end
        if fact.restart_armed then
            fact.restart_armed = false
            pcall(run, "systemctl", {"reboot"})
            return
        end
        fact.restart_armed = true
        fact.poweroff_armed = false
        if disarm_restart then cancel(disarm_restart) end
        disarm_restart = after(4000, function() fact.restart_armed = false end)
    end)

    local disarm_poweroff
    on("greeter_poweroff", function()
        if not fact.allow_power then return end
        if fact.poweroff_armed then
            fact.poweroff_armed = false
            pcall(run, "systemctl", {"poweroff"})
            return
        end
        fact.poweroff_armed = true
        fact.restart_armed = false
        if disarm_poweroff then cancel(disarm_poweroff) end
        disarm_poweroff = after(4000, function() fact.poweroff_armed = false end)
    end)

    on("fact:greeter_auth", function(state)
        fact.checking = state == 1
        if state == 2 then
            fact.wrong = false
            text.status = ""
            fact.prompt_visible = false
            fact.prompt_info = false
            fact.unlocking = true
            after(TRANSITION_MS[fact.style] or 320, function()
                if tohru_greeter and type(tohru_greeter.finish) == "function" then
                    pcall(tohru_greeter.finish)
                end
            end)
        elseif state == 3 then
            fact.wrong = true
            fact.prompt_visible = false
            fact.prompt_info = false
            text.status = (text.greeter_error and text.greeter_error ~= "" and text.greeter_error) or "Incorrect password"
            focus_password()
            rest()
        elseif state == 4 then
            fact.wrong = true
            fact.prompt_visible = false
            fact.prompt_info = false
            text.status = (text.greeter_error and text.greeter_error ~= "" and text.greeter_error) or "Login service unavailable"
            focus_password()
        elseif state == 5 then
            fact.wrong = false
            fact.prompt_visible = true
            fact.prompt_info = false
            text.visible_answer = ""
            focus("visible_answer")
        elseif state == 6 then
            fact.wrong = false
            fact.prompt_visible = false
            fact.prompt_info = true
        elseif state == 7 then
            fact.wrong = false
            fact.prompt_visible = false
            fact.prompt_info = false
            text.password = ""
            fact.typed = 0
            focus_password()
        elseif state == 0 and not fact.unlocking then
            focus_password()
        end
    end)
end
