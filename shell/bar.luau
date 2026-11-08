-- Settings ownership is in Rust, outside this reloadable VM.
local function publish(settings)
    fact.mooncoil = settings["bar.mooncoil"].value
    fact.corners = settings["appearance.corners"].value
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
    change(function() return tohru_settings.set("bar.preferred_output", text.output_request) end)
end)
