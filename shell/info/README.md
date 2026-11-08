# Read-only information and power

Root integration S: include info/state.plm + info/popup.plm; initialize
`info = require("info/controller")(tohru_info.snapshot, optional_outputs_getter)`.
Return lifecycle open(page)/close/toggle/refresh is callable directly for root
coordination; native events info_open/close/toggle/refresh, info_page, info_scroll.
Pages0System/1Displays/2DateTime/3Users/4Storage/5Generations/6Power. Same-root
popup preserves click serial; accepted CLI-idle Escape limitation. Private entry
info/entry.plm + entry.luau is test-only, answer-as tohru-info for About test route.

Snapshot data pages read only on open or explicit refresh. Rendering and scroll
use cached rows; all256 bounded accounts are reachable, current account first,
with public UID/description/home/shell. Storage shows actual block capacities and
mount metadata, free space Unknown; generation current label only from actual
resolved unique complete active identity. No user/avatar/trash/generation/GC/
network or system administration. Unknown avatars/timezone/mode/scale/refresh are
not invented; no external administration links. Native clock displays actual
minute date/time; power displays native battery presence/percent/charging only.

The optional outputs getter returns at most8 native {name,width,height} records;
these must be actual logical output facts, not a Settings window's dimensions.
Absent getter yields Unknown (native display API limit accepted). No custom
monitor observer or scale inference. Source/runtime mapping in runtime/info/README.

Presentation has at most256 cached rows and8 rendered rows, native keyboard
Up/Down/PageUp/PageDown and paging buttons. Native facts/texts retain the cached
page across VM reload without further snapshot I/O; tab changes open a page.
Some available portions can be partial; issue banner reports this without claiming
that no issues proves every field available. Native panel glass approximation.

Power integration: initialize require("info/power")(optional_notify, optional_low)
once, permission battery + say:tohru-notifications. Default notify sends ONLY fixed
native `emit power_warning_clear|low|critical|empty <finite normalized charge>`
to exact tohru-notifications; L owns banners/tone and clear removes only power.
Native battery percent0..100 is normalized exactly once to internal0..1 for
presentation/notification. Source thresholds low20% default (optional shared typed getter), critical5%,
empty2%, recovery2% hysteresis; no DND/persistence policy or automatic action.
Scene power_warning_level persists accepted transition across VM reload to prevent
duplicate tone. Delivery errors retain the previous level and error; next native
observation or power_warning_retry retries without a timer. This is request
acceptance, not acknowledgment of pixels/sound. Low getter returns percent5..100.

Optional bar/power.plm consumes these facts and opens info page6; S owns final
bar placement/inclusion. Its default cell is before the existing media label;
root may draw its own control using the same facts/event instead of including it.
No second native battery backend, notification owner or power controls.

CPU: lua tests/info/controllers.lua and tests/info/power.lua; pinned compiler
--check shell/info/entry.plm. Actual native binding/private scene input, visuals
and L cross-role warning evidence await coordinator slot; no real power/radio
mutation is needed/authorized for these development checks.

Native availability ambiguity: pinned platform/system.rs212–221 uses0.0 when
capacity and energy/charge counters cannot be read, while present stays true.
The native numeric0 cannot distinguish actually empty from missing measurement.
power_known means a native-reported value passed range/type checks, not an
independently verified kernel measurement. Missing/invalid Lua payload Unknown
fixtures do not prove native missing-counter distinction. Keep native report/
warning semantics; no extra sysfs poll/backend/patch, invented0 suppression or
automatic power action.
