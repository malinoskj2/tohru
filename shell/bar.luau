-- Settings ownership is in Rust, outside this reloadable VM.
local function publish(settings)
    fact.bar_mooncoil = settings["bar.mooncoil"].value
    fact.mooncoil = settings["bar.mooncoil"].value
    fact.corners = settings["appearance.corners"].value
    fact.corner_size = settings["appearance.corner_size"].value
    settings["bar.preferred_output"].deprecated = true
    settings["bar.preferred_output"].effective = false
    text.settings_status = json.encode({schema = 7, settings = settings})
end
local function change(operation)
    local ok, result, warning = pcall(operation)
    if ok then
        text.settings_error = warning or ""
        publish(result)
    else
        text.settings_error = tostring(result)
    end
end
publish(tohru_settings.snapshot())
on("config_reload", function() change(tohru_settings.reload) end)
on("setting_mooncoil", function(n)
    change(function() return tohru_settings.set("bar.mooncoil", n ~= 0) end)
end)
on("setting_palette", function(index)
    local palettes = {[0]="mocha", "macchiato", "rose_pine", "nord", "gruvbox", "tokyo_night", "dracula", "everforest"}
    local palette = palettes[index]
    if not palette then text.settings_error = "Invalid retained palette"; return end
    change(function() return tohru_settings.set("appearance.palette", palette) end)
end)
on("setting_corners", function(n)
    change(function() return tohru_settings.set("appearance.corners", n ~= 0) end)
end)
on("setting_corner_size", function(n)
    change(function() return tohru_settings.set("appearance.corner_size", n) end)
end)
on("setting_output", function()
    text.settings_error = "bar.preferred_output is deprecated and ignored: bars use every output"
end)

local osd = require("osd/controller")()
local sound = require("sound/controller")(osd.observe)
on("sound_open_request", osd.hide)
on("sound_toggle", function() if fact.sound_open then osd.hide() end end)

require("bar/controller")()
local make_rail = require("calendar/rail-publisher")
require("bar/workspace")(make_rail, tohru_workspace)
local calendar_rail = make_rail(tohru_workspace, "calendar", "cal_rail_")
local calendar = require("calendar/controller")(calendar_rail)
local network = require("network/controller")()
local bluetooth = require("bluetooth/controller")()
local info = require("info/controller")(tohru_info.snapshot)
local about
local function close_panels(keep)
    if keep ~= "sound" then sound.close() end
    if keep ~= "calendar" then calendar.close() end
    if keep ~= "network" then network.close() end
    if keep ~= "bluetooth" then bluetooth.close() end
    if keep ~= "info" then info.close() end
    if about and keep ~= "about" then about.close() end
end
about = require("about/controller")(tohru_info.snapshot, function(page)
    close_panels("info")
    info.open(page)
end)
require("info/power")()
-- Real native handlers run first; callbacks close other controllers directly.
-- Lua emit/fact writes are never assumed to dispatch another Lua handler.
on("calendar_open", function() close_panels("calendar") end)
on("calendar_toggle", function() if fact.cal_open then close_panels("calendar") end end)
on("sound_open_request", function() close_panels("sound") end)
on("sound_toggle", function() if fact.sound_open then close_panels("sound") end end)
on("network_open", function() close_panels("network") end)
on("network_toggle", function() if fact.net_open then close_panels("network") end end)
on("bluetooth_open", function() close_panels("bluetooth") end)
on("bluetooth_toggle", function() if fact.bt_open then close_panels("bluetooth") end end)
on("bluetooth_question", function() if fact.bt_open then close_panels("bluetooth") end end)
on("info_open", function() close_panels("info") end)
on("info_toggle", function() if fact.info_visible then close_panels("info") end end)
on("about_open", function() close_panels("about") end)
on("about_toggle", function() if fact.about_visible then close_panels("about") end end)
on("panel_close", function() close_panels() end)
if fact.bt_open then close_panels("bluetooth") end


require("session-route")
