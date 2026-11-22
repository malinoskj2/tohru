-- Runs tohru-voice only while assistant.voice is on: off means no process, no microphone and no models loaded.
return function(assistant, puppet)
    local id = nil
    local wanted = false
    local args = {}
    local conversation = nil
    local failures = 0
    local generation = 0
    -- The overlay stays a moment after she stops talking, and a click opens what she said.
    local DRAGON_LINGER = 5000
    local spoken, hide_serial = nil, 0
    local loaded = false
    local GREETINGS = {"Yes?", "I'm here.", "Mm? What is it?", "You called?", "Here I am. What do you need?", "Hello again."}
    local greeted = 0

    local function show_dragon(on)
        hide_serial = hide_serial + 1
        fact.ai_dragon_open = on
        puppet.visible(on)
    end

    -- She stays a moment after the last word, unless the conversation is still listening for more.
    local function linger()
        hide_serial = hide_serial + 1
        local mine = hide_serial
        after(DRAGON_LINGER, function()
            if mine == hide_serial and not fact.ai_speaking and not fact.ai_voice_conversation then show_dragon(false) end
        end)
    end

    local function send(message)
        if id then write(id, json.encode(message) .. "\n") end
    end

    local function quiet()
        assistant.listening(false)
        fact.ai_speaking = false
        fact.ai_mouth = 0
        fact.ai_voice_conversation = false
        puppet.calm()
        show_dragon(false)
    end

    local start

    local function voice_error(message)
        text.ai_voice_error = message
        fact.ai_voice_failed = message ~= ""
    end

    local function line(raw)
        local ok, message = pcall(json.decode, raw)
        if not ok or type(message) ~= "table" then return end
        local event = message.event
        if event == "ready" then
            failures = 0
            local art = tostring(message.dragon or "")
            -- The sheets load with the service, while she is hidden: a sheet arriving blanks her for a frame or two.
            if not loaded and art ~= "" then
                loaded = true
                puppet.load(art)
            end
            voice_error(tostring(message.warning or ""))
        elseif event == "listening" then
            assistant.listening(message.on == true)
        elseif event == "conversation" then
            fact.ai_voice_conversation = message.on == true
            if message.on then
                show_dragon(true)
            else
                if conversation then assistant.remember(conversation) end
                conversation = nil
                puppet.calm()
                if fact.ai_dragon_open and not fact.ai_speaking then linger() end
            end
        elseif event == "hearing" then
            puppet.hearing(message.on == true)
        elseif event == "sentence" then
            puppet.sentence(tostring(message.text or ""))
        elseif event == "wake" then
            local pick = math.random(#GREETINGS - (greeted > 0 and 1 or 0))
            if greeted > 0 and pick >= greeted then pick = pick + 1 end
            greeted = pick
            send({cmd = "say", text = GREETINGS[greeted], conversation = conversation})
        elseif event == "turn" then
            puppet.thinking()
            if not assistant.ask(message.text, conversation) then
                local busy = assistant.streaming and assistant.streaming()
                local unavailable = fact.ai_can_signin == false
                send({cmd = "say", conversation = conversation, text = busy and "One moment, I'm still on the last thing you asked."
                    or (unavailable and "I can't reach Orca right now. Open Orca and try again."
                    or "I can't answer yet. Sign in to an assistant from the chat popup first.")})
            end
        elseif event == "speaking" then
            fact.ai_speaking = message.on == true
            puppet.speaking(message.on == true)
            if message.on then
                spoken = message.conversation or conversation
                show_dragon(true)
            else
                fact.ai_mouth = 0
                if not fact.ai_voice_conversation then linger() end
            end
        elseif event == "mouth" then
            fact.ai_mouth = tonumber(message.level) or 0
            puppet.mouth(fact.ai_mouth, tonumber(message.viseme))
        elseif event == "barge" then
            assistant.stop()
        elseif event == "paused" then
            fact.ai_voice_paused = (message.reason or "") ~= ""
        elseif event == "error" then
            voice_error(tostring(message.message or ""))
        end
    end

    function start()
        generation = generation + 1
        local mine = generation
        id = spawn("tohru-voice", args, line, function(_, code)
            if mine ~= generation then return end
            id = nil
            quiet()
            if not wanted or code == 3 then return end
            failures = failures + 1
            if failures <= 5 then
                after(2000 * failures, function() if wanted and not id and mine == generation then start() end end)
            end
        end, {stdin = "open"})
    end

    assistant.on_answer(function(kind, conv, answer, voice)
        if not voice or not id then return end
        conversation = conv or conversation
        if kind == "done" then
            send({cmd = "say", text = answer or "", conversation = conversation})
        elseif kind == "error" then
            send({cmd = "say", text = "Hmm, that didn't work. The details are in the chat.", conversation = conversation})
        else
            send({cmd = "thinking", on = false})
        end
    end)

    if assistant.on_alert then
        assistant.on_alert(function(chat_id, msg, speak)
            if not speak or not id or not msg or msg == "" then return end
            if fact.ai_voice_paused or fact.notices_dnd or fact.ai_speaking or fact.ai_voice_conversation then return end
            send({cmd = "say", text = msg, conversation = (chat_id and chat_id ~= "") and chat_id or conversation})
        end)
    end

    on("assistant_hush", function() send({cmd = "hush"}) end)
    on("assistant_dragon", function()
        show_dragon(false)
        if spoken then assistant.show(spoken) end
    end)

    local function configure(settings)
        local function value(key, default)
            local row = settings[key]
            if row == nil or row.value == nil then return default end
            return row.value
        end
        local tts = {
            engine = value("assistant.tts_engine", "kokoro"),
            model = value("assistant.tts_model", ""),
            voicepack = value("assistant.tts_voicepack", "tide"),
            voice = value("assistant.tts_voice", "en-us+f3"),
            speed = value("assistant.tts_speed", 100),
            pitch = value("assistant.tts_pitch", 3),
            load_on_conversation = value("assistant.tts_load_on_conversation", false) == true,
        }
        local next = {
            "--vad-model", value("assistant.vad_model", ""), "--asr-model", value("assistant.asr_model", ""),
            "--turn-model", value("assistant.turn_model", ""),
            "--wake", value("assistant.voice_wake", "asr"), "--kws-model", value("assistant.kws_model", ""),
            "--tts-engine", tts.engine, "--tts-model", tts.model, "--tts-voicepack", tts.voicepack,
            "--tts-voice", tts.voice, "--tts-speed", tostring(tts.speed), "--tts-pitch", tostring(tts.pitch),
        }
        if tts.load_on_conversation then next[#next + 1] = "--tts-load-on-conversation" end
        local on = value("assistant.voice", false) == true
        local models_changed = false
        for i = 1, 10 do if next[i] ~= args[i] then models_changed = true end end
        local tts_changed = #next ~= #args
        for i = 11, #next do if next[i] ~= args[i] then tts_changed = true end end
        args = next
        fact.ai_voice_enabled = on
        if not on then
            wanted = false
            generation = generation + 1
            if id then kill(id, "term") end
            id = nil
            quiet()
            voice_error("")
            if loaded then
                loaded = false
                puppet.load("")
            end
            return
        end
        wanted = true
        if id and models_changed then
            generation = generation + 1
            kill(id, "term")
            id = nil
            quiet()
        end
        if not id then
            failures = 0
            start()
        elseif tts_changed then
            tts.cmd = "tts"
            send(tts)
        end
    end

    return {
        configure = configure,
        running = function() return id ~= nil end,
        hush = function() send({cmd = "hush"}) end,
    }
end
