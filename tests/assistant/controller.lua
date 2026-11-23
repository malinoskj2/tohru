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
local every_cbs = {}
_G.every = function(ms, f) every_cbs[#every_cbs + 1] = f end
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
local values = {["assistant.memory"] = true, ["assistant.models.claude.model"] = "claude-sonnet-5-5", ["assistant.models.claude.effort"] = "high"}
outputs.providers = json.encode({
    {id = "claude", name = "Claude", slot = 0, signed_in = true, model = "Opus 5.5"},
    {id = "codex", name = "Codex", slot = 1, signed_in = false, model = ""},
    {id = "agy", name = "Antigravity", slot = 3, signed_in = true, model = ""},
})
local assistant = assert(loadfile("shell/assistant/controller.luau"))()({
    provider = function() return provider_setting end,
    choose = function(id) chosen[#chosen + 1] = id end,
    value = function(key) return values[key] end,
})
local function has(list, a, b)
    for i = 1, #list - 1 do if list[i] == a and list[i + 1] == b then return true end end
    return false
end
local function runs_of(name)
    local found = {}
    for _, r in ipairs(runs) do if r.args[1] == name then found[#found + 1] = r end end
    return found
end
assert(runs_of("providers")[1].args[2] == "--models" and runs_of("providers")[1].args[3].claude.model == "claude-sonnet-5-5")

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
assert(has(spawned[1].args, "--model", "claude-sonnet-5-5") and has(spawned[1].args, "--effort", "high"))
assert(has(spawned[1].args, "--persona", "on") and has(spawned[1].args, "--memory", "on") and has(spawned[1].args, "--orca", "auto"))
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
assert(#runs_of("remember") == 0)
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
local speech_events = {}
assistant.on_speech(function(kind, conv, text, voice, generation, request) speech_events[#speech_events + 1] = {kind, text, generation, request} end)
assistant.on_answer(function(kind, conversation, answer, voice, generation) answers[#answers + 1] = {kind, conversation, answer, voice, generation} end)
assistant.close()
emit("assistant_choose", 0)
assert(assistant.ask("tell me a joke", nil))
local voice = spawned[#spawned]
local voiced = false
for _, a in ipairs(voice.args) do voiced = voiced or a == "--voice" end
assert(voiced)
voice.line(json.encode({event = "start", conversation = "v1"}))
voice.line(json.encode({event = "voice_origin", request = "orca:queued", canonical_request = "orca:canonical"}))
assert(speech_events[#speech_events][1] == "origin" and speech_events[#speech_events][2] == "orca:canonical" and speech_events[#speech_events][4] == "orca:queued")
voice.line(json.encode({event = "delta", text = "Why?"}))
assert(speech_events[#speech_events][1] == "delta" and speech_events[#speech_events][2] == "Why?")
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
-- A finished chat is handed to memory once when it is left; with memory off nothing is spawned for it.
emit("assistant_open")
emit("assistant_new")
local before = #runs_of("remember")
text.ai_prompt = "Remember me?"
emit("assistant_send")
spawned[#spawned].line(json.encode({event = "start", conversation = "m1"}))
spawned[#spawned].line(json.encode({event = "done", conversation = "m1"}))
assistant.close()
local remembered = runs_of("remember")
assert(#remembered == before + 1 and remembered[#remembered].args[2] == "m1" and remembered[#remembered].how.output == false)
assistant.close()
assert(#runs_of("remember") == before + 1)
values["assistant.memory"] = false
values["assistant.orca"] = "false"
emit("assistant_open")
emit("assistant_new")
text.ai_prompt = "Private"
emit("assistant_send")
assert(has(spawned[#spawned].args, "--memory", "off") and has(spawned[#spawned].args, "--orca", "false"))
spawned[#spawned].line(json.encode({event = "start", conversation = "m2"}))
spawned[#spawned].line(json.encode({event = "done", conversation = "m2"}))
assistant.close()
emit("assistant_new")
assert(#runs_of("remember") == before + 1)
values["assistant.memory"] = true
values["assistant.orca"] = "true"
emit("assistant_open")
text.ai_prompt = "Orca"
emit("assistant_send")
assert(has(spawned[#spawned].args, "--orca", "true"))
spawned[#spawned].line(json.encode({event = "done", conversation = "m3"}))
values["assistant.orca"] = nil

-- The Memory view lists what is remembered; forgetting everything takes a second press.
local long = string.rep("Keeps a sourdough starter called Clint ", 4)
outputs.memory = json.encode({memories = {{id = "aa", text = "Bakes bread", kind = "fact"}, {id = "bb", text = long, kind = "fact"}}, relationship = {conversations = 3}})
emit("assistant_memory")
assert(fact.ai_view == 2 and #model.ai_memories == 2 and text.ai_memory_note:find("3 chats"))
assert(model.ai_memories[2].text:sub(-3) == "…" and #model.ai_memories[2].text <= 75 and not model.ai_memories[2].text:find(" …"))
emit("assistant_memory_forget", 0)
local function memory_actions()
    local seen = {}
    for _, r in ipairs(runs_of("memory")) do if r.args[2] ~= "list" then seen[#seen + 1] = r.args[2] .. (r.args[3] or "") end end
    return table.concat(seen, ",")
end
assert(memory_actions() == "forgetaa")
emit("assistant_memory_clear")
assert(fact.ai_forget_armed and memory_actions() == "forgetaa")
emit("assistant_memory_clear")
assert(memory_actions() == "forgetaa,clear")
emit("assistant_history")
assert(fact.ai_view == 0)
-- Speculative draft: committed on matching transcript (no second provider call), discarded on mismatch, disabled for agy.
outputs.commit = json.encode({event = "committed", conversation = "v1"})
outputs.rollback = json.encode({event = "rolled_back", conversation = "v1", deleted = false})
emit("assistant_choose", 0) -- claude
local sp_before = #spawned
local speech_before = #speech_events
assert(assistant.draft("What is the weather in Tokyo?", "v1"))
assert(#spawned == sp_before + 1)
local d1 = spawned[#spawned]
assert(has(d1.args, "--provider", "claude"))
local has_draft_flag = false
for _, a in ipairs(d1.args) do has_draft_flag = has_draft_flag or a == "--draft" end
assert(has_draft_flag and not fact.ai_streaming)
d1.line(json.encode({event = "start", conversation = "v1", draft = true}))
d1.line(json.encode({event = "delta", text = "Clear and 20C."}))
d1.line(json.encode({event = "done", conversation = "v1", session = "fork-1"}))
local ans_before = #answers
assert(#speech_events == speech_before)
assert(assistant.ask("what is the weather in tokyo", "v1"))
assert(#spawned == sp_before + 1) -- reused finished draft, no second provider call
assert(#answers == ans_before + 1 and answers[#answers][3] == "Clear and 20C.")
local committed = runs_of("commit")
assert(#committed >= 1 and has(committed[#committed].args, "--session", "fork-1"))

-- Discarded draft kills its process and a mismatched ask spawns a fresh provider call.
local k_before = #killed
assert(assistant.draft("What is the capital of France", "v1"))
local d2 = spawned[#spawned]
assistant.discard_draft()
assert(#killed == k_before + 1 and killed[#killed][2] == "int")

-- Draft replying <<needs-tools>> (both pre-commit and while promoted) falls back to a fresh full-tools run; marker is never committed or spoken.
local early_answers, early_commits = #answers, #runs_of('commit')
local decision = nil
values['assistant.semantic_models.claude.model']='claude-opus-5-5'
values['assistant.semantic_models.claude.effort']='low'
assert(assistant.draft('Explain the index please', 'v1', {semantic=true, on_semantic=function(value) decision=value end}))
local early = spawned[#spawned]
local scenario_model, scenario_effort
for i,arg in ipairs(early.args) do
    if arg=='--scenario-model' then scenario_model=early.args[i+1] end
    if arg=='--scenario-effort' then scenario_effort=early.args[i+1] end
end
assert(scenario_model=='claude-opus-5-5' and scenario_effort=='low')
early.line(json.encode({event='start', conversation='v1', draft=true}))
early.line(json.encode({event='semantic', decision='respond'}))
assert(decision == 'respond' and #answers == early_answers and #runs_of('commit') == early_commits)
assert(assistant.ask('Explain the index please', 'v1'))
early.line(json.encode({event='delta', text='A validated answer.'}))
assert(#answers == early_answers and #runs_of('commit') == early_commits)
early.line(json.encode({event='done', conversation='v1'}))
assert(#runs_of('commit') == early_commits+1 and #answers == early_answers+1)
values['assistant.semantic_models.claude.model']=''
values['assistant.semantic_models.claude.effort']=''
assert(assistant.draft('A draft before changing voice settings', 'v1'))
local kills_before_settings = #killed
values['assistant.voice_models.claude.model']='claude-sonnet-5-5'
assistant.configure()
assert(#killed==kills_before_settings+1)
values['assistant.voice_models.claude.model']=''
assistant.configure()

local commits_before = #runs_of("commit")
local ans_nt = #answers
assert(assistant.draft("Open the sound panel please", "v1"))
local d_nt = spawned[#spawned]
d_nt.line(json.encode({event = "start", conversation = "v1", draft = true}))
d_nt.line(json.encode({event = "delta", text = "<<needs-tools>>"}))
d_nt.line(json.encode({event = "needs_tools", conversation = "v1"}))
local sp_nt = #spawned
assert(assistant.ask("Open the sound panel please", "v1"))
assert(#spawned == sp_nt + 1)
local full_run = spawned[#spawned]
for _, a in ipairs(full_run.args) do assert(a ~= "--draft") end
full_run.line(json.encode({event = "start", conversation = "v1"}))
full_run.line(json.encode({event = "delta", text = "Opened the sound panel."}))
full_run.line(json.encode({event = "done", conversation = "v1"}))
assert(#runs_of("commit") == commits_before)
assert(#answers == ans_nt + 1 and answers[#answers][3] == "Opened the sound panel.")

-- Still-streaming draft promoted to live that hits needs_tools falls back cleanly without showing or speaking the marker.
local commits_p = #runs_of("commit")
local ans_p = #answers
assert(assistant.draft("Turn up the desktop volume", "v1"))
local d_live = spawned[#spawned]
d_live.line(json.encode({event = "start", conversation = "v1", draft = true}))
assert(assistant.ask("Turn up the desktop volume", "v1"))
local sp_promoted = #spawned
d_live.line(json.encode({event = "delta", text = "<<needs-tools>>"}))
assert(not text.ai_live:find("needs-tools", 1, true))
d_live.line(json.encode({event = "needs_tools", conversation = "v1"}))
assert(#spawned == sp_promoted + 1)
local fallback_run = spawned[#spawned]
for _, a in ipairs(fallback_run.args) do assert(a ~= "--draft") end
fallback_run.line(json.encode({event = "start", conversation = "v1"}))
fallback_run.line(json.encode({event = "delta", text = "Turned it up."}))
fallback_run.line(json.encode({event = "done", conversation = "v1"}))
assert(#runs_of("commit") == commits_p)
assert(#answers == ans_p + 1 and answers[#answers][3] == "Turned it up.")

-- Cancel rolls back a non-draft live voice turn cleanly without firing answer listeners.
local ans_c = #answers
assert(assistant.ask("Tell me about the history", "v1"))
local live_frag = spawned[#spawned]
live_frag.line(json.encode({event = "start", conversation = "v1"}))
local rolled = false
assistant.cancel(function() rolled = true end)
live_frag.line(json.encode({event = "stopped", conversation = "v1"}))
assert(rolled and #answers == ans_c and #runs_of("rollback") >= 1)

-- A finished draft stays silent if its database commit fails or is cancelled.
local real_run = run
local commit_callback
_G.run = function(command, args, done, how)
    if args[1] == "commit" then commit_callback = done else real_run(command, args, done, how) end
end
local function finished_draft(prompt)
    assert(assistant.draft(prompt, "v1"))
    local d = spawned[#spawned]
    d.line(json.encode({event="start", conversation="v1", draft=true}))
    d.line(json.encode({event="delta", text="A safe answer."}))
    d.line(json.encode({event="done", conversation="v1"}))
    assert(assistant.ask(prompt, "v1", {voice_generation=77}))
    assert(assistant.streaming())
end
finished_draft("A speculative question")
local count = #answers
commit_callback({}, 1)
assert(not assistant.streaming() and #answers == count+1 and answers[#answers][1]=="error")
finished_draft("Another speculative question")
local cancelled_commit = false
assistant.cancel(function() cancelled_commit = true end)
count = #answers
commit_callback(json.encode({event="committed",conversation="v1"}),0)
assert(cancelled_commit and not assistant.streaming() and #answers==count)
-- Reconfiguring local Discord cancels a pending voice commit without speaking its draft.
assistant.configure_discord()
finished_draft("Pending commit during Discord reconfiguration")
values["assistant.discord.read"] = true
assistant.configure_discord()
local rollback_before_discord_commit = #runs_of("rollback")
count = #answers
commit_callback(json.encode({event="committed",conversation="v1"}),0)
assert(not assistant.streaming() and #runs_of("rollback") == rollback_before_discord_commit+1)
assert(#answers == count+1 and answers[#answers][1] == "stopped" and answers[#answers][5] == 77)
values["assistant.discord.read"] = false
assistant.configure_discord()
-- Cancelling a local commit during a backend transition cleans only its local
-- history, without treating an Orca conversation as a SQLite conversation.
finished_draft("A local draft crossing a backend change")
assistant.stop()
values["assistant.backend"] = "orca"
local rollback_before_transition = #runs_of("rollback")
commit_callback(json.encode({event="committed",conversation="v1"}),0)
assert(#runs_of("rollback") == rollback_before_transition+1)
assert(not assistant.streaming())
values["assistant.backend"] = "local"
_G.run = real_run

-- Drafts are disabled for providers without session forking (agy).
emit("assistant_choose", 2)
local sp_agy = #spawned
assert(not assistant.draft("What is the weather in Tokyo", "v1"))
assert(#spawned == sp_agy)
emit("assistant_choose", 0)

-- Effective Discord settings reach normal turns; changes cancel the current turn.
assistant.configure_discord()
assert(assistant.ask("Read Discord", "v1"))
local discord_run = spawned[#spawned]
local discord_flags
for i, flag in ipairs(discord_run.args) do
    if flag == "--discord-settings" then discord_flags = discord_run.args[i + 1] end
end
assert(discord_flags.enabled == false and discord_flags.read == false and discord_flags.post == false)
assert(discord_flags.allowed_user_id == "")
assert(discord_flags.desktop_first == false and discord_flags.desktop_timeout_seconds == 60)
local before_discord_change = #killed
values["assistant.discord.enabled"] = true
values["assistant.discord.read"] = true
assistant.configure_discord()
assert(#killed == before_discord_change + 1 and killed[#killed][2] == "int")
assistant.configure_discord()
assert(#killed == before_discord_change + 1, "unchanged configuration must not interrupt a turn")
discord_run.line(json.encode({event = "stopped", conversation = "v1"}))

-- Changing the required author ID propagates and cancels even if toggles are unchanged.
values["assistant.discord.allowed_user_id"] = "9"
assistant.configure_discord()
assert(assistant.ask("Read only my Discord messages", "v1"))
local author_run = spawned[#spawned]
local author_flags
for i, flag in ipairs(author_run.args) do
    if flag == "--discord-settings" then author_flags = author_run.args[i + 1] end
end
assert(author_flags.allowed_user_id == "9")
local before_author_change = #killed
values["assistant.discord.allowed_user_id"] = ""
assistant.configure_discord()
assert(#killed == before_author_change + 1 and killed[#killed][2] == "int")
author_run.line(json.encode({event = "stopped", conversation = "v1"}))


-- UUID conversation IDs and proactive alerts (`tohru assistant alert`) set ready mark or reload active chat.
local uuid = "4f71ebb6-db33-43e9-af8f-e5bc79a11077"
outputs.show = json.encode({id = uuid, provider = "orca:Tohru", turns = {{role = "assistant", text = "Morning check done."}}})
local alerts = {}
assistant.on_alert(function(cid, msg, speak) alerts[#alerts + 1] = {cid, msg, speak} end)
assistant.close()
text.ai_alert_chat = uuid
text.ai_alert_text = "Overnight build finished"
emit("assistant_alert", 1)
assert(fact.ai_ready and #alerts == 1 and alerts[1][1] == uuid and alerts[1][2] == "Overnight build finished" and alerts[1][3] == true)
emit("assistant_open")
assert(not fact.ai_ready and runs[#runs].args[1] == "show" and runs[#runs].args[2] == uuid)
text.ai_show_request = uuid
emit("assistant_show_request")
assert(runs[#runs].args[1] == "show" and runs[#runs].args[2] == uuid)

-- Opt-in Orca backend passes --backend orca, disables speculative drafts and SQLite rollback, skips remember, and syncs background replies.
values["assistant.backend"] = "orca"
values["assistant.orca_bot"] = "Tohru"
outputs.providers = json.encode({
    {id = "orca:Tohru", name = "Tohru", slot = 0, signed_in = true, model = "Orca · claude", backend = "orca", can_signin = false},
})
outputs.sync = json.encode({
    backend = "orca", online = true, bot = "Tohru", home_chat = uuid,
    chats = {{id = uuid, title = "Home", updated = 100, last_said = "Initial", home = true}},
})
assistant.configure()
assert(text.ai_provider_name == "Tohru" and fact.ai_signed_in and not fact.ai_can_signin)
local sp_orca_draft = #spawned
assert(not assistant.draft("Should never hand off speculatively", uuid))
assert(#spawned == sp_orca_draft)
local rb_before_orca = #runs_of("rollback")
local rb_orca_deleted = nil
assistant.rollback(uuid, function(deleted) rb_orca_deleted = deleted end)
assert(rb_orca_deleted == false and #runs_of("rollback") == rb_before_orca)
local chosen_before = #chosen
emit("assistant_choose", 0)
assert(#chosen == chosen_before)
local runs_before_signin = #runs
emit("assistant_signin", 0)
assert(#runs == runs_before_signin)
assert(has(runs_of("providers")[#runs_of("providers")].args, "--backend", "orca"))
assert(has(runs_of("sync")[#runs_of("sync")].args, "--orca-bot", "Tohru"))
-- Clicking + sets --new-chat on the next send.
emit("assistant_new")
text.ai_prompt = "Start a fresh thread"
emit("assistant_send")
local orca_send = spawned[#spawned]
assert(has(orca_send.args, "--backend", "orca"))
local before_remote_discord_change = #killed
values["assistant.discord.post"] = true
assistant.configure_discord()
assert(#killed == before_remote_discord_change, "local Discord settings must not interrupt a remote Orca turn")
local has_new_chat = false
for _, a in ipairs(orca_send.args) do has_new_chat = has_new_chat or a == "--new-chat" end
assert(has_new_chat)
local rem_before = #runs_of("remember")
orca_send.line(json.encode({event = "start", conversation = uuid}))
orca_send.line(json.encode({event = "done", conversation = uuid}))
assistant.close()
assert(#runs_of("remember") == rem_before)
-- Background sync detects a newer Orca reply while closed and lights up ai_ready.
fact.ai_ready = false
outputs.sync = json.encode({
    backend = "orca", online = true, bot = "Tohru", home_chat = uuid,
    chats = {{id = uuid, title = "Home", updated = 9999999999, last_said = "Overnight update", home = true}},
})
for _ = 1, 16 do every_cbs[1]() end
assert(fact.ai_ready)

-- An explicit task acknowledgement detaches only its matching voice poller.
assert(assistant.ask("Create a workspace", uuid, {voice_generation=17}))
local task_poller = spawned[#spawned]
task_poller.line({event="start", conversation=uuid, voice_request="task-request"})
assistant.voice_ready(uuid, "task-request")
assert(runs[#runs].args[1] == "voice-ready" and has(runs[#runs].args, "--conversation", uuid)
    and has(runs[#runs].args, "--request", "task-request"))
assert(not assistant.detach_voice("other-request") and assistant.streaming())
assert(assistant.detach_voice("task-request") and not assistant.streaming())
assert(not assistant.detach_voice("task-request"), "a detached poller cannot be retired twice")
assert(killed[#killed][2] == "int")
assert(assistant.ask("A follow-up question", uuid, {voice_generation=18}))
task_poller.line({event="delta", text="Late original answer"})
task_poller.exit(nil, 0)
assert(assistant.streaming())
spawned[#spawned].line({event="done", conversation=uuid})

-- Idle DM polling defers during active desktop/voice work and never speaks or
-- replaces the selected conversation. The same pending ID is offered again.
values["assistant.backend"] = "local"
values["assistant.discord.destination"] = "dm"
values["assistant.discord.receive"] = true
values["assistant.discord.enabled"] = true
values["assistant.discord.read"] = true
values["assistant.discord.allowed_user_id"] = "9"
outputs.providers = {{id = "claude", name = "Claude", slot = 0, signed_in = true}}
outputs["reset-receiver"] = {event = "reset"}
assistant.configure()
assistant.configure_discord()
local poll_tick = every_cbs[2]
local before_poll = #spawned
poll_tick()
assert(#spawned == before_poll + 1 and spawned[#spawned].args[1] == "poll")
local dm_poll = spawned[#spawned]
fact.ai_voice_conversation = true
dm_poll.line({event = "incoming", id = "101"})
assert(#spawned == before_poll + 1, "active voice defers the pending DM")
fact.ai_voice_conversation = false
text.ai_prompt = "  unfinished desktop draft  "
fact.ai_typed = false -- Actual prompt, not a possibly stale flag, gates dispatch.
dm_poll.line({event = "incoming", id = "101"})
assert(#spawned == before_poll + 1, "desktop composition must leave the DM unclaimed")
dm_poll.exit(nil, 0)
-- The next poll offers the same pending request after the prompt is cleared.
text.ai_prompt = ""
poll_tick()
dm_poll = spawned[#spawned]
local before_dm = #spawned
local before_conversation = assistant.status().conversation
local dm_speech_events = 0
assistant.on_speech(function() dm_speech_events = dm_speech_events + 1 end)
dm_poll.line({event = "incoming", id = "101"})
local dm_turn = spawned[#spawned]
assert(#spawned == before_dm + 1 and has(dm_turn.args, "--discord-incoming", "101"))
assert(dm_turn.how.input == "Discord DM request")
dm_turn.line({event = "start", conversation = "discord-cid"})
dm_turn.line({event = "delta", text = "DM answer"})
assert(assistant.status().conversation == before_conversation)
assert(dm_speech_events == 0, "DM streaming must not invoke speech listeners")
local before_second = #spawned
dm_poll.line({event = "incoming", id = "102"})
assert(#spawned == before_second, "a running DM turn defers the next request")
dm_turn.line({event = "done", conversation = "discord-cid"})
assert(not assistant.streaming())
assert(dm_speech_events == 0)
dm_poll.exit(nil, 0)
-- Typing during a DM turn interrupts it once; Submit queues the desktop text
-- until cancellation settles, then starts the normal desktop turn automatically.
poll_tick()
dm_poll = spawned[#spawned]
dm_poll.line({event = "incoming", id = "102"})
dm_turn = spawned[#spawned]
dm_turn.line({event = "start", conversation = "discord-cid"})
local interrupted_before = #killed
text.ai_prompt = "My desktop message"
emit("text:ai_prompt", text.ai_prompt)
assert(#killed == interrupted_before + 1 and killed[#killed][2] == "int")
assert(text.ai_error:find("in%-flight delivery"), "interruption must not claim that delivery was undone")
local queued_before = #spawned
emit("assistant_send")
assert(#spawned == queued_before and text.ai_prompt == "My desktop message")
emit("assistant_send")
assert(#spawned == queued_before and text.ai_error:find("already queued"), "the one desktop queue slot must reject repeated submissions explicitly")
assert(#killed == interrupted_before + 1, "repeated desktop interaction must not stack cancellations")
dm_turn.line({event = "stopped", conversation = "discord-cid"})
local desktop_turn = spawned[#spawned]
assert(dm_speech_events == 0, "interrupting Discord must not start speech")
assert(#spawned == queued_before + 1 and desktop_turn.how.input == "My desktop message")
assert(not has(desktop_turn.args, "--discord-incoming", "102"))
assert(text.ai_prompt == "" and not fact.ai_typed)
assert(assistant.status().conversation == before_conversation)
desktop_turn.line({event = "done", conversation = before_conversation})
dm_poll.exit(nil, 0)
-- Submit also wins if a prompt-change callback has not run yet.
poll_tick()
dm_poll = spawned[#spawned]
dm_poll.line({event = "incoming", id = "103"})
dm_turn = spawned[#spawned]
text.ai_prompt = "Submit before typed callback"
local direct_submit_before = #spawned
emit("assistant_send")
assert(#spawned == direct_submit_before)
dm_turn.line({event = "stopped", conversation = "discord-cid"})
desktop_turn = spawned[#spawned]
assert(desktop_turn.how.input == "Submit before typed callback")
desktop_turn.line({event = "done", conversation = before_conversation})
dm_poll.exit(nil, 0)
poll_tick()
dm_poll = spawned[#spawned]
local before_revoke = #spawned
values["assistant.discord.receive"] = false
assistant.configure_discord()
dm_poll.line({event = "incoming", id = "103"})
assert(#spawned == before_revoke, "late polls cannot dispatch after revocation")

assert(assistant.ask("A cancellation owned turn", uuid, {voice_generation=99}))
local cancelled_poller = spawned[#spawned]
cancelled_poller.line({event="start", conversation=uuid, voice_request="cancel-owned-request"})
local cancellation_finished = false
assistant.cancel(function() cancellation_finished = true end)
assert(not assistant.detach_voice("cancel-owned-request"), "admission must not fence a pending cancellation callback")
cancelled_poller.line({event="stopped", conversation=uuid})
assert(cancellation_finished and not assistant.streaming())

print("Assistant provider choice, models, streaming, Stop, ready mark, sign-in, history, voice turn, drafts, memory hand-off, alerts and Orca backend fixtures passed")
