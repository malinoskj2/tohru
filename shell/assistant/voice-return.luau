-- The desktop alone decides whether an analyzed continuation is still welcome.
return function()
    local origin, consumed, order = nil, {}, {}
    return {
        begin = function(chat, request)
            origin = request and request ~= "" and {chat = chat, request = request} or nil
        end,
        interrupt = function() origin = nil end,
        current = function(chat, request)
            return origin ~= nil and origin.chat == chat and origin.request == request
        end,
        take = function(chat, request, completion, visible, busy)
            if not completion or completion == "" or consumed[completion] then return false, "duplicate" end
            consumed[completion] = true
            order[#order + 1] = completion
            if #order > 128 then consumed[table.remove(order, 1)] = nil end
            if not origin or origin.chat ~= chat or origin.request ~= request then return false, "origin_mismatch" end
            if not visible then return false, "hidden" end
            if busy then return false, "busy" end
            return true, "matched"
        end,
    }
end
