-- What is shown always comes from the owner's units: the assignments, the files and each mpvpaper, as
-- tohru-wallpaper reports them. A preview never writes an assignment; only a choice does.
return function()
    local helper = "tohru-wallpaper"
    local page_size = 6
    local tabs_rail = require("bar/rail")(3)
    local scrub_rail = require("bar/rail")(4)
    -- The unfolded panel's fixed lengths: the tab segment and the playback rail.
    local tabs_width, scrub_width = 312, 210

    local items, by_path, meta = {}, {}, {}
    local mons, assigned, playing = {}, {}, {}
    local dir = ""
    local sel = 1
    local preview = nil
    local revert_timer, preview_timer, ticker, poller = nil, nil, nil, nil
    local thumbs_id = nil
    local queue, busy = {}, false
    local status_busy = false
    local sweep_serial = 0
    local seek_pending, seek_timer = nil, nil
    local scan

    local function clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end

    local function clock(seconds)
        seconds = math.max(0, math.floor((seconds or 0) + 0.5))
        return string.format("%d:%02d", math.floor(seconds / 60), seconds % 60)
    end

    local function basename(path) return (path or ""):match("([^/]+)$") or "" end

    local function title_of(path)
        local stem = basename(path):gsub("%.[^.]+$", ""):gsub("[-_]+", " ")
        return (stem:gsub("(%a)([%w']*)", function(a, b) return a:upper() .. b end))
    end

    local function height_label(height)
        if not height or height <= 0 then return "" end
        if height >= 2160 then return "4K" end
        return string.format("%dp", height)
    end

    local function is_video(path) return (path or ""):lower():match("%.(%w+)$") ~= nil
        and ({mp4 = true, webm = true, mkv = true, mov = true, gif = true})[path:lower():match("%.(%w+)$")] == true end

    local function report(message)
        text.wall_error = message or ""
        fact.wall_failed = text.wall_error ~= ""
    end

    -- SF Pro Text at weight 500 averages about 0.6 em a character; it only spaces pills and tabs.
    local function est(s, size) return math.floor(utf8.len(s or "") * size * 0.6 + 0.5) end

    -- Commands that change a monitor run one at a time, in order. A queued preview for a monitor is
    -- replaced by a newer one before it starts, so sweeping the pointer across tiles loads only the last.
    local function pump()
        if busy then return end
        local job = table.remove(queue, 1)
        if not job then return end
        busy = true
        run(helper, job.args, function(out, code)
            busy = false
            if code ~= 0 then report(job.fail or ((out ~= "" and out) or (job.args[1] .. " failed"))) end
            if job.done then job.done(out, code) end
            pump()
        end, {errors = true})
    end
    local function enqueue(args, done, key)
        if key then
            for i = #queue, 1, -1 do
                if queue[i].key == key then table.remove(queue, i) end
            end
        end
        queue[#queue + 1] = {args = args, done = done, key = key}
        pump()
    end

    local function mon_index(name)
        for i, m in ipairs(mons) do if m.name == name then return i end end
    end
    local function all_tab() return fact.wall_tab >= #mons end
    local function targets()
        if #mons == 0 then return {} end
        if all_tab() then
            local list = {}
            for _, m in ipairs(mons) do list[#list + 1] = m.name end
            return list
        end
        return {mons[sel].name}
    end
    local function current_of(name)
        local p = playing[name]
        if p and p.path and p.path ~= "" then return p.path end
        return assigned[name]
    end
    local function selected() return mons[sel] end

    local function item_index(path) return by_path[path] end

    local function badge_of(item)
        local m = meta[item.path] or {}
        if item.video then
            if (m.duration or 0) > 0 then return "▶ " .. clock(m.duration) end
            return "▶"
        end
        local label = height_label(m.height)
        return label ~= "" and label or "Still"
    end

    local function previewing(path)
        return preview ~= nil and preview.path == path
    end

    local function publish()
        local here = selected()
        local here_path = here and current_of(here.name)
        local list, videos, stills = {}, 0, 0
        for i, item in ipairs(items) do
            if item.video then videos = videos + 1 else stills = stills + 1 end
            local shown = {}
            for _, m in ipairs(mons) do
                local current = current_of(m.name)
                if preview and preview.saved[m.name] then current = preview.saved[m.name].path end
                if current == item.path then shown[#shown + 1] = m.name end
            end
            local mon = ""
            if #shown == 1 then mon = shown[1] elseif #shown > 1 then mon = #shown == #mons and "All" or (#shown .. " displays") end
            local m = meta[item.path] or {}
            local badge = badge_of(item)
            list[i] = {
                path = item.path, name = item.name, file = item.file, video = item.video,
                badge = badge, bw = est(badge, 9.5) + 12, thumb = m.thumb or "",
                mon = mon, shown = mon ~= "", here = false,
                live = previewing(item.path),
            }
            for _, name in ipairs(shown) do
                if here and name == here.name then list[i].here = true end
            end
            list[i].mw = est(mon, 9.5) + 12 + (list[i].here and 8 or 0)
        end
        model.wall_items = list
        fact.wall_count = #items
        fact.wall_pages = math.max(1, math.ceil(#items / page_size))
        fact.wall_page = clamp(fact.wall_page, 0, fact.wall_pages - 1)
        local plural = function(n, one) return n .. " " .. one .. (n == 1 and "" or "s") end
        text.wall_pager = string.format("%d of %d · %s, %s", fact.wall_page + 1, fact.wall_pages, plural(videos, "video"), plural(stills, "still"))

        local mon_list, x = {}, 12
        local unit = tabs_width / (#mons + (#mons > 1 and 0.72 or 0))
        for i, m in ipairs(mons) do
            local res = height_label(m.height)
            local chip = est(m.name, 11) + 16 + (i == sel and 10 or 0)
            local tx = (i - 1) * unit
            local pair = est(m.name, 11) + (res ~= "" and 4 + est(res, 10) or 0)
            local nx = tx + (unit - pair) / 2
            mon_list[i] = {name = m.name, res = res, x = x, chip = chip, tx = tx, tw = unit, nx = nx, rx = nx + est(m.name, 11) + 4}
            x = x + chip + 5
        end
        fact.wall_all_tx = #mons * unit
        fact.wall_all_tw = 0.72 * unit
        model.wall_mons = mon_list
        fact.wall_mon_count = #mons
        fact.wall_sel = sel - 1

        local index = here_path and item_index(here_path)
        local m = (here_path and meta[here_path]) or {}
        local p = here and playing[here.name]
        if not here then
            text.wall_name = "No monitors"
        elseif not here_path then
            text.wall_name = "No wallpaper"
        else
            text.wall_name = title_of(here_path)
        end
        fact.wall_missing = here_path ~= nil and index == nil
        fact.wall_video = here_path ~= nil and is_video(here_path)
        text.wall_number = index and string.format("%02d", index) or "—"
        text.wall_count_text = string.format("%02d / %02d", index or 0, #items)
        local width = (m.width or 0) > 0 and m.width or (p and 0) or 0
        text.wall_res = width > 0 and string.format("%d×%d", m.width, m.height) or ""
        if fact.wall_missing then
            text.wall_rate = "Missing file"
        elseif fact.wall_video then
            text.wall_rate = (m.fps or 0) > 0 and string.format("%d fps · loop", math.floor(m.fps + 0.5)) or "loop"
        else
            text.wall_rate = here_path and "Still" or ""
        end
        local duration = (p and p.duration and p.duration > 0 and p.duration) or m.duration or 0
        local pos = p and p.pos or 0
        fact.wall_paused = p ~= nil and p.paused == true
        fact.wall_playing = p ~= nil
        if fact.wall_video and duration > 0 then
            text.wall_time = clock(pos) .. " / " .. clock(duration)
            text.wall_elapsed = clock(pos)
            text.wall_meta = ((m.fps or 0) > 0 and string.format("%d fps · ", math.floor(m.fps + 0.5)) or "") .. clock(duration)
            fact.wall_progress = clamp(pos / duration, 0, 1)
        else
            text.wall_time = fact.wall_video and "" or (here_path and (height_label(m.height) ~= "" and height_label(m.height) .. " Still" or "Still") or "")
            text.wall_elapsed = fact.wall_video and clock(pos) or ""
            text.wall_meta = text.wall_res
            fact.wall_progress = 0.5
        end
        text.wall_soft = m.soft or ""
        fact.wall_has_soft = text.wall_soft ~= ""

        if scrub_width > 0 then
            local goal = 8 + (scrub_width - 16) * (fact.wall_video and fact.wall_progress or 0)
            if fact.wall_unfolded and not fact.wall_scrubbing then scrub_rail.chase(goal) end
        end
    end

    local function place_tabs(glide)
        local count = #mons + (#mons > 1 and 1 or 0)
        fact.wall_tabs = count
        if count == 0 then return end
        -- Tabs share the segment 1 : 1 : … : 0.72, as the mock lays out monitors and All.
        local units = #mons + (#mons > 1 and 0.72 or 0)
        local tab = math.min(fact.wall_tab, count - 1)
        local start = 0
        for i = 0, tab - 1 do start = start + (i < #mons and 1 or 0.72) end
        local span = tab < #mons and 1 or 0.72
        local goal = clamp((start + span / 2) / units * tabs_width, 8, tabs_width - 8)
        if glide then tabs_rail.glide(goal) else tabs_rail.place(goal) end
    end

    local function show_pill()
        if not preview then
            fact.wall_pill_open = false
            text.wall_pill_screen = ""
            return
        end
        local names = {}
        for name in pairs(preview.saved) do names[#names + 1] = name end
        table.sort(names)
        text.wall_pill_screen = table.concat(names, ", ")
        text.wall_pill_mon = #names == 1 and names[1] or table.concat(names, " + ")
        text.wall_pill_name = title_of(preview.path)
        text.wall_pill_file = basename(preview.path)
        fact.wall_pill_open = true
    end

    local function remember(name)
        local p = playing[name]
        return {path = current_of(name), pos = p and p.pos or 0, paused = p == nil or p.paused == true}
    end

    local function revert()
        if revert_timer then cancel(revert_timer); revert_timer = nil end
        if preview_timer then cancel(preview_timer); preview_timer = nil end
        if not preview then return end
        local was = preview
        preview = nil
        for name, saved in pairs(was.saved) do
            if saved.path and saved.path ~= "" then
                local args = {"load", name, saved.path}
                if is_video(saved.path) and saved.pos > 0 then args[4] = string.format("%.2f", saved.pos) end
                enqueue(args, nil, "load:" .. name)
                enqueue({"pause", name, saved.paused and "yes" or "no"}, nil, "pause:" .. name)
            end
            playing[name] = {path = saved.path, pos = saved.pos, duration = playing[name] and playing[name].duration or 0, paused = saved.paused}
        end
        fact.wall_previewing = false
        fact.wall_live = -1
        show_pill()
        publish()
    end

    local function start_preview(index)
        local item = items[index]
        if not item or not fact.wall_open then return end
        if preview and preview.path == item.path then return end
        preview = preview or {saved = {}}
        preview.path = item.path
        preview.index = index
        for _, name in ipairs(targets()) do
            if not preview.saved[name] then
                preview.saved[name] = remember(name)
                enqueue({"snapshot", name}, function(out, code)
                    if code == 0 and preview and preview.saved[name] then preview.saved[name].snapshot = out:gsub("%s+$", "") end
                end)
            end
            enqueue({"load", name, item.path}, nil, "load:" .. name)
            -- A preview plays even where wallpaper-autopause has paused it; the saved state comes back after.
            enqueue({"pause", name, "no"}, nil, "pause:" .. name)
            playing[name] = {path = item.path, pos = 0, duration = (meta[item.path] or {}).duration or 0, paused = false}
        end
        fact.wall_previewing = true
        fact.wall_live = index - 1
        show_pill()
        publish()
    end

    local function want_preview(index, delay)
        if revert_timer then cancel(revert_timer); revert_timer = nil end
        if preview_timer then cancel(preview_timer); preview_timer = nil end
        if delay and delay > 0 then
            preview_timer = after(delay, function() preview_timer = nil; start_preview(index) end)
        else
            start_preview(index)
        end
    end

    local function leave(index)
        if preview_timer then cancel(preview_timer); preview_timer = nil end
        if not preview then return end
        if revert_timer then cancel(revert_timer) end
        revert_timer = after(220, function()
            revert_timer = nil
            if preview and (index == nil or preview.index == index) then revert() end
        end)
    end

    -- The meniscus: up to four monitors sweep at once, each in its own slot. A slot holds the old picture
    -- over the desktop until the new file is in, then lets the water fall; a failed apply just closes it.
    local sweep_slots = {}
    local function sweep_hold(name, snapshot, veil)
        local slot
        for s = 0, 3 do if sweep_slots[s] == name then slot = s end end
        if not slot then
            for s = 0, 3 do if not sweep_slots[s] and not slot then slot = s end end
        end
        if not slot then return nil end
        sweep_slots[slot] = name
        sweep_serial = sweep_serial + 1
        local old = snapshot ~= nil and snapshot ~= "" and not veil
        text["wall_sweep_screen." .. slot] = name
        text["wall_sweep_image." .. slot] = old and (snapshot .. "?" .. sweep_serial) or ""
        fact["wall_sweep_old." .. slot] = old
        fact["wall_sweep_hold." .. slot] = sweep_serial
        fact["wall_sweep_live." .. slot] = true
        return {slot = slot, serial = sweep_serial}
    end
    local function sweep_end(held)
        local slot = held.slot
        if fact["wall_sweep_hold." .. slot] ~= held.serial then return end
        fact["wall_sweep_live." .. slot] = false
        text["wall_sweep_screen." .. slot] = ""
        sweep_slots[slot] = nil
    end
    local function sweep_go(held)
        if not held or fact["wall_sweep_hold." .. held.slot] ~= held.serial then return end
        fact["wall_sweep_go." .. held.slot] = held.serial
        after(1500, function() sweep_end(held) end)
    end

    local function apply_to(names, path)
        if not path or path == "" then return end
        if preview_timer then cancel(preview_timer); preview_timer = nil end
        if revert_timer then cancel(revert_timer); revert_timer = nil end
        for _, name in ipairs(names) do
            local saved = preview and preview.saved[name]
            local shown = saved and preview.path == path
            local function go(snapshot)
                -- The old picture is held over the desktop and fades in as it loads; mpvpaper changes
                -- under it once it is opaque, and the water falls once mpvpaper has the new file.
                local held = sweep_hold(name, snapshot, shown)
                local function commit()
                    enqueue({"apply", name, path}, function(_, code)
                        if code == 0 then
                            assigned[name] = path
                            local p = playing[name]
                            if p and p.path ~= path then p.path, p.pos = path, 0 end
                            sweep_go(held)
                            publish()
                        else
                            if held then sweep_end(held) end
                            playing[name] = nil
                            scan()
                        end
                    end)
                    if saved then enqueue({"pause", name, saved.paused and "yes" or "no"}, nil, "pause:" .. name) end
                end
                if held and not shown then after(300, commit) else commit() end
                local p = playing[name] or {}
                playing[name] = {path = path, pos = 0, duration = (meta[path] or {}).duration or 0, paused = saved and saved.paused or p.paused == true}
            end
            if saved then
                preview.saved[name] = nil
                go(saved.snapshot)
            elseif current_of(name) ~= path then
                enqueue({"snapshot", name}, function(out, code) go(code == 0 and out:gsub("%s+$", "") or nil) end)
            end
        end
        if preview and next(preview.saved) == nil then preview = nil end
        fact.wall_previewing = preview ~= nil
        if not preview then fact.wall_live = -1 end
        show_pill()
        publish()
    end

    local function choose(index)
        local item = items[index]
        if not item then return end
        apply_to(targets(), item.path)
    end

    -- As wallpaper-randomize picks: a video that differs from the monitor's last and, while there are
    -- enough to go round, from every other monitor's.
    local function pick(previous, used)
        local videos = {}
        for _, item in ipairs(items) do if item.video then videos[#videos + 1] = item.path end end
        if #videos == 0 then return nil end
        for i = #videos, 2, -1 do
            local j = math.random(1, i)
            videos[i], videos[j] = videos[j], videos[i]
        end
        for _, path in ipairs(videos) do if path ~= previous and not used[path] then return path end end
        for _, path in ipairs(videos) do if path ~= previous then return path end end
        return videos[1]
    end
    local function randomize(names)
        local chosen, used = {}, {}
        for _, m in ipairs(mons) do
            local keep = true
            for _, name in ipairs(names) do if name == m.name then keep = false end end
            if keep and current_of(m.name) then used[current_of(m.name)] = true end
        end
        for _, name in ipairs(names) do
            local previous = (preview and preview.saved[name] and preview.saved[name].path) or current_of(name)
            local path = pick(previous, used)
            if path then
                used[path] = true
                chosen[#chosen + 1] = {name, path}
            end
        end
        if preview then revert() end
        for _, c in ipairs(chosen) do apply_to({c[1]}, c[2]) end
    end

    local function adopt(report_table)
        if type(report_table) ~= "table" then return end
        dir = report_table.dir or dir
        local monitors = report_table.monitors or {}
        local was = mons[sel] and mons[sel].name
        mons = {}
        for _, m in ipairs(monitors) do
            if type(m.name) == "string" and m.name ~= "" then mons[#mons + 1] = {name = m.name, width = m.width or 0, height = m.height or 0, focused = m.focused == true} end
        end
        assigned = type(report_table.assignments) == "table" and report_table.assignments or {}
        if #mons == 0 then
            local names = {}
            for name in pairs(assigned) do names[#names + 1] = name end
            table.sort(names)
            for _, name in ipairs(names) do mons[#mons + 1] = {name = name, width = 0, height = 0} end
        end
        local fresh = {}
        for _, m in ipairs(mons) do
            local p = type(report_table.playing) == "table" and report_table.playing[m.name]
            if type(p) == "table" and preview and preview.saved[m.name] then
                -- While previewing, the report shows the candidate; the saved state is what comes back.
                fresh[m.name] = {path = preview.path, pos = p.pos or 0, duration = p.duration or 0, paused = p.paused == true}
            elseif type(p) == "table" then
                fresh[m.name] = {path = p.path, pos = p.pos or 0, duration = p.duration or 0, paused = p.paused == true}
            end
        end
        playing = fresh
        sel = 1
        for i, m in ipairs(mons) do
            if m.name == was then sel = i end
        end
        if not was then
            for i, m in ipairs(mons) do if m.focused then sel = i end end
        end
        items, by_path = {}, {}
        for _, f in ipairs(report_table.files or {}) do
            if type(f.path) == "string" then
                local item = {path = f.path, file = basename(f.path), name = title_of(f.path), video = f.kind == "video", mtime = f.mtime, size = f.size}
                items[#items + 1] = item
                by_path[f.path] = #items
                local m = meta[f.path]
                if m and (m.mtime ~= f.mtime or m.size ~= f.size) then meta[f.path] = nil end
            end
        end
        if fact.wall_tab > #mons then fact.wall_tab = #mons end
        if #mons <= 1 then fact.wall_tab = 0 end
    end

    local function want_thumbs()
        local missing = {}
        for _, item in ipairs(items) do
            if not meta[item.path] then missing[#missing + 1] = item.path end
        end
        if #missing == 0 or thumbs_id then return end
        local args = {"thumbs"}
        for _, path in ipairs(missing) do args[#args + 1] = path end
        thumbs_id = spawn(helper, args, function(line)
            local ok, m = pcall(json.decode, line)
            if not ok or type(m) ~= "table" or type(m.path) ~= "string" then return end
            local index = by_path[m.path]
            if m.missing or m.failed then
                meta[m.path] = {failed = true, mtime = index and items[index].mtime, size = index and items[index].size}
            else
                m.mtime = index and items[index].mtime
                m.size = index and items[index].size
                meta[m.path] = m
            end
            publish()
        end, function() thumbs_id = nil end)
    end

    scan = function(done)
        run(helper, {"scan"}, function(out, code)
            if code ~= 0 then report("Could not read the wallpapers"); return end
            local ok, decoded = pcall(json.decode, out)
            if not ok then report("Could not read the wallpapers"); return end
            adopt(decoded)
            publish()
            place_tabs(false)
            want_thumbs()
            if done then done() end
        end)
    end

    local function poll()
        if status_busy or not fact.wall_open then return end
        status_busy = true
        run(helper, {"status"}, function(out, code)
            status_busy = false
            if code ~= 0 then return end
            local ok, decoded = pcall(json.decode, out)
            if not ok or type(decoded) ~= "table" or type(decoded.playing) ~= "table" then return end
            for _, m in ipairs(mons) do
                local p = decoded.playing[m.name]
                if type(p) == "table" and not (fact.wall_scrubbing and mons[sel] and m.name == mons[sel].name) then
                    local path = p.path
                    if preview and preview.saved[m.name] then path = preview.path end
                    playing[m.name] = {path = path, pos = p.pos or 0, duration = p.duration or 0, paused = p.paused == true}
                end
            end
            publish()
        end)
    end

    -- Between reports the playback line carries itself on at 1×, wrapping as mpv's loop does.
    local function tick()
        for _, p in pairs(playing) do
            if not p.paused and (p.duration or 0) > 0 then
                p.pos = p.pos + 0.25
                if p.pos >= p.duration then p.pos = p.pos - p.duration end
            end
        end
        publish()
    end

    local function stop_timers()
        if ticker then cancel(ticker); ticker = nil end
        if poller then cancel(poller); poller = nil end
    end

    local function fold(unfolded)
        if fact.wall_unfolded == unfolded then return end
        fact.wall_unfolded = unfolded
        fact.wall_keyed = false
        if unfolded then
            fact.wall_focus = -1
            place_tabs(false)
            scrub_rail.place(8)
            after(180, function() if fact.wall_unfolded then publish() end end)
        else
            if preview then revert() end
        end
    end

    local function close()
        if preview then revert() end
        stop_timers()
        if thumbs_id then kill(thumbs_id); thumbs_id = nil end
        if fact.wall_open then fact.wall_open = false end
        fact.wall_unfolded = false
        fact.wall_keyed = false
        fact.wall_focus = -1
        fact.wall_scrubbing = false
    end

    local function open()
        if fact.wall_open then return end
        fact.wall_open = true
        fact.wall_unfolded = false
        fact.wall_page = 0
        fact.wall_focus = -1
        fact.wall_live = -1
        report("")
        scan(function()
            if mons[sel] then
                local index = item_index(current_of(mons[sel].name) or "")
                if index then fact.wall_page = math.floor((index - 1) / page_size) end
            end
            publish()
        end)
        stop_timers()
        ticker = every(250, tick)
        poller = every(2000, poll)
    end

    local function select_tab(tab)
        local count = #mons + (#mons > 1 and 1 or 0)
        if type(tab) ~= "number" or tab ~= math.floor(tab) or tab < 0 or tab >= count then return end
        if preview then revert() end
        fact.wall_tab = tab
        if tab < #mons then sel = tab + 1 end
        place_tabs(true)
        publish()
    end

    local function focus_move(delta)
        if #items == 0 then return end
        if not fact.wall_unfolded then fold(true) end
        fact.wall_keyed = true
        local focus = fact.wall_focus
        if focus < 0 then
            focus = fact.wall_page * page_size
        else
            focus = clamp(focus + delta, 0, #items - 1)
        end
        fact.wall_focus = focus
        fact.wall_page = math.floor(focus / page_size)
        publish()
        want_preview(focus + 1, 160)
    end

    on("wall_toggle", function() if fact.wall_open then close() else open() end end)
    on("wall_open_request", open)
    on("panel_close", close)
    on("fact:wall_open", function(value) if not value then close() end end)
    on("wall_fold_toggle", function() fold(not fact.wall_unfolded) end)
    on("wall_tab", select_tab)
    on("wall_chip", function(index)
        if type(index) ~= "number" or not mons[index + 1] then return end
        if preview then revert() end
        sel = index + 1
        fact.wall_tab = index
        place_tabs(false)
        publish()
    end)
    on("wall_hover", function(index) if fact.wall_unfolded then want_preview(index + 1, 0) end end)
    on("wall_leave", function(index) leave(index + 1) end)
    on("wall_choose", function(index) fact.wall_keyed = false; choose(index + 1) end)
    on("wall_escape", function()
        if preview then revert() else close() end
    end)
    on("wall_enter", function()
        if preview then
            local index = by_path[preview.path]
            apply_to(targets(), preview.path)
            if index then fact.wall_focus = index - 1 end
        elseif fact.wall_unfolded and fact.wall_focus >= 0 then
            choose(fact.wall_focus + 1)
        elseif not fact.wall_unfolded then
            fold(true)
        end
    end)
    on("wall_focus_step", function(delta) focus_move(delta) end)
    on("wall_page_step", function(delta)
        if type(delta) ~= "number" then return end
        fact.wall_page = clamp(fact.wall_page + delta, 0, math.max(0, math.ceil(#items / page_size) - 1))
        publish()
    end)
    on("wall_apply_all", function()
        local here = selected()
        local path = here and current_of(here.name)
        if preview and preview.saved[here.name] then path = preview.saved[here.name].path end
        if preview then revert() end
        if not path then return end
        local names = {}
        for _, m in ipairs(mons) do if current_of(m.name) ~= path then names[#names + 1] = m.name end end
        apply_to(names, path)
    end)
    on("wall_random", function() randomize(targets()) end)
    on("wall_random_all", function()
        local names = {}
        for _, m in ipairs(mons) do names[#names + 1] = m.name end
        randomize(names)
    end)
    on("wall_pause", function()
        local here = selected()
        if not here then return end
        local p = playing[here.name]
        local pause = not (p and p.paused)
        for _, name in ipairs(targets()) do
            enqueue({"pause", name, pause and "yes" or "no"}, nil, "pause:" .. name)
            if playing[name] then playing[name].paused = pause end
            if preview and preview.saved[name] then preview.saved[name].paused = pause end
        end
        publish()
    end)
    local function seek_to(fraction)
        local here = selected()
        local p = here and playing[here.name]
        if not p or (p.duration or 0) <= 0 then return end
        p.pos = clamp(fraction, 0, 1) * p.duration
        seek_pending = string.format("%.2f", p.pos)
        if not seek_timer then
            seek_timer = after(60, function()
                seek_timer = nil
                if seek_pending then enqueue({"seek", here.name, seek_pending}, nil, "seek:" .. here.name); seek_pending = nil end
            end)
        end
        scrub_rail.chase(8 + (scrub_width - 16) * clamp(fraction, 0, 1))
        publish()
    end
    on("wall_scrub_press", function(fraction) fact.wall_scrubbing = true; seek_to(fraction) end)
    on("wall_scrub_drag", function(fraction) seek_to(fraction) end)
    on("wall_scrub_release", function() fact.wall_scrubbing = false end)
    on("wall_rescan", function() if fact.wall_open then scan() end end)

    -- A bar that stopped during a preview left the candidate playing: what is recorded comes back.
    local function heal()
        run(helper, {"scan"}, function(out, code)
            if code ~= 0 then return end
            local ok, decoded = pcall(json.decode, out)
            if not ok or type(decoded) ~= "table" then return end
            adopt(decoded)
            for _, m in ipairs(mons) do
                local p, want = playing[m.name], assigned[m.name]
                if p and want and by_path[want] and p.path ~= want then
                    enqueue({"load", m.name, want}, nil, "load:" .. m.name)
                    p.path = want
                end
            end
            publish()
        end)
    end

    if fact.wall_open then
        fact.wall_open = false
        open()
    else
        heal()
    end
    return {open = open, close = close, toggle = function() if fact.wall_open then close() else open() end end}
end
