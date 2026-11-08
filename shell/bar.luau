-- Settings ownership is in Rust, outside this reloadable VM.
local refresh_settings
local configure_idle
local wallpaper
local capture
local corner_preferences
local function publish(settings)
    fact.bar_mooncoil = settings["bar.mooncoil"].value
    fact.mooncoil = settings["bar.mooncoil"].value
    fact.corners = settings["appearance.corners"].value
    fact.corner_size = settings["appearance.corner_size"].value
    settings["bar.preferred_output"].deprecated = true
    settings["bar.preferred_output"].effective = false
    text.settings_status = json.encode({schema = 6, settings = settings})
    if refresh_settings then refresh_settings(settings) end
    if configure_idle then configure_idle(settings) end
    if wallpaper then wallpaper.publish(settings) end
    if capture then capture.publish(settings) end
    if corner_preferences then corner_preferences.publish(settings) end
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
wallpaper = require("settings/wallpaper")(publish)
publish(tohru_settings.snapshot())
on("config_reload", function() change(tohru_settings.reload) end)
on("setting_mooncoil", function(n)
    change(function() return tohru_settings.set("bar.mooncoil", n ~= 0) end)
end)
on("setting_wallpaper_enabled", function(n)
    change(function() return tohru_settings.set("wallpaper.enabled", n ~= 0) end)
end)
on("setting_wallpaper_fit", function(index)
    local modes = {[0]="fill", "fit", "centre", "tile"}
    local mode = modes[index]
    if not mode then text.settings_error = "Wallpaper fit must be fill, fit, centre or tile"; return end
    change(function() return tohru_settings.set("wallpaper.fit", mode) end)
end)
on("setting_corners", function(n)
    change(function() return tohru_settings.set("appearance.corners", n ~= 0) end)
end)
on("setting_corner_size", function(n)
    change(function() return tohru_settings.set("appearance.corner_size", n) end)
end)
for _, field in ipairs({"lock_seconds", "screen_off_unlocked_seconds", "screen_off_locked_seconds",
    "suspend_unlocked_seconds", "suspend_locked_seconds", "fade_milliseconds"}) do
    local key = "idle." .. field
    on("setting_idle_" .. field, function(value)
        change(function() return tohru_settings.set(key, value) end)
    end)
end
on("setting_capture_save", function(value) change(function() return tohru_settings.set("capture.save", value ~= 0) end) end)
on("setting_capture_copy", function(value) change(function() return tohru_settings.set("capture.copy", value ~= 0) end) end)
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
local info = require("info/controller")(tohru_info.snapshot, function() return sys.ask("capture.outputs") end)
local about
local function close_panels(keep)
    if fact.capture_open then
        keep=nil
        text.routing_error="Finish or cancel the region selection before opening another panel"
    end
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

refresh_settings = require("settings/controller")(function(settings, warning)
    text.settings_error = warning or ""
    publish(settings)
end, wallpaper)
on("settings_open_request", function() close_panels(); if fact.capture_open then fact.settings_open=false end end)
on("settings_wallpaper_open", function() close_panels(); if fact.capture_open then fact.settings_open=false; wallpaper.close() end end)

corner_preferences = require("settings/corner-size")(publish)
corner_preferences.publish(tohru_settings.snapshot())

configure_idle = require("settings/idle")(require("session/idle"))
configure_idle(tohru_settings.snapshot())

capture = require("settings/capture")(publish)
capture.publish(tohru_settings.snapshot())
local function capture_keyboard()
    fact.wallpaper_picker_keyboard = not fact.capture_open
end
on("capture_open_request", function()
    if fact.capture_open then
        close_panels()
        osd.hide()
        fact.settings_open=false
        wallpaper.close()
    end
    capture_keyboard()
end)
on("capture_finish", capture_keyboard)
on("capture_cancel", capture_keyboard)
on("capture_output_changed", capture_keyboard)
on("fact:capture_open", capture_keyboard)
capture_keyboard()

require("session-route")
