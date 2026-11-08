-- Private development entry; production reads the existing root settings owner.
require("controller")(function()
    return {save = true, copy = true, directory = "", filename_pattern = ""}
end)
