# Output and microphone volume vessel

The root shares its existing declarative native audio observations with this
controller through a direct callback. Initial/unchanged/unknown reports do not
flash the OSD; later observed output/microphone volume/mute or native default-device changes
show it. Command acceptance
alone never changes the displayed level or opens the vessel. A missing displayed input/output hides it and resets that observation baseline. Opening sound hides it directly;
Lua fact writes are not treated as synthetic event redispatch.

Retained reference: read-only j2bar `src/osd/mod.rs` (44×200 vessel,1600ms hold),
`motion.rs` Jelly entrance and `vessel.rs` fill/ink/glow. This implementation uses
the accepted native approximation: native glass/frost/refraction/rim, spring
entrance and velocity squash, cyan rounded fill and existing speaker/microphone mute SVGs.
It does not implement randomized coupled Slosh or reproduce its exact motion.
Observed volume above100% is labelled honestly; the fill saturates at the vessel.
Control writes still cap100%. Native device names appear below the vessel. A
combined report changing both channels uses deterministic output priority, while
the untouched channel retains its own observation baseline.

One cancellable1600ms timer runs while visible and outside the pointer. Hover
holds the vessel without a timer; leaving starts the hold. Repeated changes
replace the timer, identical reports do not extend it. Reload retains observation
facts and visible state, clears stale hover and rearms one finite hold. There is
no recurring publication timer, extra audio stream, Rust module or runtime patch.

The native100×260 right-centred overlay surface appears on every output, takes
no keyboard/reservation and has only the visible44×200 vessel as an input zone.
The native surface remains allocated while the scene lives; drawing and input
hide at rest. Exact unmap/free and driver-idle claims are not made. Native spring
and renderer own animation to rest. No settings reader or service process is added.

The real-controller Rust/Luau fixture in `tests/osd/logic.rs` is included by the
existing sound binding harness. It tests initial/unchanged/error observations,
volume/mute, replacement/cancel/hover/hide, VM replacement and zero steady timers.
The actual-root fixture verifies the native report callback and direct sound
close path. Native compiler accepts the composed root. GPU pixels, pointer hover,
multiple output geometry and real volume observation remain private runtime gates;
the currently running B workload uses its separately frozen scene snapshot.
