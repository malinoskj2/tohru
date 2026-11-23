-- Settings ownership is in Rust, outside this reloadable VM.
local make_rail = require("bar/rail")
local flight, afk
local flight_outputs = {}
local workspace = require("bar/workspace")(make_rail(0), function() if flight then flight.interrupt() end end, function(names)
    flight_outputs = names
    if flight then flight.outputs(names) end
end)
local close_panels
local bell = require("bar/bell")(function() if close_panels then close_panels("notifications") end end)
local PALETTE_INDEX = {mocha=0, macchiato=1, rose_pine=2, nord=3, gruvbox=4, tokyo_night=5, dracula=6, everforest=7}
local last_palette = nil
local assistant, voice, needs_you
local function publish(settings)
    fact.bar_mooncoil = settings["bar.mooncoil"].value
    fact.mooncoil = settings["bar.mooncoil"].value
    fact.bar_info_button = settings["bar.info_button"].value
    fact.bar_about_button = settings["bar.about_button"].value
    fact.corners = settings["appearance.corners"].value
    fact.corner_size = settings["appearance.corner_size"].value
    local palette = settings["appearance.palette"]
    local pid = palette and PALETTE_INDEX[palette.value] or 0
    fact.palette_id = pid
    fact.rail_palette = pid
    bell.palette(pid)
    if pid ~= last_palette then
        last_palette = pid
        pcall(sys.call, "say", "tohru-session", "emit session_palette " .. pid)
        pcall(sys.call, "say", "tohru-launcher", "emit launcher_palette " .. pid)
    end
    workspace.prefer(settings["bar.preferred_output"].value)
    local position = settings["notifications.position"]
    bell.place(position and position.value == "bottom-right" and 1 or 0)
    text.settings_status = json.encode({schema = 7, settings = settings})
    if assistant then assistant.configure() end
    if voice then voice.configure(settings) end
    if afk then afk.configure(settings) end
    if needs_you then needs_you.configure() end
    if assistant then assistant.configure_discord() end
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
text.managed_config = tohru_settings.managed_path() or ""
on("config_reload", function() change(tohru_settings.reload) end)
on("setting_assistant_entrances", function(n)
    change(function() return tohru_settings.set("assistant.contextual_entrances", n ~= 0) end)
end)
on("setting_assistant_reduced_motion", function(n)
    change(function() return tohru_settings.set("assistant.reduced_motion", n ~= 0) end)
end)
on("setting_mooncoil", function(n)
    change(function() return tohru_settings.set("bar.mooncoil", n ~= 0) end)
end)
on("setting_info_button", function(n)
    change(function() return tohru_settings.set("bar.info_button", n ~= 0) end)
end)
on("setting_about_button", function(n)
    change(function() return tohru_settings.set("bar.about_button", n ~= 0) end)
end)
on("setting_palette", function(index)
    local palettes = {[0]="mocha", "macchiato", "rose_pine", "nord", "gruvbox", "tokyo_night", "dracula", "everforest"}
    local palette = palettes[index]
    if not palette then text.settings_error = "Invalid retained palette"; return end
    change(function() return tohru_settings.set("appearance.palette", palette) end)
end)
on("setting_lock_style", function(index)
    local styles = {[0]="still-glass", "ribbon-key", "flooded-monolith", "drifting-frost-veil", "unfolding-glass-capsule",
        "edge-glass-drawer", "horizon-meniscus", "refractive-typographic-lens", "corner-anchor", "bar-drop-curtain"}
    local style = styles[index]
    if not style then text.settings_error = "Invalid lock style"; return end
    change(function() return tohru_settings.set("lock.style", style) end)
end)
on("setting_polkit_agent", function(n)
    change(function() return tohru_settings.set("polkit.agent", n ~= 0) end)
end)
on("setting_notifications_position", function(n)
    if n ~= 0 and n ~= 1 then text.settings_error = "Invalid notifications position"; return end
    change(function() return tohru_settings.set("notifications.position", n == 1 and "bottom-right" or "top-right") end)
end)
on("setting_corners", function(n)
    change(function() return tohru_settings.set("appearance.corners", n ~= 0) end)
end)
on("setting_corner_size", function(n)
    change(function() return tohru_settings.set("appearance.corner_size", n) end)
end)
on("setting_output", function()
    change(function() return tohru_settings.set("bar.preferred_output", text.output_request) end)
end)

local osd = require("osd/controller")()
local sound = require("sound/controller")(osd.observe)
on("sound_open_request", osd.hide)
on("sound_toggle", function() if fact.sound_open then osd.hide() end end)

local media = require("media/controller")()
local puppet = require("assistant/puppet")(require("assistant/full-body"), require("assistant/landing-frames"))
flight = require("assistant/flight")(require("assistant/flight-path"), puppet, require("assistant/flight-actor"))
flight.outputs(flight_outputs)
afk = require("assistant/afk-native")(flight.actor)
afk.configure(tohru_settings.snapshot())
require("bar/controller")(flight.window)
require("bar/launcher")()
local calendar = require("calendar/controller")(make_rail(1, 21))
local network = require("network/controller")(make_rail(8))
local bluetooth = require("bluetooth/controller")(make_rail(4))
local wallpaper = require("wallpaper/controller")()
local tray = require("tray/controller")(function() if close_panels then close_panels("tray") end end)
local info = require("info/controller")(tohru_info.snapshot, nil, make_rail(3))
assistant = require("assistant/controller")({
    provider = function() return tohru_settings.snapshot()["assistant.provider"].value end,
    choose = function(id) change(function() return tohru_settings.set("assistant.provider", id) end) end,
    value = function(key) local entry = tohru_settings.snapshot()[key]; return entry and entry.value end,
})
assistant.configure_discord()
voice = require("assistant/voice")(assistant, puppet, flight)
voice.configure(tohru_settings.snapshot())
needs_you = require("assistant/needs-you")(function(key) local entry = tohru_settings.snapshot()[key]; return entry and entry.value end, assistant.streaming)
on("setting_assistant", function()
    local key, raw = text.setting_key or "", text.setting_value or ""
    local current = tohru_settings.snapshot()[key]
    if not current then
        text.settings_error = "Unknown setting key " .. key
        return
    end
    local value = raw
    if type(current.value) == "boolean" then
        if raw ~= "true" and raw ~= "false" then text.settings_error = key .. " must be true or false"; return end
        value = raw == "true"
    elseif type(current.value) == "number" then
        value = tonumber(raw)
        if not value or value % 1 ~= 0 then text.settings_error = key .. " must be a whole number"; return end
        value = math.floor(value)
    end
    change(function() return tohru_settings.set(key, value) end)
end)
local about
function close_panels(keep)
    if keep ~= "sound" then sound.close() end
    if keep ~= "calendar" then calendar.close() end
    if keep ~= "network" then network.close() end
    if keep ~= "media" then media.close() end
    if keep ~= "bluetooth" then bluetooth.close() end
    if keep ~= "wallpaper" then wallpaper.close() end
    if keep ~= "tray" then tray.close() end
    if keep ~= "info" then info.close() end
    if keep ~= "assistant" then assistant.close() end
    if about and keep ~= "about" then about.close() end
    if keep ~= "notifications" then bell.close_center() end
end
about = require("about/controller")(tohru_info.snapshot, function(page)
    close_panels("info")
    info.overview(page)
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
on("media_open", function() close_panels("media") end)
on("media_toggle", function() if fact.media_open then close_panels("media") end end)
on("bluetooth_open", function() close_panels("bluetooth") end)
on("bluetooth_toggle", function() if fact.bt_open then close_panels("bluetooth") end end)
on("bluetooth_question", function() if fact.bt_open then close_panels("bluetooth") end end)
on("info_open", function() close_panels("info") end)
on("info_toggle", function() if fact.info_visible then close_panels("info") end end)
on("about_open", function() close_panels("about") end)
on("wall_open_request", function() close_panels("wallpaper") end)
on("wall_toggle", function() if fact.wall_open then close_panels("wallpaper") end end)
on("about_toggle", function() if fact.about_visible then close_panels("about") end end)
on("assistant_open", function() close_panels("assistant") end)
on("assistant_toggle", function() if fact.ai_open then close_panels("assistant") end end)
on("assistant_menu", function() if fact.ai_menu_open then close_panels("assistant") end end)
on("assistant_show_history", function() close_panels("assistant") end)
on("panel_close", function() close_panels() end)
if fact.bt_open then close_panels("bluetooth") end


require("session-route")
