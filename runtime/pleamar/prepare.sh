#!/usr/bin/env bash
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
package=$(dirname -- "$here")
destination=${1:-$(dirname -- "$package")/.tmp/pleamar-runtime}
[[ ! -e $destination ]] || { printf 'Destination already exists: %s\n' "$destination" >&2; exit 2; }
pin() { sed -n "s/^ *\"$1\": *\"\\([^\"]*\\)\".*/\\1/p" "$here/pin.json"; }
archive_url=$(pin archive)
hash=$(pin hash)
[[ -n $archive_url && $hash == sha256-* ]] || { printf 'Cannot read archive and hash from %s\n' "$here/pin.json" >&2; exit 2; }
sha256=$(printf '%s' "${hash#sha256-}" | base64 -d | od -An -v -tx1 | tr -d ' \n')
mkdir -p -- "$destination"
destination=$(cd -- "$destination" && pwd)
archive=$(mktemp)
trap 'rm -f -- "$archive"' EXIT
curl --fail --location --silent --show-error "$archive_url" -o "$archive"
printf '%s  %s\n' "$sha256" "$archive" | sha256sum --check --status
tar -xzf "$archive" -C "$destination" --strip-components=1
# The ceiling keeps git apply from treating an enclosing checkout as the repository, where it would skip these paths.
for patch in "$here"/patches/*.patch; do
    (cd -- "$destination" && GIT_CEILING_DIRECTORIES=$(dirname -- "$destination") git apply "$patch")
done
mkdir "$destination/foundation"
cp "$package/Cargo.toml" "$package/Cargo.lock" "$destination/foundation/"
cp -r "$package/src" "$package/tests" "$destination/foundation/"
for binding in settings workspace info; do
    cp "$here/bindings/${binding}_luau.rs" "$destination/src/tohru_$binding.rs"
done
mkdir -p "$destination/tests/runtime"
cp "$here/tests/role-install.rs" "$destination/tests/runtime/"
cp "$here/Cargo.lock" "$destination/Cargo.lock"
printf 'Prepared %s\n' "$destination"
