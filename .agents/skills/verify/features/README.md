# Tohru verification map

The maintained seed map starts with the bar/CLI user experience and standalone
setup. Read the matching file before driving. The helper's three runnable features
are `calendar`, `settings` and `session-units`; the remaining entry points below
are explicit gates, not inferred coverage.

Baseline: fresh HOME/XDG state with voice/Discord/polkit/corners off, owned private
Sway display and session bus, no system bus or audio server, patched runtime
supplied explicitly, and a GPU lease for native runs. `doctor.json` identifies
the instance. Never send commands to a pre-existing host session. Start each
recipe in a fresh `run`; the driver retains evidence and removes scratch state.

## Proof rules

- Capture the public action plus its resulting state, with exact command/exits.
- Native proof pairs scene observations with actual screenshots; it does not
  establish physical input, hardware rendering, host compositor or pixel parity.
- Persisted settings require both stored TOML and public status.
- Record feature ID and entry point. Report unrun and unreachable paths separately.
- Failed preconditions stay failures; do not seed host credentials or invent a
  workspace, media player, portal grant, notification sender or voice caller.

## Features

- [Panels and calendar](panels.md): CLI open/toggle/close is runnable; clock click,
  Escape, date navigation, other panels and sound side effects remain separate.
- [Settings](settings.md): public CLI edit/reload/persistence is runnable; Settings
  UI editing, managed overlays and process restart need the existing native suite.
- [Launcher](launcher.md): mapped existing native harness; not yet exercised here.
- [Notifications](notifications.md): mapped private-bus harness; not exercised here.
- [Standalone units](session-units.md): actual staging generation and no-overwrite.

Other identified surfaces outside this seed are session power actions, lock/PAM,
greeter, wallpapers, network/Bluetooth devices, media players, assistant/provider
chats, voice/acoustics, workspace activation and optional Hyprland effects. Their
source/docs do not imply verification. Adding coverage requires a dedicated map
entry and real isolated preconditions; never activate host services to fill a gap.

The source was interviewed through README, `docs/standalone.md`, runtime README,
`bin/tohru`, `shell/bar.plm`, feature scene/controllers and `tests/WINDOW-RUNNER.md`.
The initial delivery's run ledger is retained at `.tmp/docs/verification-report.md`
in the originating worktree, not as a portable prerequisite for this skill.
