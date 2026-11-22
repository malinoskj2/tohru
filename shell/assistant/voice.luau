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
    local barged, cancelling, queued, interrupted_spoken = false, false, nil, nil
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
        barged, cancelling, queued, interrupted_spoken = false, false, nil, nil
        if assistant.discard_draft then assistant.discard_draft() end
        assistant.listening(false)
        fact.ai_speaking = false
        fact.ai_mouth = 0
        fact.ai_voice_conversation = false
        puppet.calm()
        show_dragon(false)
    end

    local function ask_turn(prompt, opts)
        puppet.thinking()
        if not assistant.ask(prompt, conversation, opts) then
            if assistant.streaming and assistant.streaming() then
                queued = {text = prompt, opts = opts}
            else
                send({cmd = "say", conversation = conversation,
                    text = "I can't answer yet. Sign in to an assistant from the chat popup first."})
            end
        end
    end

    local function flush_queued()
        if queued then
            local next_turn = queued
            queued = nil
            ask_turn(next_turn.text, next_turn.opts)
        end
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
                if assistant.discard_draft then assistant.discard_draft() end
                interrupted_spoken = nil
                if conversation then assistant.remember(conversation) end
                conversation = nil
                puppet.calm()
                if fact.ai_dragon_open and not fact.ai_speaking then linger() end
            end
        elseif event == "hearing" then
            puppet.hearing(message.on == true)
        elseif event == "sentence" then
            puppet.sentence(tostring(message.text or ""))
        elseif event == "draft" then
            if assistant.draft and not cancelling then
                local opts = interrupted_spoken and {interrupted = interrupted_spoken} or nil
                if not assistant.draft(message.text, conversation, opts) then
                    send({cmd = "draft", ok = false})
                end
            else
                send({cmd = "draft", ok = false})
            end
        elseif event == "draft_cancel" then
            if assistant.discard_draft then assistant.discard_draft() end
        elseif event == "wake" then
            local pick = math.random(#GREETINGS - (greeted > 0 and 1 or 0))
            if greeted > 0 and pick >= greeted then pick = pick + 1 end
            greeted = pick
            send({cmd = "say", text = GREETINGS[greeted], conversation = conversation})
        elseif event == "turn" then
            puppet.thinking()
            local opts = interrupted_spoken and {interrupted = interrupted_spoken} or nil
            interrupted_spoken = nil
            if cancelling or (assistant.streaming and assistant.streaming()) then
                queued = {text = message.text, opts = opts}
            else
                queued = nil
                ask_turn(message.text, opts)
            end
        elseif event == "speaking" then
            local state = message.paused and "paused" or (message.interrupted and "interrupted" or nil)
            fact.ai_speaking = message.on == true and not message.paused
            puppet.speaking(fact.ai_speaking, state)
            if fact.ai_speaking then
                spoken = message.conversation or conversation
                show_dragon(true)
            else
                fact.ai_mouth = 0
                if state ~= "paused" and not fact.ai_voice_conversation then linger() end
            end
        elseif event == "mouth" then
            fact.ai_mouth = tonumber(message.level) or 0
            puppet.mouth(fact.ai_mouth, tonumber(message.viseme))
        elseif event == "barge" then
            if assistant.discard_draft then assistant.discard_draft() end
            if type(message.spoken) == "string" and message.spoken ~= "" then
                interrupted_spoken = message.spoken
            end
            if message.continuation then
                if assistant.streaming and assistant.streaming() then
                    if assistant.cancel then
                        cancelling = true
                        assistant.cancel(function(next_conv)
                            cancelling = false
                            conversation = next_conv
                            flush_queued()
                        end)
                    else
                        barged = true
                        assistant.stop()
                    end
                elseif conversation and assistant.rollback then
                    cancelling = true
                    assistant.rollback(conversation, function(deleted)
                        cancelling = false
                        if deleted then conversation = nil end
                        flush_queued()
                    end)
                end
            else
                if assistant.streaming and assistant.streaming() then barged = true end
                assistant.stop()
            end
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
        if not id then return end
        if not voice then
            if queued then flush_queued() end
            return
        end
        conversation = conv or conversation
        if barged then
            barged = false
            flush_queued()
            return
        end
        if kind == "done" then
            send({cmd = "say", text = answer or "", conversation = conversation})
        elseif kind == "error" then
            send({cmd = "say", text = "Hmm, that didn't work. The details are in the chat.", conversation = conversation})
        else
            send({cmd = "thinking", on = false})
        end
        flush_queued()
    end)

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
        puppet.reduced(value("assistant.reduced_motion", false) == true)
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
