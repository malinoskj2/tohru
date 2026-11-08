-- Private protocol fixture entry; every suspend timeout stays disabled.
local idle = require("idle")
local function configure(lock_seconds, off_seconds)
    return idle.configure({
        lock_seconds = lock_seconds,
        screen_off_unlocked_seconds = off_seconds,
        screen_off_locked_seconds = off_seconds,
        suspend_unlocked_seconds = 0,
        suspend_locked_seconds = 0,
        fade_milliseconds = 200,
    })
end
configure(0, 0)
on("fixture_arm_lock", function() configure(1, 0) end)
on("fixture_arm_off", function() configure(0, 1) end)
on("fixture_disable", function() configure(0, 0) end)
on("fixture_stop", function() tohru_idle.stop() end)
