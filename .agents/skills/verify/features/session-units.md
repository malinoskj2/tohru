# Standalone service staging

Users generate reviewable systemd user unit files before deciding whether to
install and enable Tohru components.

## Sub-features

- `units-generate`: stage bar, session and launcher plus on-demand lock.
- `units-no-overwrite`: refuse to overwrite an existing output file.
- `units-activate`: install/enable and desktop lifecycle, outside this safe scope.

## How to get to it (user POV)

- Run `tohru session-units --output DIR --prefix /absolute/path/to/bin
  --components bar,session,launcher`.
- Inspect the generated files before installation. `docs/standalone.md` documents
  optional managed config/target/component flags and later activation.

## Driving it with drive.py

Preconditions: Python 3.11+, Bash and this checkout. No runtime/display/GPU needed.

- **Launch/doctor.** Run `drive.py run --feature session-units`. The helper creates
  empty XDG/HOME staging and checks `tohru help` with no production endpoints.
- **Generate.** It executes the public staging command with the checkout's bin
  prefix; require exactly four named service files and the correct bar executable,
  graphical-session target and lock `Type=notify`.
- **Repeat.** It runs generation again against the same directory, requires a
  nonzero exit and independently hashes all files to confirm none changed.
- **Evidence.** Inspect `commands.jsonl`, copied `units/`, `staging.json`,
  `cleanup.json` and post-cleanup `summary.json` hashes.

## Gotchas

- Generation is not activation, systemd semantic validation or a functioning
  desktop. The helper never calls systemctl or installs files in a user manager.
- A repeat can fail after earlier files if component sets differ; this journey
  repeats the same initial file so it specifically verifies no overwrite.
- All installed-session, alternate component/target/managed and non-Nix packaging
  paths remain unverified by this one staging journey.
