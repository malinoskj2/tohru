# Sound popup unfolded glass mockups

Pleamar renders exploring how to keep the unfolded sound popup's glass depth over dark neutral app/terminal backdrops while preserving its look over colourful wallpapers. Each `*-pair.png` shows the real Pleamar render over a dark UI backdrop (left) and the mock gradient wallpaper (right) at 2x scale, with the matching prototype WGSL under `shaders/`.

**Chosen for production (`shell/sound/fill.wgsl`):** `n05` (partly frosted water + drifting workspace-palette light + diffused underwater light plumes, wired to live `s.time`, `sound_quiet`, and adaptive light glass `pk`).

## Variants

### Batch 1 — Dark-backdrop glass treatments
- `v0-current`: flat `0.62 -> 0.46` glass gradient baseline
- `v1-rail-bloom`: soft radial bloom from the master rail bead's palette colour
- `v2-adaptive-chroma`: `behind_frosted()` dark-backdrop plum/mauve chroma lift
- `v3-sheen-rim`: overhead glass sheen + restrained `refraction`/`mirror`/`highlight` rim
- `v4-combo`: balanced mix of `v1`, `v2`, and `v3`
- `v5a-frosted-water-full`: tank's partly frosted water + drifting palette light across the full unfolded panel
- `v5b-frosted-water-level`: tank's water level, softened meniscus, frost, and palette light behind the rows

### Batch 2 — `5a` + subtle nods to the waterline
- `w1-soft-frost-tide`: clearer glass above volume height, frosted water + palette light below across a soft feathered tide
- `w2-capillary-rims`: curved luminous capillary meniscus only at the left and right glass walls
- `w3-blurred-meniscus-ghost`: wide defocused meniscus band behind frosted glass
- `w4-tide-plus-capillary`: `w1` frost tide combined with `w2` capillary walls
- `w5-bottom-pool-refraction`: swaying liquid-refraction glow pooling along the bottom glass rim

### Batch 3 — 10 additional `5a` water-nod explorations
- `n01-liquid-rim-gauge`: glass bezel catches internal-reflection light below the volume level
- `n02-under-rail-wake`: concentric wake ripples radiating down from the master rail bead
- `n03-tilted-refraction-plane`: angled optical water horizon with soft refractive sheen
- `n04-gap-whisper-meniscus`: translucent wave nestled in the 14 px section gap between devices and mic
- `n05-buoyant-light-plumes`: diffused underwater light plumes through partly frosted water *(shipped)*
- `n06-left-bevel-water-column`: slim glowing liquid column tucked in the left bevel margin
- `n07-chilled-condensation-line`: clear glass above volume height, multi-scale condensation mist below
- `n08-top-down-ripples`: top-down concentric surface-tension rings around the rail bead
- `n09-prismatic-dispersion-tide`: soft peach-above / mint-lavender-below chromatic prism band at volume height
- `n10-submerged-corner-traps`: frost tide + warm peach/mauve light pooled in the bottom rounded corners
