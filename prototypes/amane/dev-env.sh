#!/usr/bin/env bash
# Native dependencies: Wayland, xkbcommon, fontconfig, PulseAudio, Vulkan loader.
# In this Nix sandbox the development outputs exist but are outside pkg-config's path.
if [[ -d /nix/store ]]; then
  export PKG_CONFIG_PATH="$(printf '%s\n' /nix/store/*-dev/lib/pkgconfig | paste -sd:)${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export LD_LIBRARY_PATH="$(printf '%s\n' /nix/store/*-{wayland,libxkbcommon,fontconfig,libpulseaudio,vulkan-loader,mesa}-*/lib | paste -sd:)${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export LD_LIBRARY_PATH="/nix/store/amcyi2h5qll35baaj4wvm9nb5pyis35h-linux-pam-1.7.2/lib:$LD_LIBRARY_PATH"
  export LIBRARY_PATH="$LD_LIBRARY_PATH${LIBRARY_PATH:+:$LIBRARY_PATH}"
fi
