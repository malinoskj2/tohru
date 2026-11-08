# Private session/auth entry points

These are independent development scenes, not root imports or production activation.
Use the assembled pinned runtime with reviewed native lock release-authority and
secret-export protections. An unpatched upstream runtime permits IPC unlock and
secret reads and is unsuitable for these scenes. Native resize/hotplug/edit/export/
reload/crash checks have passed on frozen53f8ea2; controlled fixture PAM and final
native idle/guard integration remain acceptance gates. No physical-session safety
or activation is inferred. Scene-only changes reuse the compiled runtime.
After coordinator GPU allocation, on private compositor and private buses only:

```sh
pleamar --scene shell/session/menu.plm --answer-as tohru-session --no-hud
pleamar --scene shell/lock/lock.plm --answer-as tohru-lock --no-hud
pleamar --scene shell/polkit/polkit.plm --answer-as tohru-polkit --no-hud
```

Run each command in its own dedicated process. Polkit must use a private system
bus too: its service connects to the system authority. Never launch those commands
against the user's display or buses. See OWNERSHIP.md for required lifetimes.
There is no production launcher/CLI/Nix integration in this slice.

| Native name | Request | Observable status |
| --- | --- | --- |
| tohru-session | `emit menu_open`, `emit menu_close`, `emit menu_toggle` | `open`, `confirming`, `remaining`, `status`, `error` |
| tohru-session | `emit choose N` (1 lock, 2 sleep, 3 restart, 4 shutdown, 5 logout) | 3–5 open confirmation; `emit accept_choice` or `emit cancel_choice` |
| tohru-lock | `emit lock_request`, `emit lock_and_suspend` | `engaged` is requested state; `lock.held` is native compositor acknowledgment; `acknowledged` is the rule's mirrored fact; `checking`, `status` |
| tohru-polkit | Native authority drives prompt, `emit cancel_request` | Native `auth.ready/pending/asking/error`, `status` |

The UI accepts passwords only in native secret inputs: do not expose a public unlock event,
record password texts or put secrets in CLI arguments. Escape clears the lock
input and cancels pending suspend; it never releases a lock. Native successful
`auth.check` is the UI path that sets `engaged=false`; unrestricted native IPC
can bypass that UI path, the blocking security gap above. Submitted sleep requests stay deduplicated across logic reload until successful
authentication or `system_resumed` from the future real resume observer. That event
is not a public user CLI action, and no native resume observer is wired yet.
A new lock queued behind successful PAM waits for the old native held=false edge
and a fresh held=true edge; cached readiness cannot authorize the new cycle.
A five-second missing
acknowledgment cancels sleep while leaving the lock requested. Native rule
`on change lock.held` mirrors acknowledgment into Luau because the upstream render
updates `lock.held` without directly sending its fact event to logic.

The user explicitly accepted native session execution semantics: native commands
spawn systemctl/loginctl, do not observe child exit or asynchronous completion,
and may fail after request acceptance. Logout uses native terminate-session when
XDG_SESSION_ID is available and otherwise native terminate-user fallback; no safer
wrapper is substituted. Text status says request acceptance, never power success.
The user separately accepted native polkit's selected single identity and omitted
expandable details; no backend extension or old auth backend is included.

CPU validation:

```sh
pleamar --check shell/session/menu.plm
pleamar --check shell/lock/lock.plm
pleamar --check shell/polkit/polkit.plm
lua tests/session/logic.lua
```

The test executes actual Lua-compatible entry scripts with fake native boundaries
and a deterministic timer. It checks confirmation expiration/cancellation, native
routing, missing/late acknowledgment, lock retention on denial/unavailable PAM,
reload retention, and polkit cancellation/duplicate answer protection. This is
not a native Luau/compositor/PAM/helper integration test. Frozen53f8ea2 private native tests cover acknowledgment delivery, actual keyboard
editing under publicfalse, multioutput resize/hotplug, crash fail-locked and reload.
Private controlled PAM/account-denial and polkit authority/helper flows remain
unverified. Native popup click grab/Escape and CLI idle Escape behavior are accepted;
no catcher/backend extension is added.

Presentation is an initial native scene implementation with glass; source-matched
layout/motion, wallpaper treatment, font/assets, authentication transitions and
matched screenshots remain unfinished. No visual parity or independent visual acceptance,
physical-session safety or performance gate is claimed.

`menu_toggle` closes an open menu and cancels any confirmation; opening resets
selection and errors. The root owns its visible menu control: this feature scene
has only a 1px transparent anchor surface, without a duplicate visible button.

After coordinator GPU allocation, `tests/session/native.sh --runtime /absolute/pleamar`
runs private menu/lock protocol checks without PAM/power by default. Optional
explicit guarded fixture modes are documented in tests/session/pam_fixture.py;
they require executable wtype/bwrap, exact private namespace/policy guard, synthetic
credentials and a separate authorized allocation. Optional notifications workload
uses private buses and an owned null audio sink. There is never a host-PAM fallback.
See L progress log for exact frozen artifact/hashes and evidence paths.

The ordinary-root private idle entry is `shell/session/idle.plm` + `idle.luau`.
Native copy/install hooks and API are in `runtime/idle/README.md`; the new adapters
are source checkpoints awaiting native compilation/protocol validation. Dedicated
locker `state_request` emits `idle_lock_state`0/1 to fixed root `tohru` using actual
native held query, solely a timing hint. The locker-local native logind delay
helper bypasses Lua/PAM liveness and requires confirmed local held authority;
no public fact/event or ordinary-root hint can release its descriptor.
