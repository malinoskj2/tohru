# Runtime

This folder holds the Rust side of Tohru:

- the `tohru-shell` crate (`Cargo.toml`, `src/`, `tests/`), which keeps state that has to outlive a scene's Luau logic;
- `pleamar/`, everything that turns Pleamar 0.7.27 into the runtime Tohru needs.

## The `tohru-shell` crate

| Module | Luau global | Used by |
|---|---|---|
| `src/settings.rs` | `tohru_settings` | Bar, Settings and the session menu: layered TOML settings (defaults, writable file, managed file, runtime overrides) that keep comments and unknown keys when written |
| `src/info/` | `tohru_info` | The information and About popups: a read-only snapshot of the OS, kernel, CPU, GPUs, memory, uptime, accounts, disks, mounts and system generations |

The crate builds on its own with Rust 1.85 (its `rust-version`) and denies unsafe code. Its tests:

```sh
cargo test --locked --manifest-path runtime/Cargo.toml
```

## Building the patched Pleamar

| Path | Contents |
|---|---|
| `pleamar/pin.json` | Upstream repository, version 0.7.27, revision `dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86`, archive URL and hash |
| `pleamar/patches/` | Tohru's patches, applied in name order. [docs/architecture.md](../docs/architecture.md#runtime-patches) lists what each one does |
| `pleamar/bindings/` | The Luau globals, copied into the Pleamar tree as `src/tohru_settings.rs` and `src/tohru_info.rs` |
| `pleamar/Cargo.lock` | Replaces Pleamar's lock file. It must contain every package in `runtime/Cargo.lock`; `python3 tests/packaging-locks.py` checks this |
| `pleamar/tests/role-install.rs` | Copied to `tests/runtime/` and included by patch 0001. Checks that the globals are installed only when `TOHRU_SHELL=1`, and never into plugin scripts |
| `pleamar/prepare.sh` | Builds a patched source tree from the pieces above |
| `pleamar/dev-env.sh` | Adds `pkg-config`, library and loader paths from `/nix/store` for building outside a Nix development shell |

`prepare.sh` downloads the archive named in `pin.json`, checks its hash, applies every patch, copies in the crate, the bindings, `role-install.rs` and the lock file, and writes the tree to `.tmp/pleamar-runtime` (or the directory given as its only argument). It refuses to overwrite an existing destination. `nix/default.nix` builds the same tree from the same files.

Building the runtime needs Rust 1.90 or newer, because of Pleamar's locked dependencies.

```sh
./runtime/pleamar/prepare.sh
source runtime/pleamar/dev-env.sh   # only without a Nix development shell
cargo build --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --bin pleamar
cargo test --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --lib tohru_
```

The `tohru_` filter runs the binding tests and the role-install test, the same set the Nix build runs. Pleamar's other tests may need desktop services.

## How the bindings are installed

Patches 0001 and 0006 install the two globals into a scene's main Luau state only when `TOHRU_SHELL=1`. `bin/tohru-run`, `bin/tohru-session-run` and `bin/tohru-settings-run` set it; the launcher and notification daemon do not, and have no Tohru globals. The state behind each global lives in Rust, outside the Luau state, so it survives a logic reload but not a process restart.

To run a scene by hand with the bindings:

```sh
TOHRU_SHELL=1 .tmp/pleamar-runtime/target/release/pleamar --scene shell/bar.plm --answer-as tohru
```

`bin/tohru-run` does the same with `TOHRU_RUNTIME` set to the binary.
