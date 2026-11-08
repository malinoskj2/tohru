#!/usr/bin/env bash
# Requires an exclusive GPU slot; never uses the live display or bus.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
mkdir -p "$root/.tmp"
bus_config=$(mktemp "$root/.tmp/private-bus.XXXXXX.conf")
trap 'rm -f -- "$bus_config"' EXIT
python3 - "$bus_config" "$root/.tmp" <<'PY'
import sys
from xml.sax.saxutils import escape
from pathlib import Path
Path(sys.argv[1]).write_text('''<busconfig>
<type>session</type>
<listen>unix:tmpdir=''' + escape(sys.argv[2]) + '''</listen>
<auth>EXTERNAL</auth>
<policy context="default">
<allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/>
</policy>
</busconfig>
''')
PY
source "$root/runtime/dev-env.sh"
export LD_LIBRARY_PATH="/run/opengl-driver/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
TOHRU_PRIVATE_TEST_BUS=1 dbus-run-session --config-file "$bus_config" -- python3 "$root/tests/native-settings.py" "$@"
