local palettes = {"mocha", "macchiato", "rose_pine", "nord", "gruvbox", "tokyo_night", "dracula", "everforest"}
local overrides = {
    ["bar.mooncoil"] = function(value) return "setting_mooncoil " .. (value and 1 or 0) end,
    ["bar.info_button"] = function(value) return "setting_info_button " .. (value and 1 or 0) end,
    ["bar.about_button"] = function(value) return "setting_about_button " .. (value and 1 or 0) end,
    ["appearance.corners"] = function(value) return "setting_corners " .. (value and 1 or 0) end,
    ["appearance.corner_size"] = function(value) return "setting_corner_size " .. value end,
    ["appearance.palette"] = function(value)
        for index, name in ipairs(palettes) do if name == value then return "setting_palette " .. (index - 1) end end
    end,
}
local refresh, corner_size
local function tell_bar(command)
    local ok, err = pcall(sys.call, "say", "tohru", command)
    if not ok then text.settings_ui_error = "The bar did not take the change: " .. tostring(err) end
end
local function applied(settings, warning)
    text.settings_error = warning or ""
    corner_size.publish(settings)
    refresh(settings)
    tell_bar("emit config_reload")
    for key, command in pairs(overrides) do
        local setting = settings[key]
        local event = setting and setting.runtime_override and command(setting.value)
        if event then tell_bar("emit " .. event) end
    end
end
corner_size = require("corner-size")(function(settings) applied(settings, text.settings_error) end)
refresh = require("controller")(applied)
corner_size.publish(tohru_settings.snapshot())
on("settings_open_request", function()
    local ok, settings = pcall(tohru_settings.reload)
    if ok then corner_size.publish(settings); refresh(settings) else text.settings_ui_error = tostring(settings) end
end)
fact.settings_open = true
