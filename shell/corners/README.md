# Native screen corners

`entry.plm` is the standalone private-display entry; root integration includes
`state.plm` and `view.plm` in the ordinary shell process. The existing root
`corners` fact must be declared only once. Root's existing settings publisher
already updates it from `appearance.corners`; publish `corner_size` from a new
integer `appearance.corner_size` key (1..100, default 32) through the same owner.
Do not start a second settings owner or standalone production corners process.

Four native layer surfaces per output sit on the top layer with exclusive zone
-1, so reserved bar space does not move them. They contain no zones, so native
input regions are empty and clicks pass through. Each is at most 100 logical
pixels square; the shader occupies the configured size at the output's outer
corner. Native output removal/reconnect, scale and transform handling own their
surface/buffer lifecycle. Disabled corners close all four surfaces. No timer,
frame subscription, pointer input or shader time is used.

The shader ports j2bar's exponent-4 superellipse and derivative-based linear
coverage, preserving orientation and MIT attribution. GPU/matched pixels,
stacking after bar recreation, click-through, hotplug/scale/rotation and actual
native close/reopen remain validation gates; source inspection alone proves
none of those runtime outcomes. Standalone startup takes wallpaper/notification/
auth/idle ownership of nothing.
