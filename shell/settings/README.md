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
rows, source 15/13/11px typography. Categories are Bar, Appearance, System,
Wallpaper and Capture. Corner size is edited on the Appearance page. Nix-managed
rows describe temporary process edits; unmanaged edits persist through the
writable TOML owner. Deprecated preferred_output is not offered as a control.

Search matches setting title/detail/key literally, with 15 result slots and 256
query bytes processed. Back/Forward retains at most 64 routes and truncates stale
forward routes when navigating after Back. Ctrl+F focuses native search; Tab
switches sidebar/results; Up/Down navigates; Enter/Space activates selected
editable results; Alt+Left/Right navigates history; Escape closes.

CPU: `lua tests/settings-ui/process.lua`, `lua tests/settings-ui/corner-size.lua`
and `lua tests/settings-ui/wallpaper-choices.lua`. Native: `tests/native-settings.sh
--settings` (see `tests/WINDOW-RUNNER.md`).
No omitted gaming/Assistant/DND/admin controls are reintroduced.


Wallpaper uses W's actual540×526 picker at244,104 in this same window. Its folder,
assignment map and enabled flag belong to the one Rust owner, including Nix
process overrides and durability warnings. Folder loading happens only on explicit
picker open/refresh/change, with native scoped catalog and output queries. No
second preferences reader is introduced. Assignment failures leave the accepted
map/routes unchanged; offline assignments survive. Video entries are retained
with an honest pending-renderer notice until that separately owned renderer is
integrated. Native decoding/pixels are not inferred from successful persistence.

`tohru wallpaper open` enters this route, `wallpaper randomize` uses retained
implicit output selection, and `config set wallpaper.enabled true|false` changes
the existing flag. Mouse/category/history navigation remains in the same window;
Ctrl+F targets picker search on this page, Up/Down belong to file selection, and
Escape returns to Appearance. Closing the window hides the picker. Native input,
geometry/hotplug and rendered catalog evidence remain runtime gates.


Capture preferences use the same owner: Save/Copy toggles and native directory/
filename fields submitted with Enter. Empty path/pattern follow native defaults.
Unrelated settings publication preserves an in-progress path/pattern draft;
accepted value changes/reload refresh that field. Invalid submission leaves the
accepted configuration unchanged and shows its error. Disabling both flags is
allowed, but a region request then fails before freezing.

`tohru screenshot region|cancel` and `screenshot status --json` route to the one
root capture controller. An active selection excludes root menus and Settings,
and disables wallpaper picker keyboard handling. A second region request reports
busy while preserving the frozen selection. Native export results drive the fixed
notification role status; saving, copying and durability remain separate facts.
Actual native clipboard/GPU/topology/input/close-to-recapture evidence remains
pending the allocated private runtime gates.
