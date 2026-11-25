# Installation and recovery

## Sub-features

`session-units` (C) is a supported end-to-end shipped source CLI journey. It
generates real private session files, observes their contents and tests refusal
to overwrite and rejection of an invalid target. It does not install/start units.

`package-runtime` (C/Package) and `owned-recovery` (C/H) remain unverified by this
runner. Entrypoints: installed CLI help/smoke, GUI executable, supervisor prepare,
rehearsal, harmless independent unit, separately authorized actual restart.

## How to get to it (user POV)

Use `bin/tohru session-units --output PRIVATE_DIRECTORY --components bar,session,launcher`
to prepare standalone session integration. The output contains the bar, session,
launcher and on-demand lock services; attempting to generate again refuses replacement.

Run the exact packaged command with minimal PATH and observe useful help/action.
For recovery prove ownership and readiness of the same profile after automatic
owned launch; an independently manually opened app is not automatic recovery proof.

## Driving it with the shared runner

From the discoverable project `verify` skill:

```sh
python3 tools/verify/verify.py doctor --scenario session-units --target sandbox --json
python3 tools/verify/verify.py run session-units --target sandbox
```

Prerequisites: executable bin/tohru, bin/tohru-portability built with
`cargo build --release --manifest-path desktop/Cargo.toml` (the shim honours
`CARGO_TARGET_DIR` and `TOHRU_DESKTOP_TARGET`), the shipped
share/tohru/session/services.json, Python >=3.11, bash and coreutils dirname/readlink.
The runner invokes the actual executable through its shipped shebang, with a
private HOME/XDG/TMPDIR and no inherited caller, desktop sockets or credentials.
It does not substitute a runtime or service-manager fixture. Launch sets up
private directories; the user action is the actual CLI invocation.

Required observable outcome: four persisted service files with exact executable
paths, graphical-session target binding, launcher KillMode=process and the lock's
notify/on-demand policy. A repeat invocation must fail without changing any byte;
an invalid target must fail without creating its output directory. Exit status
alone cannot pass. Observations retain full file contents and SHA-256; assertions
record each expected effect. Cleanup verifies the files remain unchanged and
retains them and immutable evidence under the returned absolute run path.
Use the common cleanup/collect/validate commands for the returned run; cleanup
twice must retain the same successful result and surviving evidence.

`python3 tools/verify/verify.py doctor --scenario owned-recovery --target sandbox --json`
reports blocked. Reuse integrations/orca/restart-supervisor.py and
integrations/orca/test-restart-supervisor.py. Read-only rehearsal and a harmless
independent unit precede any live restart; neither grants permission to stop a GUI.
The package gate needs exact committed local source/ref/NAR hash and candidate
output, minimal interpreter/tool/library closure and model-file prerequisites.
No package build or commit authorizes host-switch.

## Gotchas

This is the shipped checkout command, not a Nix-installed package or a running
session. Service activation, native GUI, physical lock UX, minimal-PATH package
closure and host recovery remain unverified. No services are enabled/started,
and the generated files stay in the run's private output rather than a systemd
search directory. The invalid/overwrite attempts are explicit negative user
actions, not automatic retries after an ambiguous effect.

Negative cases: missing env/dirname/flock, wrong native ABI or source pin, stale
PID/birth/runtime, manager-inherited denied environment, lost/uncertain launch
reply. Active/unknown launch retains its original unit and never starts a duplicate.
Record controller and GUI units separately, service-level UnsetEnvironment,
bounded readiness and exact process chain; preserve persistent daemon/profile.
Only the coordinator resumes retained reviewers. No current task authority permits
host switch, live Orca/bar restart, device settings or broad sandbox changes.
