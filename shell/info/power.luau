-- Native battery observations only. L remains the sole banner/sound owner.
return function(notify, low_setting)
    local names = {[0] = "clear", "low", "critical", "empty"}
    if not notify then
        notify = function(level, charge)
            sys.call("say", "tohru-notifications", "emit power_warning_" .. names[level] .. " " .. string.format("%.9g", charge))
        end
    end
    local function valid(value) return type(value) == "number" and value == value and value >= 0 and value <= 1 end
    local function publish_warning()
        local charge = fact.power_percent
        local low = low_setting and low_setting() or 20
        if type(low) ~= "number" or low ~= low then low = 20 end
        low = math.max(5, math.min(100, low)) / 100
        local level = 0
        if fact.power_present and fact.power_known and not fact.power_charging then
            if charge <= .02 then level = 3 elseif charge <= .05 then level = 2 elseif charge <= low then level = 1 end
        end
        local previous = fact.power_warning_level or 0
        if previous < 0 or previous > 3 or previous ~= math.floor(previous) then previous = 0 end
        local thresholds = {low, .05, .02}
        if level < previous and fact.power_present and fact.power_known and not fact.power_charging and charge <= thresholds[previous] + .02 then level = previous end
        if level == previous then text.power_error = ""; return end
        local ok, result = pcall(notify, level, valid(charge) and charge or 0)
        if ok and result ~= false then fact.power_warning_level = level; text.power_error = ""
        else text.power_error = ok and "Power warning delivery unavailable" or tostring(result) end
    end
    sys.watch("battery", function(value)
        fact.power_present = value.present == true
        fact.power_known = type(value.percent) == "number" and value.percent == value.percent and value.percent >= 0 and value.percent <= 100
        fact.power_percent = fact.power_known and value.percent / 100 or 0
        fact.power_charging = value.charging == true
        publish_warning()
    end)
    on("power_warning_retry", publish_warning)
    return {retry = publish_warning}
end
