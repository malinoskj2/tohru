-- Execute the production transaction factory with explicit native transport fixtures.
local factory = assert(loadfile("shell/capture/controller.luau"))()
local count = 0
local function check(name, body)
    local ok, failure = pcall(body)
    assert(ok, name .. ": " .. tostring(failure))
    count = count + 1
end
local function output(name, x, width)
    return {name=name, x=x, y=-20, width=width, height=120, scale=1.25, transform=0, routable=true}
end
local function harness(configure)
    fact, text = {}, {}
    local state = {calls={}, handlers={}, notices={}, layouts={output("LEFT", -100, 100), output("RIGHT", 0, 200)}, freezes=0,
        settings={save=true, copy=true, directory="/pictures", filename_pattern="fixed"},
        result={saved=true, copied=true, path="/pictures/fixed-region.png", save_error="", clipboard_error=""}}
    json = {encode=function(value) state.status=value; return "fixture-status" end}
    on = function(name, handler) state.handlers[name]=handler end
    sys = {ask=function(name, ...)
        local args={...}
        state.calls[#state.calls+1]={name=name, args=args}
        if name == "capture.destination" then return "/pictures/fixed-region.png" end
        if name == "capture.outputs" then return state.layouts end
        if name == "capture.freeze" then
            assert(not fact.capture_open, "overlay must stay closed until every frame exists")
            assert(text.capture_output_0 == "" and text.capture_output_1 == "", "routes must remain empty during freeze")
            state.freezes=state.freezes+1
            if state.during_freeze then state.during_freeze(state) end
            if state.freeze_failure == state.freezes then error("freeze failed") end
            return "capture:" .. args[1] .. "?" .. state.freezes
        end
        if name == "capture.region" then
            assert(not fact.capture_open and not fact.capture_dragging, "export closes the selection")
            state.export=args
            if state.export_failure then error("export failed") end
            return state.result
        end
        if name == "capture.release" then
            state.released=state.released or {}
            state.released[#state.released+1]=args[1]
            if state.release_failure then error("release failed") end
            return {released=true}
        end
        error("unexpected native query: " .. name)
    end}
    if configure then configure(state) end
    state.controller=factory(function() return state.settings end, function(message, failure)
        state.notices[#state.notices+1]={message=message, failure=failure}
        if state.notify_failure then error("notification service unavailable") end
    end)
    return state
end
local function drag(state)
    fact.capture_start_x, fact.capture_start_y = 99.2, 55.8
    fact.capture_current_x, fact.capture_current_y = -70.3, -5.2
    fact.capture_dragging = true
    state.controller.finish()
end
check("all outputs frozen before routes; reversed cross-output selection and transaction settings", function()
    local state=harness()
    assert(state.controller.open() and fact.capture_open and state.freezes == 2)
    assert(text.capture_output_0 == "LEFT" and fact.capture_x_0 == -100 and text.capture_frame_1 == "capture:RIGHT?2")
    state.settings.copy=false
    drag(state)
    local args=state.export
    assert(#args[1] == 2 and args[2] == -71 and args[3] == -6 and args[4] == 171 and args[5] == 62)
    assert(args[6] == "/pictures/fixed-region.png" and args[7] == true)
    assert(#state.released == 2 and #state.notices == 1 and state.status.phase == "idle")
    assert(text.capture_output_0 == "" and text.capture_frame_1 == "" and not fact.capture_open)
end)
check("partial freeze failure releases existing frame and never opens", function()
    local state=harness(function(s) s.freeze_failure=2 end)
    assert(not state.controller.open())
    assert(#state.released == 1 and state.released[1] == "capture:LEFT?1")
    assert(not fact.capture_open and text.capture_output_0 == "" and text.capture_error:find("freeze failed"))
end)
check("Escape and plain click release all frames without export", function()
    local state=harness()
    assert(state.controller.open())
    state.handlers.capture_cancel()
    assert(#state.released == 2 and not state.export)
    assert(state.controller.open())
    fact.capture_start_x, fact.capture_current_x = 1, 1
    fact.capture_start_y, fact.capture_current_y = 1, 1
    fact.capture_dragging = true
    state.handlers.capture_finish()
    assert(#state.released == 4 and not state.export and not fact.capture_open)
end)
check("disabled side effects and invalid routing reject before freeze", function()
    local state=harness(function(s) s.settings.save=false; s.settings.copy=false end)
    assert(not state.controller.open() and state.freezes == 0)
    state=harness(function(s) s.layouts[1].routable=false end)
    assert(not state.controller.open() and state.freezes == 0)
    state=harness(function(s) s.layouts[2].name="LEFT" end)
    assert(not state.controller.open() and state.freezes == 0)
end)
check("copy-only skips filename and reports honest partial success", function()
    local state=harness(function(s)
        s.settings.save=false
        s.result={saved=false,copied=true,save_error="save unavailable",clipboard_error=""}
    end)
    assert(state.controller.open())
    drag(state)
    assert(state.export[6] == "" and state.notices[1].message == "Copied image to clipboard")
    assert(text.capture_error == "save unavailable" and state.status.result.copied)
    for _, call in ipairs(state.calls) do assert(call.name ~= "capture.destination") end
end)
check("export and cleanup failure retain error and finish idle", function()
    local state=harness(function(s) s.export_failure=true; s.release_failure=true end)
    assert(state.controller.open())
    drag(state)
    assert(#state.released == 2 and text.capture_error:find("export failed") and text.capture_error:find("release failed"))
    assert(state.status.phase == "idle" and not fact.capture_open and #state.notices == 1)
end)
check("notification failure does not erase actual save/copy result", function()
    local state=harness(function(s) s.notify_failure=true end)
    assert(state.controller.open())
    drag(state)
    assert(state.status.result.saved and state.status.result.copied and text.capture_error:find("Result notification unavailable"))
end)
check("duplicate request preserves current frozen transaction", function()
    local state=harness()
    assert(state.controller.open())
    assert(not state.controller.open() and state.freezes == 2 and fact.capture_open and not state.released)
    assert(text.capture_error:find("already active"))
    state.controller.cancel()
end)
check("topology notification ignores equal layout and cancels changed scale/origin", function()
    local state=harness()
    assert(state.controller.open())
    state.handlers.capture_output_changed()
    assert(fact.capture_open and not state.released)
    -- Native query returns fresh metadata, not a mutation of the original snapshot.
    state.layouts={output("LEFT", -100, 100), output("RIGHT", 0, 200)}
    state.layouts[2].scale=2
    state.handlers.capture_output_changed()
    assert(not fact.capture_open and #state.released == 2 and text.capture_error:find("layout changed"))
end)
check("lost output metadata cancels and reload releases preserved tokens", function()
    local state=harness(function() text.capture_frame_0="capture:PREVIOUS?77" end)
    assert(state.released[1] == "capture:PREVIOUS?77")
    assert(state.controller.open())
    state.layouts={}
    state.handlers.capture_output_changed()
    assert(not fact.capture_open and #state.released == 3 and text.capture_error:find("unavailable"))
end)
check("layout changing during freeze releases all frames before any overlay opens", function()
    local state=harness(function(s)
        s.during_freeze=function(current)
            if current.freezes == 1 then
                current.layouts={output("LEFT", -90, 100), output("RIGHT", 10, 200)}
            end
        end
    end)
    assert(not state.controller.open() and state.freezes == 2 and #state.released == 2)
    assert(not fact.capture_open and text.capture_output_0 == "" and text.capture_error:find("changed while freezing"))
end)
print("capture production controller: " .. count .. " cases passed")
