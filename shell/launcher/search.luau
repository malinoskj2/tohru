local TIER = 2 * 5 * 65535
local function list(value)
    local items = {}
    for item in (value or ""):gmatch("[^;]+") do items[#items + 1] = item:lower() end
    return items
end

return function(fuzzy)
    local function rank(app, pattern, score)
        local name = (app.local_name ~= "" and app.local_name or app.name or ""):lower()
        local found = score(name)
        if found then return found * 5 + (name:sub(1, #pattern) == pattern and TIER or 0) + 2 * TIER end
        local untranslated = (app.name or ""):lower()
        found = untranslated ~= "" and untranslated ~= name and score(untranslated)
        if found then return found * 5 + TIER end
        local best
        local function consider(value, weight)
            found = score(value)
            if found and (not best or found * weight > best) then best = found * weight end
        end
        consider((app.generic_name or ""):lower(), 2)
        for _, keyword in ipairs(list(app.keywords)) do consider(keyword, 0.8) end
        for _, category in ipairs(list(app.categories)) do consider(category, 0.3) end
        consider((app.id or ""):lower(), 1.5)
        consider((app.exec or ""):lower(), 1)
        return best
    end
    local function matches(catalog, pattern, typos)
        local score, found = fuzzy(pattern, typos), {}
        for index, app in ipairs(catalog) do
            local value = rank(app, pattern, score)
            if value then found[#found + 1] = {app = app, score = value, index = index} end
        end
        return found
    end
    return function(query, catalog, counts)
        local pattern = query:lower()
        if pattern == "" then return {} end
        local found = matches(catalog, pattern, 0)
        if #found == 0 then found = matches(catalog, pattern, math.floor((utf8.len(pattern) or #pattern) / 4)) end
        for _, result in ipairs(found) do
            result.score = result.score + math.min((counts and counts[result.app.id] or 0) * 2.5, 12.5)
        end
        table.sort(found, function(a, b) if a.score == b.score then return a.index < b.index end; return a.score > b.score end)
        local results, scores = {}, {}
        for index = 1, math.min(#found, 50) do results[index], scores[index] = found[index].app, found[index].score end
        return results, scores
    end
end
