-- Settings ownership is in Rust, outside this reloadable VM.
local function publish(settings)
    fact.bar_mooncoil = settings["bar.mooncoil"].value
    fact.mooncoil = settings["bar.mooncoil"].value
    fact.corners = settings["appearance.corners"].value
    settings["bar.preferred_output"].deprecated = true
    settings["bar.preferred_output"].effective = false
    text.settings_status = json.encode({schema = 1, settings = settings})
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
on("setting_output", function()
    text.settings_error = "bar.preferred_output is deprecated and ignored: bars use every output"
end)

require("sound/controller")()

require("bar/controller")()
require("calendar/controller")()
on("calendar_toggle", function() emit("panel_close_sound") end)
on("calendar_open", function() emit("panel_close_sound") end)
on("sound_toggle", function() emit("calendar_close") end)
on("sound_open_request", function() emit("calendar_close") end)
on("panel_close", function() emit("calendar_close") end)
