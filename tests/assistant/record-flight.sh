#!/usr/bin/env bash
# Private two-client native fixture. Run from the worktree under gpu-run.
set -euo pipefail
: "${AGENT_SANDBOX_INSTANCE:?sandbox required}" "${GPU_LEASE_ID:-${RESOURCE_JOB_ID:?reservation required}}"
: "${FLIGHT_RUNTIME:?set to the pinned pleamar executable}" "${FLIGHT_RECORDER:=wf-recorder}"
root=$(git rev-parse --show-toplevel)
cd "$root"
export WAYLAND_DISPLAY=hyprland-1
instance=
for lock in "$XDG_RUNTIME_DIR"/hypr/*/hyprland.lock; do
    readarray -t lines < "$lock"
    if [[ "$XDG_RUNTIME_DIR/${lines[1]}" -ef "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]]; then
        instance=$(dirname "$lock")
        break
    fi
done
: "${instance:?private nested Hyprland required}"
export HYPRLAND_INSTANCE_SIGNATURE=${instance##*/}
export XDG_CONFIG_HOME=$root/.tmp/flight-config XDG_CACHE_HOME=$root/.tmp/flight-cache PATH=$root/bin:$PATH
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
scene=$(python3 tests/assistant/flight-demo.py)
tag="tohru-flight-test-$$"
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
    wait || :
    hyprctl eval 'if flight_test_rule then flight_test_rule:set_enabled(false); flight_test_rule=nil end' >/dev/null
}
trap cleanup EXIT
hyprctl eval "flight_test_rule=hl.window_rule({name='$tag',match={class='$tag'},float=true,size='940 570',move='160 145'})"
foot --app-id "$tag" --title "Contextual flight / native test window" \
    sh -c 'echo "Native window · compositor occlusion"; exec sleep 100' >.tmp/flight-window.log 2>&1 &
window=$!
pids+=("$window")
sleep 1
hyprctl -j activewindow | jq '{at,size,workspace,monitor,mapped,hidden,address}' >.tmp/window.json
for entry in '1 wrap' '2 rise' '3 right' '4 sweep' '1 moved' '1 interrupted' '5 reduced'; do
    read -r number variant <<< "$entry"
    if [[ -n ${1:-} && $variant != "$1" ]]; then continue; fi
    "$FLIGHT_RUNTIME" --scene "$scene" --answer-as "$tag" --no-hud --stall 0 \
        --record ai_flying,ai_flight_x,ai_flight_y,ai_flight_behind,ai_flight_alpha >".tmp/trace-$variant.log" 2>&1 &
    actor=$!
    pids+=("$actor")
    sleep 2.5
    "$FLIGHT_RECORDER" -y -f ".tmp/$variant.mp4" -c libx264 -p preset=ultrafast -p threads=2 -r 24 >".tmp/$variant-record.log" 2>&1 &
    recorder=$!
    pids+=("$recorder")
    sleep .3
    "$FLIGHT_RUNTIME" --say "$tag" "emit test_show $number"
    sleep .65
    if [[ "$variant" == moved ]]; then
        hyprctl eval "hl.dispatch(hl.dsp.window.move({x=250,y=200,relative=false,window='class:$tag'}))"
        hyprctl -j activewindow | jq '{at,size,address}' >.tmp/window-moved.json
    fi
    if [[ "$variant" == interrupted ]]; then kill "$window"; fi
    sleep 3.3
    kill -INT "$recorder"
    wait "$recorder"
    "$FLIGHT_RUNTIME" --say "$tag" quit
    wait "$actor"
    pids=()
    if [[ "$variant" != interrupted && "$variant" != reduced ]]; then pids+=("$window"); fi
done
