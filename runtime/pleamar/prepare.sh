#!/usr/bin/env bash
# Prepare clean upstream with reviewed shell patches; no prototype vendor copy.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
package=$(dirname -- "$here")
destination=${1:-$(dirname -- "$package")/.tmp/pleamar-runtime}
[[ ! -e $destination ]] || { printf 'Destination already exists: %s\n' "$destination" >&2; exit 2; }
mkdir -p -- "$destination"
archive=$(mktemp)
trap 'rm -f -- "$archive"' EXIT
curl --fail --location --silent --show-error \
    https://github.com/k4ditano/pleamar/archive/dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86.tar.gz -o "$archive"
printf '%s  %s\n' '3b9e9320c4240ff0af7b43d5bba6bc8625f5f8d110c75eefd0d9904cbf1bafbf' "$archive" | sha256sum --check --status
tar -xzf "$archive" -C "$destination" --strip-components=1
for patch in "$here"/patches/*.patch; do
    git apply --unsafe-paths --directory="$destination" "$patch"
done
mkdir "$destination/foundation"
cp "$package/Cargo.toml" "$package/Cargo.lock" "$destination/foundation/"
cp -r "$package/src" "$package/tests" "$destination/foundation/"
for binding in settings info; do
    cp "$here/bindings/${binding}_luau.rs" "$destination/src/tohru_$binding.rs"
done
mkdir -p "$destination/tests/runtime"
cp "$here/tests/role-install.rs" "$destination/tests/runtime/"
cp "$here/Cargo.lock" "$destination/Cargo.lock"
printf 'Prepared %s\n' "$destination"
