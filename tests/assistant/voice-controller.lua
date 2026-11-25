package.path = "shell/?.luau;" .. package.path
local encoded = {__concat = function(a) return a end}
local json = {encode = function(value) return setmetatable({value = value}, encoded) end, decode = function(value) return value end}
_G.json = json
local events, spawned, killed, written, timers = {}, {}, {}, {}, {}
_G.fact = {}
_G.text = {}
_G.on = function(name, handler) events[name] = handler end
_G.after = function(seconds, f)
    if seconds == 16 then f() else timers[#timers + 1] = f end
end
local ticking = {}
_G.every = function(ms, f) ticking[#ticking + 1] = f; return #ticking end
_G.cancel = function(t) ticking[t] = nil end
_G.kill = function(id, how) killed[#killed + 1] = {id, how} end
local diagnostics, presences = {}, {}
_G.write = function(id, line)
    if line.value.cmd == "diagnostic" then diagnostics[#diagnostics + 1] = line.value
    elseif line.value.cmd == "presence" then presences[#presences + 1] = line.value.visible
    else written[#written + 1] = {id = id, message = line.value} end
end
_G.spawn = function(command, args, line, exit, how)
    spawned[#spawned + 1] = {command = command, args = args, line = line, exit = exit, how = how}
    return #spawned
end

local asked, remembered, stopped, listeners, alert_listeners, visibility_listeners = {}, {}, 0, {}, {}, {}
local drafted, discarded, cancelled, rolled_back = {}, 0, 0, {}
local refusing = false
local draft_ok = true
local shown = nil
local assistant = {
    listening = function(on) fact.ai_listening = on end,
    ask = function(prompt, conversation, opts) asked[#asked + 1] = {prompt, conversation, opts}; return not refusing end,
    draft = function(prompt, conversation, opts) drafted[#drafted + 1] = {prompt, conversation, opts}; return draft_ok end,
    discard_draft = function() discarded = discarded + 1 end,
    streaming = function() return refusing == "busy" end,
    stop = function() stopped = stopped + 1 end,
    cancel = function(on_done) cancelled = cancelled + 1; if on_done then on_done(nil, true) end end,
    rollback = function(cid, on_done) rolled_back[#rolled_back + 1] = cid; if on_done then on_done(true) end end,
    remember = function(id) remembered[#remembered + 1] = id end,
    show = function(id) shown = id end,
    on_answer = function(listener) listeners[#listeners + 1] = listener end,
    on_alert = function(listener) alert_listeners[#alert_listeners + 1] = listener end,
    on_visibility = function(listener) visibility_listeners[#visibility_listeners + 1] = listener end,
}
local speech_listener
assistant.on_speech = function(listener) speech_listener = listener end
local puppet = assert(loadfile("shell/assistant/puppet.luau"))()()
local voice = assert(loadfile("shell/assistant/voice.luau"))()(assistant, puppet)
local function settings(values)
    local rows = {}
    for key, value in pairs(values) do rows[key] = {value = value} end
    return rows
end
local function has(list, a, b)
    for i = 1, #list - 1 do if list[i] == a and list[i + 1] == b then return true end end
    return false
end

-- Disabled means no process at all.
voice.configure(settings({["assistant.voice"] = false}))
assert(#spawned == 0 and not voice.running() and not fact.ai_voice_enabled)

-- Enabling starts one service with stdin open and the speech settings.
voice.configure(settings({["assistant.voice"] = true, ["assistant.tts_voicepack"] = "tide", ["assistant.vad_model"] = "/v.onnx", ["assistant.asr_model"] = "/asr"}))
assert(#spawned == 1 and spawned[1].command == "tohru-voice" and spawned[1].how.stdin == "open")
assert(has(spawned[1].args, "--vad-model", "/v.onnx") and has(spawned[1].args, "--asr-model", "/asr") and has(spawned[1].args, "--turn-model", "") and has(spawned[1].args, "--tts-engine", "kokoro") and has(spawned[1].args, "--tts-voicepack", "tide"))
for _, a in ipairs(spawned[1].args) do assert(a ~= "--tts-load-on-conversation") end
local service = spawned[1]
service.line({event = "ready", warning = "speech: assistant.tts_model is not set; using espeak-ng", dragon = "/art"})
-- The sheets are set once the service is ready and drawn unseen for a moment, so they are loaded before she shows.
assert(fact.ai_dg_warm and #timers == 1)
timers[1]()
assert(not fact.ai_dg_warm)
timers = {}
assert(text.ai_voice_error == "speech: assistant.tts_model is not set; using espeak-ng" and fact.ai_voice_failed)
service.line({event = "listening", on = true})
assert(fact.ai_listening)

-- A turn goes to the assistant; its answer is spoken in the same conversation.
service.line({event = "conversation", on = true})
service.line({event = "turn", text = "What time is it", turn_id = "capture-1"})
assert(asked[1][1] == "What time is it" and asked[1][2] == nil and asked[1][3].capture_id == "capture-1")
local settled = {}
assistant.settled = function(turn_id) settled[#settled + 1] = turn_id end
service.line({event = "settled", turn_id = "capture-1"})
assert(#settled == 1 and settled[1] == "capture-1", "the settle signal reaches the turn's helper")
listeners[1]("done", "v1", "Half past two.", true)
assert(written[1].message.cmd == "say" and written[1].message.text == "Half past two." and written[1].message.conversation == "v1")
listeners[1]("done", "c9", "Typed answer", false)
assert(#written == 1)
-- Failures are spoken in Tohru's own words when the helper voiced them, with a short fallback otherwise.
listeners[1]("error", "v1", "I tripped over that model name.", true)
assert(written[#written].message.cmd == "say" and written[#written].message.text == "I tripped over that model name.")
listeners[1]("error", "v1", "", true)
assert(written[#written].message.text == "Sorry, that went wrong on my end.")
table.remove(written); table.remove(written)
service.line({event = "turn", text = "And tomorrow"})
assert(asked[2][2] == "v1")
service.line({event = "speaking", on = true, conversation = "v1"})
assert(fact.ai_dragon_open and fact.ai_speaking and #ticking == 2)
service.line({event = "mouth", level = 0.6, viseme = 5})
assert(fact.ai_mouth == 0.6)
-- In a conversation she stays on screen, listening for the next turn; a click opens what she said.
service.line({event = "speaking", on = false, conversation = "v1"})
assert(fact.ai_mouth == 0 and fact.ai_dragon_open and #timers == 0)
events.assistant_dragon()
assert(shown == "v1" and not fact.ai_dragon_open and ticking[1] and ticking[2] == nil)
service.line({event = "barge"})
assert(stopped == 1)

-- Pausing speech for barge-in closes the mouth and moves her to listening without lingering; resuming talks again.
service.line({event = "conversation", on = true})
service.line({event = "speaking", on = true, conversation = "v1"})
service.line({event = "mouth", level = 0.5, viseme = 3})
service.line({event = "speaking", on = false, paused = true, conversation = "v1"})
assert(not fact.ai_speaking and fact.ai_mouth == 0 and fact.ai_dragon_open and #timers == 0)
local _, target = puppet.pose()
assert(target == "listening")
service.line({event = "speaking", on = true, conversation = "v1"})
_, target = puppet.pose()
assert(fact.ai_speaking and target == "explain")
-- Confirming barge leaves her listening/thinking instead of switching to pleased when interrupted speaking ends.
service.line({event = "speaking", on = false, paused = true, conversation = "v1"})
service.line({event = "barge"})
service.line({event = "speaking", on = false, interrupted = true, conversation = "v1"})
_, target = puppet.pose()
assert(target == "listening")

-- Interrupting while thinking cancels the pending answer, swallows its stale reply and sends the replacement turn.
local streaming_now = true
local real_streaming = assistant.streaming
local real_cancel = assistant.cancel
assistant.streaming = function() return streaming_now end
local cancel_cb = nil
assistant.cancel = function(cb) cancelled = cancelled + 1; cancel_cb = cb end
local asked_before, written_before = #asked, #written
service.line({event = "barge", continuation = true})
assert(cancelled == 1)
service.line({event = "turn", text = "Tell me about the history of Rome"})
assert(#asked == asked_before and #written == written_before + 1 and written[#written].message.cmd == "hush")
streaming_now = false
cancel_cb("v1", false)
assert(#written == written_before + 1)
assert(#asked == asked_before + 1 and asked[#asked][1] == "Tell me about the history of Rome")
listeners[1]("done", "v1", "Rome started in 753 BC.", true)
assert(written[#written].message.cmd == "say" and written[#written].message.text == "Rome started in 753 BC.")

-- Barge with spoken progress passes --interrupted context on the next draft/turn.
service.line({event = "barge", spoken = "Rome started in…"})
service.line({event = "draft", text = "Wait what about Greece instead"})
assert(drafted[#drafted][1] == "Wait what about Greece instead" and drafted[#drafted][3].interrupted == "Rome started in…")
service.line({event = "draft_cancel"})
assert(discarded > 0)
service.line({event = "turn", text = "Wait what about Athens instead"})
assert(asked[#asked][1] == "Wait what about Athens instead" and asked[#asked][3].interrupted == "Rome started in…")
-- Unsupported draft notifies tohru-voice so it waits the full silence window.
draft_ok = false
service.line({event = "draft", text = "Tell me another quick fact"})
assert(written[#written].message.cmd == "draft" and written[#written].message.ok == false)
draft_ok = true
assistant.streaming = real_streaming
assistant.cancel = real_cancel

-- The wake word alone gets a short spoken reply, never the same one twice running.
local greetings = {}
for i = 1, 12 do
    service.line({event = "wake"})
    local said = written[#written].message
    assert(said.cmd == "say" and said.text ~= "" and said.text ~= greetings[i - 1])
    greetings[i] = said.text
end
-- A turn the assistant cannot take when signed out is answered aloud; when streaming it queues silently without the busy line.
refusing = true
service.line({event = "turn", text = "Are you there"})
assert(written[#written].message.cmd == "say" and written[#written].message.text:find("sign me in"))
local w_count = #written
refusing = "busy"
service.line({event = "turn", text = "Are you there"})
assert(#written == w_count)
refusing = false
listeners[1]("done", "c9", "Typed answer", false)
assert(asked[#asked][1] == "Are you there")

-- Completed committed sentences stream, final does not repeat the full answer.
service.line({event = "turn", text = "Stream this answer"})
local turn = asked[#asked][3].voice_generation
speech_listener("start", "v1", "", true, turn)
assert(written[#written].message.cmd == "speech_start")
local before_notice_timers = #timers
speech_listener("notice", "v1", "I've queued that for Home.", true, turn)
assert(#timers == before_notice_timers)
assert(written[#written].message.cmd == "speech_end" and written[#written].message.text == "I've queued that for Home.")
local before_answer = #written
speech_listener("delta", "v1", "First sentence. ", true, turn)
assert(#written == before_answer)
listeners[1]("done", "v1", "First sentence. ", true, turn)
assert(#written == before_answer)
-- Slow acknowledgement synthesis is not evidence of idle playback.
for _, tick in pairs(ticking) do tick() end
assert(#written == before_answer)
service.line({event = "speaking", on = true, generation = turn, conversation = "v1"})
service.line({event = "hearing", on = true})
service.line({event = "speaking", on = false, generation = turn, conversation = "v1"})
assert(#written == before_answer)
service.line({event = "hearing", on = false})
assert(written[before_answer + 1].message.cmd == "speech_start")
assert(written[#written].message.cmd == "speech_end")
assert(written[#written].message.text == "First sentence. ")
timers[#timers]() -- finish the hearing transcript linger timer
table.remove(timers)
local count = #written
speech_listener("delta", "v1", "Typed", false, turn)
assert(#written == count)
service.line({event = "barge"})
count = #written
speech_listener("delta", "v1", "Obsolete.", true, turn)
listeners[1]("done", "v1", "Obsolete.", true, turn)
assert(#written == count)
service.line({event = "transcript", text = "Recognized words", final = false})
assert(text.ai_transcript == "Recognized words")

-- The end of a voice conversation hands it to memory; she lingers after her last word, then goes.
service.line({event = "speaking", on = true, conversation = "v1"})
service.line({event = "conversation", on = false})
assert(remembered[1] == "v1" and not fact.ai_voice_conversation and fact.ai_dragon_open and #timers == 0)
service.line({event = "speaking", on = false, conversation = "v1"})
assert(#timers == 1)
service.line({event = "speaking", on = true, conversation = "v1"})
timers[1]()
assert(fact.ai_dragon_open)
service.line({event = "speaking", on = false, conversation = "v1"})
timers[2]()
assert(not fact.ai_dragon_open)
timers = {}

-- The sheets load from the service's folder once it is ready, and unload when voice is turned off.
assert(text.ai_dg_body_src == "/art/body.webp" and text.ai_dg_heart_src == "/art/heart.png" and fact.ai_dg_ready)

-- Speech settings change in place; loading on conversation is passed through.
voice.configure(settings({["assistant.voice"] = true, ["assistant.tts_voicepack"] = "af_heart", ["assistant.vad_model"] = "/v.onnx", ["assistant.asr_model"] = "/asr", ["assistant.tts_load_on_conversation"] = true}))
assert(#spawned == 1 and written[#written].message.cmd == "tts" and written[#written].message.voicepack == "af_heart")
assert(written[#written].message.load_on_conversation == true)

-- Disabling stops it completely and clears the mic dot; a late exit does not restart it.
voice.configure(settings({["assistant.voice"] = false}))
assert(#killed == 1 and killed[1][1] == 1 and killed[1][2] == "term" and not fact.ai_listening and not voice.running())
assert(text.ai_dg_body_src == "" and not fact.ai_dg_ready)
service.exit(nil, 0)
assert(#spawned == 1 and #timers == 0)

-- A crash restarts it after a pause; missing models (exit 3) do not.
voice.configure(settings({["assistant.voice"] = true, ["assistant.tts_load_on_conversation"] = true}))
assert(#spawned == 2 and has(spawned[2].args, "--vad-model", "") and has(spawned[2].args, "--asr-model", "") and spawned[2].args[#spawned[2].args] == "--tts-load-on-conversation")
spawned[2].exit(nil, 1)
assert(#timers == 1)
timers[1]()
assert(#spawned == 3)
spawned[3].line({event = "error", message = "voice models missing"})
spawned[3].exit(nil, 3)
assert(#timers == 1 and text.ai_voice_error == "voice models missing" and fact.ai_voice_failed)
-- Changing how the wake word is heard restarts the service; the spotter is only asked for with kws.
assert(has(spawned[3].args, "--wake", "asr"))
voice.configure(settings({["assistant.voice"] = true, ["assistant.voice_wake"] = "kws", ["assistant.kws_model"] = "/k"}))
assert(#spawned == 4 and has(spawned[4].args, "--wake", "kws") and has(spawned[4].args, "--kws-model", "/k"))
local kills = #killed
voice.configure(settings({["assistant.voice"] = true, ["assistant.voice_wake"] = "asr"}))
assert(#killed == kills + 1 and #spawned == 5 and has(spawned[5].args, "--wake", "asr"))
-- Motion preferences apply live without restarting the microphone/model service.
local starts = #spawned
voice.configure(settings({["assistant.voice"] = true, ["assistant.voice_wake"] = "asr", ["assistant.reduced_motion"] = true}))
assert(#spawned == starts and fact.ai_dg_reduced)
voice.configure(settings({["assistant.voice"] = true, ["assistant.voice_wake"] = "asr", ["assistant.reduced_motion"] = false}))
assert(#spawned == starts and not fact.ai_dg_reduced)

-- Proactive alerts speak only when --speak is set and neither locked nor in DND.
local before_alert = #written
alert_listeners[1]("4f71ebb6-db33-43e9-af8f-e5bc79a11077", "Silent update", false)
assert(#written == before_alert)
fact.notices_dnd = true
alert_listeners[1]("4f71ebb6-db33-43e9-af8f-e5bc79a11077", "Suppressed by DND", true)
assert(#written == before_alert)
fact.notices_dnd = false
fact.ai_voice_paused = true
alert_listeners[1]("4f71ebb6-db33-43e9-af8f-e5bc79a11077", "Suppressed while locked", true)
assert(#written == before_alert)
fact.ai_voice_paused = false
alert_listeners[1]("4f71ebb6-db33-43e9-af8f-e5bc79a11077", "Your overnight task finished.", true)
assert(#written == before_alert + 2 and written[#written].message.cmd == "speech_end"
    and written[#written].message.text == "Your overnight task finished."
    and written[#written-1].message.cmd == "speech_start"
    and written[#written].message.generation == written[#written-1].message.generation
    and written[#written-1].message.conversation == "4f71ebb6-db33-43e9-af8f-e5bc79a11077")

-- When Orca backend is unavailable (ai_can_signin == false), ask_turn speaks Orca-unavailable instead of Sign in.
refusing = true
fact.ai_can_signin = false
spawned[5].line({event = "turn", text = "Hello"})
assert(written[#written].message.text:find("Orca"))
refusing = false
fact.ai_can_signin = true

print("Voice service gating, turns, spoken answers, barge-in, memory hand-off, proactive alerts, reduced motion, settings and restarts passed")
-- Repeated identical transcript text gets its own linger lifetime.
local current_service = spawned[#spawned]
current_service.line({event="transcript", text="Repeat this", final=true})
local old_caption_timer = timers[#timers]
current_service.line({event="transcript", text="Repeat this", final=true})
old_caption_timer()
assert(text.ai_transcript == "Repeat this")
timers[#timers]()
assert(text.ai_transcript == "")
-- Retired process callbacks cannot restore captions after a restart.
service.line({event="transcript",text="Old process",final=false})
assert(text.ai_transcript == "")
-- A stale terminal callback still releases the queued replacement turn.
local real_stream = assistant.streaming
assistant.streaming = function() return false end
current_service.line({event="turn",text="Start old answer"})
local old_generation = asked[#asked][3].voice_generation
assistant.streaming = function() return true end
current_service.line({event="barge"})
current_service.line({event="turn",text="Replacement answer"})
local count_asked = #asked
assistant.streaming = function() return false end
listeners[1]("stopped","v1","obsolete",true,old_generation)
assert(#asked == count_asked+1 and asked[#asked][1] == "Replacement answer")
assistant.streaming = real_stream

print("Voice service gating, turns, spoken answers, barge-in, memory hand-off, settings and restarts passed")

-- A backend without deltas still delivers its full final answer exactly once.
current_service.line({event="turn",text="Ask Orca"})
local final_generation = asked[#asked][3].voice_generation
local ready_origins = {}
assistant.voice_ready = function(chat, request) ready_origins[#ready_origins + 1] = {chat, request} end
speech_listener("start","orca-chat","",true,final_generation, "")
assert(#ready_origins == 0, "a queued receipt is not yet an admitted desktop origin")
speech_listener("origin","orca-chat","orca:canonical",true,final_generation,"orca:queued")
assert(#ready_origins == 1 and ready_origins[1][2] == "orca:queued")
assert(diagnostics[#diagnostics].stage == "assistant_origin" and diagnostics[#diagnostics].canonical_request == "orca:canonical")
listeners[1]("done","orca-chat","The bot finished successfully.",true,final_generation)
assert(written[#written].message.cmd=="speech_end" and written[#written].message.text=="The bot finished successfully.")
local final_count = #written
events.assistant_hush()
speech_listener("delta","orca-chat","Obsolete.",true,final_generation)
assert(#written==final_count+1 and written[#written].message.cmd=="hush")
-- Busy turns hold proactive speech until they end rather than interrupting or invalidating their generation.
assistant.streaming=function() return true end
final_count=#written
alert_listeners[1]("orca-chat","Wait for the current answer",true)
alert_listeners[1]("orca-chat","And then this one",true)
assert(#written==final_count)
timers[#timers]()
assert(#written==final_count)
assistant.streaming=real_stream
fact.ai_voice_conversation=true
timers[#timers]()
assert(#written==final_count)
fact.ai_voice_conversation=false
timers[#timers]()
assert(#written==final_count+2 and written[#written].message.text=="Wait for the current answer")
timers[#timers]()
assert(#written==final_count+4 and written[#written].message.text=="And then this one")
-- Do Not Disturb drops held speech instead of saving it for later.
assistant.streaming=function() return true end
final_count=#written
alert_listeners[1]("orca-chat","Held then hushed",true)
assistant.streaming=real_stream
fact.notices_dnd=true
timers[#timers]()
fact.notices_dnd=false
assert(#written==final_count)
alert_listeners[1]("orca-chat","Spoken straight away",true)
assert(#written==final_count+2 and written[#written].message.text=="Spoken straight away")
print("Cross-feature final-only replies, alert generations and held alerts passed")

-- An analyzed continuation uses the current visible voice origin, without opening a popup.
refusing = false
fact.ai_voice_paused, fact.notices_dnd, fact.ai_speaking, fact.ai_listening = false, false, false, false
current_service.line({event="turn", text="Inspect in the background"})
local background_generation = asked[#asked][3].voice_generation
speech_listener("start", "voice-origin-chat", "", true, background_generation, "orca:spoken-request")
listeners[1]("done", "voice-origin-chat", "I will inspect it.", true, background_generation)
fact.ai_dragon_open = true
local popup_shown = shown
local function background_result(id, visible, chat, request)
    fact.ai_dragon_open = visible
    text.ai_voice_result_chat = chat or "voice-origin-chat"
    text.ai_voice_result_request = request or "orca:spoken-request"
    text.ai_voice_result_completion = id
    text.ai_voice_result_text = "The analyzed result."
    events.assistant_voice_result()
end
local before_result = #written
current_service.line({event="listening", on=true}) -- the microphone stays open between utterances
background_result("final-summary", true)
assert(#written == before_result + 2 and written[#written].message.cmd == "speech_end")
background_result("final-summary", true)
assert(#written == before_result + 2)
background_result("hidden-summary", false)
background_result("hidden-summary", true)
background_result("foreign-summary", true, "other-chat")
assert(#written == before_result + 2 and shown == popup_shown)
assistant.status = function() return {open=true, conversation="another-popup-chat"} end
background_result("other-popup-summary", true)
assert(#written == before_result + 2)
assistant.status = nil
current_service.line({event="hearing", on=true})
background_result("capturing-summary", true)
assert(#written == before_result + 2)
current_service.line({event="hearing", on=false})
background_result("capturing-summary", true)
assert(#written == before_result + 2) -- rejected during speech, never replayed afterward
background_result("idle-listening-summary", true)
assert(#written == before_result + 4)
events.assistant_hush()
before_result = #written
background_result("interrupted-summary", true)
assert(#written == before_result)
speech_listener("start", "voice-origin-chat", "", true, nil, "orca:new-request")
speech_listener("start", "typed-chat", "", false)
background_result("typed-superseded", true, nil, "orca:new-request")
assert(#written == before_result + 1) -- only the preceding voice stream start
print("Visible, hidden, duplicate, cross-chat and interrupted background delivery passed")

-- Explicit task speech is independent of final-only answer streaming. It retires
-- the poller, never the Orca task, and the microphone can remain open.
local detached = {}
local registered = {}
assistant.voice_ready = function(chat, request)
    assert(written[#written].message.cmd == "speech_start")
    registered[#registered+1] = {chat, request}
end
local retired_pollers = {}
assistant.detach_voice = function(request)
    if retired_pollers[request] then return false end
    retired_pollers[request] = true
    detached[#detached+1] = request
    return true
end
fact.ai_voice_paused, fact.notices_dnd, fact.ai_speaking, fact.ai_listening = false, false, false, true
fact.ai_dragon_open = true
current_service.line({event="turn", text="Create an independent workspace"})
speech_listener("start", "task-chat", "", true, asked[#asked][3].voice_generation, "task-request")
assert(registered[#registered][1] == "task-chat" and registered[#registered][2] == "task-request")
local function task_update(kind, sequence, message, task_request)
    text.ai_voice_task_chat, text.ai_voice_task_request = "task-chat", task_request or "task-request"
    text.ai_voice_task_id, text.ai_voice_task_sequence = "workspace", tostring(sequence)
    text.ai_voice_task_kind, text.ai_voice_task_text = kind, message
    events.assistant_voice_task()
end
task_update("ack", 1, "I'll create that workspace now.")
assert(detached[#detached] == "task-request")
assert(written[#written].message.cmd == "speech_end" and written[#written].message.text == "I'll create that workspace now.")
local ack_generation = written[#written].message.generation
assert(written[#written - 1].message.cmd == "speech_start" and written[#written - 1].message.background == true)
assert(diagnostics[#diagnostics].stage == "task_update" and diagnostics[#diagnostics].request == "task-request")
local after_ack = #written
task_update("ack", 1, "Replay")
assert(#written == after_ack)
current_service.line({event="speaking", on=true, generation=ack_generation, conversation="task-chat"})
task_update("progress", 2, "The workspace exists; the agent is starting.")
current_service.line({event="speaking", on=false, generation=ack_generation})
ticking[1]()
assert(written[#written].message.text == "The workspace exists; the agent is starting.")
local progress_generation = written[#written].message.generation
current_service.line({event="speaking", on=true, generation=progress_generation})
task_update("done", 3, "The agent session is attached and verified.")
assert(written[#written].message.text == "The agent session is attached and verified.")
local completion_generation = written[#written].message.generation
current_service.line({event="speaking", on=false, generation=progress_generation})
assert(completion_generation ~= progress_generation)
current_service.line({event="turn", text="Another question while that task finishes"})
local after_new_turn = #written
task_update("done", 4, "Late replay")
assert(#written == after_new_turn)
current_service.line({event="speaking", on=true, generation=completion_generation})
assert(not fact.ai_speaking)
speech_listener("start", "task-chat", "", true, asked[#asked][3].voice_generation, "next-task-request")
fact.ai_dragon_open = true
task_update("ack", 1, "I'll check it now.", "next-task-request")
events.assistant_dragon()
local hidden_count = #written
task_update("progress", 2, "Hidden progress", "next-task-request")
assert(#written == hidden_count and not fact.ai_dragon_open)
assert(diagnostics[#diagnostics].stage == "task_update_dropped" and diagnostics[#diagnostics].reason == "hidden")
fact.ai_dragon_open = true
current_service.line({event="hearing", on=true})
task_update("progress", 3, "Owner has the floor", "next-task-request")
assert(diagnostics[#diagnostics].stage == "task_update_dropped" and diagnostics[#diagnostics].reason == "hearing")
current_service.line({event="hearing", on=false})
-- Durable task acceptance must release foreground continuation even when speech
-- is ineligible. This is a work-state transition, not permission to play audio.
speech_listener("start", "task-chat", "", true, nil, "hidden-accepted-request")
local pending_poll_request = "hidden-accepted-request"
assistant.detach_voice = function(request)
    if request ~= pending_poll_request then return false end
    pending_poll_request = nil
    return true
end
fact.ai_dragon_open = false
local hidden_accept_before = #written
task_update("ack", 1, "Accepted but hidden", "hidden-accepted-request")
assert(written[hidden_accept_before + 1] and written[hidden_accept_before + 1].message.cmd == "task_accepted", "hidden admission must release foreground thinking before the speech gate")
assert(written[#written].message.cmd == "hush")
assert(diagnostics[#diagnostics].reason == "hidden")
fact.ai_dragon_open = true
speech_listener("start", "task-chat", "", true, nil, "hearing-accepted-request")
pending_poll_request = "hearing-accepted-request"
current_service.line({event="hearing", on=true})
local hearing_accept_before = #written
task_update("ack", 1, "Accepted while owner speaks", "hearing-accepted-request")
assert(written[hearing_accept_before + 1].message.cmd == "task_accepted")
assert(#written == hearing_accept_before + 2 and written[#written].message.cmd == "hush")
assert(diagnostics[#diagnostics].reason == "hearing")
current_service.line({event="hearing", on=false})
speech_listener("start", "task-chat", "", true, nil, "new-question-request")
pending_poll_request = "new-question-request"
local stale_ack_before = #written
task_update("ack", 1, "Stale accepted event", "hearing-accepted-request")
assert(#written == stale_ack_before and pending_poll_request == "new-question-request", "late admission must not retire a new owner poller")
-- A completed follow-up can queue before the acceptance event arrives. Retire
-- old authority first, drain once, and do not play the old ack over the new turn.
for _, visible in ipairs({false, true}) do
    local request = visible and "visible-queued-request" or "hidden-queued-request"
    local poller_busy = true
    assistant.streaming = function() return poller_busy end
    assistant.detach_voice = function(candidate)
        if candidate ~= request or not poller_busy then return false end
        poller_busy = false
        return true
    end
    speech_listener("start", "task-chat", "", true, nil, request)
    fact.ai_dragon_open = visible
    local queued_before, release_before = #asked, #written
    current_service.line({event="turn", text="Follow up before acknowledgement", turn_id="capture-queued"})
    assert(#asked == queued_before)
    task_update("ack", 1, "An obsolete acknowledgement", request)
    assert(#asked == queued_before + 1 and asked[#asked][1] == "Follow up before acknowledgement")
    assert(asked[#asked][3].capture_id == "capture-queued")
    assert(written[release_before + 1].message.cmd == "task_accepted")
    assert(written[release_before + 2].message.cmd == "hush")
    assert(#written == release_before + 2 and diagnostics[#diagnostics].stage == "assistant_request")
    local submitted = #asked
    task_update("ack", 1, "Replayed obsolete acknowledgement", request)
    assert(#asked == submitted and #written == release_before + 2)
end
assistant.streaming = real_stream
fact.ai_dragon_open = true
-- A capture onset that never becomes a turn (a VAD blip) stops task audio but
-- keeps the voice origin, so a background launch's ack and readiness still speak.
assistant.detach_voice = function() return false end
assistant.voice_ready = function() end
speech_listener("origin", "task-chat", "orca:blip-canonical", true, nil, "blip-request")
current_service.line({event="hearing", on=true})
current_service.line({event="hearing", on=false})
task_update("ack", 1, "I have accepted that work.", "blip-request")
assert(diagnostics[#diagnostics].stage == "task_update", diagnostics[#diagnostics].reason)
assert(written[#written].message.cmd == "speech_end" and written[#written].message.text == "I have accepted that work.")
local blip_ack = written[#written].message.generation
current_service.line({event="speaking", on=true, generation=blip_ack, conversation="task-chat"})
local before_blip = #written
current_service.line({event="hearing", on=true})
assert(#written == before_blip + 1 and written[#written].message.cmd == "hush")
task_update("progress", 2, "Recorded while the owner has the floor.", "blip-request")
assert(diagnostics[#diagnostics].stage == "task_update_dropped" and diagnostics[#diagnostics].reason == "hearing")
current_service.line({event="hearing", on=false})
current_service.line({event="speaking", on=false, generation=blip_ack})
ticking[1]()
assert(written[#written].message.cmd == "hush", "a progress muted during capture is never replayed")
task_update("progress", 3, "The background worker is ready.", "blip-request")
assert(diagnostics[#diagnostics].stage == "task_update", diagnostics[#diagnostics].reason)
assert(written[#written].message.cmd == "speech_end" and written[#written].message.text == "The background worker is ready.")
current_service.line({event="turn", text="A committed owner turn"})
local after_committed = #written
task_update("done", 4, "Superseded completion", "blip-request")
assert(#written == after_committed and diagnostics[#diagnostics].reason == "origin_mismatch")
print("Task updates: a capture blip keeps the origin; the next ack and progress speak")
print("Task updates: prompt acknowledgement, microphone open, progress, completion priority, replay, follow-up input and hide passed")

-- Switching backends invalidates old audio and never reuses a local conversation ID.
local before_switch = #spawned
voice.configure(settings({["assistant.voice"]=true, ["assistant.backend"]="orca"}))
assert(#spawned==before_switch and written[#written].message.cmd=="end")
current_service.line({event="turn",text="A fresh Orca conversation"})
assert(asked[#asked][2]==nil)
final_count=#written
listeners[1]("done","local-old","Old local reply.",true,final_generation)
assert(#written==final_count)


-- The service hears when she comes onto the screen, by dragon or popup, once per change.
local function arg(list, name)
    for i = 1, #list - 1 do if list[i] == name then return list[i + 1] end end
end
local profiled = {["assistant.voice"] = true, ["assistant.voice_profiles.high.enabled"] = true,
    ["assistant.voice_profiles.high.python"] = "/gpu/bin/python3", ["assistant.voice_profiles.vram_threshold_percent"] = 40,
    ["assistant.voice_profiles.low.precision"] = "int8"}
local before_profiles = #spawned
voice.configure(settings(profiled))
assert(#spawned == before_profiles + 1, "enabling the high profile restarts the service")
local profile_args = spawned[#spawned].args
local encoded_profiles = arg(profile_args, "--profiles").value
assert(encoded_profiles.high.enabled == true and encoded_profiles.high.python == "/gpu/bin/python3")
assert(encoded_profiles.vram_threshold_percent == 40 and encoded_profiles.low.precision == "int8")
assert(arg(profile_args, "--source") == "")
voice.configure(settings(profiled))
assert(#spawned == before_profiles + 1, "unchanged profiles keep the running service")
local profile_service = spawned[#spawned]
presences = {}
fact.ai_open = false
profile_service.line({event = "conversation", on = true})
assert(#presences == 1 and presences[1] == true)
profile_service.line({event = "speaking", on = true})
assert(#presences == 1, "a second reason to stay visible sends nothing")
fact.ai_open = true
visibility_listeners[1](true)
assert(#presences == 1)
events.assistant_dragon()
assert(#presences == 1, "the open popup keeps her present after the dragon hides")
fact.ai_open = false
visibility_listeners[1](false)
assert(#presences == 2 and presences[2] == false)
fact.ai_open = true
visibility_listeners[1](true)
assert(#presences == 3 and presences[3] == true)
-- A restarted service is told at once that she is already visible.
profiled["assistant.voice_profiles.high.tts_speaker"] = "Vivian"
voice.configure(settings(profiled))
assert(#spawned == before_profiles + 2 and presences[#presences] == true)
fact.ai_open = false
print("Voice profile settings and presence passed")
