#!/usr/bin/env bash
# Exclusive GPU slot required. No live display/bus or real power/auth commands.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
mkdir -p "$root/.tmp"
config=$(mktemp "$root/.tmp/session-bus.XXXXXX.conf")
trap 'rm -f -- "$config"' EXIT
python3 - "$config" "$root/.tmp" <<'PY'
from pathlib import Path
from xml.sax.saxutils import escape
import sys
Path(sys.argv[1]).write_text('<busconfig><type>session</type><listen>unix:tmpdir='+escape(sys.argv[2])+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
PY
source "$root/runtime/dev-env.sh"
export LD_LIBRARY_PATH="/run/opengl-driver/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
TOHRU_PRIVATE_SESSION_TEST=1 dbus-run-session --config-file "$config" -- python3 "$root/tests/session/native.py" "$@"
