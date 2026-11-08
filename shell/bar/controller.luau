-- Native subscriptions only. Caller owns output placement and settings lifecycle.
return function()
    local icons, window = {}, {}
    local function app_icon()
        local class = window.class or ""
        local tail = class:match("([^/]+)$") or class
        local icon = icons[class] or icons[class:lower()] or icons[tail] or ""
        text.bar_icon = icon
        fact.bar_has_icon = icon ~= ""
    end
    local function index_apps(apps)
        icons = {}
        local function add(key, icon)
            if key == "" or #key > 1024 or icon == "" or #icon > 4096 then return end
            if not icons[key] then icons[key] = icon end
            if not icons[key:lower()] then icons[key:lower()] = icon end
        end
        for _, app in ipairs(apps) do
            local id, icon = app.id or "", app.icon or ""
            add(id, icon)
            local suffix = id:match("%.([^%.]+)$")
            if suffix then add(suffix, icon) end
            local base, ending = id:match("^(.*)%-([^%-]+)$")
            if ending == "bin" or ending == "desktop" then add(base, icon) end
            add(app.wmclass or "", icon)
        end
        app_icon()
    end
    if not sys.watch("window", function(value)
        window = value
        text.bar_title = value.title or ""
        app_icon()
    end) then text.bar_error = "Focused-window service unavailable" end
    sys.watch("apps", index_apps)
    sys.watch("media", function(value)
        local title, artist = value.title or "", value.artist or ""
        local label = title
        if title ~= "" and artist ~= "" then label = title .. " - " .. artist
        elseif title == "" then label = artist end
        if label == "" then
            local chosen, playing
            for _, player in ipairs(value.players or {}) do
                if player.chosen then chosen = player end
                if player.playing and not playing then playing = player end
            end
            local player = chosen or playing
            label = player and player.name or ""
            if label == "" then label = "Playing" end
        end
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
