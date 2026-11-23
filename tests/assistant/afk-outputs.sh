#!/usr/bin/env bash
# Private output-profile acceptance only; no host display or configuration.
set -euo pipefail
: "${AGENT_SANDBOX_INSTANCE:?sandbox required}" "${GPU_LEASE_ID:?GPU reservation required}"
export WAYLAND_DISPLAY=hyprland-1
instance=
for lock in "$XDG_RUNTIME_DIR"/hypr/*/hyprland.lock; do
    readarray -t lines < "$lock"
    if [[ "$XDG_RUNTIME_DIR/${lines[1]}" -ef "$XDG_RUNTIME_DIR/hyprland-1" ]]; then instance=$(dirname "$lock");break;fi
done
: "${instance:?private nested compositor required}"
export HYPRLAND_INSTANCE_SIGNATURE=${instance##*/}
runtime=${AFK_RUNTIME:?set the pinned native runtime executable}
cleanup() {
    hyprctl output remove AFK-PORTRAIT >/dev/null 2>&1 || :
    hyprctl eval 'hl.monitor({output="NESTED-1",mode="1920x1080@60",position="0x0",scale=1,transform=0})' >/dev/null 2>&1 || :
}
trap cleanup EXIT
hyprctl eval 'hl.monitor({output="NESTED-1",mode="1920x1080@60",position="0x0",scale=1.25,transform=0})'
python3 tests/assistant/afk-capture.py --runtime "$runtime" --scene screen_window --seconds 20
python3 tests/assistant/afk-capture.py --runtime "$runtime" --scene glass_inspection --seconds 20
hyprctl output create headless AFK-PORTRAIT
hyprctl eval 'hl.monitor({output="AFK-PORTRAIT",mode="1920x1080@60",position="-864x0",scale=1.25,transform=1})'
python3 tests/assistant/afk-capture.py --runtime "$runtime" --output AFK-PORTRAIT --scene screen_window --seconds 9
python3 tests/assistant/afk-capture.py --runtime "$runtime" --output AFK-PORTRAIT --scene glass_inspection --seconds 20
python3 tests/assistant/afk-capture.py --runtime "$runtime" --output AFK-PORTRAIT --scene window_perch --seconds 21
hyprctl eval 'hl.monitor({output="NESTED-1",mode="3840x2160@60",position="0x0",scale=2,transform=0})'
python3 tests/assistant/afk-capture.py --runtime "$runtime" --scene screen_window --seconds 9
python3 tests/assistant/afk-capture.py --runtime "$runtime" --scene wing_grooming --seconds 21
