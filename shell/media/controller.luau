-- The player report is authoritative after each command.
return function()
    local rail = require("bar/rail")(5)
    local clock = os.clock
    local report = {players = {}}
    local heard_at, heard_pos = clock(), 0
    local active = -1
    local pending, timer = nil, nil
    local art_cache = {}
    local ART_RETRY_SEC = 60
    local function rail_x(seconds)
        local length = fact.media_length or 0
        if length <= 0 then return 8 end
        return 8 + 188 * math.clamp(seconds / length, 0, 1)
    end
    local function estimate()
        local position = heard_pos
        if report.playing then position = position + (clock() - heard_at) * (report.rate or 1) end
        local length = report.length or 0
        if length > 0 then position = math.min(position, length) end
        return math.max(0, position)
    end
    local function seed(position)
        fact.media_seed = position
        fact.media_seed_n = (fact.media_seed_n or 0) + 1
    end
    local function fail(message)
        text.media_error = message
        fact.media_failed = message ~= ""
    end
    local function call(command, ...)
        local ok, err = pcall(sys.call, command, ...)
        fail(ok and "" or tostring(err))
        return ok
    end
    local function rows() return math.min(5, fact["media_players.count"] or 0) end
    local function player_id(index)
        if type(index) ~= "number" or index ~= math.floor(index) or index < 0 or index >= rows() then return nil end
        local id = text["media_players." .. index .. ".id"]
        if not id or id == "" then return nil end
        return id
    end
    -- The reported player: the chosen one, else the one whose bus name and song match the report.
    local function find_active(players)
        for i, p in ipairs(players) do if p.chosen then return i - 1 end end
        local short = report.player or ""
        local fallback = -1
        for i, p in ipairs(players) do
            local id = p.id or ""
            local head = id:match("^([^%.]+)") or id
            if head == short then
                if (p.title or "") == (report.title or "") and (p.playing == true) == (report.playing == true) then return i - 1 end
                if fallback < 0 then fallback = i - 1 end
            end
        end
        return fallback
    end
    -- A row's second line: the app, and the artist where there is one.
    local function sub(p)
        local name = (p.name or "") ~= "" and p.name or (p.id or "")
        if (p.artist or "") == "" then return name end
        return name .. " · " .. p.artist
    end
    local refresh_art
    local function resolve_art(path)
        if type(path) ~= "string" or path == "" then return "" end
        if path:sub(1, 1) == "/" then return path end
        if not path:match("^https?://[^%s]+$") or #path > 2048 then return "" end
        local cached = art_cache[path]
        if type(cached) == "string" then return cached end
        if type(cached) == "table" then
            if cached.pending then return "" end
            if cached.failed_at and (clock() - cached.failed_at) < ART_RETRY_SEC then return "" end
        end
        if type(run) ~= "function" then return "" end
        art_cache[path] = {pending = true}
        pcall(run, "tohru-media-art", {path}, function(out, code)
            local local_path = type(out) == "string" and out:match("^%s*(/[^\r\n]-)%s*$") or nil
            if code == 0 and local_path and local_path ~= "" then
                art_cache[path] = local_path
                refresh_art()
            else
                art_cache[path] = {failed_at = clock()}
            end
        end)
        return ""
    end

    local function build_players(players, found)
        local order = {}
        if found >= 0 then order[1] = players[found + 1] end
        for i, p in ipairs(players) do if i - 1 ~= found then order[#order + 1] = p end end
        local list = {}
        for i, p in ipairs(order) do
            if i > 8 then break end
            local art = resolve_art(p.art)
            list[i] = {id = p.id or "", name = (p.name or "") ~= "" and p.name or (p.id or ""), title = (p.title or "") ~= "" and p.title or (p.name or ""), sub = sub(p), art = art, has_art = art ~= "", playing = p.playing == true, active = i - 1 == active}
        end
        return list
    end

    function refresh_art()
        local players = type(report.players) == "table" and report.players or {}
        local found = find_active(players)
        active = found >= 0 and 0 or -1
        model.media_players = build_players(players, found)
        local art = resolve_art(report.art)
        text.media_art = art
        fact.media_has_art = art ~= ""
    end

    local function observe(value)
        value = type(value) == "table" and value or {}
        local previous = report
        report = value
        local players = type(value.players) == "table" and value.players or {}
        local has = type(value.player) == "string" and value.player ~= ""
        local position = tonumber(value.position) or 0
        local length = math.max(0, tonumber(value.length) or 0)
        local expected = estimate()
        heard_at, heard_pos = clock(), position
        -- The reported player heads the rows; the others keep the bus's order.
        local found = find_active(players)
        active = found >= 0 and 0 or -1
        local list = build_players(players, found)
        model.media_players = list
        local entry = list[active + 1]
        local art = resolve_art(value.art)
        fact.media_has = has
        fact.media_playing = has and value.playing == true
        fact.media_can_seek = has and value.can_seek == true
        fact.media_length = length
        fact.media_rate = tonumber(value.rate) or 1
        local volume = tonumber(value.volume)
        fact.media_volume = (has and volume and volume >= 0) and math.min(volume, 1) or -1
        text.media_title = (value.title or "") ~= "" and value.title or (entry and entry.name or "")
        text.media_artist = value.artist or ""
        fact.media_has_artist = (value.artist or "") ~= ""
        text.media_name = entry and entry.name or (value.player or "")
        text.media_art = art
        fact.media_has_art = art ~= ""
        if not has and fact.media_unfolded then fact.media_unfolded = false end
        if fact.media_focus > rows() + 1 then fact.media_focus = rows() + 1 end
        if not fact.media_seeking then
            seed(position)
            -- A jump (a seek, another song, another player) carries the bead with the ribbon's wave.
            local same = previous.title == value.title and previous.player == value.player
            if fact.media_open and (not same or math.abs(position - expected) > 1.5) then rail.glide(rail_x(position)) end
        end
        if fact.media_vol_dragging and not pending and not timer and (volume == nil or math.abs(volume - fact.media_vol_requested) < 0.006) then
            fact.media_vol_dragging = false
        end
    end

    local function close()
        if fact.media_seeking then fact.media_seeking = false end
        fact.media_open = false
        fact.media_unfolded = false
        fact.media_keyed = false
    end
    local function open()
        fact.media_unfolded = false
        fact.media_focus = 0
        fact.media_keyed = false
        fail("")
        local position = estimate()
        seed(position)
        rail.place(rail_x(position))
        fact.media_open = true
    end
    local function toggle() if fact.media_open then close() else open() end end

    local function flush()
        if timer then cancel(timer); timer = nil end
        if pending ~= nil then
            local v = pending
            pending = nil
            call("media.volume", v)
        end
    end
    local function queue_volume(v)
        if not (fact.media_has and fact.media_volume >= 0) then return end
        if type(v) ~= "number" or v ~= v then return end
        v = math.clamp(v, 0, 1)
        fact.media_vol_requested = v
        fact.media_vol_dragging = true
        pending = v
        if not timer then timer = after(50, function() timer = nil; flush() end) end
    end
    local function settle_volume()
        flush()
        after(500, function() if not pending and not timer then fact.media_vol_dragging = false end end)
    end

    local function seek_to(fraction)
        local length = fact.media_length or 0
        if type(fraction) ~= "number" or fraction ~= fraction or length <= 0 then return nil end
        return math.clamp(fraction, 0, 1) * length
    end
    local function seek_press(fraction)
        local to = seek_to(fraction)
        if not to or not fact.media_can_seek then fact.media_seeking = false; return end
        fact.media_seeking = true
        fact.media_requested = to
        rail.chase(rail_x(to))
    end
    local function seek_drag(fraction)
        local to = seek_to(fraction)
        if not to or not fact.media_seeking then return end
        fact.media_requested = to
        rail.chase(rail_x(to))
    end
    local function seek_release()
        if not fact.media_seeking then return end
        local to = fact.media_requested
        -- The clock carries on from where the bead was let go; the player's report corrects it.
        if call("media.seek", to) then heard_at, heard_pos = clock(), to; seed(to)
        else seed(estimate()); rail.glide(rail_x(estimate())) end
        fact.media_seeking = false
    end

    local function choose(index)
        local id = player_id(index)
        if not id then return false end
        return call("media.choose", id)
    end
    local function row_play(index)
        if choose(index) then call("media.toggle") end
    end
    local function step_volume(delta)
        if type(delta) ~= "number" or delta ~= delta then return end
        local base = fact.media_vol_dragging and fact.media_vol_requested or fact.media_volume
        if not (fact.media_has and fact.media_volume >= 0) then return end
        queue_volume(base + 0.05 * math.sign(delta))
        settle_volume()
    end
    local function focus_move(delta)
        if not fact.media_unfolded or type(delta) ~= "number" then return end
        fact.media_keyed = true
        fact.media_focus = math.clamp(fact.media_focus + delta, 0, rows() + 1)
    end
    local function focus_step(delta)
        if not fact.media_unfolded or type(delta) ~= "number" then return end
        fact.media_keyed = true
        if fact.media_focus == rows() + 1 then step_volume(delta)
        elseif fact.media_focus == 0 and fact.media_can_seek then call("media.step", 5 * math.sign(delta)) end
    end
    local function activate()
        if not fact.media_unfolded then call("media.toggle"); return end
        fact.media_keyed = true
        local focus = fact.media_focus
        if focus == 0 then call("media.toggle")
        elseif focus <= rows() then choose(focus - 1) end
    end

    on("media_open", open)
    on("media_close", close)
    on("media_toggle", toggle)
    on("media_fold_toggle", function()
        if not fact.media_has then fact.media_unfolded = false; return end
        fact.media_unfolded = not fact.media_unfolded
        fact.media_focus = 0
    end)
    on("media_play", function() if fact.media_has then call("media.toggle") end end)
    on("media_next", function() if fact.media_has then call("media.next") end end)
    on("media_previous", function() if fact.media_has then call("media.previous") end end)
    on("media_step", function(seconds)
        if fact.media_has and fact.media_can_seek and type(seconds) == "number" and seconds == seconds then call("media.step", seconds) end
    end)
    on("media_seek_press", seek_press)
    on("media_seek_drag", seek_drag)
    on("media_seek_release", seek_release)
    on("media_choose", choose)
    on("media_row_play", row_play)
    on("media_volume_press", queue_volume)
    on("media_volume_drag", queue_volume)
    on("media_volume_release", settle_volume)
    on("media_volume_step", step_volume)
    on("media_focus_move", focus_move)
    on("media_focus_step", focus_step)
    on("media_activate", activate)
    on("fact:media_open", function(value) if not value then close() end end)
    sys.watch("media", observe)
    return {open = open, close = close, toggle = toggle}
end
