# Session process ownership contract

Status: proposed activation contract; no owner is started by the ordinary bar.
The root shell may open the session menu and send requests; it must never import
or start a notification singleton owner.

| Role | Lifetime and private entry point | Restart/failure rule |
| --- | --- | --- |
| Session menu | Ordinary UI, `shell/session/menu.plm` | Closing/reloading cancels confirmation. |
| Locker | hyprlock, started by hypridle's `lock_cmd` on `loginctl lock-session`; not part of Tohru | Owned by `~/nix`. |
| Polkit | hyprpolkitagent; not part of Tohru | Owned by `~/nix`; exactly one agent per login session. |
| Notifications | Dedicated native role; internal retention accepted | Exactly one bus owner; no activation before singleton wiring/validation. |

Nix owns independent service/process lifetime, the locker, the polkit agent and polkit policy.

Sleep requests the lock before suspending: the menu runs `loginctl lock-session`,
and only after it exits successfully runs `systemctl suspend`. hypridle's
`before_sleep_cmd` locks as well, so the locker, not Tohru, guarantees the session
is locked before sleep.

All development execution uses a private compositor and private session/system
buses, with explicit coordinator GPU allocation first. No live login session
lock/power commands or live bus service names are permitted.
