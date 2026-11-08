-- Retained visual-row policy, including the Home section gap.
local function position(columns, pinned, index)
    if pinned == 0 then return index % columns, math.floor(index / columns) * 72 end
    if index < pinned then return index % columns, 29 + math.floor(index / columns) * 72 end
    local rest = index - pinned
    return rest % columns, 58 + math.ceil(pinned / columns) * 72 + math.floor(rest / columns) * 72
end
local function content(columns, pinned, total)
    local rows = pinned > 0 and (math.ceil(pinned / columns) + math.ceil((total - pinned) / columns)) or math.ceil(total / columns)
    return rows * 72 + (pinned > 0 and 58 or 0) + 12
end
local function neighbour(columns, pinned, index, total, down)
    local x, y = position(columns, pinned, index)
    local row
    for candidate = 0, total - 1 do
        local _, cy = position(columns, pinned, candidate)
        local dy = (cy - y) * down
        if dy > 10 and (not row or dy < row) then row = dy end
    end
    if not row then return index end
    local chosen, distance = index, math.huge
    for candidate = 0, total - 1 do
        local cx, cy = position(columns, pinned, candidate)
        local dy = (cy - y) * down
        if dy > 10 and dy < row + 6 and math.abs(cx - x) < distance then
            chosen, distance = candidate, math.abs(cx - x)
        end
    end
    return chosen
end
local function follow(columns, pinned, index, scroll, viewport)
    local _, y = position(columns, pinned, index)
    if y - 30 < scroll then return math.max(0, y - 30) end
    if y + 82 > scroll + viewport then return y + 82 - viewport end
    return scroll
end
return {position = position, content = content, neighbour = neighbour, follow = follow}
