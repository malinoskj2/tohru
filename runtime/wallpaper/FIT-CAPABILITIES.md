# Retained static wallpaper fit: capability review

Source review only; no behavior change or upstream/native patch is implemented by
this document. Coordinator requested this review after status commit 4e7b855.

## Retained contract

Read-only j2bar `src/preferences.rs:311–337` defines `fill` (default), `fit`,
`centre`, and `tile`. `src/app/wallpaper.rs:494–504` draws static wallpaper using
that accepted preference, against the physical target size. The video backend
uses its existing cover/panscan policy independently.

`src/shaders/image.vert:14–34` and `image.frag:32–52` establish the actual rules:

| Setting | Required static behavior | Pinned native facility |
| --- | --- | --- |
| Fill | Aspect-preserving cover/crop | `fit: cover` |
| Fit | Aspect-preserving contain, transparency outside picture | `fit: contain` |
| Centre | Original raster size in physical pixels, centred, crop overflow and leave transparent padding | Missing literal/operation |
| Tile | Repeat original raster size in physical pixels from destination origin | Missing literal/operation |

Native `fit: fill` means stretch and is **not** the retained Fill preference.
Naming must map accepted `fill` to native `cover`, and accepted `fit` to native
`contain`; silently passing the same word changes behavior.

The retained decoder in j2bar `src/wallpaper.rs:329–356` checks 64MiB file,
32Mi pixels, side/allocation limits, and decodes without resizing. Consequently
Centre/Tile use decoded source dimensions, not thumbnail dimensions or logical
surface size. Root starts fresh; this does not authorize reading old preferences.

## Actual pinned native facilities

Pinned Pleamar dc902feb/0.7.27 source:

- `src/language/vocabulary.rs:186`: `IMAGE_FITS = [fill, contain, cover]`.
- `src/language/compiler.rs:4369–4375`: a literal vocabulary word becomes an
  instruction `u8`; a fact/property expression is not accepted here.
- `src/scene.rs:1362–1363`: existing Image instruction carries image, target,
  alpha, tint, sheet-cell and fit. No independent renderer is required.
- `src/gpu.rs:2744–2790`: image frame comes from existing `Texts::frame`, atlas
  UVs and standalone picture pages are already selected, and Cover/Contain alter
  UV/destination before the existing sprite draw. No Centre/Tile branch exists.
- `src/shape.wgsl:477–501`: texture coordinates are affine inside destination;
  picture/atlas sampling is already supported. Repeating a subrectangle needs
  wrapping **inside its own UV rectangle**, not the entire atlas. Native sharp
  texel handling and alpha/tint/clip behavior must survive.
- `src/text.rs:434–472`: native original-size `0,0` loading already reads raster
  dimensions with the existing image library. The resulting texture can be
  reduced by the native maximum texture dimension. `AtlasSlot` at lines25–38
  exposes only stored texture extent/UV, so it must not automatically stand in
  for original pixel size after such a reduction.
- Image declarations produce image handles, not width/height facts. Surface
  `.width/.height` and screen dimensions describe destination, not source pixels.
- `src/shaders.rs:1–66,116–128`: user shaders receive numeric/color/geometry values
  plus `behind`, `behind_frosted`, and `inside` (a rendered group). They cannot
  declare bindings/globals or bind a wallpaper image/its native atlas region.
  `inside` can postprocess an already-rendered group but does not discover the
  source dimensions or restore a cropped image. Using it requires a separate
  render target and metadata ingress; it is larger than extending native fit.
- `lens = image` in compiler.rs2559–2586 accepts one static picture per scene,
  not per-output live assignment bindings; it is a glass backdrop facility.

Actual installed native `--check` probes with private HOME/XDG and unset
DISPLAY/WAYLAND_DISPLAY/WAYLAND_SOCKET/DBUS/PLEAMAR_SOCKETS:
`cover`, `contain`, `fill` each exit0; `centre`, `tile`, property `selected_fit`
each exit1 as expected. Raw command/status/diagnostics are at
`.tmp/check-results/wallpaper-fit-capabilities.txt`; probe scenes remain under
`.tmp/wallpaper-fit-capabilities/`. No image decode/display/backend was started.

## Smallest practical proposed boundary

Subject to coordinator direction, extend the existing native Image fit path with
two literal modes (`centre`, `tile`) and preserve native decoding/cache/output
ownership. Keep current three modes' discriminants and behavior stable. This is
an image operation extension, not a wallpaper-only renderer or decoder.

Centre can compute the centred pixel-size destination using existing native
output scale, intersect with the requested image box, and crop UVs consistently.
Tile can use a per-sprite repeat flag and pixel-size mapping in the existing
texture shader, wrapping within the selected image/sheet-cell UV extent. It must
remain one draw, independent of how many repeats fit; expanding a scene loop for
a one-pixel image creates a draw-count/resource problem. Do not change global
sampler addressing: that repeats the atlas rather than the image.

Carry original raster dimensions with the existing workshop image result/cache
identity when storage was reduced; do not do file I/O in the render loop or add
a second image decoder. Native logical-coordinate rendering must divide physical
pixel extents by actual output scale exactly once, including fractional scale.
Texture storage bounds remain native bounds; expose an unsupported/error state
rather than claim uncapped exact source pixels where the native backend cannot
represent them. The exact metadata carrier/element field must be chosen against
actual struct usage and tested before coding a shader flag.

W scene/controller can choose four static image operations with mutually
exclusive `show` expressions while referencing the **same** native image handle
per output; this avoids extending the fit instruction to a dynamic expression.
Hidden operations are already skipped before image resolution in gpu.rs2746.
Root S would own one accepted `wallpaper.fit` enum (`fill`, `fit`, `centre`,
`tile`, default `fill`), whole-config validation and publisher hook; W would own
four-way projection/picker controls and compatibility default. No schema,
settings, common file or view change is authorized merely by this report.

Required later checks: actual compiler accept/reject rules; pure mapping/clipping
math for source smaller/larger than target, rotation/fractional scale and reduced
storage; atlas page/standalone page and sheet-cell boundaries; image identity
reset and replacement; private pixel checks for centred padding/crop/tile origin
and seams. Existing decoder/image lifecycle and settled static-idle behavior must
remain intact. Source/CPU evidence does not satisfy those native/GPU gates.

## Authorized source implementation checkpoint

Coordinator msg_d3481a554d5c authorized the native extension and
msg_0a8fdb153294 fixed the interface: `publish(enabled, assignments, fit)`, canonical
`fill|fit|centre|tile`, omitted/invalid internal fit safely defaults to `fill`.
The implementation is now in `fit.patch`, applied **after image-reset.patch**.
It edits five existing native source files and adds one compiler regression scene.
Existing mode indices0/1/2, branch math, sampler and picture-page dispatch remain
unchanged. W view declares four mutually exclusive draws of the same native image
handle per output; hidden branches skip image resolution. At most one image draw
per assigned static output, with no repeat-count-dependent allocations or loops.

`AtlasSlot.source_size` carries raster size through existing worker/static/live
cache paths, including reduced texture storage. Atlas allocation/glyph/frame
constructors initialize it to their existing extent; image worker results replace
it with source geometry. No second decoder is added and there is no render-loop
file access. Existing source-dimension header reads are reused for original-size
images; explicitly sized file declarations now also read dimensions on the worker
so new source-pixel modes are accurate there. Native original-size SVG semantics
remain the native scale-aware convention; retained wallpaper accepts raster types.

Image sprites encode original physical source dimensions and the new fit tag in
otherwise-unused ordinary texture element line fields. The shader excludes text
with effects before reading those fields. It divides by **actual surface uniform
scale**, not `Texts::scale()`: render.rs3999 uses one finest atlas scale across
all outputs, while gpu.rs4875 writes each surface scale to `u.header.w`. This
matters for mixed-DPI outputs and avoids changing cache/compose keys or adding a
new scale uniform. Sheet-cell dimensions divide original extent by columns/rows.
Centre maps around destination centre and discards transparent padding. Tile
wraps inside the selected UV subrectangle, then clamps to its half-texel bounds
after native sharp sampling, preventing reads from an adjacent atlas image/cell.
Texture storage reduction retains period/centering geometry, but this does not
recover detail the native storage limit discarded.

Source-only evidence: `tests/wallpaper/fit-source.py --source PINNED_SOURCE`
applies both patches to a private source copy, asserts old mode math/dispatch are
unchanged, and runs five independent pure-math oracle cases including bounded
mixed-scale/upright-rotation grid, padding/crop, origin/repeats, reduced storage,
atlas and sheet cells. The oracle does **not** execute WGSL or native Rust.
Actual production Lua covers all four canonical mappings, invalid/legacy defaults
and existing assignment/randomization behavior. Native compiler regression scene
and original-size/reduced-storage native test assertions are supplied in the
patch but **not run**, pending independent source review and a compiler allocation.
The installed unpatched compiler necessarily rejects the new view modes; do not
assemble this view against that old backend or infer a native check from Lua.

S integration after review: apply patch after image-reset, preserve all canonical
user values/default, pass accepted snapshot fit as the optional third publication
argument, and expose the fit choice through S's settings owner. The current
factory's optional second argument remains the native video status getter and is
unrelated to publication fit. Video snapshot/backend behavior stays unchanged.
