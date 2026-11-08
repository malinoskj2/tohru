local rail
if tohru_workspace then rail = require("rail-publisher")(tohru_workspace, "calendar", "cal_rail_") end
require("controller")(rail)
