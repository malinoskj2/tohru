-- The polkit dialog's states. Whether a request is let through is decided by
-- polkit's helper; this only shows what the native agent reports and carries
-- the typed answer to it, clearing the field as soon as it is sent.
local ROWS = 6
local HOLD = { granted = 1100, cancelled = 900, failed = 1600 }
local LEVEL = { request = 0.24, checking = 0.40, wrong = 0.44, granted = 0.78, cancelled = 0.12, failed = 0.30 }

return function()
    local seen = nil
    local ended = nil
    local outcome_timer = nil
    local showing_outcome = false
    local chars = 0
    local preferred, focused = "", ""

    -- The monitor the card goes to: the bar's, else the focused one, chosen once per showing.
    local function place()
        if fact.polkit_shown then return end
        local name = preferred ~= "" and preferred or focused
        if name == "" then name = text["screen.0.name"] or "" end
        text.polkit_output = name
    end

    -- Emptying the output unmaps the layer, so the next request maps it afresh and the
    -- compositor hands it the keyboard again; a layer only drawn empty would not get it back.
    local function hide()
        fact.polkit_shown = false
        text.polkit_output = ""
    end

    local function two(n)
        n = math.max(0, math.floor(n))
        return n < 10 and ("0" .. n) or tostring(n)
    end

    local function plural(n, one, many)
        return n .. " " .. (n == 1 and one or many)
    end

    local function clear_answer()
        text.polkit_answer = ""
        chars = 0
        fact.polkit_chars = 0
    end

    local function phase_of(v)
        if v.error == "wrong" and chars == 0 then return "wrong" end
        if chars > 0 then return "typing" end
        if v.pending and not v.asking then return "checking" end
        return "request"
    end

    local function level_of(phase)
        if phase == "typing" then return math.min(0.64, 0.24 + 0.04 * math.min(chars, 10)) end
        return LEVEL[phase] or 0.24
    end

    local function set_identities(v)
        local list = v.identities or {}
        fact.polkit_identities = math.min(#list, ROWS)
        fact.polkit_identity = -1
        for i = 0, ROWS - 1 do
            local id = list[i + 1]
            text["polkit_id" .. i] = id and id.name or ""
            text["polkit_id" .. i .. "_meta"] = id and (id.group ~= "" and (id.group .. " · " .. math.floor(id.uid)) or tostring(math.floor(id.uid))) or ""
            if id and id.name == v.user then fact.polkit_identity = i end
        end
        if fact.polkit_picked < 0 or fact.polkit_picked >= fact.polkit_identities then
            fact.polkit_picked = math.max(0, fact.polkit_identity)
        end
    end

    local function footer(v, phase)
        local n = #(v.identities or {})
        local base = plural(n, "admin", "admins") .. " · Tab to unfold"
        if v.warning ~= nil and v.warning ~= "" then return v.warning, true end
        if phase == "wrong" then return "Incorrect password · " .. plural(v.tries, "attempt", "attempts") .. " remaining", true end
        if v.info ~= nil and v.info ~= "" then return v.info, false end
        if phase == "checking" then return "Checking…", false end
        if (v.queued or 0) > 0 then return base .. " · " .. math.floor(v.queued) .. " waiting", false end
        return base, false
    end

    -- What the card says while a request is attended.
    local function show_request(v)
        local phase = phase_of(v)
        place()
        text.polkit_phase = phase
        fact.polkit_shown = true
        fact.polkit_outcome = false
        fact.polkit_asking = v.asking == true
        text.polkit_app = v.app ~= "" and v.app or "Authentication"
        text.polkit_message = v.message or ""
        text.polkit_action = v.action or ""
        text.polkit_command = v.command or ""
        text.polkit_user = v.user or ""
        text.polkit_placeholder = (v.prompt ~= "" and v.prompt ~= "Password: " and v.prompt ~= "Password:")
            and v.prompt:gsub("%s+$", "") or ("Password for " .. (v.user or "") .. "…")
        if phase == "typing" then
            text.polkit_numeral = two(chars); text.polkit_caption = "CHARS"
        elseif phase == "wrong" then
            text.polkit_numeral = two(v.tries); text.polkit_caption = "TRIES LEFT"
        else
            text.polkit_numeral = two(v.tries); text.polkit_caption = "ATTEMPTS"
        end
        fact.polkit_level = level_of(phase)
        fact.polkit_tint = phase == "wrong" and 1 or 0
        fact.polkit_slosh = phase == "wrong" and 2.35 or (phase == "typing" and 1.35 or 0.65)
        local line, alarm = footer(v, phase)
        text.polkit_footer = line
        fact.polkit_alarm = alarm
        set_identities(v)
        if v.asking then focus("polkit_answer") end
    end

    -- What the card says once a request has ended, for a moment.
    local function show_outcome(v)
        place()
        showing_outcome = true
        fact.polkit_shown = true
        fact.polkit_outcome = true
        fact.polkit_asking = false
        fact.polkit_unfolded = false
        clear_answer()
        focus()
        local o = v.outcome
        text.polkit_phase = o
        fact.polkit_level = LEVEL[o] or 0.12
        fact.polkit_tint = o == "granted" and -1 or (o == "failed" and 1 or 0)
        fact.polkit_slosh = o == "failed" and 2.0 or (o == "granted" and 0.55 or 0.2)
        if o == "granted" then
            text.polkit_numeral = "✓"; text.polkit_caption = "UNLOCKED"
            text.polkit_result = "Authenticated as " .. (v.outcome_user or "")
            text.polkit_result_tag = "Granted"
            text.polkit_footer = "Session token issued"
            text.polkit_closing = "Continuing…"
        elseif o == "failed" then
            text.polkit_numeral = "00"; text.polkit_caption = "TRIES LEFT"
            text.polkit_result = "Authentication failed"
            text.polkit_result_tag = "Denied"
            text.polkit_footer = "The request was refused"
            text.polkit_closing = "Closed"
        else
            text.polkit_numeral = "—"; text.polkit_caption = "DISMISSED"
            text.polkit_result = "Authentication cancelled"
            text.polkit_result_tag = "Esc"
            text.polkit_footer = "Request denied by user"
            text.polkit_closing = "Closed"
        end
        fact.polkit_alarm = o == "failed"
        if outcome_timer then cancel(outcome_timer) end
        outcome_timer = after(HOLD[o] or 900, function()
            outcome_timer = nil
            showing_outcome = false
            if seen and seen.pending then show_request(seen) else hide() end
        end)
    end

    local function report(v)
        seen = v
        fact.polkit_ready = v.ready == true
        if ended == nil then ended = v.ended end
        if v.ended ~= ended then
            ended = v.ended
            clear_answer()
            if v.outcome == "granted" or v.outcome == "cancelled" or v.outcome == "failed" then
                show_outcome(v)
                return
            end
        end
        if showing_outcome then return end
        if not v.pending then
            hide()
            fact.polkit_unfolded = false
            clear_answer()
            focus()
            return
        end
        if not v.asking and chars > 0 then clear_answer() end
        show_request(v)
    end

    local function submit()
        if not (seen and seen.pending and seen.asking) or showing_outcome then return end
        local answer = text.polkit_answer or ""
        if answer == "" then return end
        clear_answer()
        local ok, err = pcall(sys.call, "polkit.answer", answer)
        answer = nil
        if not ok then
            text.polkit_footer = tostring(err)
            fact.polkit_alarm = true
            return
        end
        text.polkit_phase = "checking"
        fact.polkit_level = LEVEL.checking
        text.polkit_footer = "Checking…"
        fact.polkit_alarm = false
    end

    local function cancel_request()
        clear_answer()
        if seen and seen.pending and not showing_outcome then pcall(sys.call, "polkit.cancel") end
    end

    local function fold(open)
        if not (seen and seen.pending) or showing_outcome then return end
        fact.polkit_unfolded = open
        if open then fact.polkit_picked = math.max(0, fact.polkit_identity) end
        if seen.asking then focus("polkit_answer") end
    end

    local function choose(index)
        if not (seen and seen.pending) or showing_outcome then return end
        if type(index) ~= "number" or index ~= math.floor(index) or index < 0 or index >= fact.polkit_identities then return end
        local name = text["polkit_id" .. index]
        fact.polkit_unfolded = false
        if name == nil or name == "" or name == seen.user then return end
        clear_answer()
        local ok, err = pcall(sys.call, "polkit.identity", name)
        if not ok then
            text.polkit_footer = tostring(err)
            fact.polkit_alarm = true
        end
    end

    local function typed(value)
        if not (seen and seen.pending and seen.asking) or showing_outcome then
            if (value or "") ~= "" then text.polkit_answer = "" end
            chars = 0
            fact.polkit_chars = 0
            return
        end
        chars = utf8.len(value or "") or #(value or "")
        fact.polkit_chars = chars
        show_request(seen)
    end

    local function key(name)
        if not fact.polkit_shown or showing_outcome then return end
        if name == "Escape" then
            if fact.polkit_unfolded then fold(false) else cancel_request() end
        elseif name == "Tab" then
            fold(not fact.polkit_unfolded)
        elseif fact.polkit_unfolded and (name == "Down" or name == "Up") then
            local step = name == "Down" and 1 or -1
            fact.polkit_picked = math.max(0, math.min(fact.polkit_identities - 1, fact.polkit_picked + step))
        elseif fact.polkit_unfolded and (name == "Return" or name == "KP_Enter") then
            choose(fact.polkit_picked)
        end
    end

    local function start(enabled, output)
        hide()
        fact.polkit_enabled = enabled == true
        preferred = output or ""
        if not enabled then return false end
        if preferred == "" then sys.watch("window", function(w) focused = w.monitor or "" end) end
        return sys.watch("polkit", report)
    end

    on("submit:polkit_answer", submit)
    on("text:polkit_answer", typed)
    on("polkit_submit", submit)
    on("polkit_cancel", cancel_request)
    on("polkit_fold", function() fold(not fact.polkit_unfolded) end)
    on("polkit_choose", choose)
    on("key", key)
    -- Another surface took the keyboard: ask for it back while a request waits.
    on("blur", function() if seen and seen.asking and not showing_outcome then focus("polkit_answer") end end)

    return { start = start, report = report }
end
