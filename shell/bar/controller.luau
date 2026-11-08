-- Native subscriptions only. Caller owns output placement and settings lifecycle.
return function()
    local apps, window = {}, {}
    local function app_icon()
        local class = (window.class or ""):lower()
        local icon = ""
        if class ~= "" then
            for _, app in ipairs(apps) do
                if (app.id or ""):lower() == class or (app.wmclass or ""):lower() == class then
                    icon = app.icon or ""
                    break
                end
            end
        end
        text.bar_icon = icon
        fact.bar_has_icon = icon ~= ""
    end
    if not sys.watch("window", function(value)
        window = value
        text.bar_title = value.title or ""
        app_icon()
    end) then text.bar_error = "Focused-window service unavailable" end
    sys.watch("apps", function(value) apps = value; app_icon() end)
    sys.watch("media", function(value)
        local title, artist = value.title or "", value.artist or ""
        local label = title
        if title ~= "" and artist ~= "" then label = title .. " - " .. artist
        elseif title == "" then label = artist end
        if label == "" then label = value.player or "" end
        text.bar_media = "▶ " .. label
        fact.bar_playing = value.playing == true
    end)
    sys.watch("clock", function(value)
        local days = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}
        local months = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"}
        text.bar_date = string.format("%s %d %s", days[value.weekday + 1], value.day, months[value.month])
        text.bar_time = string.format("%02d:%02d %s", (value.hour + 11) % 12 + 1, value.minute, value.hour < 12 and "AM" or "PM")
    end)
end
