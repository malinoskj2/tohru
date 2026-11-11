# Settings process

`settings.plm` is its own Pleamar process, started by `bin/tohru-settings-run`
with `--answer-as tohru-settings` and `TOHRU_SHELL=1`, so it gets its own
settings owner. `tohru settings open` sends `settings_open_request` to a running
Settings (which reloads from disk) or starts one; `tohru settings close` asks it
to quit. Closing the window, Escape or `settings_close` ends the process.

After an accepted edit it says `emit config_reload` to `tohru`. Runtime
overrides of Nix-managed values live in the process that made them, so Settings
also forwards each override it holds for mooncoil, corners, corner size and
palette through the bar's `setting_*` events. Changes made through the CLI reach
an open Settings window the next time it is opened.

Initial source-mapped dimensions: 800x800 window, 220px sidebar, 30px navigation
rows, source 15/13/11px typography. Categories are Bar, Appearance and
System. Corner size is edited on the Appearance page. Nix-managed
rows describe temporary process edits; unmanaged edits persist through the
writable TOML owner. `bar.preferred_output` is shown on the Bar page but not offered
as a control.

Search matches setting title/detail/key literally, with 15 result slots and 256
query bytes processed. Back/Forward retains at most 64 routes and truncates stale
forward routes when navigating after Back. Ctrl+F focuses native search; Tab
switches sidebar/results; Up/Down navigates; Enter/Space activates selected
editable results; Alt+Left/Right navigates history; Escape closes.

CPU: `lua tests/settings-ui/process.lua` and `lua tests/settings-ui/corner-size.lua`. Native: `tests/native-settings.sh
--settings` (see `tests/WINDOW-RUNNER.md`).
No omitted gaming/Assistant/DND/admin controls are reintroduced.
