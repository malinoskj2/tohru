#!/usr/bin/env bash
# Exclusive GPU slot required. No live display/bus or real power/auth commands.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
mkdir -p "$root/.tmp"
source "$root/runtime/pleamar/dev-env.sh"
export LD_LIBRARY_PATH="/run/opengl-driver/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
python3 "$root/tests/session/native.py" "$@"
