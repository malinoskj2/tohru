-- Runs tohru-voice only while assistant.voice is on: off means no process, no microphone and no models loaded.
return function(assistant, puppet, flight, service_executable)
    local voice_return = require("assistant/voice-return")()
    local id = nil
    local presence
    local wanted = false
    local args = {}
    local conversation = nil
    local failures = 0
    local generation = 0
    local capture_id = nil
    local backend_signature = nil
    local transcript_serial = 0
    local turn_generation, streaming_speech, streamed_text = 0, false, false
    -- The overlay stays a moment after she stops talking, and a click opens what she said.
    local DRAGON_LINGER = 5000
    local spoken, hide_serial = nil, 0
    local loaded = false
    local hearing = false
    local barged, cancelling, queued, interrupted_spoken = false, false, nil, nil
    local present = false
    local profiles_signature = nil
    local GREETINGS = {"Yes?", "I'm here.", "Mm? What is it?", "You called?", "Here I am. What do you need?", "Hello again."}
    local greeted = 0
    local greeting_serial = 0
    local send
    local notice_reply, flush_notice_reply
    local task_clock = 0
    local waiting_chatter = false
    local task_speech = require("assistant/task-speech")(function() return task_clock end,
        function(message, chat)
            turn_generation = turn_generation + 1
            send({cmd = "speech_start", generation = turn_generation, conversation = chat, background = true})
            send({cmd = "speech_end", generation = turn_generation, text = message})
            return turn_generation
        end,
        function()
            turn_generation = turn_generation + 1
            send({cmd = "hush"})
            fact.ai_speaking, fact.ai_mouth = false, 0
            puppet.speaking(false, "interrupted")
        end)

    local function task_visible(chat)
        local status = assistant.status and assistant.status() or {}
        return id ~= nil and not fact.ai_voice_paused and not fact.notices_dnd
            and (fact.ai_dragon_open or status.open) == true
            and (not status.open or status.conversation == (chat or conversation))
    end
    every(1000, function()
        task_clock = task_clock + 1000
        task_speech.tick(task_visible(), hearing or fact.ai_speaking
            or (assistant.streaming and assistant.streaming()))
        if flush_notice_reply then flush_notice_reply() end
    end)

    -- The service picks its voice profile when she comes onto the screen: the dragon or the popup.
    function presence(force)
        local now = fact.ai_dragon_open == true or fact.ai_open == true
        if now ~= present or force then
            present = now
            send({cmd = "presence", visible = now})
        end
    end

    local function show_dragon(on)
        if not on then notice_reply = nil end
        if not on then task_speech.interrupt() end
        hide_serial = hide_serial + 1
        if not on then greeting_serial = greeting_serial + 1 end
        if flight then
            if on and not fact.ai_dragon_open then flight.show()
            elseif not on then flight.hide() end
        end
        fact.ai_dragon_open = on
        local mine = hide_serial
        local requested_capture = capture_id
        send({cmd = "diagnostic", stage = "ui_visibility_request", turn_id = requested_capture, visible = on, ui_sequence = mine})
        after(16, function()
            if mine == hide_serial then
                send({cmd = "diagnostic", stage = "ui_state_observed", turn_id = requested_capture,
                    visible = fact.ai_dragon_open, ui_sequence = mine, flight_alpha = fact.ai_flight_alpha or 1})
            end
        end)
        puppet.visible(on)
        presence()
    end

    -- She stays a moment after the last word, unless the conversation is still listening for more.
    local function linger()
        hide_serial = hide_serial + 1
        local mine = hide_serial
        after(DRAGON_LINGER, function()
            if mine == hide_serial and not fact.ai_speaking and not fact.ai_voice_conversation then show_dragon(false) end
        end)
    end

    send = function(message)
        if id then write(id, json.encode(message) .. "\n") end
    end

    flush_notice_reply = function()
        local reply = notice_reply
        if not reply then return end
        if reply.generation ~= turn_generation then notice_reply = nil; return end
        if not reply.done or not reply.ack_ended or hearing or fact.ai_speaking then return end
        if not task_visible(reply.chat) then notice_reply = nil; return end
        notice_reply = nil
        streaming_speech, streamed_text = false, false
        send({cmd = "speech_start", generation = turn_generation, conversation = reply.chat, background = true})
        send({cmd = "speech_end", generation = turn_generation, text = reply.text})
    end

    local function quiet()
        notice_reply = nil
        hearing = false
        task_speech.interrupt()
        voice_return.interrupt()
        barged, cancelling, queued, interrupted_spoken = false, false, nil, nil
        turn_generation = turn_generation + 1
        streaming_speech = false
        text.ai_transcript = ""
        transcript_serial = transcript_serial + 1
        if assistant.discard_draft then assistant.discard_draft() end
        assistant.listening(false)
        fact.ai_speaking = false
        fact.ai_mouth = 0
        fact.ai_voice_conversation = false
        puppet.calm()
        show_dragon(false)
    end

    local function ask_turn(prompt, opts)
        task_speech.interrupt()
        voice_return.interrupt()
        turn_generation = turn_generation + 1
        streaming_speech = false
        barged = false
        opts = opts or {}
        opts.voice_generation = turn_generation
        capture_id = opts.capture_id
        send({cmd = "diagnostic", stage = "assistant_request", turn_id = capture_id, generation = turn_generation})
        puppet.thinking()
        if not assistant.ask(prompt, conversation, opts) then
            if assistant.streaming and assistant.streaming() then
                queued = {text = prompt, opts = opts}
            else
                task_speech.interrupt()
                local unavailable = fact.ai_can_signin == false
                send({cmd = "say", conversation = conversation,
                    text = unavailable and "I can't reach Orca at the moment."
                        or "I can't think straight yet. Could you sign me in from the chat popup?"})
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
        if message.turn_id then capture_id = message.turn_id end
        if event == "ready" then
            failures = 0
            send({cmd = "diagnostic", stage = "waiting_config", enabled = waiting_chatter})
            local art = tostring(message.dragon or "")
            -- The sheets load with the service, while she is hidden: a sheet arriving blanks her for a frame or two.
            if not loaded and art ~= "" then
                loaded = true
                puppet.load(art)
            end
            voice_error(tostring(message.warning or ""))
        elseif event == "transcript" then
            transcript_serial = transcript_serial + 1
            local mine = transcript_serial
            text.ai_transcript = tostring(message.text or "")
            if message.final then
                after(5000, function() if transcript_serial == mine then text.ai_transcript = "" end end)
            end
        elseif event == "listening" then
            if not message.on then hearing = false end
            text.ai_voice_source = tostring(message.source or "default")
            text.ai_voice_aec = tostring(message.aec or "unknown")
            assistant.listening(message.on == true)
        elseif event == "conversation" then
            fact.ai_voice_conversation = message.on == true
            if message.on then
                show_dragon(true)
            else
                if assistant.discard_draft then assistant.discard_draft() end
                task_speech.interrupt()
                interrupted_spoken = nil
                if conversation then assistant.remember(conversation) end
                conversation = nil
                text.ai_transcript = ""
                turn_generation = turn_generation + 1
                send({cmd = "hush"})
                puppet.calm()
                if fact.ai_dragon_open and not fact.ai_speaking then linger() end
            end
        elseif event == "hearing" then
            hearing = message.on == true
            if message.on then task_speech.silence() end
            if message.on then greeting_serial = greeting_serial + 1 end
            if not message.on then
                local mine = transcript_serial
                after(500, function() if transcript_serial == mine then text.ai_transcript = "" end end)
            end
            if flight then flight.hearing(message.on == true) end
            puppet.hearing(message.on == true)
            if not hearing then flush_notice_reply() end
        elseif event == "sentence" then
            puppet.sentence(tostring(message.text or ""))
        elseif event == "draft" then
            if assistant.draft and not cancelling then
                local opts = interrupted_spoken and {interrupted = interrupted_spoken} or {}
                opts.semantic = message.semantic == true
                opts.on_semantic = function(decision)
                    send({cmd = "semantic", token = message.token, text = message.text, decision = decision})
                end
                if not assistant.draft(message.text, conversation, opts) then
                    if opts.semantic then opts.on_semantic("unsupported") end
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
            greeting_serial = greeting_serial + 1
            local mine, service_generation = greeting_serial, generation
            local greeting = GREETINGS[pick]
            local function greet()
                if mine ~= greeting_serial or service_generation ~= generation or not fact.ai_dragon_open then return end
                if flight and (fact.ai_flying or (fact.ai_flight_alpha or 1) < 1) then
                    after(50, greet)
                    return
                end
                send({cmd = "say", text = greeting, conversation = conversation})
            end
            greet()
        elseif event == "turn" then
            task_speech.interrupt()
            greeting_serial = greeting_serial + 1
            puppet.thinking()
            local opts = interrupted_spoken and {interrupted = interrupted_spoken} or {}
            opts.capture_id = message.turn_id
            interrupted_spoken = nil
            if cancelling or (assistant.streaming and assistant.streaming()) then
                queued = {text = message.text, opts = opts}
            else
                queued = nil
                ask_turn(message.text, opts)
            end
        elseif event == "speaking" then
            if message.generation and message.generation ~= turn_generation then return end
            if not message.on and not message.paused then task_speech.finished(message.generation) end
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
            if notice_reply and not message.on and (message.generation == nil or message.generation == notice_reply.generation) then
                if state then notice_reply = nil
                else notice_reply.ack_ended = true; flush_notice_reply() end
            end
        elseif event == "mouth" then
            fact.ai_mouth = tonumber(message.level) or 0
            puppet.mouth(fact.ai_mouth, tonumber(message.viseme))
        elseif event == "barge" then
            notice_reply = nil
            task_speech.silence()
            voice_return.interrupt()
            turn_generation = turn_generation + 1
            streaming_speech = false
            send({cmd = "hush"})
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
        id = spawn(service_executable or "tohru-voice", args, function(raw) if mine == generation then line(raw) end end, function(_, code)
            if mine ~= generation then return end
            id = nil
            quiet()
            if not wanted or code == 3 then return end
            failures = failures + 1
            if failures <= 5 then
                after(2000 * failures, function() if wanted and not id and mine == generation then start() end end)
            end
        end, {stdin = "open", stderr = "inherit"})
        if id and (fact.ai_dragon_open == true or fact.ai_open == true) then presence(true) end
    end

    local function answer_event(kind, conv, answer, voice, turn, request)
        if not id then return end
        if not voice then
            if kind == "start" then notice_reply = nil end
            if kind == "start" then task_speech.interrupt() end
            if kind == "start" then voice_return.interrupt() end
            if queued then flush_queued() end
            return
        end
        if turn and turn ~= turn_generation then
            if kind ~= "start" and kind ~= "delta" and queued then flush_queued() end
            return
        end
        conversation = conv or conversation
        if barged then
            barged = false
            flush_queued()
            return
        end
        if kind == "origin" then
            voice_return.begin(conv, request)
            task_speech.begin(conv, request)
            send({cmd = "diagnostic", stage = "assistant_origin", request = request,
                canonical_request = answer, generation = turn_generation, turn_id = capture_id})
            if assistant.voice_ready and request and request ~= "" then assistant.voice_ready(conv, request) end
            return
        elseif kind == "start" then
            voice_return.begin(conv, request)
            task_speech.begin(conv, request)
            if not notice_reply or notice_reply.generation ~= turn_generation then
                notice_reply = nil
                streaming_speech = true
                streamed_text = false
                send({cmd = "speech_start", generation = turn_generation, conversation = conversation, turn_id = capture_id, request = request})
            end
            if assistant.voice_ready and request and request ~= "" then assistant.voice_ready(conv, request) end
            return
        elseif kind == "notice" then
            -- Acknowledge now and hold the substantive answer until the owner
            -- and the acknowledgement have both finished speaking.
            if answer and answer ~= "" and not notice_reply then
                notice_reply = {generation = turn_generation, chat = conversation, text = "", done = false, ack_ended = false}
                send({cmd = "speech_end", generation = turn_generation, text = answer})
                streaming_speech, streamed_text = false, false
            end
            return
        elseif kind == "delta" then
            task_speech.silence()
            if notice_reply and notice_reply.generation == turn_generation then
                notice_reply.text = notice_reply.text .. (answer or "")
                return
            end
            if not streaming_speech then
                send({cmd = "speech_start", generation = turn_generation, conversation = conversation, turn_id = capture_id})
                streaming_speech, streamed_text = true, false
            end
            if streaming_speech and answer and answer ~= "" then
                streamed_text = true
                send({cmd = "speech_delta", generation = turn_generation, text = answer})
            end
            return
        elseif kind == "done" then
            task_speech.silence()
            if notice_reply and notice_reply.generation == turn_generation then
                notice_reply.text = answer or notice_reply.text
                notice_reply.done = true
                flush_notice_reply()
                flush_queued()
                return
            end
            if streaming_speech then
                send({cmd = "speech_end", generation = turn_generation, text = not streamed_text and (answer or "") or nil})
            else
                send({cmd = "say", text = answer or "", conversation = conversation, generation = turn_generation, turn_id = capture_id})
            end
        elseif kind == "error" then
            notice_reply = nil
            task_speech.silence()
            send({cmd = "say", text = answer ~= nil and answer ~= "" and answer or "Sorry, that went wrong on my end.", conversation = conversation})
        else
            notice_reply = nil
            send({cmd = "hush"})
            send({cmd = "thinking", on = false})
        end
        flush_queued()
    end
    if assistant.on_visibility then assistant.on_visibility(function() presence() end) end
    assistant.on_answer(answer_event)
    if assistant.on_speech then assistant.on_speech(answer_event) end

    if assistant.on_alert then
        local ALERTS_KEPT = 3
        local held, waiting = {}, false
        local function speak_alert(chat_id, msg)
            turn_generation = turn_generation + 1
            streaming_speech, streamed_text = false, false
            send({cmd = "speech_start", generation = turn_generation, conversation = (chat_id and chat_id ~= "") and chat_id or conversation})
            send({cmd = "speech_end", generation = turn_generation, text = msg})
        end
        local function hushed() return fact.ai_voice_paused or fact.notices_dnd end
        local function talking()
            return fact.ai_speaking or fact.ai_voice_conversation or (assistant.streaming and assistant.streaming())
        end
        local function release()
            waiting = false
            if not id or hushed() then held = {}; return end
            if talking() then
                waiting = true
                after(1000, release)
                return
            end
            local alert = table.remove(held, 1)
            if alert then speak_alert(alert.chat, alert.text) end
            if #held > 0 then
                waiting = true
                after(1000, release)
            end
        end
        assistant.on_alert(function(chat_id, msg, speak)
            if not speak or not id or not msg or msg == "" then return end
            if hushed() then return end
            if talking() or #held > 0 then
                if #held >= ALERTS_KEPT then table.remove(held, 1) end
                held[#held + 1] = {chat = chat_id, text = msg}
                if not waiting then
                    waiting = true
                    after(1000, release)
                end
                return
            end
            speak_alert(chat_id, msg)
        end)
    end

    on("assistant_voice_result", function()
        local status = assistant.status and assistant.status() or {}
        local visible = (fact.ai_dragon_open or status.open) and (not status.open or status.conversation == text.ai_voice_result_chat)
        local allowed, reason = voice_return.take(text.ai_voice_result_chat, text.ai_voice_result_request,
            text.ai_voice_result_completion, visible == true,
            fact.ai_voice_paused or fact.notices_dnd or fact.ai_speaking or hearing
                or (assistant.streaming and assistant.streaming()))
        send({cmd = "diagnostic", stage = allowed and "continuation_speech" or "continuation_dropped",
            request = text.ai_voice_result_request, completion = text.ai_voice_result_completion, visible = visible == true, reason = reason})
        if not allowed then return end
        task_speech.interrupt()
        local result = text.ai_voice_result_text or ""
        if result == "" then return end
        turn_generation = turn_generation + 1
        streaming_speech, streamed_text = false, false
        send({cmd = "speech_start", generation = turn_generation, conversation = text.ai_voice_result_chat})
        send({cmd = "speech_end", generation = turn_generation, text = result})
    end)
    on("assistant_voice_task", function()
        local event = {chat = text.ai_voice_task_chat, request = text.ai_voice_task_request,
            task = text.ai_voice_task_id, sequence = tonumber(text.ai_voice_task_sequence),
            kind = text.ai_voice_task_kind, text = text.ai_voice_task_text}
        -- Admission retires the matching direct reply even when the owner has
        -- the floor or the desktop is hidden. Audio eligibility is a later gate.
        if event.kind == "ack" and assistant.detach_voice and assistant.detach_voice(event.request) then
            notice_reply = nil
            send({cmd = "task_accepted", request = event.request, generation = turn_generation, turn_id = capture_id})
            send({cmd = "hush"})
            voice_return.interrupt()
            streaming_speech, streamed_text = false, false
            if queued then
                send({cmd = "diagnostic", stage = "task_update_dropped", task = event.task,
                    sequence = event.sequence, kind = event.kind, reason = "busy", request = event.request,
                    generation = turn_generation, turn_id = capture_id})
                flush_queued()
                return
            end
        end
        local accepted, reason
        if not task_visible(event.chat) then
            task_speech.interrupt()
            accepted, reason = false, "hidden"
        elseif hearing then
            task_speech.silence()
            task_speech.update(event, true)
            accepted, reason = false, "hearing"
        else
            accepted, reason = task_speech.update(event)
        end
        send({cmd = "diagnostic", stage = accepted and "task_update" or "task_update_dropped",
            task = event.task, sequence = event.sequence, kind = event.kind, reason = reason,
            request = event.request, generation = turn_generation, turn_id = capture_id})
        if not accepted then return end
        if event.kind == "ack" then
            -- The bot owns the asynchronous task; retire only the desktop poller.
            -- Its final answer remains in shared history, not a second speech path.
            voice_return.interrupt()
            streaming_speech, streamed_text = false, false
        end
        task_speech.tick(true, hearing or (fact.ai_speaking and event.kind ~= "done" and event.kind ~= "failed"))
    end)
    on("assistant_hush", function() task_speech.interrupt(); voice_return.interrupt(); turn_generation = turn_generation + 1; send({cmd = "hush"}) end)
    on("assistant_dragon", function()
        task_speech.interrupt()
        voice_return.interrupt()
        if not fact.ai_dragon_open then return end
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
        local chatter = value("assistant.waiting_chatter", false) == true
        if chatter ~= waiting_chatter then send({cmd = "diagnostic", stage = "waiting_config", enabled = chatter}) end
        waiting_chatter = chatter
        task_speech.configure(waiting_chatter, value("assistant.waiting_seed", 1))
        if flight then flight.configure(value) end
        local backend = table.concat({
            tostring(value("assistant.backend", "local")), tostring(value("assistant.orca_bot", "Tohru")),
            tostring(value("assistant.orca_voice_chat", "foreground")), tostring(value("assistant.orca_offline", "error")),
        }, "|")
        if backend_signature and backend_signature ~= backend then
            task_speech.interrupt()
            turn_generation = turn_generation + 1
            streaming_speech, streamed_text = false, false
            conversation, queued, interrupted_spoken, capture_id = nil, nil, nil, nil
            barged, cancelling = false, false
            transcript_serial = transcript_serial + 1
            text.ai_transcript = ""
            send({cmd = "hush"})
            send({cmd = "end"})
            if assistant.stop then assistant.stop() end
            show_dragon(false)
        end
        backend_signature = backend
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
        next[#next + 1] = "--source"
        next[#next + 1] = value("assistant.voice_source", "")
        local profile_keys = {
            "vram_threshold_percent", "hidden_unload_seconds", "queue_timeout_seconds", "model_root",
            "low.device", "low.precision", "high.enabled", "high.device", "high.precision", "high.python",
            "high.tts_model", "high.tts_speaker", "high.asr_model", "high.vram_mib",
        }
        local profiles, signature = {low = {}, high = {}}, {}
        for _, key in ipairs(profile_keys) do
            local v = value("assistant.voice_profiles." .. key, nil)
            signature[#signature + 1] = tostring(v)
            if v ~= nil then
                local group, field = string.match(key, "^(%a+)%.(.+)$")
                if group then profiles[group][field] = v else profiles[key] = v end
            end
        end
        signature = table.concat(signature, "|")
        next[#next + 1] = "--profiles"
        next[#next + 1] = json.encode(profiles)
        if tts.load_on_conversation then next[#next + 1] = "--tts-load-on-conversation" end
        local on = value("assistant.voice", false) == true
        local models_changed = false
        for i = 1, 10 do if next[i] ~= args[i] then models_changed = true end end
        local tts_changed = #next ~= #args
        for i = 11, 24 do if next[i] ~= args[i] then tts_changed = true end end
        if next[24] ~= args[24] or signature ~= profiles_signature then models_changed = true end
        profiles_signature = signature
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
        hush = function() task_speech.interrupt(); send({cmd = "hush"}) end,
    }
end
