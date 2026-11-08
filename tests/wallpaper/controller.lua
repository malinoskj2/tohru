-- Runs the production controller, with settings as a boundary fixture.
text, fact, events = {}, {}, {}
function on(name, fn) events[name] = fn end
local commits, removed, assigned = 0
local controller = assert(loadfile("shell/wallpaper/controller.luau"))()(function(output, path)
    commits, removed, assigned = commits + 1, output, path
end)
local videos = controller.publish(false, {["DP-1"] = "/pictures/a.png", ["DP-2"] = "/movies/a.webm"})
assert(not fact.wallpaper_enabled and text.wallpaper_output_0 == "DP-1")
assert(text.wallpaper_path_0 == "/pictures/a.png" and videos["DP-2"] == "/movies/a.webm")
controller.publish(true, {["DP-1"] = "/pictures/b.jpg", ["HDMI-A-1"] = "/pictures/c.webp"})
assert(fact.wallpaper_enabled and text.wallpaper_output_0 == "DP-1" and text.wallpaper_path_0 == "/pictures/b.jpg")
assert(text.wallpaper_output_1 == "HDMI-A-1")
-- Offline outputs stay assigned: no current-output list filters persistent choices.
controller.publish(true, {["HDMI-A-1"] = "/pictures/c.webp"})
assert(text.wallpaper_output_0 == "" and text.wallpaper_path_0 == "")
assert(text.wallpaper_output_1 == "HDMI-A-1")
controller.publish(true, {["DP-3"] = "/pictures/new.jpeg", ["HDMI-A-1"] = "/pictures/c.webp"})
assert(text.wallpaper_output_0 == "DP-3" and text.wallpaper_path_0 == "/pictures/new.jpeg")
-- Reject a malformed snapshot atomically, without dropping the currently drawn routes.
for _, invalid in ipairs({{["DP-1,DP-2"] = "/bad.png"}, {["DP-1"] = "relative.png"}, {["DP-1"] = "/bad.xyz"}}) do
    assert(not pcall(controller.publish, true, invalid))
    assert(text.wallpaper_output_0 == "DP-3" and fact.wallpaper_enabled)
end
local oversized = {}
for i = 1, 33 do oversized["OUTPUT-" .. i] = "/pictures/a.png" end
assert(not pcall(controller.publish, true, oversized))
assert(text.wallpaper_output_0 == "DP-3")
text.wallpaper_request_output, text.wallpaper_request_path = "DP-1", "/pictures/selected.PNG"
events.wallpaper_assign()
assert(commits == 1 and removed == "DP-1" and assigned == "/pictures/selected.PNG" and text.wallpaper_error == "")
events.wallpaper_clear()
assert(commits == 2 and removed == "DP-1" and assigned == nil)
text.wallpaper_request_output = ""
events.wallpaper_assign()
assert(commits == 2 and text.wallpaper_error ~= "")
controller.publish(false, {})
assert(not fact.wallpaper_enabled and text.wallpaper_output_0 == "" and text.wallpaper_output_1 == "")
-- Integration failure is visible, and requests never claim rendering completed.
assert(loadfile("shell/wallpaper/controller.luau"))()()
text.wallpaper_request_output = "DP-1"
events.wallpaper_assign()
assert(text.wallpaper_error:find("integration is unavailable", 1, true))
print("Wallpaper production controller: assignment routing/failure/disable fixtures passed")
-- Source randomization order: unused+different, different, then first.
local function canonical(path) return path == "/pictures/alias.png" and "/pictures/a.png" or path end
local files = {"/pictures/a.png", "/pictures/b.png", "/pictures/c.png"}
local original = {["DP-1"] = files[1], ["DP-2"] = files[2], ["DP-3"] = files[3]}
local next = controller.randomize(original, {"DP-1", "DP-1", "DP-2"}, files, canonical)
assert(next["DP-1"] == files[2] and next["DP-2"] == files[1] and next["DP-3"] == files[3])
assert(original["DP-1"] == files[1] and original["DP-2"] == files[2])
next = controller.randomize({["DP-1"] = "/pictures/alias.png"}, {"DP-1"}, files, canonical)
assert(next["DP-1"] == files[2])
next = controller.randomize(original, {"DP-1"}, {files[1], files[2]}, canonical)
assert(next["DP-1"] == files[2]) -- relax duplicate before repeating previous
next = controller.randomize(original, {"DP-1"}, {files[1]}, canonical)
assert(next["DP-1"] == files[1])
next = controller.randomize({["DP-1"] = "/movies/a.webm", ["DP-2"] = "/outside/still.png"}, {}, files, canonical)
assert(next["DP-1"] == files[1] and next["DP-2"] == "/outside/still.png")
next = controller.randomize(original, {}, {}, canonical)
assert(next["DP-1"] == files[1])
assert(not pcall(controller.randomize, original, {"DP-1"}, files, function() return nil end))
print("Wallpaper randomization: preference, alias, implicit-output and atomic proposal fixtures passed")
-- Q-W2: native selectors use Unicode White_Space, not Lua's ASCII %s.
for _, cp in ipairs({9,10,11,12,13,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288}) do
    local edge = utf8.char(cp)
    assert(not pcall(controller.validate, edge .. "DP-1", "/pictures/a.png"))
    assert(not pcall(controller.validate, "DP-1" .. edge, "/pictures/a.png"))
end
assert(controller.validate("Display α", "/pictures/a.png") == "image")
assert(controller.validate("DP" .. utf8.char(160) .. "1", "/pictures/a.png") == "image")
-- Zero-width space/BOM are not Rust White_Space and must remain literal.
assert(controller.validate(utf8.char(8203) .. "DP-1", "/pictures/a.png") == "image")
assert(controller.validate("DP-1" .. utf8.char(65279), "/pictures/a.png") == "image")
local previous = text.wallpaper_output_0
assert(not pcall(controller.publish, true, {[utf8.char(160) .. "DP-1"] = "/pictures/a.png"}))
assert(text.wallpaper_output_0 == previous)
print("Wallpaper Unicode selector boundary fixtures passed")
