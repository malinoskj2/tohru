-- Capture transactions borrow one accepted snapshot from the existing owner.
return function(publish)
    local previous_directory, previous_pattern
    local function apply(key, value)
        local ok, settings, warning = pcall(tohru_settings.set, key, value)
        if not ok then text.settings_ui_error=tostring(settings); return end
        text.settings_ui_error, text.settings_error = warning or "", warning or ""
        publish(settings)
    end
    on("submit:capture_directory_edit", function(value) apply("capture.directory", value) end)
    on("submit:capture_pattern_edit", function(value) apply("capture.filename_pattern", value) end)
    local capture = require("capture/controller")(function()
        local settings = tohru_settings.snapshot()
        return {save=settings["capture.save"].value, copy=settings["capture.copy"].value,
            directory=settings["capture.directory"].value, filename_pattern=settings["capture.filename_pattern"].value}
    end, require("settings/capture-result"))
    return {
        open = capture.open, cancel = capture.cancel,
        publish = function(settings)
            local directory, pattern = settings["capture.directory"], settings["capture.filename_pattern"]
            if previous_directory ~= directory.value then text.capture_directory_edit=directory.value; previous_directory=directory.value end
            if previous_pattern ~= pattern.value then text.capture_pattern_edit=pattern.value; previous_pattern=pattern.value end
            text.capture_preferences_notice = (directory.managed or pattern.managed)
                and "Nix-managed path/pattern edits are temporary until shell exit."
                or "Directory and filename pattern edits are saved in config.toml."
        end,
    }
end
