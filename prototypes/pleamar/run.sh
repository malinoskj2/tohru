#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
source ./env.sh
mode=${1:-live}
case "$mode" in live|replay|idle|active) ;; *) echo 'Usage: run.sh live|replay|idle|active [Pleamar options]' >&2; exit 2 ;; esac
shift || true
export WORKSPACE_MODE=$mode
exec ./vendor/pleamar/target/release/pleamar --scene "$PWD/workspace.plm" --no-hud --stall 0 --answer-as workspace-pleamar "$@"
