-- Deliberately small native-catalog matcher: prefix > substring > subsequence.
-- Luau's string.lower folds ASCII; UTF-8 codepoints stay intact for matching.
return function(query, catalog)
    local pattern = query:match("^%s*(.-)%s*$"):lower()
    if pattern == "" then return catalog end
    local characters = {}
    for _, codepoint in utf8.codes(pattern) do characters[#characters + 1] = utf8.char(codepoint) end
    local function rank(value)
        local candidate = (value or ""):lower()
        if candidate:sub(1, #pattern) == pattern then return 3 end
        if candidate:find(pattern, 1, true) then return 2 end
        local from = 1
        for _, character in ipairs(characters) do
            local at = candidate:find(character, from, true)
            if not at then return 0 end
            from = at + #character
        end
        return 1
    end
    local found = {}
    for index, app in ipairs(catalog) do
        local score = 0
        for _, field in ipairs({"local_name", "name", "generic_name", "id", "exec", "keywords", "categories", "comment"}) do
            score = math.max(score, rank(app[field]))
            if score == 3 then break end
        end
        if score > 0 then found[#found + 1] = {app = app, score = score, index = index} end
    end
    table.sort(found, function(a, b) if a.score == b.score then return a.index < b.index end; return a.score > b.score end)
    local results = {}
    for index = 1, math.min(#found, 50) do results[index] = found[index].app end
    return results
end
