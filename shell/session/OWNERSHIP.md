# Session/auth process ownership contract

Status: proposed activation contract; no owner is started by the ordinary bar.
Production security BLOCKED: unrestricted native IPC can write `engaged=false`
and bypass UI authentication; the proposed auth-only release rule below is not
enforced at the native control boundary. User decision pending, no workaround.
The root shell may open the session menu and send requests; it must never import
or start a session-lock, polkit, notification or idle singleton owner.

| Role | Lifetime and private entry point | Restart/failure rule |
| --- | --- | --- |
| Session menu | Ordinary UI, `shell/session/menu.plm` | Closing/reloading cancels confirmation; never releases a lock. |
| Locker | Dedicated Pleamar process, `shell/lock/lock.plm`, native `kind: lock` | Independent of bar process/group/service. Keep alive while locked. Only successful native PAM result releases `open`; never release on Escape, denial, timeout, scene reload or power failure. Compositor must retain lock after crash. |
| Polkit | Dedicated native Pleamar agent, `shell/polkit/polkit.plm` | Exactly one per login session. Cancel pending conversation on owner stop; no j2bar backend. |
| Idle | One integrated process-owned boundary in existing shell package | Own standard idle/inhibitor observation, fade cancellation, screen-power adapter and logind delay inhibitor; never an additional daemon. |
| Notifications | Dedicated native role; internal retention accepted | Exactly one bus owner; no activation before singleton wiring/validation. |

Nix owns independent service/process lifetime, PAM policy and polkit policy.
An ordinary UI restart must not propagate stop/reload to the locker service.
A locker reload must not reset its open state: do not enable scene hot reload for
an engaged production locker until native behavior is verified. The owner must
remain reachable for authenticated unlock and output hotplug while the bar exits.

Lock-before-suspend is a completion contract: send lock request to locker, await
native compositor `lock.held`, then suspend from the locker while that lock remains
held. Native message acceptance, setting `open`, a drawn cover and loginctl's
request acceptance are insufficient. Failure, missing owner, denial or readiness
timeout must leave suspend unissued. Never implement a timed sleep as readiness.
Repeated requests cannot overlap authentication/power operations. Resume restores
screen power and cancels fades; it never changes lock state to unlocked.

All development execution uses a private compositor and private session/system
buses, with explicit coordinator GPU allocation first. No live login session
lock/power commands or live bus service names are permitted.
