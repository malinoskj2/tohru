-- Native capture owns pixels/protocols; this module owns one selection transaction.
return function(options, notify)
    local phase, outputs, tokens, destination = "idle", {}, {}, ""
    local last_result
    local function status(error)
        text.capture_error = error or ""
        text.capture_status = json.encode({phase = phase, error = text.capture_error, result = last_result})
    end
    local function release()
        fact.capture_open, fact.capture_dragging = false, false
        local failures = {}
        for i = 0, 31 do
            text["capture_output_" .. i], text["capture_frame_" .. i] = "", ""
        end
        for _, token in ipairs(tokens) do
            local ok, err = pcall(sys.ask, "capture.release", token)
            if not ok then failures[#failures + 1] = tostring(err) end
        end
        outputs, tokens, destination = {}, {}, ""
        return table.concat(failures, "; ")
    end
    local function cancel(reason)
        local cleanup = release()
        phase = "idle"
        status(reason and (cleanup ~= "" and reason .. "; cleanup: " .. cleanup or reason) or cleanup)
    end
    local function integer(value)
        return type(value) == "number" and value == value and value >= -2147483648 and value <= 2147483647 and value == math.floor(value)
    end
    local function layout(value)
        if type(value) ~= "table" or #value == 0 or #value > 32 then error("Capture requires one to 32 ready outputs") end
        local seen = {}
        for _, output in ipairs(value) do
            if type(output.name) ~= "string" or output.name == "" or seen[output.name] or output.routable ~= true
                or not integer(output.x) or not integer(output.y) or not integer(output.width) or not integer(output.height)
                or output.width <= 0 or output.height <= 0 or not integer(output.x + output.width) or not integer(output.y + output.height)
                or type(output.scale) ~= "number" or output.scale ~= output.scale or output.scale <= 0 or output.scale == math.huge
                or not integer(output.transform) or output.transform < 0 or output.transform > 7 then
                error("Capture output layout is incomplete or cannot be routed natively")
            end
            seen[output.name] = true
        end
        return value
    end
    local function start()
        if phase ~= "idle" then status("A region selection is already active; finish it or press Escape"); return false end
        phase, last_result = "freezing", nil
        status()
        local ok, err = pcall(function()
            local settings = options()
            if type(settings.save) ~= "boolean" or type(settings.copy) ~= "boolean" then error("Capture save/copy settings must be booleans") end
            if not settings.save and not settings.copy then error("Enable saving or image clipboard before capturing") end
            if settings.save then
                destination = sys.ask("capture.destination", settings.directory or "", settings.filename_pattern or "")
                if type(destination) ~= "string" or destination:sub(1, 1) ~= "/" then error("Capture destination is unavailable") end
            end
            outputs = layout(sys.ask("capture.outputs"))
            for _, output in ipairs(outputs) do
                local token = sys.ask("capture.freeze", output.name)
                if type(token) ~= "string" or token:sub(1, 8) ~= "capture:" then error("Native capture returned an invalid freeze token") end
                tokens[#tokens + 1] = token
            end
            -- Routes stay empty until all frames exist; never freeze our own overlay.
            for index, output in ipairs(outputs) do
                local i = index - 1
                fact["capture_x_" .. i], fact["capture_y_" .. i] = output.x, output.y
                text["capture_frame_" .. i] = tokens[index]
                text["capture_output_" .. i] = output.name
            end
            -- Save the selected options through this transaction even if config changes.
            last_result = {copy_requested = settings.copy}
            fact.capture_dragging = false
            fact.capture_open = true
            phase = "selecting"
            status()
        end)
        if not ok then cancel(tostring(err)) end
        return ok
    end
    local function finish()
        if phase ~= "selecting" or not fact.capture_dragging then return end
        local ax, ay, bx, by = fact.capture_start_x, fact.capture_start_y, fact.capture_current_x, fact.capture_current_y
        if type(ax) ~= "number" or type(ay) ~= "number" or type(bx) ~= "number" or type(by) ~= "number"
            or ax ~= ax or ay ~= ay or bx ~= bx or by ~= by then cancel("Invalid capture selection"); return end
        local x, y = math.floor(math.min(ax, bx)), math.floor(math.min(ay, by))
        local width, height = math.ceil(math.max(ax, bx)) - x, math.ceil(math.max(ay, by)) - y
        if width < 2 or height < 2 then cancel(); return end
        if not integer(x) or not integer(y) or not integer(width) or not integer(height) then cancel("Capture selection is outside supported coordinates"); return end
        phase = "exporting"
        fact.capture_open, fact.capture_dragging = false, false
        status()
        local ok, result = pcall(sys.ask, "capture.region", tokens, x, y, width, height, destination, last_result.copy_requested)
        local cleanup = release()
        phase = "idle"
        local message
        if ok and type(result) == "table" and type(result.saved) == "boolean" and type(result.copied) == "boolean" then
            last_result = result
            local outcomes = {}
            if result.saved then outcomes[#outcomes + 1] = "Saved " .. tostring(result.path) end
            if result.copied then outcomes[#outcomes + 1] = "Copied image to clipboard" end
            if #outcomes == 0 then outcomes[1] = "Screenshot could not be saved or copied" end
            message = table.concat(outcomes, " · ")
            local failures = {}
            for _, value in ipairs({result.save_error or "", result.clipboard_error or "", cleanup}) do
                if value ~= "" then failures[#failures + 1] = value end
            end
            status(table.concat(failures, "; "))
        else
            last_result = nil
            message = ok and "Native capture returned an invalid result" or tostring(result)
            status(cleanup ~= "" and message .. "; cleanup: " .. cleanup or message)
        end
        if notify then
            local delivered, failure = pcall(notify, message, text.capture_error)
            if not delivered then status(text.capture_error .. (text.capture_error ~= "" and "; " or "") .. "Result notification unavailable: " .. tostring(failure)) end
        end
    end
    local function topology_changed()
        if phase ~= "selecting" then return end
        local ok, current = pcall(function() return layout(sys.ask("capture.outputs")) end)
        if not ok then cancel("Output layout became unavailable; capture cancelled"); return end
        local by_name = {}
        for _, output in ipairs(current) do by_name[output.name] = output end
        if #current ~= #outputs then cancel("Output layout changed; capture cancelled"); return end
        for _, previous in ipairs(outputs) do
            local now = by_name[previous.name]
            if not now or now.x ~= previous.x or now.y ~= previous.y or now.width ~= previous.width or now.height ~= previous.height
                or now.scale ~= previous.scale or now.transform ~= previous.transform then cancel("Output layout changed; capture cancelled"); return end
        end
    end
    -- Retire any preserved text tokens when logic reloads; shared native state survives Lua.
    for i = 0, 31 do
        local token = text["capture_frame_" .. i]
        if type(token) == "string" and token:sub(1, 8) == "capture:" then pcall(sys.ask, "capture.release", token) end
    end
    cancel()
    on("capture_open_request", start)
    on("capture_finish", finish)
    on("capture_cancel", function() cancel() end)
    on("capture_output_changed", topology_changed)
    return {open = start, cancel = cancel, finish = finish}
end
