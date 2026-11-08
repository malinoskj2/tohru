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
atomic private scene-local persistence. Action menus are virtualized rather than
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

**Incomplete, not an accepted reduced launcher:** exact retained fuzzy search
(frizbee exact/typo pass, weighted fields/aliases and history bonus), search field,
liquid tile/reveal/ripple/menu visuals, exact keyboard geometric movement across
pin sections, native glass/font pixel matching and final output/root lifecycle
remain. This intermediate scene deliberately exposes the complete independent
grid/category/pin/action path; it is not a search replacement or parity claim.
CLI-open outside-click dismissal remains a separate native pending decision.
No catcher workaround or plugin dependency is introduced.

CPU: `lua tests/launcher/controllers.lua` executes production modules and checks
category precedence, pin order, native launch refusal,100 actions including last
action, history/persistence snapshot, bounds and1200-app virtualization.
`pleamar --check shell/launcher/entry.plm` passes the pinned compiler. Actual
native private entrypoint/persistence/input checks are still required before
this slice is called runtime-checked; coordinator must allocate the GPU slot.
S owns final startup/CLI/root/setting integration; do not claim launcher delivery
or wire a final reduced search behavior while the pending decisions remain.
