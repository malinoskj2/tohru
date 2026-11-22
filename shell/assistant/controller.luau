-- What is shown always comes from tohru-assistant: the providers it finds, the chats it saves and the lines
-- it streams. settings = {provider = function() -> id, choose = function(id), value = function(key) -> value}.
return function(settings)
    local helper = "tohru-assistant"
    local providers, current = {}, nil
    local conversation, shown_turns = nil, {}
    -- The one answer in flight: its process, conversation, provider and text so far.
    local live = nil
    local history, search_serial = {}, 0
    local answer_listeners = {}
    local probing, probed_at = false, -1
    local PROBE_AGAIN = 30000
    local MEMORY_CHARS = 72
    -- A chat counts as ended, and is handed to memory, five minutes after its last answer or when it is left.
    local IDLE_ENDS = 300000
    local clock = 0
    local unremembered = {}
    local remember
    every(1000, function()
        clock = clock + 1000
        for id, at in pairs(unremembered) do
            if clock - at >= IDLE_ENDS and not (live and live.conversation == id) then remember(id) end
        end
    end)

    local function setting(key, default)
        local value = settings.value and settings.value(key)
        if value == nil then return default end
        return value
    end
    local PROVIDER_IDS = {"claude", "codex", "gemini", "agy", "opencode"}
    local function model_flags(id)
        return {"--model", setting("assistant.models." .. id .. ".model", ""), "--effort", setting("assistant.models." .. id .. ".effort", "")}
    end
    local function memory_on() return setting("assistant.memory", true) == true end
    local function models_config()
        local models = {}
        for _, id in ipairs(PROVIDER_IDS) do
            models[id] = {model = setting("assistant.models." .. id .. ".model", ""), effort = setting("assistant.models." .. id .. ".effort", "")}
        end
        return models
    end

    function remember(id)
        unremembered[id] = nil
        if not id or not memory_on() then return end
        run(helper, {"remember", id, "--memory", "on", "--models", json.encode(models_config())}, nil, {output = false})
    end

    local function decode(line)
        local ok, value = pcall(json.decode, line)
        if ok and type(value) == "table" then return value end
        return nil
    end

    local function fail(message)
        text.ai_error = message or ""
        fact.ai_failed = message ~= nil and message ~= ""
    end

    local function placeholder()
        local name = text.ai_provider_name
        if fact.ai_live_here then
            text.ai_placeholder = "Answering…"
        elseif fact.ai_current >= 0 and not fact.ai_signed_in then
            text.ai_placeholder = "Sign in to chat with " .. name
        elseif fact.ai_current < 0 then
            text.ai_placeholder = "No AI CLIs found"
        elseif #shown_turns > 0 then
            text.ai_placeholder = "Reply to " .. name
        else
            text.ai_placeholder = "Ask " .. name
        end
    end

    local function publish_current()
        local provider = current and providers[current]
        fact.ai_current = current and (current - 1) or -1
        fact.ai_slot = provider and provider.slot or 0
        fact.ai_signed_in = provider and provider.signed_in == true or false
        text.ai_provider_name = provider and provider.name or "Assistant"
        text.ai_provider_model = provider and provider.model or ""
        text.ai_provider_cli = provider and provider.id or ""
        placeholder()
    end

    local function index_of(id)
        for index, provider in ipairs(providers) do
            if provider.id == id then return index end
        end
        return nil
    end

    -- The configured provider when it is installed and signed in, else the first signed-in one, else the first.
    local function settle()
        local wanted = index_of(settings.provider())
        if wanted and providers[wanted].signed_in then current = wanted; return publish_current() end
        if current and providers[current] and providers[current].signed_in then return publish_current() end
        current = nil
        for index, provider in ipairs(providers) do
            if provider.signed_in then current = index; break end
        end
        if not current and #providers > 0 then current = wanted or 1 end
        publish_current()
    end

    local function probe(force)
        if probing or (not force and probed_at >= 0 and clock - probed_at < PROBE_AGAIN) then return end
        probing = true
        run(helper, {"providers", "--models", json.encode(models_config())}, function(out, code)
            probing = false
            probed_at = clock
            local list = code == 0 and decode(out) or nil
            if list then
                for _, provider in ipairs(list) do
                    provider.meta = provider.model ~= "" and provider.model or "Signed in"
                end
            end
            if not list then
                fail("Could not list the AI CLIs")
                return
            end
            local id = current and providers[current] and providers[current].id
            providers = list
            model.ai_providers = list
            current = id and index_of(id) or nil
            fact.ai_loaded = true
            settle()
        end)
    end

    local function show_turns(turns)
        shown_turns = turns
        local rows = {}
        local first = math.max(1, #turns - 63)
        for i = first, #turns do
            local turn = turns[i]
            local shown = turn.text or ""
            if turn.error then
                shown = "⚠ " .. (shown ~= "" and (shown .. "\n\n") or "") .. turn.error
            elseif turn.stopped then
                shown = (shown ~= "" and (shown .. " ") or "") .. "*— stopped*"
            end
            rows[#rows + 1] = {text = shown, mine = turn.role == "user", failed = turn.error ~= nil,
                stopped = turn.stopped == true, voice = turn.voice == true}
        end
        model.ai_turns = rows
        local last = turns[#turns]
        fact.ai_actions = last ~= nil and last.role == "assistant" and last.error == nil and (last.text or "") ~= ""
            and not fact.ai_live_here
        fact.ai_copied = false
        placeholder()
    end

    local function live_here()
        return live ~= nil and conversation ~= nil and live.conversation == conversation
    end

    local function refresh_live()
        fact.ai_live_here = live_here() or (live ~= nil and live.conversation == nil and conversation == nil)
        text.ai_live = live and live.text or ""
        fact.ai_streaming = live ~= nil
        fact.ai_stream_slot = live and live.slot or 0
        placeholder()
    end

    local ready_for = nil
    local function seen()
        if fact.ai_open and fact.ai_view == 0 and ready_for ~= nil and ready_for == conversation then
            fact.ai_ready = false
            ready_for = nil
        end
    end

    local function leave(next)
        if conversation and conversation ~= next and unremembered[conversation] and not (live and live.conversation == conversation) then
            remember(conversation)
        end
    end

    local function load(id, done)
        run(helper, {"show", id}, function(out, code)
            local data = code == 0 and decode(out) or nil
            if not data then fail("That chat could not be opened"); return end
            leave(data.id)
            conversation = data.id
            show_turns(data.turns or {})
            refresh_live()
            seen()
            if done then done(data) end
        end)
    end

    local function finish(event)
        local finished = live
        live = nil
        refresh_live()
        if not finished then return end
        if event.event == "error" then fail(event.message or "The answer failed") end
        if finished.conversation and (event.event == "done" or event.event == "stopped") then
            unremembered[finished.conversation] = clock
        end
        local here = conversation == finished.conversation
        if here then
            load(finished.conversation)
        end
        if event.event == "done" and (not fact.ai_open or not here or fact.ai_view ~= 0) then
            fact.ai_ready = true
            fact.ai_ready_slot = finished.slot
            ready_for = finished.conversation
        end
        for _, listener in ipairs(answer_listeners) do
            listener(event.event, finished.conversation, finished.text, finished.voice)
        end
    end

    local function send(prompt, voice)
        prompt = (prompt or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if prompt == "" or live then return false end
        local provider = current and providers[current]
        if not provider or not provider.signed_in then return false end
        fail(nil)
        local args = {"chat", "--provider", provider.id}
        if conversation then
            args[#args + 1] = "--conversation"
            args[#args + 1] = conversation
        end
        if voice then args[#args + 1] = "--voice" end
        for _, flag in ipairs(model_flags(provider.id)) do args[#args + 1] = flag end
        args[#args + 1] = "--persona"
        args[#args + 1] = setting("assistant.persona", true) == true and "on" or "off"
        args[#args + 1] = "--persona-file"
        args[#args + 1] = setting("assistant.persona_file", "")
        args[#args + 1] = "--memory"
        args[#args + 1] = memory_on() and "on" or "off"
        local orca = setting("assistant.orca", "auto")
        if orca == true then orca = "true" elseif orca == false then orca = "false" end
        args[#args + 1] = "--orca"
        args[#args + 1] = (orca == "true" or orca == "false") and orca or "auto"
        live = {conversation = conversation, slot = provider.slot, text = "", voice = voice == true, provider = provider.id}
        local turns = {}
        for _, turn in ipairs(shown_turns) do turns[#turns + 1] = turn end
        turns[#turns + 1] = {role = "user", text = prompt, voice = voice}
        if conversation or not voice or fact.ai_open then show_turns(turns) end
        fact.ai_actions = false
        refresh_live()
        local mine = live
        mine.id = spawn(helper, args, function(line)
            local event = decode(line)
            if not event or live ~= mine then return end
            if event.event == "start" then
                mine.conversation = event.conversation
                if conversation == nil and (not voice or fact.ai_open) then conversation = event.conversation end
                refresh_live()
            elseif event.event == "delta" then
                mine.text = mine.text .. (event.text or "")
                if live_here() then text.ai_live = mine.text end
            elseif event.event == "done" or event.event == "stopped" or event.event == "error" then
                finish(event)
            end
        end, function(_, code)
            if live == mine then finish({event = "error", message = "tohru-assistant exited with " .. tostring(code)}) end
        end, {input = prompt})
        return true
    end

    local function open()
        fact.ai_picker = false
        fact.ai_menu_open = false
        fact.ai_open = true
        probe(false)
        if fact.ai_ready and ready_for then
            fact.ai_view = 0
            if conversation ~= ready_for then load(ready_for) else seen() end
        else
            seen()
        end
    end
    local function close()
        fact.ai_open = false
        fact.ai_picker = false
        fact.ai_menu_open = false
        fact.ai_forget_armed = false
        if conversation and unremembered[conversation] and not (live and live.conversation == conversation) then
            remember(conversation)
        end
    end

    local function new_chat()
        leave(nil)
        conversation = nil
        show_turns({})
        fact.ai_view = 0
        fact.ai_picker = false
        text.ai_prompt = ""
        fact.ai_typed = false
        fail(nil)
        refresh_live()
    end

    local function list_history()
        search_serial = search_serial + 1
        local serial = search_serial
        local args = {"history", "--limit", "30"}
        fact.ai_searching = text.ai_search ~= ""
        if text.ai_search ~= "" then
            args[#args + 1] = "--query"
            args[#args + 1] = text.ai_search
        end
        run(helper, args, function(out, code)
            if serial ~= search_serial then return end
            local list = code == 0 and decode(out) or {}
            history = {}
            local rows, group = {}, nil
            for _, chat in ipairs(list) do
                if chat.group ~= group then
                    group = chat.group
                    rows[#rows + 1] = {header = true, group = group or "", title = "", when = "", id = ""}
                end
                rows[#rows + 1] = {id = chat.id, title = chat.title or "", when = chat.when or "", slot = chat.slot or 0,
                    voice = chat.voice == true, match = chat.match or "", matched = (chat.match or "") ~= "" and chat.match ~= chat.title,
                    group = group or ""}
                if #rows >= 40 then break end
            end
            history = rows
            model.ai_history = rows
            fact.ai_history_sel = -1
        end)
    end

    local memories = {}
    local function list_memory()
        fact.ai_memory_on = memory_on()
        fact.ai_forget_armed = false
        run(helper, {"memory", "list", "--json"}, function(out, code)
            local data = code == 0 and decode(out) or {}
            memories = data.memories or {}
            local rows = {}
            for i, m in ipairs(memories) do
                if i > 40 then break end
                local line = m.text or ""
                if #line > MEMORY_CHARS then
                    line = line:sub(1, MEMORY_CHARS):gsub("%s+%S*$", "") .. "…"
                end
                rows[#rows + 1] = {id = m.id, text = line, kind = m.kind or "fact"}
            end
            model.ai_memories = rows
            local state = data.relationship or {}
            if not fact.ai_memory_on then
                text.ai_memory_note = "Memory is off: chats are kept in history, nothing else is learned."
            elseif (state.conversations or 0) > 0 then
                text.ai_memory_note = string.format("%d remembered from %d chats. Say \"forget that\" in a chat to drop it.", #memories, state.conversations)
            else
                text.ai_memory_note = "Things Tohru learns about you appear here."
            end
        end)
    end

    local function pick(index)
        local row = history[index + 1]
        if not row or row.header then return end
        fact.ai_view = 0
        load(row.id, function(data)
            local provider = index_of(data.provider)
            if provider and providers[provider].signed_in then current = provider; publish_current() end
        end)
    end

    local function choose(index)
        local provider = providers[index + 1]
        if not provider then return end
        if not provider.signed_in then
            current = index + 1
            publish_current()
            fact.ai_picker = false
            return
        end
        current = index + 1
        publish_current()
        settings.choose(provider.id)
        fact.ai_picker = false
    end

    local function signin(index)
        local provider = providers[index + 1]
        if not provider then return end
        run(helper, {"signin", provider.id}, function(out, code)
            if code ~= 0 then fail("Could not open a terminal to sign in"); return end
            probe(true)
        end)
    end

    on("assistant_open", function() open() end)
    on("assistant_show_request", function()
        local id = text.ai_show_request or ""
        if id:match("^%x+$") then fact.ai_view = 0; load(id) end
    end)
    on("assistant_close", function() close() end)
    on("assistant_toggle", function() if fact.ai_open then close() else open() end end)
    on("assistant_picker", function()
        if fact.ai_view == 1 and not fact.ai_picker then fact.ai_view = 0 end
        fact.ai_picker = not fact.ai_picker
        if fact.ai_picker then probe(false) end
    end)
    on("assistant_choose", function(index) choose(index) end)
    on("assistant_signin", function(index) signin(index) end)
    on("assistant_memory", function()
        fact.ai_picker = false
        fact.ai_view = 2
        list_memory()
    end)
    on("assistant_memory_forget", function(index)
        local row = model.ai_memories and model.ai_memories[index + 1]
        if not row then return end
        run(helper, {"memory", "forget", row.id}, function() list_memory() end)
    end)
    on("assistant_memory_clear", function()
        if not fact.ai_forget_armed then
            fact.ai_forget_armed = true
            after(4000, function() fact.ai_forget_armed = false end)
            return
        end
        fact.ai_forget_armed = false
        run(helper, {"memory", "clear"}, function() list_memory() end)
    end)
    on("assistant_history", function()
        fact.ai_picker = false
        if fact.ai_view ~= 0 then fact.ai_view = 0; seen(); return end
        fact.ai_view = 1
        text.ai_search = ""
        list_history()
    end)
    on("assistant_new", function() new_chat() end)
    on("assistant_menu", function()
        local opening = not fact.ai_menu_open
        fact.ai_open = false
        fact.ai_picker = false
        fact.ai_menu_open = opening
        if opening then probe(false) end
    end)
    on("assistant_menu_close", function() fact.ai_menu_open = false end)
    on("assistant_show_history", function()
        open()
        fact.ai_view = 1
        text.ai_search = ""
        list_history()
    end)
    on("assistant_send", function()
        if live and live_here() then
            kill(live.id, "int")
            return
        end
        if send(text.ai_prompt, false) then text.ai_prompt = ""; fact.ai_typed = false end
    end)
    on("assistant_stop", function() if live then kill(live.id, "int") end end)
    on("assistant_copy", function()
        local last = shown_turns[#shown_turns]
        if not last or last.role ~= "assistant" then return end
        local ok = pcall(run, "wl-copy", {}, nil, {input = last.text, output = false})
        fact.ai_copied = ok
        if ok then after(1500, function() fact.ai_copied = false end) end
    end)
    on("assistant_retry", function()
        for i = #shown_turns, 1, -1 do
            if shown_turns[i].role == "user" then send(shown_turns[i].text, false); return end
        end
    end)
    on("assistant_pick", function(index) pick(index) end)
    on("assistant_search", function() if fact.ai_view == 1 then list_history() end end)
    on("text:ai_search", function() if fact.ai_view == 1 then list_history() end end)
    on("text:ai_prompt", function(value) fact.ai_typed = value ~= nil and value:match("%S") ~= nil end)
    on("assistant_move", function(delta)
        if fact.ai_picker then
            local count = #providers
            if count == 0 then return end
            local index = ((fact.ai_current < 0 and 0 or fact.ai_current) + delta) % count
            choose(index)
            fact.ai_picker = true
            return
        end
        local index = fact.ai_history_sel
        repeat index = index + delta until index < 0 or index >= #history or not history[index + 1].header
        if index >= 0 and index < #history then fact.ai_history_sel = index end
    end)
    on("assistant_activate", function()
        if fact.ai_picker then fact.ai_picker = false; return end
        if fact.ai_history_sel >= 0 then pick(fact.ai_history_sel) end
    end)

    probe(true)

    return {
        open = open,
        close = close,
        toggle = function() if fact.ai_open then close() else open() end end,
        new_chat = new_chat,
        show_history = function()
            open()
            fact.ai_view = 1
            text.ai_search = ""
            list_history()
        end,
        listening = function(on) fact.ai_listening = on == true end,
        remember = function(id) if id then remember(id) end end,
        choose = choose,
        signin = signin,
        settle = function() if #providers > 0 then settle() end end,
        -- Voice: one conversation per spell of conversation mode; answers go to the provider in use.
        ask = function(prompt, voice_conversation)
            if live then return false end
            local keep = conversation
            conversation = voice_conversation
            local ok = send(prompt, true)
            if not ok then conversation = keep end
            return ok, live
        end,
        stop = function() if live then kill(live.id, "int") end end,
        streaming = function() return live ~= nil end,
        on_answer = function(listener) answer_listeners[#answer_listeners + 1] = listener end,
        show = function(id) open(); fact.ai_view = 0; load(id) end,
        status = function()
            return {open = fact.ai_open, streaming = live ~= nil, ready = fact.ai_ready, conversation = conversation,
                provider = current and providers[current] and providers[current].id or ""}
        end,
    }
end
