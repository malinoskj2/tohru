#!/usr/bin/env bash
# Optional native headers/loader paths for the supplied Nix development sandbox.
if [[ -d /nix/store ]]; then
    for dependency in libxkbcommon wayland fontconfig freetype expat libffi; do
        for folder in /nix/store/*"$dependency"*-dev/lib/pkgconfig; do
            [[ ! -d $folder ]] || export PKG_CONFIG_PATH="$folder${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
        done
    done
    for folder in /nix/store/*linux-pam*/lib; do
        [[ ! -d $folder ]] || export LIBRARY_PATH="$folder${LIBRARY_PATH:+:$LIBRARY_PATH}"
    done
    for folder in /nix/store/*vulkan-loader*/lib /nix/store/*libglvnd*/lib; do
        [[ ! -d $folder ]] || export LD_LIBRARY_PATH="$folder${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    done
fi
