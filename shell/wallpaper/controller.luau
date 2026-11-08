-- The root settings owner supplies persistence/provenance; this module only routes views.
return function(assign)
    local slots, paths = {}, {}
    local static = {png = true, jpg = true, jpeg = true, webp = true}
    local video = {mp4 = true, mkv = true, webm = true, mov = true, gif = true}
    local function validate(output, path)
        if type(output) ~= "string" or #output == 0 or #output > 255
            or output:find("[,%z\r\n]") or output:match("^%s") or output:match("%s$") then
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
        -- so another output can never display the previous output's picture.
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
    return {publish = publish, validate = validate}
end
