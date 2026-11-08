#pragma once
// Use the real candidate's build macros without pulling compositor-global objects
// into a standalone test executable. Only the ABI helper's required host call is stubbed.
#include <hyprland/src/version.h>
extern "C" const char* __hyprland_api_get_hash();
