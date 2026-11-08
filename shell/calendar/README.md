# Calendar interaction slice

Private entry: `pleamar --scene shell/calendar/entry.plm --answer-as tohru-calendar --stall 0`.
This entry is test-only. Click Calendar; Escape closes it. Its native events are
`calendar_toggle`, `calendar_open`, `calendar_close`.

Production composition (S): include `bar/state.plm`, `calendar/state.plm`,
`bar/contents.plm` and `calendar/popup.plm` in the SAME root scene; initialize
`require("bar/controller")()` and `require("calendar/controller")(rail)` once.
Construct rail with `require("calendar/rail-publisher")(tohru_workspace,"calendar","cal_rail_")`
from root. Native require is entry-relative, not module-relative; private entry
uses `require("rail-publisher")` explicitly. A failed reset prevents observations
until the next open attempt so its error remains visible.
The calendar initializer returns `{open, close, toggle}` callables: use these
from root Lua panel coordination, because native Lua `emit` reaches render
rules and does not redispatch Lua event handlers. Native calendar_* events
remain the UI/IPC entry points.
Give the root `keyboard: exclusive while cal_open`, permissions
`window`, `apps`, `media`, `clock`, and native `screens: all` as accepted.
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

The exact reviewed twelve-slot240Hz rail is published at native60Hz while moving.
The controller resets it on open and cancels its timer on close/rest/error.
State includes rail declarations; the popup includes the shared shader asset.
S must provide tohru_workspace observe/tick/reset as documented in
../../runtime/workspace/README.md and the authorized16-scalar shader patch.
Native glass/frost/refraction/ripple provide the accepted visual approximation;
physical output/visual review remains coordinator scope.
