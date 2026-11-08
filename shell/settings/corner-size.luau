-- The process owner validates/persists the setting; this module owns only its draft.
return function(publish)
    local previous
    on("submit:settings_corner_size_edit", function()
        local draft = text.settings_corner_size_edit
        if type(draft) ~= "string" or not draft:match("^[1-9][0-9]?[0-9]?$")
            or tonumber(draft) > 100 then
            text.settings_ui_error = "Corner size must be a whole number from 1 to 100."
            return
        end
        local ok, settings, warning = pcall(tohru_settings.set, "appearance.corner_size", tonumber(draft))
        if not ok then text.settings_ui_error = tostring(settings); return end
        text.settings_ui_error = warning or ""
        text.settings_error = warning or ""
        publish(settings)
    end)
    return {
        publish = function(settings)
            local size = settings["appearance.corner_size"]
            if size.value ~= previous then
                text.settings_corner_size_edit = tostring(size.value)
                previous = size.value
            end
            text.settings_corner_size_notice = size.managed
                and (size.runtime_override and "Nix managed · temporary runtime override is active."
                    or "Nix managed · edits last until the shell exits.")
                or "Edits are saved in config.toml."
        end,
    }
end
