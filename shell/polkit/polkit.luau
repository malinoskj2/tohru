local settings = tohru_settings.snapshot()
local function value(key) return (settings[key] or {}).value end
local PALETTE_INDEX = {mocha=0, macchiato=1, rose_pine=2, nord=3, gruvbox=4, tokyo_night=5, dracula=6, everforest=7}
local pid = PALETTE_INDEX[value("appearance.palette")]
if pid ~= nil then fact.palette_id = pid end
local polkit = require("controller")()
if not polkit.start(value("polkit.agent") == true, value("bar.preferred_output") or "") then
    log("polkit · Tohru's agent is off: set polkit.agent = true to use it")
end
