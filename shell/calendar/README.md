# Calendar interaction slice

Private entry: `pleamar --scene shell/calendar/entry.plm --answer-as tohru-calendar --stall 0`.
This entry is test-only. Click Calendar; Escape closes it. Its native events are
`calendar_toggle`, `calendar_open`, `calendar_close`.

Production composition (S): include `bar/state.plm`, `calendar/state.plm`,
`bar/contents.plm` and `calendar/popup.plm` in the SAME root scene; initialize
`require("bar/controller")()` and `require("calendar/controller")(rail)` once,
with `rail = require("bar/rail")(1)`, and include `bar/rail-state.plm`. Native
require is entry-relative and cannot climb out of the entry's folder, so the
private entry runs the controller without a rail: the ribbon shows, the bead does not.
The calendar initializer returns `{open, close, toggle}` callables: use these
from root Lua panel coordination, because native Lua `emit` reaches render
rules and does not redispatch Lua event handlers. Native calendar_* events
remain the UI/IPC entry points.
Give the root `keyboard: exclusive while cal_open`, permissions
`window`, `apps`, `media`, `clock`, and the root's text-driven `screens:`.
The popup receives the root clock-button's real press serial directly; do not
start another visible calendar bar or forward the click across processes.
The native parser accepts this composition, including the shared calendar_toggle
event declaration. Root calendar CLI actions should target `tohru` with
`emit calendar_open` / `calendar_close` / `calendar_toggle`; S owns that mapping.
Both controllers use the same native process-owned clock hub in this composition.

The Gregorian month grid, adjacent-month selection, previous/next navigation,
12-month choice, Today action, Sunday-start week number and selected-date footer
follow j2bar `src/panel/calendar.rs` (MIT, notice in ../bar/J2BAR-LICENSE).
Every open resets to today; clock changes update today's marker without losing
an already open view/selection. No shell process/date polling is used.

CLI-open after idle may require Escape; native click grab and Escape behavior
are accepted. Actual same-root input/grab/outside dismissal and stable VM reload
are recorded in progress/B.md. No cross-process catcher is used.

The month rail is the workspace rail's twin (rail 1 in `../bar/rail-state.plm`,
see ../bar/README.md), stretched to twelve slots with the palette spread across
the year. Each open resets it, so the bead lands on the month without moving.
Native glass/frost/refraction/ripple provide the accepted visual approximation;
physical output/visual review remains coordinator scope.

Visible logic reload retains cal_year/month/selected date facts. The rail's
springs live in the scene, so they carry on; the new logic reads the slot on
screen back from the rail's facts instead of landing the bead again.
