-- Settings ownership is in Rust, outside this reloadable VM.
local refresh_settings
local function publish(settings)
    fact.bar_mooncoil = settings["bar.mooncoil"].value
    fact.mooncoil = settings["bar.mooncoil"].value
    fact.corners = settings["appearance.corners"].value
    fact.corner_size = settings["appearance.corner_size"].value
    settings["bar.preferred_output"].deprecated = true
    settings["bar.preferred_output"].effective = false
    text.settings_status = json.encode({schema = 4, settings = settings})
    if refresh_settings then refresh_settings(settings) end
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
on("calendar_toggle", sound.close)
on("calendar_open", sound.close)
on("sound_toggle", calendar.close)
on("sound_open_request", calendar.close)
on("panel_close", calendar.close)

refresh_settings = require("settings/controller")(function(settings, warning)
    text.settings_error = warning or ""
    publish(settings)
end)
on("settings_open_request", function() sound.close(); calendar.close() end)
