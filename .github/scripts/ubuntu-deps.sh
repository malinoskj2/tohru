#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl git pkg-config python3 \
    libffi-dev libfontconfig-dev libpam0g-dev libwayland-dev libxkbcommon-dev
