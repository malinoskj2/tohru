-- One fixed native notification route; classify actual export data, never prose.
local function bounded(value)
    value = tostring(value or "")
    if not utf8.len(value) then return "Screenshot result contained invalid UTF-8" end
    local chunks, bytes, count = {}, 0, 0
    for _, cp in utf8.codes(value) do
        if cp >= 32 and cp ~= 127 or cp == 9 or cp == 10 or cp == 13 then
            local character = utf8.char(cp)
            if count == 512 or bytes + #character > 2048 then break end
            chunks[#chunks+1], count, bytes = character, count+1, bytes+#character
        end
    end
    return #chunks > 0 and table.concat(chunks) or "Screenshot result unavailable"
end
return function(message, failure, result)
    local status = "error"
    if type(result) == "table" and (result.saved == true or result.copied == true) then
        if failure and failure ~= "" or result.saved == true and result.durable == false then status = "partial"
        elseif result.saved and result.copied then status = "saved_copied"
        elseif result.saved then status = "saved"
        else status = "copied" end
    end
    if failure and failure ~= "" then message = tostring(message) .. " · " .. tostring(failure)
    elseif status == "partial" then message = tostring(message) .. " · Saved file durability is unconfirmed" end
    local payload = json.encode({status=status, message=bounded(message)})
    if type(payload) ~= "string" or #payload > 3072 then error("Capture notification exceeds its native payload bound") end
    return sys.call("say", "tohru-notifications", "submit capture_result " .. payload)
end
