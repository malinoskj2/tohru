# Installation and recovery

## Sub-features

`package-runtime` (C/Package) and `owned-recovery` (C/H) remain unverified by this
runner. Entrypoints: installed CLI help/smoke, GUI executable, supervisor prepare,
rehearsal, harmless independent unit, separately authorized actual restart.

## How to get to it (user POV)

Run the exact packaged command with minimal PATH and observe useful help/action.
For recovery prove ownership and readiness of the same profile after automatic
owned launch; an independently manually opened app is not automatic recovery proof.

## Driving it with the shared runner

`python3 tools/verify/verify.py doctor --scenario owned-recovery --target sandbox --json`
reports blocked. Reuse integrations/orca/restart-supervisor.py and
integrations/orca/test-restart-supervisor.py. Read-only rehearsal and a harmless
independent unit precede any live restart; neither grants permission to stop a GUI.
The package gate needs exact committed local source/ref/NAR hash and candidate
output, minimal interpreter/tool/library closure and model-file prerequisites.
No package build or commit authorizes host-switch.

## Gotchas

Negative cases: missing env/dirname/flock, wrong native ABI or source pin, stale
PID/birth/runtime, manager-inherited denied environment, lost/uncertain launch
reply. Active/unknown launch retains its original unit and never starts a duplicate.
Record controller and GUI units separately, service-level UnsetEnvironment,
bounded readiness and exact process chain; preserve persistent daemon/profile.
Only the coordinator resumes retained reviewers. No current task authority permits
host switch, live Orca/bar restart, device settings or broad sandbox changes.
