-- The root settings owner supplies persistence/provenance; this module only routes views.
return function(assign)
    local slots, paths = {}, {}
    local static = {png = true, jpg = true, jpeg = true, webp = true}
    local video = {mp4 = true, mkv = true, webm = true, mov = true, gif = true}
    -- Match Rust str::trim (Unicode White_Space), which native screens uses.
    local function space(cp)
        return (cp >= 9 and cp <= 13) or cp == 32 or cp == 133 or cp == 160
            or cp == 5760 or (cp >= 8192 and cp <= 8202) or cp == 8232 or cp == 8233
            or cp == 8239 or cp == 8287 or cp == 12288
    end
    local function altered_by_trim(output)
        local first, last
        for _, cp in utf8.codes(output) do first = first or cp; last = cp end
        return (first and space(first)) or (last and space(last))
    end
    local function validate(output, path)
        if type(output) ~= "string" or #output == 0 or #output > 255
            or output:find("[,%z\r\n]") or altered_by_trim(output) then
            error("Wallpaper output must be one native output name (no commas or surrounding whitespace)")
        end
        if type(path) ~= "string" or #path > 4096 or path:sub(1, 1) ~= "/" or path:find("%z") then
            error("Wallpaper path must be an absolute local file path of at most 4096 bytes")
        end
        local ext = path:match("%.([^./]+)$")
        ext = ext and ext:lower()
        if static[ext] then return "image" end
        if video[ext] then return "video" end
        error("Unsupported wallpaper file type")
    end
    local function publish(enabled, assignments)
        if type(enabled) ~= "boolean" or type(assignments) ~= "table" then
            error("Wallpaper publication requires an enabled boolean and an assignment table")
        end
        local images, videos, ordered, count = {}, {}, {}, 0
        -- Validate the whole snapshot before changing any route or image.
        for output, path in pairs(assignments) do
            local kind = validate(output, path)
            count = count + 1
            if count > 32 then error("Wallpaper assignment capacity is 32 outputs; nothing was published") end
            if kind == "image" then images[output] = path; ordered[#ordered + 1] = output
            else videos[output] = path end
        end
        table.sort(ordered)
        -- Retain slots across reload publications/reassignments. Detach before reuse,
        -- Native image identity reset is also required before a reused route paints.
        for i = 0, 31 do
            if slots[i] and not images[slots[i]] then
                text["wallpaper_output_" .. i] = ""
                text["wallpaper_path_" .. i] = ""
                slots[i], paths[i] = nil, nil
            end
        end
        local used = {}
        for i = 0, 31 do if slots[i] then used[slots[i]] = i end end
        for _, output in ipairs(ordered) do
            local i = used[output]
            if i == nil then
                for available = 0, 31 do if not slots[available] then i = available; break end end
                slots[i] = output
            end
            if paths[i] ~= images[output] then
                text["wallpaper_path_" .. i] = images[output]
                paths[i] = images[output]
            end
            text["wallpaper_output_" .. i] = output
        end
        fact.wallpaper_enabled = enabled
        -- Video projection goes to the approved integrated renderer, never the image decoder.
        return videos
    end
    local function request(clear)
        local output, path = text.wallpaper_request_output, text.wallpaper_request_path
        local ok, err = pcall(function()
            if clear then validate(output, "/validation.png") else validate(output, path) end
            if not assign then error("Wallpaper assignment settings integration is unavailable") end
            if clear then assign(output, nil) else assign(output, path) end
        end)
        text.wallpaper_error = ok and "" or tostring(err)
    end
    on("wallpaper_assign", function() request(false) end)
    on("wallpaper_clear", function() request(true) end)
    local function randomize(assignments, outputs, files, canonical)
        -- The catalog boundary supplies already-shuffled files and canonical identity.
        -- Keep the retained preference order without doing filesystem I/O in Luau.
        if type(assignments) ~= "table" or type(outputs) ~= "table" or type(files) ~= "table"
            or type(canonical) ~= "function" then error("Randomization requires assignments, outputs, a shuffled catalog and canonical identities") end
        if #files > 4096 then error("Wallpaper catalog exceeds 4096 files") end
        local next, catalog, used, selected, seen, identities = {}, {}, {}, {}, {}, {}
        local function identity(path)
            if identities[path] then return identities[path] end
            local key = canonical(path)
            if type(key) ~= "string" or #key == 0 then error("Wallpaper canonical identity unavailable") end
            identities[path] = key
            return key
        end
        local count = 0
        for output, path in pairs(assignments) do
            validate(output, path)
            count = count + 1
            if count > 32 then error("Wallpaper assignment capacity is 32 outputs; nothing was changed") end
            next[output] = path
        end
        for _, path in ipairs(files) do
            validate("catalog", path)
            catalog[identity(path)] = true
        end
        if #files == 0 then return next end
        if #outputs == 0 then
            for output, path in pairs(assignments) do
                if validate(output, path) == "video" or catalog[identity(path)] then selected[#selected + 1] = output end
            end
            table.sort(selected)
        else
            for _, output in ipairs(outputs) do
                validate(output, "/validation.png")
                if not seen[output] then selected[#selected + 1], seen[output] = output, true end
            end
        end
        if #selected == 0 or #files == 0 then return next end
        local rerolled = {}
        for _, output in ipairs(selected) do rerolled[output] = true end
        for output, path in pairs(assignments) do
            if not rerolled[output] then used[identity(path)] = true end
        end
        for _, output in ipairs(selected) do
            local previous = assignments[output] and identity(assignments[output])
            local chosen
            for _, path in ipairs(files) do
                local key = identity(path)
                if key ~= previous and not used[key] then chosen = path; break end
            end
            if not chosen then
                for _, path in ipairs(files) do if identity(path) ~= previous then chosen = path; break end end
            end
            chosen = chosen or files[1]
            used[identity(chosen)] = true
            next[output] = chosen
        end
        count = 0
        for _ in pairs(next) do count = count + 1 end
        if count > 32 then error("Wallpaper assignment capacity is 32 outputs; nothing was changed") end
        return next
    end
    return {publish = publish, validate = validate, randomize = randomize}
end
