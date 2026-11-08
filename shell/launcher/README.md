# Native launcher grid interaction slice

Private entry: `pleamar --scene shell/launcher/entry.plm --answer-as tohru-launcher --stall 0`.
Open via `--say tohru-launcher 'emit launcher_open'`; native events also include
launcher_toggle/launcher_close. It uses native apps discovery, native apps.launch,
native scene-local files and native widget input. It has no compositor/plugin
command and works independently of optional launcher recession. The pending
layer namespace decision affects only that optional compositor integration.

Implemented here: responsive3–8-column72px grid, Home plus the source's six
categories/precedence, pin order and separate pinned/application sections,
keyboard tile/category navigation, virtualized catalogs beyond1024 records,
right-click Open/desktop-actions/Pin/Unpin menu with scroll and keyboard input,
native launch error display,64 pins/2000 counts/six recent launches, and native
atomic private scene-local persistence. Native files.read reads/parses the entire
file before Lua can apply retained-state caps; bounded ingress is a reported
native gap pending user decision, not a proven byte/read-work limit. Action menus are virtualized rather than
dropping commands beyond the native model capacity. Data starts fresh; no j2bar
state is read. Counts are recorded when native launch accepts the request; a
new application window appearing is not promised by that acceptance.

Native scene data models are presentation windows (96 tiles,14 menu rows), not
catalog/action count limits. Closed launcher releases keyboard/surface input;
the pinned native apps hub remains process-owned and checks signatures every2s.
No feature polling/timers/frame listeners or generic service framework is added.

Source mapping: read-only j2bar launcher/{category,usage,view}.rs and
app/launcher.rs; adapted behavior is MIT (notice ../bar/J2BAR-LICENSE). Native
metadata cannot yet preserve all source discovery/launch behavior: immediate-only
scan/name dedup, visibility/TryExec/Path/D-Bus/activation/terminal/alias gaps were
reported with exact source evidence, and await user decision. No alternate
catalog/launcher backend or runtime extension is implemented.

The user selected simple fuzzy search instead of exact frizbee scoring. The
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
rules; source grid-first horizontal behavior is a separately reported pending
decision. No custom editor/key-priority patch is used or horizontal parity claimed. Up/Down use the
retained nearest visual row across pin sections rather than adding the column
count. Native Menu/Shift+F10 open actions on the selected tile. A fresh empty
query starts with no selected highlight, as in the source.

Incomplete gates: liquid tile/reveal/ripple/menu visuals, exact native glass/font
pixel matching and final output/root lifecycle. Native metadata/discovery/launch
and CLI-open outside-dismissal decisions are still pending independently of the
search simplification. No catcher or alternate catalog workaround is included.

CPU: `lua tests/launcher/controllers.lua` executes production modules and checks
category precedence, pin order, native launch refusal,100 actions including last
action, history/persistence snapshot, bounds and1200-app virtualization.
`lua tests/launcher/search.lua` checks the approved simple matching order and
stable bound, and `lua tests/launcher/geometry.lua` checks retained visual row
movement across sections. `pleamar --check shell/launcher/entry.plm` passes the pinned compiler. Actual native private entrypoint/input/launch/persistence checks passed on
Sway with NVIDIA RTX5090 Vulkan/GLES2, without the optional plugin. The runner
uses its own exact catalog, SVG and marker script, real native field/key/menu
paths, and restart persistence. Scope and raw results are in progress/B.md.
Run only after coordinator GPU allocation:
`python3 tests/bar/native.py --launcher-only --runtime PATH`.
Raw development runtime needs native apps.launch dependencies `setsid` and `sh`
in PATH (Pillow is used for screenshot pixel evidence); S owns production package
PATH. Fixture code preflights setsid and pins XDG_DATA_DIRS to its own directory.
S owns final startup/CLI/root/setting integration; this is not complete launcher
visual or native metadata parity while the independent pending decisions remain.
