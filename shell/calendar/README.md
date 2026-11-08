# Calendar interaction slice

Private entry: `pleamar --scene shell/calendar/entry.plm --answer-as tohru-calendar --stall 0`.
Click Calendar; Escape closes it. Native public scene events are
`calendar_toggle`, `calendar_open`, `calendar_close`. One controller owns the
selected date, displayed month and the native clock subscription.

The Gregorian month grid, adjacent-month selection, previous/next navigation,
12-month choice, Today action, Sunday-start week number and selected-date footer
follow j2bar `src/panel/calendar.rs` (MIT, notice in ../bar/J2BAR-LICENSE).
Every open resets to today; clock changes update today's marker without losing
an already open view/selection. No shell process/date polling is used.

`state.plm` / `popup.plm` are root-consumable parts if a shared surface is later
chosen. Standalone lifecycle is the current integration contract: root routes
its clock button to named `tohru-calendar`; S owns startup/CLI/output placement. The private scene now uses native
`screens: all` under the accepted every-output bar policy.
A click-open popup has a real press serial. CLI-open after idle does not acquire
an outside-click grab in the pinned runtime; the native gap is awaiting decision.
No catcher surface or replacement dismissal is implemented here.

Current checks exercise real native scene events, month/year/Today transitions,
click/Escape, and production-code fixtures covering Sunday-start 4/5/6-week
months, century leap years and adjacent-month selection. Screenshots are evidence
of this intermediate slice, not visual parity. Liquid month-rail/selection motion,
exact native glass, typography and pixel comparison are still incomplete; the
visible numbered month controls provide the real month action entry points while
the exact track integration awaits the frame decision. No parity reduction has
been accepted.
