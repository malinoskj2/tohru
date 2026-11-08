-- Fresh scene-owned state; never import j2bar's mutable state.
local function valid(id) return type(id) == "string" and #id > 0 and #id <= 255 end
local function unique(list, limit)
    local result, seen = {}, {}
    if type(list) ~= "table" then return result end
    for _, id in ipairs(list) do
        if valid(id) and not seen[id] and #result < limit then
            result[#result + 1], seen[id] = id, true
        end
    end
    return result
end
return function(saved)
    saved = type(saved) == "table" and saved or {}
    local counts, ordered = {}, {}
    if type(saved.counts) == "table" then
        for id, count in pairs(saved.counts) do
            if valid(id) and type(count) == "number" and count > 0 and count <= 4294967295 and count == math.floor(count) then
                ordered[#ordered + 1] = {id = id, count = count}
            end
        end
    end
    table.sort(ordered, function(a, b) if a.count == b.count then return a.id < b.id end; return a.count > b.count end)
    for i = 1, math.min(#ordered, 2000) do counts[ordered[i].id] = ordered[i].count end
    local state = {counts = counts, pins = unique(saved.pins, 64), recent = unique(saved.recent, 6)}
    local function pinned(id)
        for _, pin in ipairs(state.pins) do if pin == id then return true end end
        return false
    end
    local function toggle(id)
        if not valid(id) then return end
        for index, pin in ipairs(state.pins) do
            if pin == id then table.remove(state.pins, index); return end
        end
        if #state.pins < 64 then state.pins[#state.pins + 1] = id end
    end
    local function record(id)
        if not valid(id) then return end
        for index = #state.recent, 1, -1 do if state.recent[index] == id then table.remove(state.recent, index) end end
        table.insert(state.recent, 1, id)
        if #state.recent > 6 then table.remove(state.recent) end
        if not counts[id] then
            local size, least, least_count = 0, nil, nil
            for key, count in pairs(counts) do
                size = size + 1
                if not least or count < least_count or (count == least_count and key > least) then least, least_count = key, count end
            end
            if size >= 2000 then counts[least] = nil end
        end
        counts[id] = math.min((counts[id] or 0) + 1, 4294967295)
    end
    return {state = state, pinned = pinned, toggle = toggle, record = record}
end
