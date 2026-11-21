local json = {encode = function(value) return value end, decode = function(value) assert(type(value) == "table", "fixture answers are tables"); return value end}
_G.json = json
local events, runs, spawned, killed, timers = {}, {}, {}, {}, {}
_G.fact = {ai_open = false, ai_view = 0, ai_picker = false, ai_ready = false, ai_current = -1, ai_history_sel = -1,
    ai_live_here = false, ai_signed_in = false}
_G.text = {ai_prompt = "", ai_search = ""}
_G.model = {}
_G.on = function(name, handler)
    local list = events[name] or {}
    list[#list + 1] = handler
    events[name] = list
end
local function emit(name, ...) for _, handler in ipairs(events[name] or {}) do handler(...) end end
_G.every = function() end
_G.after = function(_, f) timers[#timers + 1] = f end
_G.kill = function(id, how) killed[#killed + 1] = {id, how} end
local outputs = {}
_G.run = function(command, args, done, how)
    runs[#runs + 1] = {command = command, args = args, how = how}
    local reply = outputs[args[1]]
    if done and reply then done(reply, 0) end
end
_G.spawn = function(command, args, line, exit, how)
    spawned[#spawned + 1] = {args = args, line = line, exit = exit, how = how}
    return #spawned
end

local chosen = {}
local provider_setting = "codex"
outputs.providers = json.encode({
    {id = "claude", name = "Claude", slot = 0, signed_in = true, model = "Opus 5.5"},
    {id = "codex", name = "Codex", slot = 1, signed_in = false, model = ""},
    {id = "agy", name = "Antigravity", slot = 3, signed_in = true, model = ""},
})
local assistant = assert(loadfile("shell/assistant/controller.luau"))()({
    provider = function() return provider_setting end,
    choose = function(id) chosen[#chosen + 1] = id end,
})

-- A configured provider that is signed out is passed over for the first signed-in one.
assert(fact.ai_loaded and fact.ai_current == 0 and text.ai_provider_name == "Claude" and fact.ai_signed_in)
assert(model.ai_providers[2].meta == "Signed in" and model.ai_providers[1].meta == "Opus 5.5")
assert(text.ai_placeholder == "Ask Claude")

-- Sending streams into the live answer; Stop is SIGINT to the helper.
emit("assistant_open")
text.ai_prompt = "How many ounces in a cup?"
emit("assistant_send")
assert(#spawned == 1 and spawned[1].how.input == "How many ounces in a cup?")
assert(spawned[1].args[1] == "chat" and spawned[1].args[3] == "claude")
assert(fact.ai_streaming and fact.ai_live_here and text.ai_prompt == "")
spawned[1].line(json.encode({event = "start", conversation = "c1", slot = 0}))
spawned[1].line(json.encode({event = "delta", text = "Eight "}))
spawned[1].line(json.encode({event = "delta", text = "ounces."}))
assert(text.ai_live == "Eight ounces.")
emit("assistant_send")
assert(#killed == 1 and killed[1][1] == 1 and killed[1][2] == "int")

-- An answer that finishes while the popup is closed leaves the ready mark until that chat is shown.
outputs.show = json.encode({id = "c1", provider = "claude", turns = {
    {role = "user", text = "How many ounces in a cup?"}, {role = "assistant", text = "Eight ounces."}}})
assistant.close()
spawned[1].line(json.encode({event = "done", conversation = "c1"}))
assert(not fact.ai_streaming and fact.ai_ready and fact.ai_ready_slot == 0)
emit("assistant_open")
assert(not fact.ai_ready and fact.ai_actions and #model.ai_turns == 2 and model.ai_turns[1].mine)

-- A helper that dies mid-answer is an error, not a stuck stream.
text.ai_prompt = "And a pint?"
emit("assistant_send")
assert(spawned[2].args[5] == "c1")
spawned[2].exit(nil, 1)
assert(not fact.ai_streaming and fact.ai_failed)

-- A signed-out provider is shown with its sign-in note and never sent to; choosing a signed-in one saves it.
emit("assistant_choose", 1)
assert(text.ai_provider_name == "Codex" and not fact.ai_signed_in and #chosen == 0)
assert(text.ai_placeholder == "Sign in to chat with Codex")
text.ai_prompt = "hi"
emit("assistant_send")
assert(#spawned == 2)
emit("assistant_signin", 1)
assert(runs[#runs].args[1] == "signin" and runs[#runs].args[2] == "codex")
emit("assistant_choose", 2)
assert(chosen[1] == "agy" and fact.ai_slot == 3)

-- History groups rows under their day and skips headers when moving.
outputs.history = json.encode({
    {id = "c2", title = "Cups", group = "Today", when = "1:15 PM", slot = 0, match = ""},
    {id = "c3", title = "Sonder", group = "Today", when = "9:40 AM", slot = 1, match = ""},
    {id = "c4", title = "Saturday", group = "Yesterday", when = "8:31 PM", slot = 3, voice = true, match = ""},
})
emit("assistant_history")
assert(fact.ai_view == 1 and #model.ai_history == 5 and model.ai_history[1].header and model.ai_history[4].header)
emit("assistant_move", 1)
assert(fact.ai_history_sel == 1)
emit("assistant_move", 1); emit("assistant_move", 1)
assert(fact.ai_history_sel == 4 and model.ai_history[5].voice)
text.ai_search = "sat"
emit("text:ai_search")
assert(runs[#runs].args[#runs[#runs].args] == "sat" and fact.ai_searching and fact.ai_history_sel == -1)
emit("assistant_move", 1)
emit("assistant_activate")
assert(fact.ai_view == 0 and runs[#runs].args[1] == "show" and runs[#runs].args[2] == "c2")

-- New chat clears the conversation; a voice turn goes to a conversation of its own.
emit("assistant_new")
assert(#model.ai_turns == 0 and not fact.ai_actions)
local answers = {}
assistant.on_answer(function(kind, conversation, answer, voice) answers[#answers + 1] = {kind, conversation, answer, voice} end)
assistant.close()
emit("assistant_choose", 0)
assert(assistant.ask("tell me a joke", nil))
local voice = spawned[#spawned]
assert(voice.args[#voice.args] == "--voice")
voice.line(json.encode({event = "start", conversation = "v1"}))
voice.line(json.encode({event = "delta", text = "Why?"}))
voice.line(json.encode({event = "done", conversation = "v1"}))
assert(answers[1][1] == "done" and answers[1][2] == "v1" and answers[1][3] == "Why?" and answers[1][4] == true)
-- The mark's menu: right-click opens it over a closed popup; opening the popup closes it.
emit("assistant_menu")
assert(fact.ai_menu_open and not fact.ai_open)
emit("assistant_open")
assert(fact.ai_open and not fact.ai_menu_open)
emit("assistant_menu")
assert(fact.ai_menu_open and not fact.ai_open)
emit("assistant_show_history")
assert(fact.ai_open and fact.ai_view == 1 and not fact.ai_menu_open)
assistant.listening(true)
assert(fact.ai_listening)
assistant.listening(false)
assert(not fact.ai_listening)
print("Assistant provider choice, streaming, Stop, ready mark, sign-in, history and voice-turn fixtures passed")
