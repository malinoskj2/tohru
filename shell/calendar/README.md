# Calendar interaction slice

Private entry: `pleamar --scene shell/calendar/entry.plm --answer-as tohru-calendar --stall 0`.
This entry is test-only. Click Calendar; Escape closes it. Its native events are
`calendar_toggle`, `calendar_open`, `calendar_close`.

Production composition (S): include `bar/state.plm`, `calendar/state.plm`,
`bar/contents.plm` and `calendar/popup.plm` in the SAME root scene; initialize
`require("bar/controller")()` and `require("calendar/controller")()` once.
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

CLI-open after idle does not acquire an outside-click grab in the pinned runtime;
the native gap is awaiting decision. No catcher or replacement dismissal is used.
Same-scene click-open composition needs runtime input/serial evidence at the next
allocated native pass; parser acceptance alone is not proof of popup grabbing.

Current checks exercise native events, month/year/Today transitions, click/Escape,
and production-code fixtures for 4/5/6-week months, century leap years and adjacent
selection. Screenshots are intermediate evidence, not visual parity. Liquid
month-rail/selection motion, exact native glass, typography and pixel comparison
remain incomplete; numbered month controls are real action entry points while
exact track integration awaits the frame decision. No parity reduction accepted.
