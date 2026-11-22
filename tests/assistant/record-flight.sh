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
scene=$(python3 tests/assistant/flight-demo.py --voice)
out=$(mktemp -d "$root/.tmp/flight-voice-XXXXXX")
echo "Evidence $out"
mkdir "$out/bin"
export PATH=$out/bin:$PATH
export FLIGHT_DEMO_EVENTS=$out/events
mkfifo "$FLIGHT_DEMO_EVENTS"
cat >"$out/bin/tohru-flight-fixture-voice" <<'SH'
#!/bin/sh
exec cat "$FLIGHT_DEMO_EVENTS"
SH
chmod +x "$out/bin/tohru-flight-fixture-voice"
exec 6<>"$FLIGHT_DEMO_EVENTS"
send() { printf '%s\n' "$1" >&6; }
wake() {
    send '{"event":"hearing","on":true}'; send '{"event":"hearing","on":false}'
    send '{"event":"conversation","on":true}'; send '{"event":"turn","text":"Can you hear me?"}'
}
monitor=${FLIGHT_OUTPUT:-NESTED-1}
case $monitor in
    NESTED-1) size="940 570"; pos="160 145";;
    FLIGHT-PORTRAIT) size="432 768"; pos="136 195";;
    *) exit 2;;
esac
names=$(hyprctl -j monitors | jq -r 'map(.name)|join(",")')
hyprctl -j monitors >"$out/monitors.json"
tag="tohru-flight-test-$$"
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
    wait || :
    hyprctl eval 'if flight_test_rule then flight_test_rule:set_enabled(false); flight_test_rule=nil end' >/dev/null
}
trap cleanup EXIT
hyprctl eval "flight_test_rule=hl.window_rule({name='$tag',match={class='$tag'},float=true,monitor='$monitor',size='$size',move='$pos'})"
foot --app-id "$tag" --title "Contextual flight / native test window" \
    sh -c 'echo "Native window · compositor occlusion"; exec sleep 100' >"$out/terminal.log" 2>&1 &
window=$!
pids+=("$window")
sleep 1
hyprctl -j activewindow | jq '{at,size,workspace,monitor,mapped,hidden,address}' >"$out/window.json"
for entry in '1 cold-warm' '1 wrap' '2 rise' '3 right' '4 sweep' '1 moved' '1 hearing' '1 speaking' '1 interrupted' '5 reduced'; do
    read -r number variant <<< "$entry"
    if [[ -n ${1:-} && $variant != "$1" ]]; then continue; fi
    "$FLIGHT_RUNTIME" --scene "$scene" --screen "$monitor" --answer-as "$tag" --no-hud --stall 0 \
        --record ai_flying,ai_flight_x,ai_flight_y,ai_flight_behind,ai_flight_alpha,ai_flight_phase,ai_flight_art,ai_flight_cell,ai_dg_frame,ai_dg_warm,ai_dragon_open,ai_speaking,ai_listening >"$out/trace-$variant.log" 2>&1 &
    actor=$!
    pids+=("$actor")
    sleep 1
    "$FLIGHT_RUNTIME" --say "$tag" "text test_output $names" "emit test_show $number"
    send "{\"event\":\"ready\",\"dragon\":\"$root/shell/assistant/dragon\"}"
    send '{"event":"listening","on":true}'
    if [[ $variant != cold-warm ]]; then sleep 3.3; fi
    "$FLIGHT_RECORDER" -o "$monitor" -y -f "$out/$variant.mp4" -c libx264 -p preset=ultrafast -p threads=2 -r 24 >"$out/$variant-record.log" 2>&1 &
    recorder=$!
    pids+=("$recorder")
    sleep .3
    wake
    if [[ $variant == cold-warm ]]; then
        sleep .8
        send '{"event":"conversation","on":false}'
        sleep 5.5
        wake
    fi
    sleep .65
    if [[ "$variant" == moved ]]; then
        hyprctl eval "hl.dispatch(hl.dsp.window.move({x=250,y=200,relative=false,window='class:$tag'}))"
        hyprctl -j activewindow | jq '{at,size,address}' >"$out/window-moved.json"
    fi
    if [[ $variant == interrupted ]]; then
        kill "$window"; wait "$window" || :
        window=
    fi
    if [[ $variant == hearing ]]; then send '{"event":"hearing","on":true}'; fi
    if [[ $variant == speaking ]]; then send '{"event":"speaking","on":true,"conversation":"test"}'; fi
    "$FLIGHT_RUNTIME" --say "$tag" 'get ai_flight_trace' >"$out/selection-$variant.json"
    hyprctl -j layers >"$out/layers-$variant.json"
    if [[ $variant != reduced ]]; then
        jq -se --arg name "$monitor" 'length == 1 and .[0].scene.output == $name' "$out/selection-$variant.json" >/dev/null || {
            echo "No entrance trace for $variant on $monitor" >&2; exit 1;
        }
    fi
    if [[ $variant == hearing || $variant == speaking ]]; then
        sleep .35
        "$FLIGHT_RUNTIME" --say "$tag" 'get ai_flying' 'get ai_flight_art' 'get ai_dragon_open' >"$out/preempt-$variant.log"
        [[ $(tr '\n' / <"$out/preempt-$variant.log") == false/false/true/ ]] || exit 1
        sleep 2.95
    else
        sleep 3.3
    fi
    if [[ $variant == hearing ]]; then send '{"event":"hearing","on":false}'; fi
    send '{"event":"speaking","on":true,"conversation":"test"}'
    sleep .3
    send '{"event":"speaking","on":false,"conversation":"test"}'
    send '{"event":"conversation","on":false}'
    kill -INT "$recorder"
    wait "$recorder"
    "$FLIGHT_RUNTIME" --say "$tag" quit
    wait "$actor"
    pids=()
    if [[ -n $window ]]; then pids+=("$window"); fi
done
