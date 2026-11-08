-- Private dev-only state; production injects its existing settings owner instead.
local value={folder="",assignments={},palette="mocha"}
local wallpaper=require("controller")()
require("picker")(wallpaper,function() return value end,
    function(assignments,preset) value.assignments=assignments;if preset then value.preset=preset end end,
    function(folder,assignments) value.folder=folder;if assignments then value.assignments=assignments end end,
    function() return sys.ask("capture.outputs") end)
