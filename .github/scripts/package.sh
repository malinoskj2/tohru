#!/usr/bin/env bash
# Usage: package.sh PREPARED_PLEAMAR_TREE PLEAMAR_BINARY OUT_DIR VERSION
set -euo pipefail
source=$(cd -- "$1" && pwd)
binary=$2
out=$3
version=$4
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
root=$stage/tohru

mkdir -p "$root/bin" "$root/licenses/pleamar" "$out"
cp -a bin/. "$root/bin/"
install -m 755 "$binary" "$root/bin/pleamar"
strip "$root/bin/pleamar"
cp -a shell "$root/"
cp LICENSE "$root/LICENSE"
printf '%s\n' "$version" > "$root/VERSION"
cp "$source/LICENSE" "$source/THIRD-PARTY.md" "$root/licenses/pleamar/"
cargo about generate --locked --fail --manifest-path "$source/Cargo.toml" \
    --config "$here/about.toml" --output-file "$root/licenses/pleamar/THIRD-PARTY-LICENSES.txt" "$here/about.hbs"
# Notices for Tohru code compiled into the runtime that carries its own licence files.
while IFS= read -r notice; do
    mkdir -p "$root/licenses/tohru-shell/$(dirname -- "$notice")"
    cp "runtime/src/$notice" "$root/licenses/tohru-shell/$notice"
done < <(cd runtime/src && find . -name '*LICENSE*' -printf '%P\n' | sort)

tar --sort=name --owner=0 --group=0 --numeric-owner \
    -C "$stage" -czf "$out/tohru-x86_64-linux.tar.gz" tohru
