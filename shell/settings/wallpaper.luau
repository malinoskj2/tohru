-- The picker borrows the one settings owner; it never persists its own state.
return function(publish)
    local function update(operation)
        local settings, warning = operation()
        text.settings_error = warning or ""
        publish(settings)
        return settings
    end
    local wallpaper = require("wallpaper/controller")(function(output, path)
        return update(function() return tohru_settings.wallpaper_assign(output, path) end)
    end)
    local picker = require("wallpaper/picker")(wallpaper,
        function()
            local settings = tohru_settings.snapshot()
            return {assignments=settings["wallpaper.assignments"].value, folder=settings["wallpaper.folder"].value}
        end,
        function(assignments)
            return update(function() return tohru_settings.set("wallpaper.assignments", assignments) end)
        end,
        function(folder)
            return update(function() return tohru_settings.set("wallpaper.folder", folder) end)
        end,
        function() return sys.ask("capture.outputs") end)
    return {
        open = picker.open,
        close = function() fact.wallpaper_picker_open=false end,
        publish = function(settings)
            local assignments = settings["wallpaper.assignments"]
            local folder = settings["wallpaper.folder"]
            local fit = settings["wallpaper.fit"]
            local videos = wallpaper.publish(settings["wallpaper.enabled"].value, assignments.value, fit.value)
            local managed = assignments.managed or folder.managed or fit.managed or settings["wallpaper.enabled"].managed
            text.wallpaper_settings_notice = managed and "Nix-managed wallpaper edits are temporary until the shell exits." or "Wallpaper edits are saved in config.toml."
            if next(videos) then
                text.wallpaper_settings_notice = text.wallpaper_settings_notice .. " Video renderer integration is pending."
            end
        end,
    }
end
