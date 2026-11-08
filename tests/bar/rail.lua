local timers, cancelled, calls = {}, {}, {}
_G.fact, _G.text = {}, {}
_G.every = function(ms, fn) assert(ms == 1000 / 60); timers[#timers + 1] = fn; return #timers end
_G.cancel = function(id) cancelled[id] = true end
local moving = false
local function sample()
    return {shape = {8,0,2,0,8,0,0,8,4,0,1,0.5,0,0,16,4.5}, color = {1,0.5,0,1}, width = 80, moving = moving}
end
local binding = {
    observe = function(role, highest, active) calls[#calls+1]={role,highest,active}; return sample() end,
    tick = function(role) assert(role == "workspace"); return sample() end,
    reset = function(role) calls[#calls+1]={role,"reset"}; moving=false; return sample() end,
}
local rail = assert(loadfile("shell/calendar/rail-publisher.luau"))()(binding,"workspace","ws_")
rail.observe(5,1)
assert(#timers == 0 and fact.ws_r == 255 and fact.ws_g == 127.5 and not fact.ws_moving)
moving=true; rail.observe(5,5); rail.observe(5,2)
assert(#timers == 1 and fact.ws_moving)
moving=false; timers[1]()
assert(cancelled[1] and not fact.ws_moving)
moving=true; rail.observe(5,4)
assert(#timers == 2)
rail.stop(); assert(cancelled[2])
moving=true; rail.observe(5,3)
rail.reset(); assert(cancelled[3] and calls[#calls][2] == "reset")
binding.tick = function() error("binding failed") end
moving=true; rail.observe(5,4); timers[4]()
assert(cancelled[4] and text.ws_error:find("binding failed",1,true))
print("Native60Hz publication has one timer, rest/error/close cancellation and event resume")
