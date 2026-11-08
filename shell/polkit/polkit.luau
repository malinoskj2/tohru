-- Dedicated native agent; native authority owns identity, helper and request queue.
local current = { pending = false, asking = false }
text.answer = ""
text.status = ""
fact.answering = false
local function invoke(command, answer)
    local ok, err = pcall(function()
        if answer ~= nil then sys.call(command, answer) else sys.call(command) end
    end)
    text.status = ok and "" or tostring(err)
    return ok
end
local function answer(value)
    if not current.pending or not current.asking or fact.answering then text.answer = ""; return end
    fact.answering = true
    text.answer = ""
    if invoke("polkit.answer", value) then current.asking = false end
    fact.answering = false
end
local function cancel_request()
    text.answer = ""
    if current.pending then invoke("polkit.cancel") end
end
sys.watch("polkit", function(state)
    current = state
    if not state.pending or not state.asking then text.answer = "" end
    if state.asking then focus("answer") end
end)
on("submit:answer", answer)
on("answer_request", function() answer(text.answer) end)
on("cancel_request", cancel_request)
on("key", function(key) if key == "Escape" then cancel_request() end end)
