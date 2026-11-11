#!/usr/bin/env bash
# Requires an exclusive GPU slot; Python owns its private bus/display and cleanup.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
source "$root/runtime/pleamar/dev-env.sh"
export LD_LIBRARY_PATH="/run/opengl-driver/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec python3 "$root/tests/native-settings.py" "$@"
