# Native launcher grid interaction slice

Private entry: `pleamar --scene shell/launcher/entry.plm --answer-as tohru-launcher --stall 0`.
Open via `--say tohru-launcher 'emit launcher_open'`; native events also include
launcher_toggle/launcher_close. It uses native apps discovery, native apps.launch,
native scene-local files and native widget input. It has no compositor/plugin
command and works independently of optional launcher recession. The optional exact namespace affects only compositor recession; its narrow native
property patch is runtime/pleamar/patches/0005-launcher-namespace.patch.

Implemented here: responsive3–8-column72px grid, Home plus the source's six
categories/precedence, pin order and separate pinned/application sections,
keyboard tile/category navigation, virtualized catalogs beyond1024 records,
right-click Open/desktop-actions/Pin/Unpin menu with scroll and keyboard input,
native launch error display,64 pins/2000 counts/six recent launches, and native
atomic private scene-local persistence. Native files.read reads/parses the entire
file before Lua can apply retained-state caps; bounded ingress is a reported
accepted native limit, not a proven byte/read-work bound. Action menus are virtualized rather than
dropping commands beyond the native model capacity. Data starts fresh; no j2bar
state is read. Counts are recorded when native launch accepts the request; a
new application window appearing is not promised by that acceptance.

Native scene data models are presentation windows (96 tiles,14 menu rows), not
catalog/action count limits. Closed launcher releases keyboard/surface input;
the pinned native apps hub remains process-owned and checks signatures every2s.
No feature service polling/frame listener or generic service framework. A native
conditional16.67ms timer exists ONLY during closing to observe final spring snap,
then unmaps the named layer and disables itself.

Source mapping: read-only j2bar launcher/{category,usage,view}.rs and
app/launcher.rs; adapted behavior is MIT (notice ../bar/J2BAR-LICENSE). Native
metadata cannot yet preserve all source discovery/launch behavior: immediate-only
scan/name dedup, visibility/TryExec/Path/D-Bus/activation/terminal/alias gaps were
reported with exact source evidence and explicitly accepted (MIGRATION.md). No alternate
catalog/launcher backend or runtime extension is implemented.

Uses simple fuzzy search instead of exact frizbee scoring. The
native input matches available catalog fields case-insensitively with Luau's
ASCII string.lower, preserving whole UTF-8 codepoints during subsequence matching.
Prefix outranks substring, which outranks in-order subsequence; ties preserve
native catalog order and at most50 results are shown. Whitespace-only input
returns Home/category arrangement. There is no typo correction, transposition,
history ranking, weighted-field algorithm, dependency or native binding. Non-ASCII
case folding is not provided by Luau's string.lower; exact UTF-8 characters still
match. This is explicit simple behavior, not an exact source-scorer claim.

The search field focuses on open, arrows navigate results, Enter launches,
Escape clears a nonempty query before closing, Tab/Shift+Tab choose categories
and clear search, PageUp/Down (actual XKB Prior/Next) scroll and clear tile selection.
The active native text field consumes Left/Right for caret movement before scene
rules; native caret was explicitly accepted instead of source grid-first. No custom editor/key-priority patch is used or horizontal parity claimed. Up/Down use the
retained nearest visual row across pin sections rather than adding the column
count. Native Menu/Shift+F10 open actions on the selected tile. A fresh empty
query starts with no selected highlight, as in the source.

Accepted native approximation: reveal spring/scale/opacity/vertical motion,
per-tile spring/velocity squash/impulse and glow, panel/menu native glass, frost,
refraction and ripple. Nested group blur+glow is rejected by the native compiler;
background frost supplies blur instead. Exact Warp/Slosh/turbulence was dropped
by MIGRATION.md. Native caret, catalog/launch, whole-file read and
CLI idle Escape limitations are accepted. No catcher/alternate catalog is used.
Actual entry now has a1px transparent/empty-input/no-reserve native bootstrap
per output (native each max8) and one fullscreen named launcher_layer, whose
namespace is EXACT tohru-launcher-backdrop. It is mapped to native focused output
on open, otherwise first native name, and preserves output while open unless it
disappears. Native output names/count events reconcile hotplug; stale removed
tail names are excluded. Literal interior spaces/Unicode are preserved; only
commas and Rust Unicode White_Space edges (which native selectors alter) reject
a name. No custom compositor observer, IPC or backend. The
bootstrap retains default pleamar namespace and never engages recession.

Closing keeps the real role through native reveal motion. Existing native rules
evaluate before spring step and final idle snap, so a closing-only conditional
Every appointment checks EXACT zero position/velocity on the next native round;
this uses separate timer condition and firing guard. Native rescreen clears the
output route, actually drops the layer and disables the appointment. Reopen
cancels closing, native unmap payload carries exact24-bit close-cycle token,
and only the matching current close clears the route. Lua owns clearing closing
so a delayed native false-fact cannot overwrite a newer cycle. Closing survives VM
reload. This is not a fixed animation timeout or recurring rest poll. Actual
private role/Wayland/input/hotplug validation remains required after shared build.

CPU: `lua tests/launcher/controllers.lua` executes production modules and checks
category precedence, pin order, native launch refusal,100 actions including last
action, history/persistence snapshot, bounds and1200-app virtualization.
`lua tests/launcher/search.lua` checks the approved simple matching order and
stable bound, and `lua tests/launcher/geometry.lua` checks retained visual row
movement across sections. The actual named-layer/closing-timer production composition passes the existing
pinned compiler with ONLY the new namespace property omitted explicitly for that
old binary. Full actual entry/parser matrix requires S shared namespace build.
CPU output/actual entry fixtures cover focus/fallback/hotplug/rapid reopen/reload;
tests/launcher/phase.py executes isolated native Spring/Animated/Every source and
phase contract (no role/GPU claim). Earlier entrypoint/input/launch/persistence passed on
Sway with NVIDIA RTX5090 Vulkan/GLES2, without the optional plugin. The runner
uses its own exact catalog, SVG and marker script, real native field/key/menu
paths, and restart persistence. Scope and raw results are in progress/B.md.
Run only after coordinator GPU allocation:
`python3 tests/bar/native.py --launcher-only --runtime PATH`.
Raw development runtime needs native apps.launch dependencies `setsid` and `sh`
in PATH (Pillow is used for screenshot pixel evidence); S owns production package
PATH. Fixture code preflights setsid and pins XDG_DATA_DIRS to its own directory.
S owns final startup/CLI/root/setting integration; this is not complete launcher
visual/physical/performance parity; native catalog/caret/files/effects limits are
accepted and no longer block implementation.

Native focus ownership follows direct ordered Lua `focus`/`blur` callbacks.
The retained `launcher_focus_armed` fact resets on each closed-to-open cycle/hide and arms only
when a visible cycle actually gains keyboard focus. Blur always clears ownership
and dismisses only an armed visible cycle. This prevents a removed layer's late
leave from closing a replacement before it ever gained focus; a genuine blur
after focus still dismisses. Repeated open while visible retains the same role's
ownership because it does not promise a new native enter. Visible VM reload
retains ownership. The scene has
no unguarded asynchronous `on blur -> launcher_close` forwarding rule: native
render batches focus triggers by presence and its emitted signal can arrive
after a new CLI open. No timer, focus backend or runtime patch is involved.
