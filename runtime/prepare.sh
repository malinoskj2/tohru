#!/usr/bin/env bash
# Prepare clean upstream with reviewed shell patches; no prototype vendor copy.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
destination=${1:-$root/.tmp/pleamar-runtime}
[[ ! -e $destination ]] || { printf 'Destination already exists: %s\n' "$destination" >&2; exit 2; }
mkdir -p -- "$destination"
archive=$(mktemp)
trap 'rm -f -- "$archive"' EXIT
curl --fail --location --silent --show-error \
    https://github.com/k4ditano/pleamar/archive/dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86.tar.gz -o "$archive"
printf '%s  %s\n' '3b9e9320c4240ff0af7b43d5bba6bc8625f5f8d110c75eefd0d9904cbf1bafbf' "$archive" | sha256sum --check --status
tar -xzf "$archive" -C "$destination" --strip-components=1
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0001-shell-settings.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0002-lock-boundary.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/workspace/shader-inputs.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0004-workspace-binding.patch"
mkdir "$destination/foundation"
cp "$root/Cargo.toml" "$destination/foundation/"
cp -r "$root/src" "$destination/foundation/"
mkdir -p "$destination/foundation/runtime" "$destination/foundation/tests/session"
cp -r "$root/runtime/idle" "$destination/foundation/runtime/"
mkdir "$destination/foundation/runtime/workspace"
cp "$root"/runtime/workspace/*.rs "$destination/foundation/runtime/workspace/"
cp "$root/tests/session/idle.rs" "$destination/foundation/tests/session/"
cp "$root/runtime/settings_luau.rs" "$destination/src/tohru_settings.rs"
cp "$root/runtime/workspace_luau.rs" "$destination/src/tohru_workspace.rs"
mkdir -p "$destination/src/tohru_lock" "$destination/tests/session"
cp "$root/runtime/lock/native.rs" "$destination/src/tohru_lock.rs"
cp "$root/runtime/lock/authority.rs" "$destination/src/tohru_lock/authority.rs"
cp "$root/tests/session/lock_authority.rs" "$root/tests/session/native_security.rs" "$root/tests/session/native_binding.rs" "$destination/tests/session/"
cp "$root"/tests/session/security-* "$destination/tests/session/"
cp "$root/runtime/Cargo.lock" "$destination/Cargo.lock"
printf 'Prepared %s\n' "$destination"
