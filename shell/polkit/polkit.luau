local settings = tohru_settings.snapshot()
local function value(key) return (settings[key] or {}).value end
local polkit = require("controller")()
if not polkit.start(value("polkit.agent") == true, value("bar.preferred_output") or "") then
    log("polkit · Tohru's agent is off: set polkit.agent = true to use it")
end
