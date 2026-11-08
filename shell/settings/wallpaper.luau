-- The picker borrows the one settings owner; it never persists its own state.
return function(publish)
    local rotation,accepted_change
    local function update(operation)
        local settings,warning=operation()
        text.settings_error=warning or ""
        text.settings_ui_error=warning or ""
        local policy=settings["wallpaper.change"].value
        local changed=policy~=accepted_change
        publish(settings)
        -- Only accepted wallpaper edits reset the retained deadline, never a
        -- failed proposal or a duplicate/unrelated snapshot publication.
        if rotation and not changed then rotation.configure(policy)end
        return settings
    end
    local function commit(proposal)
        return update(function()return tohru_settings.wallpaper_update(proposal)end)
    end
    local function read()
        local settings=tohru_settings.snapshot()
        return {assignments=settings["wallpaper.assignments"].value,folder=settings["wallpaper.folder"].value,
            preset=settings["wallpaper.preset"].value,palette=settings["appearance.palette"].value}
    end
    local function connected()return sys.ask("capture.outputs")end
    local function prepare(preset,palette)return sys.ask("wallpaper.preset",preset,palette)end
    local wallpaper=require("wallpaper/controller")(function(output,path)
        return update(function()return tohru_settings.wallpaper_assign(output,path)end)
    end)
    local picker=require("wallpaper/picker")(wallpaper,read,
        function(assignments,preset)
            local proposal={assignments=assignments,preset=preset}
            if preset=="solid"then proposal.palette=read().palette end
            return commit(proposal)
        end,
        function(folder,assignments)return commit({folder=folder,assignments=assignments})end,
        connected,
        function(result)text.wallpaper_preset_status=json.encode(result)end)
    local actions=require("settings/wallpaper-actions")(wallpaper,read,commit,connected,prepare)
    local function refresh_visible()
        if not fact.wallpaper_picker_open then return end
        local warning=text.wallpaper_picker_warning
        picker.refresh()
        text.wallpaper_picker_warning=warning
    end
    local function rotate()
        if not actions.rotate()then error(text.wallpaper_picker_error~="" and text.wallpaper_picker_error or "Wallpaper rotation failed")end
        refresh_visible()
    end
    local initial=tohru_settings.snapshot()["wallpaper.change"].value
    rotation=require("wallpaper/rotation")(rotate,function(failure)text.wallpaper_rotation_error=failure or ""end,
        fact.wallpaper_rotation_initialized and "never" or initial)
    if fact.wallpaper_rotation_initialized then rotation.configure(initial)end
    fact.wallpaper_rotation_initialized=true
    accepted_change=initial
    on("wallpaper_rotate",function()
        local ok,err=pcall(rotate)
        text.wallpaper_rotation_error=ok and "" or tostring(err)
    end)
    return {
        open=picker.open,
        close=function()fact.wallpaper_picker_open=false end,
        stop=rotation.stop,
        palette=function(proposed)
            local result=actions.palette(proposed)
            refresh_visible()
            return result
        end,
        change_policy=function(policy)return update(function()return tohru_settings.set("wallpaper.change",policy)end)end,
        publish=function(settings)
            local assignments,folder,fit=settings["wallpaper.assignments"],settings["wallpaper.folder"],settings["wallpaper.fit"]
            local videos=wallpaper.publish(settings["wallpaper.enabled"].value,assignments.value,fit.value)
            local temporary=false
            for _,key in ipairs({"wallpaper.assignments","wallpaper.folder","wallpaper.fit","wallpaper.enabled","wallpaper.preset","appearance.palette"})do
                local field=settings[key];temporary=temporary or field.managed or field.runtime_override
            end
            text.wallpaper_settings_notice=temporary and "Nix-managed or related wallpaper edits are temporary until shell exit." or "Wallpaper edits are saved in config.toml."
            if next(videos)then text.wallpaper_settings_notice=text.wallpaper_settings_notice.." Video renderer integration is pending."end
            local change=settings["wallpaper.change"].value
            if change~=accepted_change then accepted_change=change;rotation.configure(change)end
        end,
    }
end
