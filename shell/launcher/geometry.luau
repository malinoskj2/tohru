-- Rows of 82 px (78 px tiles and a 4 px gap) under 26 px section headings; Home's applications get a
-- heading of their own below the pins.
local heading, row = 26, 82
local function position(columns, pinned, index)
    if index < pinned then return index % columns, heading + math.floor(index / columns) * row end
    local rest = index - pinned
    local top = heading + (pinned > 0 and math.ceil(pinned / columns) * row + heading or 0)
    return rest % columns, top + math.floor(rest / columns) * row
end
local function content(columns, pinned, total)
    if total == 0 then return 0 end
    local rows = math.ceil(pinned / columns) + math.ceil((total - pinned) / columns)
    return heading + rows * row + (pinned > 0 and heading or 0) + 8
end
local function neighbour(columns, pinned, index, total, down)
    local x, y = position(columns, pinned, index)
    local nearest
    for candidate = 0, total - 1 do
        local _, cy = position(columns, pinned, candidate)
        local dy = (cy - y) * down
        if dy > 10 and (not nearest or dy < nearest) then nearest = dy end
    end
    if not nearest then return index end
    local chosen, distance = index, math.huge
    for candidate = 0, total - 1 do
        local cx, cy = position(columns, pinned, candidate)
        local dy = (cy - y) * down
        if dy > 10 and dy < nearest + 6 and math.abs(cx - x) < distance then
            chosen, distance = candidate, math.abs(cx - x)
        end
    end
    return chosen
end
-- Scrolls just enough to show the tile, with its section's heading when it is in the first row.
local function follow(columns, pinned, index, scroll, viewport)
    local _, y = position(columns, pinned, index)
    if y - heading < scroll then return math.max(0, y - heading) end
    if y + row + 4 > scroll + viewport then return y + row + 4 - viewport end
    return scroll
end
return {position = position, content = content, neighbour = neighbour, follow = follow, heading = heading, row = row, cell = 80}
