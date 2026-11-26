# Standalone portability delivery and verification

Branch: `malinoskj2/wayland-portability`. Implementation commits:
`8b49059` (output/workspace separation) and `df055d6` (standalone ownership,
configuration, diagnostics, portal shortcuts, packaging and focused tests).
Documentation/test commit `0688f82` contains the full report; the subsequent doctor readiness refinement also requires an owned wallpaper supervisor and a loadable locker unit when their dependent features are enabled. The implementation was integrated into main on 2026-11-22 at the user's request, with the independent worktree scheduled for closeout after evidence preservation.
The selected session was confirmed by the supervisor as Codex gpt-6.1-sol, medium.

Implementation is available across the requested areas. **Combined compilation,
Nix evaluation/builds and live acceptance remain unverified**, not silently waived.
This report separates source/policy/protocol-fixture evidence from real desktop
compatibility. Setup instructions are in [standalone.md](standalone.md).

## Main integration checks

The integration preserves the native-audit single Settings reopen handler: show,
reload, publish effective settings, probe the effective microphone, then query
shortcut ownership. Its combined fixture retains stale microphone rejection and
checks one shortcut query after successful reload and none after failed reload.
Both portal UI and managed font coverage remain present.

Nineteen lightweight check groups passed on the integrated source: portability
policy/lifecycle (23 cases), private fake-portal D-Bus (9), selected CLI/locker/
greeter (10), voice path/cancellation (9), disabled Discord (1), package lock-graph,
nine Lua controller groups including the native-audit Calendar/media regressions,
and four Nix syntax checks. Nix parsing is not evaluation or a build. The heavy
and live gates below remain unexecuted; merging does not waive them or activate
the host.

Integration logs and a copy of the original worker evidence are retained under
`.tmp/integration-top-level/portability/` in main. Original scripts/logs may contain
the removed worktree's absolute paths; consult `PRESERVATION.md` there before
reusing them. The source-only native-audit changes remain merged alongside this
implementation.

## Resource gate

At both initial and final inspection, `/etc/resource-scheduler/mode` did not exist
and `resource-run` was absent from PATH. No resource job was already allocated.
Heavy verification was not started or bypassed. The supervisor was notified.
No GPU/model workload, host activation, idle/audio route change, real PAM login,
Discord traffic, display-manager installation or host portal grant was performed.

The pin/patch preparation step downloads/extracts source only. The optional portal
harness used 86 KB of upstream dbus-fast source, hash-checked against its PyPI
metadata and extracted under `.tmp/portal-test-deps`; nothing was installed on the
host or vendored. No voice dependency/model installer or model download ran.

## Requirement matrix

| Requested area | Implementation / boundary | Actual available checks | Remaining limits/gates |
|---|---|---|---|
| 1. Generic bar outputs | Patch 0034 adds independent wl_output/xdg-output discovery; `shell/bar/workspace.luau` uses physical names, preferred-output intersection/fallback and opaque workspace IDs | Real controller test: no workspace service, empty list, missing monitor data, multiple outputs, reordered names, scale/rotation metadata, unplug/replug; wayland-info parser fixture covers older xdg-output names/logical size | No live hotplug/scale/rotation run; comma-containing names remain unsupported by the runtime selector |
| 2. Window/workspace boundary | Existing Hyprland IPC retained; generic wlr foreign toplevel + ext-workspace; explicit protocol/per-workspace activation capability; native handles never inferred from display names | Workspace and launcher fallback tests, bar-title/controller regressions, fresh patch application and Rust syntax parsing | No new Sway/niri/COSMIC IPC adapters; unsupported protocols are reported. Generic protocol implementation is not universal WM certification. Ribbon shows ten slots on the active page; compositor controls reach other pages |
| 3. Wallpaper | `tohru-wallpaper` owns configured mpvpaper children/private sockets; XDG assignments, atomic locked persistence, scan/status, apply/load/pause/seek/stop; original media/picker preview path retained | Actual local supervisor/control/player sockets, shutdown cleanup and disabled restart; actual child crash/restart and independent external player/socket preservation; failed-start retry bound; assignment/path tests; full Lua picker regression | Fixture players/output reports are not a live compositor/libmpv test. Policy changes require service restart. No claim that IPC command success proves visible media decoding |
| 4. Lock/idle | `tohru-session-action`, optional swayidle owner, shared notify locker unit; native locked event sends READY, fresh nonce/current-cycle status query gates lock/suspend; PAM permits/plain recovery retained | Helper refusal/inactive/stale/ready-then-exit/challenge tests; real local status socket fixture; locker wrapper duplicate/refusal/fallback/missing-flock tests; session controller fallback; native repeated-cycle authority regression added | Native authority test not compiled/run. No real PAM, compositor crash/restore, repeated physical lock/unlock, suspend/inhibitor or hotplug-while-locked run. Logind delay inhibitors are finite; concurrent later unlock is not atomically prevented |
| 5. Standalone/HM session integration | Shared `share/tohru/session/services.json`; explicit generated units, managed environment, graphical target and journal policy; launcher keeps KillMode=process; optional ownership choices | Nine generated units pass systemd-analyze verify with installed example targets; unit generation/escaping/no-overwrite tests; HM source updated/parsed | No running-session systemd lifecycle/teardown test. One graphical-session environment per user manager; not multiseat certification. HM/Nix evaluation/build remains gated |
| 6. Fonts | Native text/display/mono families; runtime role mapping includes greeter/lock and preserves features/weights; Noto/DejaVu defaults with normal system fallback; real Appearance controls, managed read-only values | Native validation/ownership regression added; real Settings controller tests for editable native and managed read-only fonts; Rust/scene-source inspection | Native regression not compiled/run; no physical layout/fallback rendering check |
| 7. Greeter | Administrator templates for greetd/Sway/TUI recovery, native runtime paths, assets, power policy and setup validation; distro UID_MIN/high-UID discovery; NixOS Noto fonts and child-exit status propagation | Four greeter wrapper checks including fatal-child status propagation; template validator returns expected missing prerequisites without host changes; Nix files parsed; UID regression added | No installation, PAM/authentication or actual greetd session. NixOS VM and distro recovery/permission/avatar/power checks remain gated |
| 8. Voice | Configurable interpreter and explicit TOML environment; arbitrary local paths retained; offline defaults and spaCy downloader guard; explicit dependency/model provenance docs | Nine existing model-path/cancellation regressions; environment layering test; Python syntax; direct dependency versions/artifact hashes verified from primary PyPI metadata | No dependency environment installation/inference or microphone test. Candidate Python 3.11 environment is not transitively hash-locked or inference-certified. Models remain user-supplied |
| 9. Echo cancellation | Shared HM/standalone PipeWire drop-in, explicit opt-in, exclusive file creation, no route/restart action; separate configured/installed/module/node/selection/effectiveness checks | Shared-config/exclusive-write policy tests, HM source/parse checks, doctor JSON/human paths | No PipeWire restart/module activation, physical loopback or acoustic validation. Module/node availability is not successful cancellation |
| 10. Packaging/doctor | Nix/release helpers + setup data/docs; declared required vs optional dependencies; read-only bounded subprocesses; structured required/severity/summary and exit 0/1/2/3 | 23 policy/lifecycle tests cover invalid/inaccessible config, missing tools, timeout/output bounds, conditional requirements, read-only status, redaction and relocated non-Nix layout. Actual doctor emits valid JSON (87 checks) and human output, expected exit 1 here | No compiled release/dynamic-loader/Nix closure test. Sandbox has missing runtime/session/tools; its nonzero doctor result is expected and not a healthy-session claim |
| 11. Generic Nix/non-Nix UI | Native XDG config remains first-class, managed overlay preserved; Nix generation reads are gated on platform identity; ordinary UI replaces Generations with Platform | XDG/managed/alias/path/unit/layout tests; native non-Nix snapshot regression added; Nix syntax and lock-graph consistency checks | Native snapshot regression not run; HM/NixOS evaluation and non-Nix rendered System Info remain gated |
| 12. Shortcuts ownership/editing | Default compositor mode displays actual Hyprland runtime bindings read-only; Sway explicitly cannot enumerate active bindings. Opt-in native portal mode owns only three fixed Tohru actions, persistent session, user Setup/v2 Configure, actual trigger descriptions, owner/session/grant filtering and cleanup | Nine tests on a real private dbus-daemon with a fake portal: delayed/cancelled/denied/partial requests, actual descriptions vs preferred hints, changes/revocation/restart, previous grants inactive, v1 limit, duplicate owner, unknown IDs/sessions, disabled/compositor no portal contact. Settings tests exercise real Setup/Configure wiring/read-only rows/managed mode | No host grant/configure dialog, desktop app-identity/backend compatibility or actual key delivery test. v1 cannot Configure; service/portal restart requires explicit Setup to bind remembered grants, avoiding automatic prompt loops. No global capture/config rewrite |

## Recorded checks

Latest results under this worktree's `.tmp/`:

- `portability-policy.log`: **23 tests passed**. This includes the added final
  end-to-end wallpaper supervisor/disabled-restart and relocated layout checks.
- `shortcuts-portal.log`: **9 private D-Bus tests passed**, dbus-fast 5.2.0 pure
  Python path; no desktop portal or host grant interaction.
- `cli-portability.log`: **10 tests passed**, with resident util-linux on PATH,
  including four greeter wrapper checks. No skipped tests in this selected set.
- `voice-regression.log`: **9 model-path/cancellation tests passed**, no models.
- `discord-regression.log`: **1 disabled/capability test passed**, no traffic.
- Lua logs: workspace, launcher output, bar controllers, session menu, Settings
  (fonts/managed/portal UI) and wallpaper preview/revert/apply regression passed.
- `unit-verify.log`: all nine units verified with systemd 260.5 and its example
  user targets. Initial verification without those target files correctly failed;
  `systemd-analyze verify` does not activate units or establish session behavior.
- `packaging-locks.log`: all 22 foundation registry identities match the runtime
  vendor lock graph. No Rust dependency was added.
- `rustfmt-check.log`, Nix parse logs, Python and shell syntax checks passed.
- `final-patch-prepare.log`: pinned source integrity/patch application succeeded
  into `.tmp/portability-verified-runtime`; changed Rust modules parsed with
  rustfmt. **Parsing is not type checking, linking or passing Rust tests.**
- `doctor-json.log`/`doctor-human.log`: expected exit **1**, required failures in
  this sandbox; structured summary verified. `greeter-template-check.log`: expected
  exit **1** for uninstalled host/user/PAM/session/font prerequisites.
- `verification-results.json`: 31 grouped lightweight checks, all expected exit
  statuses. The final policy/doctor extensions were rerun afterwards and passed 23 tests.

An actual sanitized JSON excerpt is in
[examples/doctor-sandbox.json](examples/doctor-sandbox.json). Doctor's explicit
contract: 0 required checks healthy (optional warnings allowed), 1 required failure,
2 invalid/inaccessible config, 3 required unknown with no definite failure.
Every check includes required/severity and automation can inspect summary lists.

## Reproduce lightweight checks

With Python 3.11+, Lua 5.4, util-linux and dbus-daemon on PATH:

```sh
mkdir -p .tmp
cargo test --locked --manifest-path desktop/Cargo.toml
cargo build --locked --release --manifest-path desktop/Cargo.toml
python3 tests/cli.py Cli.test_portable_config_keys_route_to_native_schema \
  Cli.test_session_menu_routes_never_start_a_role \
  LockWrapper.test_one_locker_at_a_time \
  LockWrapper.test_a_refusal_from_the_compositor_is_not_retried \
  LockWrapper.test_without_required_flock_it_reports_failure_before_lock_start \
  LockWrapper.test_a_style_that_does_not_load_locks_with_the_plain_one GreeterWrapper
python3 tests/assistant/voice.py ModelPathTests StreamingTests.test_cancel_waiting_for_provider_exits_worker
lua tests/bar/workspace.lua
lua tests/launcher/output.lua
lua tests/bar/controllers.lua
lua tests/session/logic.lua
lua tests/settings-ui/process.lua
lua tests/wallpaper/controller.lua
python3 tests/packaging-locks.py
cargo fmt --check --manifest-path runtime/Cargo.toml
```

These are small source/policy/protocol fixtures, not the substantial native suites.
Do not treat a fixture portal/compositor-output/player as real advertised desktop
compatibility. The screenshot/gallery path was not needed; no media was generated.

## Unexecuted combined gates

Restore the shared admission service/tool first. The current sandbox cannot run
these by bypassing it. No host activation/switch/merge is part of these commands.
Reserve the full Nix daemon lifetime; daemon work can escape the caller's cgroup.
Do not nest reservations when RESOURCE_JOB_ID/GPU_LEASE_ID is already set.

CPU/RAM gate (use the prepared source above or prepare a fresh destination first):

```sh
resource-run --cpu 4 --ram 8GiB --project tohru --name "portability Rust verification" --category build -- \
  bash tests/portability-heavy.sh
resource-run --cpu 4 --ram 8GiB --project tohru --name "portability Nix package" --category build -- \
  nix build --no-link .#tohru
resource-run --cpu 2 --ram 4GiB --project tohru --name "portability NixOS evaluation" --category test -- \
  nix eval --impure --json --expr 'let t=builtins.getFlake ("path:"+toString ./.); in import ./nix/tests/nixos.nix { nixpkgs=t.inputs.nixpkgs; tohru=t; }'
# Point this at a caller-approved existing Home Manager source/flake:
export TOHRU_HOME_MANAGER_SOURCE=/absolute/path/to/home-manager
resource-run --cpu 2 --ram 4GiB --project tohru --name "portability HM evaluation" --category test -- \
  nix eval --impure --json --expr 'let t=builtins.getFlake ("path:"+toString ./.); hm=builtins.getFlake ("path:"+builtins.getEnv "TOHRU_HOME_MANAGER_SOURCE"); in import ./nix/tests/home-manager.nix { pkgs=t.inputs.nixpkgs.legacyPackages.x86_64-linux; homeManager=hm; tohru=t; }'
```

Native private-display gate, only after the build passes and sandbox display/tool
setup is available. These suites require exclusive graphics admission:

```sh
runtime="$PWD/.tmp/portability-verified-runtime/target/release/pleamar"
resource-run --exclusive --cpu 4 --ram 8GiB --vram 4GiB --project tohru --name "portability native bar" --category test -- \
  python3 tests/bar/native.py --runtime "$runtime" --services-only
resource-run --exclusive --cpu 4 --ram 8GiB --vram 4GiB --project tohru --name "portability native launcher" --category test -- \
  python3 tests/bar/native.py --runtime "$runtime" --launcher-only
resource-run --exclusive --cpu 4 --ram 8GiB --vram 4GiB --project tohru --name "portability native settings" --category test -- \
  bash tests/native-settings.sh --runtime "$runtime" --settings
resource-run --exclusive --cpu 4 --ram 8GiB --vram 4GiB --project tohru --name "portability native session" --category test -- \
  bash tests/session/native.sh --runtime "$runtime" --notifications
```

Still required beyond those automated suites: actual non-Hyprland ext-workspace
and missing-workspace sessions, scale/rotation/hotplug; real mpvpaper still/video;
PAM, repeat lock/unlock/crash/restore and logind suspend/inhibitors; running user-unit
startup/environment/teardown and child survival; NixOS greetd VMs or distro VM
login/recovery/avatar/power; physical font rendering; portal desktop grant/v1/v2
configuration/app identity and real key delivery; PipeWire route/reference/acoustic
validation; optional voice environment/model inference and cancellation under load.
Do those on disposable/private sessions, not by changing the host's live desktop.
