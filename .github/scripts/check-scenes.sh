#!/usr/bin/env bash
set -euo pipefail
pleamar=$1
status=0
while IFS= read -r scene; do
    printf 'pleamar --check %s\n' "$scene"
    "$pleamar" --check "$scene" || status=1
done < <(grep -rlE --include='*.plm' '^scene ' shell | sort)
exit "$status"
