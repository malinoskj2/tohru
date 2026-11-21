-- Every text and input the shell draws names an Apple family; without one Pleamar falls back to fontconfig's sans-serif.
local apple = {"SF Pro Text", "SF Pro Display", "SF Pro Rounded", "SF Compact Text", "SF Compact Display", "SF Compact Rounded", "SF Mono", "New York"}

local function apple_family(name)
    local base = name:match("^[^:]*")
    for _, family in ipairs(apple) do
        if base == family or base:match("^New York ") then return true end
    end
    return false
end

local function skip_string(source, i)
    i = i + 1
    while i <= #source do
        local c = source:sub(i, i)
        if c == "\\" then i = i + 2
        elseif c == '"' then return i + 1
        else i = i + 1 end
    end
    return i
end

local function skip_comment(source, i)
    local finish = source:find("\n", i, true)
    return finish and finish + 1 or #source + 1
end

local function block_end(source, i)
    local depth = 0
    while i <= #source do
        local c = source:sub(i, i)
        if c == '"' then i = skip_string(source, i)
        elseif source:sub(i, i + 1) == "//" then i = skip_comment(source, i)
        else
            if c == "{" then depth = depth + 1
            elseif c == "}" then
                depth = depth - 1
                if depth == 0 then return i end
            end
            i = i + 1
        end
    end
    error("unbalanced braces")
end

local function elements(source)
    local found, i = {}, 1
    while i <= #source do
        local c = source:sub(i, i)
        if c == '"' then i = skip_string(source, i)
        elseif source:sub(i, i + 1) == "//" then i = skip_comment(source, i)
        elseif c:match("[%a_]") then
            local word = source:match("^[%w_]+", i)
            local after = i + #word
            if (word == "text" or word == "input") and not source:sub(i - 1, i - 1):match("[%w_.]") then
                local target_start = after + #source:match("^%s*", after)
                local target_end
                if source:sub(target_start, target_start) == '"' then
                    target_end = skip_string(source, target_start)
                else
                    local name = source:match("^[%w_.$]+", target_start)
                    target_end = name and target_start + #name
                end
                if target_end then
                    local brace = target_end + #source:match("^[ \t]*", target_end)
                    if source:sub(brace, brace) == "{" then
                        local finish = block_end(source, brace)
                        local line = select(2, source:sub(1, i):gsub("\n", "")) + 1
                        found[#found + 1] = {kind = word, target = source:sub(target_start, target_end - 1), body = source:sub(brace, finish), line = line}
                        i = finish + 1
                        goto continue
                    end
                end
            end
            i = after
        else
            i = i + 1
        end
        ::continue::
    end
    return found
end

local files = assert(io.popen("git ls-files 'shell/**/*.plm' 'shell/*.plm'"))
local failures, checked = {}, 0
for path in files:lines() do
    local handle = assert(io.open(path))
    local source = handle:read("a")
    handle:close()
    for _, element in ipairs(elements(source)) do
        checked = checked + 1
        local family = element.body:match('[{;%s]family:%s*"([^"]*)"')
        if not family or not apple_family(family) then
            failures[#failures + 1] = string.format("%s:%d %s %s: %s", path, element.line, element.kind, element.target, family and ('family "' .. family .. '"') or "no family")
        end
    end
end
files:close()

assert(checked > 300, "found only " .. checked .. " text and input elements")
assert(#failures == 0, "text without an Apple family:\n" .. table.concat(failures, "\n"))
