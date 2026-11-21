-- The wallpaper controller against a fake tohru-wallpaper: every command it would run is recorded and
-- answered from the fixture below, timers run when the test says so.
local dir = "/w"
local function file(name) return dir .. "/" .. name end
local V1, V2, V3, S1 = file("344774.mp4"), file("344783.mp4"), file("344789.mp4"), file("mojave-day.jpg")

local events, calls, timers, spawned = {}, {}, {}, {}
local clock, next_timer = 0, 0
local state

local function reset_state()
    state = {
        files = {
            {path = V1, kind = "video", size = 1, mtime = 1},
            {path = V2, kind = "video", size = 1, mtime = 1},
            {path = V3, kind = "video", size = 1, mtime = 1},
            {path = S1, kind = "still", size = 1, mtime = 1},
        },
        assignments = {["DP-2"] = V1, ["HDMI-A-1"] = V3},
        playing = {
            ["DP-2"] = {path = V1, pos = 4, duration = 10, paused = false},
            ["HDMI-A-1"] = {path = V3, pos = 2, duration = 10, paused = true},
        },
        monitors = {{name = "DP-2", width = 2560, height = 1440, focused = true}, {name = "HDMI-A-1", width = 1920, height = 1080}},
        fail = {},
    }
end
reset_state()

_G.fact = {
    wall_open = false, wall_unfolded = false, wall_keyed = false, wall_tab = 0, wall_tabs = 0, wall_sel = 0,
    wall_mon_count = 0, wall_count = 0, wall_page = 0, wall_pages = 1, wall_focus = -1, wall_live = -1,
    wall_previewing = false, wall_missing = false, wall_video = false, wall_playing = false, wall_paused = false,
    wall_progress = 0.5, wall_scrubbing = false, wall_pill_open = false, wall_has_soft = false, wall_failed = false,
    wall_all_tx = 0, wall_all_tw = 0,
}
for s = 0, 3 do
    fact["wall_sweep_live." .. s] = false; fact["wall_sweep_old." .. s] = false
    fact["wall_sweep_hold." .. s] = 0; fact["wall_sweep_go." .. s] = 0
end
_G.text = {}
_G.model = {}
_G.on = function(name, handler) events[name] = handler end
_G.json = {decode = function(value) assert(type(value) == "table", "fixture answers are tables"); return value end}
_G.after = function(ms, f) next_timer = next_timer + 1; timers[next_timer] = {at = clock + ms, f = f}; return next_timer end
_G.every = function(ms, f) next_timer = next_timer + 1; timers[next_timer] = {at = clock + ms, f = f, every = ms}; return next_timer end
_G.cancel = function(id) timers[id] = nil end
_G.kill = function(id) spawned[id] = nil end

local function answer(args)
    local verb = args[1]
    if state.fail[verb] then return state.fail[verb], 1 end
    if verb == "scan" then
        local present = {}
        for _, f in ipairs(state.files) do present[#present + 1] = f end
        return {dir = dir, monitors = state.monitors, files = present, assignments = state.assignments, playing = state.playing}, 0
    elseif verb == "status" then
        return {playing = state.playing}, 0
    elseif verb == "snapshot" then
        return "/cache/snapshot-" .. args[2] .. ".jpg\n", 0
    elseif verb == "load" then
        local p = state.playing[args[2]]
        p.path = args[3]; p.pos = tonumber(args[4] or "0")
        return "", 0
    elseif verb == "apply" then
        local found = false
        for _, f in ipairs(state.files) do if f.path == args[3] then found = true end end
        if not found then return "tohru-wallpaper: missing: " .. args[3], 1 end
        state.assignments[args[2]] = args[3]
        state.playing[args[2]].path = args[3]
        return "", 0
    elseif verb == "pause" then
        state.playing[args[2]].paused = args[3] == "yes"
        return "", 0
    elseif verb == "seek" then
        state.playing[args[2]].pos = tonumber(args[3])
        return "", 0
    end
    error("unexpected command " .. verb)
end
_G.run = function(command, args, done)
    assert(command == "tohru-wallpaper")
    calls[#calls + 1] = args
    local out, code = answer(args)
    if done then done(out, code) end
end
_G.spawn = function(command, args, on_line, on_exit)
    assert(command == "tohru-wallpaper" and args[1] == "thumbs")
    local id = #spawned + 1
    spawned[id] = {args = args, line = on_line, exit = on_exit}
    return id
end
_G.require = function(name)
    assert(name == "bar/rail")
    return function()
        return {place = function() end, glide = function() end, chase = function() end, target = function() return 8 end}
    end
end

local function advance(ms)
    local stop = clock + ms
    while true do
        local soonest, id
        for k, t in pairs(timers) do if t.at <= stop and (not soonest or t.at < soonest.at) then soonest, id = t, k end end
        if not soonest then break end
        clock = soonest.at
        if soonest.every then soonest.at = clock + soonest.every else timers[id] = nil end
        soonest.f()
    end
    clock = stop
end
local function since(mark) local list = {} for i = mark + 1, #calls do list[#list + 1] = calls[i] end return list end
local function verbs(list) local v = {} for _, c in ipairs(list) do v[#v + 1] = table.concat(c, " ") end return table.concat(v, "\n") end
local function has(list, ...)
    local want = table.concat({...}, " ")
    for _, c in ipairs(list) do if table.concat(c, " "):sub(1, #want) == want then return true end end
    return false
end
local function item(path) for _, it in ipairs(model.wall_items) do if it.path == path then return it end end end

local wallpaper = assert(loadfile("shell/wallpaper/controller.luau"))()()

-- Starting heals nothing when every monitor plays its assignment.
assert(#calls == 1 and calls[1][1] == "scan")

-- Open: one scan, thumbnails asked for off the bar's thread, compact info for the focused monitor.
local mark = #calls
wallpaper.open()
assert(fact.wall_open and not fact.wall_unfolded)
assert(has(since(mark), "scan"))
assert(#spawned == 1 and #spawned[1].args == 5, "one thumbnail worker for the four files")
assert(text.wall_name == "344774" and text.wall_number == "01" and text.wall_count_text == "01 / 04")
assert(fact.wall_video and text.wall_time == "0:04 / 0:10" and math.abs(fact.wall_progress - 0.4) < 1e-9)
assert(fact.wall_mon_count == 2 and fact.wall_sel == 0 and model.wall_mons[1].name == "DP-2" and model.wall_mons[1].res == "1440p")
assert(text.wall_pager == "1 of 1 · 3 videos, 1 still")
assert(item(V1).mon == "DP-2" and item(V1).here and item(V3).mon == "HDMI-A-1" and not item(V3).here)

-- Thumbnails arrive line by line; a file that vanished meanwhile gets an empty tile, not an error.
spawned[1].line({path = V1, width = 1920, height = 1080, fps = 23.976, duration = 10, thumb = "/c/v1.jpg", soft = "/c/v1s.jpg"})
spawned[1].line({path = S1, width = 3840, height = 2160, fps = 25, duration = 0.04, thumb = "/c/s1.jpg", soft = "/c/s1s.jpg"})
spawned[1].line({path = V2, missing = true})
spawned[1].exit()
assert(item(V1).thumb == "/c/v1.jpg" and item(V1).badge == "▶ 0:10" and item(S1).badge == "4K" and item(V2).thumb == "")
assert(text.wall_rate == "24 fps · loop" and text.wall_res == "1920×1080" and fact.wall_has_soft)

-- The playback line carries on between reports and wraps with the loop.
advance(250 * 24)
assert(math.abs(state.playing["DP-2"].pos - 4) < 1e-9, "the fake never moves; the controller does")
assert(text.wall_elapsed ~= nil)

-- Folding.
events.wall_fold_toggle(); assert(fact.wall_unfolded)
events.wall_fold_toggle(); assert(not fact.wall_unfolded)
events.wall_fold_toggle(); assert(fact.wall_unfolded and fact.wall_tabs == 3)

-- Preview: the candidate loads into DP-2's mpvpaper and plays; nothing is recorded.
mark = #calls
events.wall_hover(1)
local list = since(mark)
assert(has(list, "snapshot DP-2") and has(list, "load DP-2 " .. V2) and has(list, "pause DP-2 no"))
assert(not has(list, "apply") and state.assignments["DP-2"] == V1)
assert(fact.wall_previewing and fact.wall_pill_open and text.wall_pill_screen == "DP-2" and text.wall_pill_file == "344783.mp4")
assert(item(V2).live and fact.wall_live == 1)
assert(not has(list, "load HDMI-A-1"), "a preview stays on the selected monitor")

-- Moving to another tile swaps the candidate and keeps the original to come back to.
mark = #calls
events.wall_leave(1); events.wall_hover(3)
list = since(mark)
assert(has(list, "load DP-2 " .. S1) and not has(list, "snapshot"))
-- Leaving the library reverts after a moment: the original file, where it was, as paused as it was.
mark = #calls
events.wall_leave(3)
advance(100); assert(fact.wall_previewing, "a quick pass between tiles does not revert")
advance(200)
list = since(mark)
assert(has(list, "load DP-2 " .. V1 .. " 4.00") and has(list, "pause DP-2 no"))
assert(not fact.wall_previewing and not fact.wall_pill_open and state.playing["DP-2"].path == V1)

-- Escape reverts a preview first and only closes the next time.
events.wall_hover(2)
mark = #calls
events.wall_escape()
assert(has(since(mark), "load DP-2 " .. V1) and fact.wall_open and not fact.wall_previewing)

-- Enter applies the previewed file: recorded, no second snapshot, the meniscus over a clear veil.
events.wall_hover(1)
mark = #calls
events.wall_enter()
list = since(mark)
assert(has(list, "apply DP-2 " .. V2) and not has(list, "snapshot"))
assert(state.assignments["DP-2"] == V2 and state.assignments["HDMI-A-1"] == V3)
assert(fact["wall_sweep_live.0"] and not fact["wall_sweep_old.0"] and text["wall_sweep_screen.0"] == "DP-2")
assert(fact["wall_sweep_go.0"] == fact["wall_sweep_hold.0"], "the water falls once the file is in")
assert(not fact.wall_previewing and fact.wall_live == -1)
advance(1600); assert(not fact["wall_sweep_live.0"] and text["wall_sweep_screen.0"] == "")

-- Choosing without a preview holds a snapshot of the old picture first and applies once it is up.
mark = #calls
events.wall_choose(0)
assert(has(since(mark), "snapshot DP-2") and not has(since(mark), "apply"))
assert(fact["wall_sweep_live.0"] and fact["wall_sweep_old.0"] and text["wall_sweep_image.0"]:find("^/cache/snapshot%-DP%-2%.jpg%?%d+$"))
advance(300)
assert(has(since(mark), "apply DP-2 " .. V1) and state.assignments["DP-2"] == V1)
advance(1600)

-- Per monitor: the HDMI-A-1 tab chooses for that monitor only.
events.wall_tab(1)
assert(fact.wall_tab == 1 and fact.wall_sel == 1 and text.wall_name == "344789")
mark = #calls
events.wall_choose(1); advance(400)
assert(has(since(mark), "apply HDMI-A-1 " .. V2) and not has(since(mark), "apply DP-2"))
assert(state.assignments["HDMI-A-1"] == V2 and state.assignments["DP-2"] == V1)
advance(1600)

-- All: a preview covers every monitor, and choosing applies to both, each with its own sweep slot.
events.wall_tab(2)
assert(fact.wall_tab == 2)
mark = #calls
events.wall_hover(3)
assert(has(since(mark), "load DP-2 " .. S1) and has(since(mark), "load HDMI-A-1 " .. S1))
assert(text.wall_pill_screen == "DP-2, HDMI-A-1")
events.wall_enter(); advance(400)
assert(state.assignments["DP-2"] == S1 and state.assignments["HDMI-A-1"] == S1)
assert(text["wall_sweep_screen.0"] ~= text["wall_sweep_screen.1"])
advance(1600)

-- Apply to Both copies the selected monitor's wallpaper to the others.
events.wall_tab(0); events.wall_choose(0); advance(2000)
events.wall_tab(2)
mark = #calls
events.wall_apply_all(); advance(400)
assert(has(since(mark), "apply HDMI-A-1 " .. V1) and not has(since(mark), "apply DP-2"))
assert(state.assignments["HDMI-A-1"] == V1)
advance(1600)

-- Randomize All picks videos only, each different from its last and from the other monitor's.
local rolls = {1, 1, 1, 1, 1, 1}
local real_random = math.random
math.random = function(a, b) local r = table.remove(rolls, 1) or a; return math.max(a, math.min(b, r)) end
mark = #calls
events.wall_random_all(); advance(400)
math.random = real_random
local dp, hd = state.assignments["DP-2"], state.assignments["HDMI-A-1"]
assert(dp ~= V1 and hd ~= V1 and dp ~= hd, "new and different on each monitor")
for _, path in ipairs({dp, hd}) do assert(path ~= S1, "never a still") end
advance(1600)

-- A wallpaper deleted after the scan: the apply fails, the sweep is let go, the error shows, the list is read again.
events.wall_tab(0); events.wall_choose(0); advance(2000)
table.remove(state.files, 2)
mark = #calls
events.wall_choose(1); advance(400)
assert(has(since(mark), "apply DP-2 " .. V2) and has(since(mark), "scan"))
assert(fact.wall_failed and text.wall_error:find("missing", 1, true))
assert(not fact["wall_sweep_live.0"])
assert(fact.wall_count == 3 and item(V2) == nil)

-- An assignment whose file is gone shows as missing rather than as some other wallpaper.
state.assignments["DP-2"] = file("gone.mp4"); state.playing["DP-2"] = {path = file("gone.mp4"), pos = 0, duration = 10, paused = false}
events.wall_rescan()
assert(fact.wall_missing and text.wall_rate == "Missing file" and text.wall_number == "—")

-- Closing reverts a preview and folds.
state.assignments["DP-2"] = V1; state.playing["DP-2"] = {path = V1, pos = 1, duration = 10, paused = false}
events.wall_rescan()
events.wall_fold_toggle(); events.wall_fold_toggle()
events.wall_hover(2)
mark = #calls
wallpaper.close()
assert(has(since(mark), "load DP-2 " .. V1) and not fact.wall_open and not fact.wall_unfolded and not fact.wall_pill_open)
assert(next(timers) == nil or (function() for _, t in pairs(timers) do if t.every then return false end end return true end)(), "no polling while closed")

-- Folding during a preview reverts it too.
wallpaper.open(); events.wall_fold_toggle(); events.wall_hover(2)
mark = #calls
events.wall_fold_toggle()
assert(has(since(mark), "load DP-2 " .. V1) and not fact.wall_previewing)
wallpaper.close()

-- A bar that stopped mid-preview: starting again loads the recorded wallpaper back.
state.playing["DP-2"].path = V3
calls = {}
assert(loadfile("shell/wallpaper/controller.luau"))()()
assert(has(calls, "load DP-2 " .. V1))

print("Wallpaper controller: fold, per-monitor choice, preview and revert, apply, random, missing files and heal passed")
