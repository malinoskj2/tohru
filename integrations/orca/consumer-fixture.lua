-- Line driver for isolated transport tests; production voice policy, fake devices.
package.path = "shell/?.luau;" .. package.path
local events, timers, service, speech = {}, {}, nil, nil
local generation
fact, text = {}, {}
json = {
    encode = function(value) return setmetatable(value, {__concat = function(a) return a end}) end,
    decode = function(value) return value end,
}
on = function(name, callback) events[name] = callback end
every = function(_, callback) timers[#timers+1] = callback end
after = function(ms, callback) if ms == 16 then callback() end end
kill = function() end
spawn = function(_, _, callback) service = callback; return 1 end
write = function(_, message)
    if message.cmd == "speech_start" then generation = message.generation end
    if message.cmd == "speech_end" then
        io.write("speech\t", tostring(message.text), "\n"); io.flush()
    elseif message.cmd == "hush" then
        io.write("hush\n"); io.flush()
    end
end
local idle = function() end
local puppet = setmetatable({}, {__index = function() return idle end})
local assistant = {
    listening = idle, remember = idle, show = idle, stop = idle,
    streaming = function() return false end,
    ask = function() return true end,
    on_speech = function(callback) speech = callback end,
    on_answer = idle,
    voice_ready = function(chat, request)
        io.write("ready\t", chat, "\t", request, "\n"); io.flush()
    end,
    detach_voice = function(request)
        io.write("detached\t", request, "\n"); io.flush(); return true
    end,
}
local voice = require("assistant/voice")(assistant, puppet)
voice.configure({["assistant.voice"]={value=true}, ["assistant.backend"]={value="orca"}})
for line in io.lines() do
    local command, rest = line:match("^(%S+)%s?(.*)$")
    if command == "start" then
        local chat, request = rest:match("^(%S+)%s+(%S+)$")
        fact.ai_dragon_open = true
        service({event="turn", text="Create a workspace"})
        speech("start", chat, "", true, nil, request)
    elseif command == "text" then
        local key, value = rest:match("^(%S+)%s?(.*)$")
        text[key] = value
    elseif command == "emit" then
        if events[rest] then events[rest]() end
    elseif command == "finished" then
        service({event="speaking", on=false, generation=generation})
        for _, timer in ipairs(timers) do timer() end
    elseif command == "hide" then events.assistant_dragon()
    elseif command == "interrupt" then service({event="hearing", on=true})
    elseif command == "sync" then io.write("synced\n"); io.flush()
    else error("Unknown test command " .. tostring(command)) end
end
