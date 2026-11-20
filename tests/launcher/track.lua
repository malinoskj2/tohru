-- j2bar's Fill track in Luau: a wave per length change, at most ten, the liquid on the caret,
-- and the ticks stopping once it is all at rest.
local make = assert(loadfile("shell/launcher/track.luau"))()
local now, ticks, cancelled, published = 0, {}, 0, nil
local timer = {every = function(_, fn) ticks[#ticks + 1] = fn; return #ticks end, cancel = function() cancelled = cancelled + 1 end}
local track = make(function(state) published = state end, function() return now end, timer)
track.resize(564)
track.snap(38)
assert(published.start == 16 and published.finish == 32 and #published.slices == 0)
local function run(seconds)
    local stop = now + seconds
    while now < stop and published do
        now = now + 1 / 60
        ticks[#ticks]()
    end
end
track.retarget(46)
assert(#ticks == 1)
run(0.1)
assert(#published.slices == 188, "the whole track is sliced while it waves")
local moved = 0
for _, slice in ipairs(published.slices) do moved = math.max(moved, math.abs(slice.o0), slice.d0) end
assert(moved > 0.05, "the wave moves the track")
assert(published.finish > 32 and published.swell > 0)
for i = 1, 14 do track.retarget(46 + 8 * i) end
assert(#ticks == 1, "one tick while running")
run(4)
assert(cancelled == 1 and #published.slices == 0, "it settles and stops ticking")
assert(math.abs(published.finish - (46 + 8 * 14 - 14 + 8)) < 0.01)
track.retarget(38)
assert(#ticks == 2)
print("Fill track waves, slices, at most one tick, and settling at rest")
