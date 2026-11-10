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
git apply --unsafe-paths --directory="$destination" "$root/runtime/launcher/namespace.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0003-window-lifecycle.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0006-info-binding.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/idle/0004-idle-native.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0008-idle-binding.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0010-key-rule-order.patch"
git apply --unsafe-paths --directory="$destination" "$root/runtime/patches/0011-focus-round.patch"
mkdir "$destination/foundation"
cp "$root/Cargo.toml" "$destination/foundation/"
cp "$root/Cargo.lock" "$destination/foundation/"
cp -r "$root/src" "$destination/foundation/"
mkdir -p "$destination/foundation/runtime" "$destination/foundation/tests/session"
mkdir "$destination/foundation/runtime/idle"
cp "$root/runtime/idle/mod.rs" "$root/runtime/idle/cookies.rs" "$root/runtime/idle/delay.rs" "$root/runtime/idle/input.rs" "$root/runtime/idle/logind_state.rs" "$root/runtime/idle/configuration.rs" "$root/runtime/idle/teardown.rs" "$destination/foundation/runtime/idle/"
mkdir "$destination/foundation/runtime/workspace"
cp "$root"/runtime/workspace/*.rs "$destination/foundation/runtime/workspace/"
mkdir -p "$destination/foundation/runtime/info" "$destination/foundation/tests/info"
cp "$root"/runtime/info/*.rs "$destination/foundation/runtime/info/"
cp "$root/tests/info/snapshot.rs" "$destination/foundation/tests/info/"
cp "$root/tests/info.rs" "$root/tests/settings.rs" "$destination/foundation/tests/"
cp "$root/tests/session/idle.rs" "$destination/foundation/tests/session/"
cp "$root/tests/session/idle_cookies.rs" "$root/tests/session/idle_delay.rs" "$root/tests/session/idle_input.rs" "$root/tests/session/idle_logind.rs" "$root/tests/session/idle_configuration.rs" "$root/tests/session/idle_teardown.rs" "$destination/foundation/tests/session/"
cp "$root/runtime/settings_luau.rs" "$destination/src/tohru_settings.rs"
cp "$root/runtime/workspace_luau.rs" "$destination/src/tohru_workspace.rs"
cp "$root/runtime/window/visibility.rs" "$destination/src/tohru_window.rs"
cp "$root/runtime/info_luau.rs" "$destination/src/tohru_info.rs"
mkdir -p "$destination/src/tohru_idle"
cp "$root/runtime/idle/native.rs" "$destination/src/tohru_idle.rs"
cp "$root/runtime/idle/observer.rs" "$root/runtime/idle/screensaver.rs" "$root/runtime/idle/logind.rs" "$root/runtime/idle/sleep_guard.rs" "$destination/src/tohru_idle/"
mkdir -p "$destination/src/tohru_lock" "$destination/tests/session"
cp "$root/runtime/lock/native.rs" "$destination/src/tohru_lock.rs"
cp "$root/runtime/lock/authority.rs" "$destination/src/tohru_lock/authority.rs"
cp "$root/runtime/lock/publication.rs" "$destination/src/tohru_lock/publication.rs"
cp "$root/tests/session/lock_authority.rs" "$root/tests/session/lock_publication.rs" "$root/tests/session/native_security.rs" "$root/tests/session/native_binding.rs" "$destination/tests/session/"
cp "$root"/tests/session/security-* "$destination/tests/session/"
mkdir -p "$destination/tests/runtime"
cp "$root/tests/runtime/role-install.rs" "$destination/tests/runtime/"
cp "$root/runtime/Cargo.lock" "$destination/Cargo.lock"
printf 'Prepared %s\n' "$destination"
