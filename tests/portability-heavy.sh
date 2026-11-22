#!/usr/bin/env bash
set -euo pipefail
[[ -n ${RESOURCE_JOB_ID:-} ]] || { echo 'Run this through the shared resource-run CPU/RAM admission.' >&2; exit 2; }
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
export CARGO_BUILD_JOBS=4
export RUST_TEST_THREADS=4
export CARGO_TARGET_DIR="$root/.tmp/portability-verified-runtime/target"
cargo clippy --locked --all-targets --manifest-path runtime/Cargo.toml -- -D warnings
cargo test --locked --manifest-path runtime/Cargo.toml
cargo build --release --locked --manifest-path .tmp/portability-verified-runtime/Cargo.toml --bin pleamar
cargo test --release --locked --manifest-path .tmp/portability-verified-runtime/Cargo.toml --lib tohru_
