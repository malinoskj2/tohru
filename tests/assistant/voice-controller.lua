local encoded = {__concat = function(a) return a end}
local json = {encode = function(value) return setmetatable({value = value}, encoded) end, decode = function(value) return value end}
_G.json = json
local events, spawned, killed, written, timers = {}, {}, {}, {}, {}
_G.fact = {}
_G.text = {}
_G.on = function(name, handler) events[name] = handler end
_G.after = function(seconds, f) timers[#timers + 1] = f end
local ticking = {}
_G.every = function(ms, f) ticking[#ticking + 1] = f; return #ticking end
_G.cancel = function(t) ticking[t] = nil end
_G.kill = function(id, how) killed[#killed + 1] = {id, how} end
_G.write = function(id, line) written[#written + 1] = {id = id, message = line.value} end
_G.spawn = function(command, args, line, exit, how)
    spawned[#spawned + 1] = {command = command, args = args, line = line, exit = exit, how = how}
    return #spawned
end

local asked, remembered, stopped, listeners, alert_listeners = {}, {}, 0, {}, {}
local refusing = false
local shown = nil
local assistant = {
    listening = function(on) fact.ai_listening = on end,
    ask = function(prompt, conversation) asked[#asked + 1] = {prompt, conversation}; return not refusing end,
    streaming = function() return refusing == "busy" end,
    stop = function() stopped = stopped + 1 end,
    remember = function(id) remembered[#remembered + 1] = id end,
    show = function(id) shown = id end,
    on_answer = function(listener) listeners[#listeners + 1] = listener end,
    on_alert = function(listener) alert_listeners[#alert_listeners + 1] = listener end,
}
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

-- The wake word alone gets a short spoken reply, never the same one twice running.
local greetings = {}
for i = 1, 12 do
    service.line({event = "wake"})
    local said = written[#written].message
    assert(said.cmd == "say" and said.text ~= "" and said.text ~= greetings[i - 1])
    greetings[i] = said.text
end
-- A turn the assistant cannot take is answered aloud rather than with silence.
refusing = true
service.line({event = "turn", text = "Are you there"})
assert(written[#written].message.cmd == "say" and written[#written].message.text:find("Sign in"))
refusing = "busy"
service.line({event = "turn", text = "Are you there"})
assert(written[#written].message.text:find("still on the last"))
refusing = false

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
assert(#written == before_alert + 1 and written[#written].message.cmd == "say"
    and written[#written].message.text == "Your overnight task finished."
    and written[#written].message.conversation == "4f71ebb6-db33-43e9-af8f-e5bc79a11077")

-- When Orca backend is unavailable (ai_can_signin == false), voice says Orca is unreachable instead of Sign in.
refusing = true
fact.ai_can_signin = false
spawned[5].line({event = "turn", text = "Hello"})
assert(written[#written].message.text:find("Orca"))
refusing = false
fact.ai_can_signin = true

print("Voice service gating, turns, spoken answers, barge-in, memory hand-off, proactive alerts, settings and restarts passed")
