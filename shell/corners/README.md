# Screen corners

Rounded black corners drawn over each output's four corners. They run in the bar process.

## Files

| File | Contents |
|---|---|
| `state.plm` | The `corners` and `corner_size` facts and the `screen_corner` shader |
| `view.plm` | Four 100×100 px layer surfaces, one per corner, on every output |
| `corner.wgsl` | j2bar's corner shader: an exponent-4 superellipse with one-pixel antialiasing |
| `entry.plm` | A standalone scene with only the corners, for development |

## How it is wired

`shell/bar.plm` includes `corners/state.plm` and `corners/view.plm`. `shell/bar.luau` sets `fact.corners` from `appearance.corners` and `fact.corner_size` from `appearance.corner_size` (1 to 100, default 32).

Each surface is on the top layer with `reserve: -1`, so the bar's exclusive zone does not move it, and has no zones, so clicks pass through. The shader fills a `corner_size` square in the output's corner of its surface; 100 px, the surface size, is the largest size the setting allows. When `appearance.corners` is off, all four surfaces close. Corners always show on every output, whatever `bar.preferred_output` says.

Nothing in the corners animates or uses a timer.

## Running

```sh
pleamar --check shell/corners/entry.plm
pleamar --scene shell/corners/entry.plm --answer-as tohru-corners
```

The standalone scene uses the defaults: corners on, 32 px.
