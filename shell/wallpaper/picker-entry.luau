-- Private dev-only state; production injects its existing settings owner instead.
local value={folder="",assignments={}}
local wallpaper=require("controller")()
require("picker")(wallpaper,function() return value end,
    function(assignments) value.assignments=assignments end,
    function(folder) value.folder=folder end,
    function() return sys.ask("capture.outputs") end)
