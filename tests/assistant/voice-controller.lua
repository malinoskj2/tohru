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
local diagnostics = {}
_G.write = function(id, line)
    if line.value.cmd == "diagnostic" then diagnostics[#diagnostics + 1] = line.value
    else written[#written + 1] = {id = id, message = line.value} end
end
_G.spawn = function(command, args, line, exit, how)
    spawned[#spawned + 1] = {command = command, args = args, line = line, exit = exit, how = how}
    return #spawned
end

local asked, remembered, stopped, listeners, alert_listeners = {}, {}, 0, {}, {}
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
service.line({event = "turn", text = "What time is it"})
assert(asked[1][1] == "What time is it" and asked[1][2] == nil)
listeners[1]("done", "v1", "Half past two.", true)
assert(written[1].message.cmd == "say" and written[1].message.text == "Half past two." and written[1].message.conversation == "v1")
listeners[1]("done", "c9", "Typed answer", false)
assert(#written == 1)
service.line({event = "turn", text = "And tomorrow"})
assert(asked[2][2] == "v1")
service.line({event = "speaking", on = true, conversation = "v1"})
assert(fact.ai_dragon_open and fact.ai_speaking and #ticking == 1)
service.line({event = "mouth", level = 0.6, viseme = 5})
assert(fact.ai_mouth == 0.6)
-- In a conversation she stays on screen, listening for the next turn; a click opens what she said.
service.line({event = "speaking", on = false, conversation = "v1"})
assert(fact.ai_mouth == 0 and fact.ai_dragon_open and #timers == 0)
events.assistant_dragon()
assert(shown == "v1" and not fact.ai_dragon_open and next(ticking) == nil)
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
assert(written[#written].message.cmd == "say" and written[#written].message.text:find("Sign in"))
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
speech_listener("delta", "v1", "First sentence. ", true, turn)
assert(written[#written].message.cmd == "speech_delta")
listeners[1]("done", "v1", "First sentence. ", true, turn)
assert(written[#written].message.cmd == "speech_end")
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
speech_listener("start","orca-chat","",true,final_generation)
listeners[1]("done","orca-chat","The bot finished successfully.",true,final_generation)
assert(written[#written].message.cmd=="speech_end" and written[#written].message.text=="The bot finished successfully.")
local final_count = #written
events.assistant_hush()
speech_listener("delta","orca-chat","Obsolete.",true,final_generation)
assert(#written==final_count+1 and written[#written].message.cmd=="hush")
-- Busy turns suppress proactive speech rather than invalidating their generation.
assistant.streaming=function() return true end
final_count=#written
alert_listeners[1]("orca-chat","Wait for the current answer",true)
assert(#written==final_count)
assistant.streaming=real_stream
print("Cross-feature final-only replies, alert generations and busy suppression passed")

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

-- Switching backends invalidates old audio and never reuses a local conversation ID.
local before_switch = #spawned
voice.configure(settings({["assistant.voice"]=true, ["assistant.backend"]="orca"}))
assert(#spawned==before_switch and written[#written].message.cmd=="end")
current_service.line({event="turn",text="A fresh Orca conversation"})
assert(asked[#asked][2]==nil)
final_count=#written
listeners[1]("done","local-old","Old local reply.",true,final_generation)
assert(#written==final_count)
