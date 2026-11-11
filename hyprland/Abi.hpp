#pragma once

#include <hyprland/src/plugins/PluginAPI.hpp>

#include <string>
#include <string_view>

namespace J2barABI {

// PluginAPI.hpp exports its inline client-hash helper as a weak symbol. A direct
// call may bind to the compositor's copy and compare the server with itself.
// Build the same version string privately so the plugin checks its own headers.
static std::string expectedHash() {
    const auto stripPatch = [](std::string_view version) {
        if (!version.contains('.'))
            return std::string{version};
        return std::string{version.substr(0, version.find_last_of('.'))};
    };

    return std::string{GIT_COMMIT_HASH} + "_aq_" + stripPatch(AQUAMARINE_VERSION) + "_hu_" + stripPatch(HYPRUTILS_VERSION) + "_hg_" +
           stripPatch(HYPRGRAPHICS_VERSION) + "_hc_" + stripPatch(HYPRCURSOR_VERSION) + "_hlg_" + stripPatch(HYPRLANG_VERSION);
}

static bool matchesHost() {
    return expectedHash() == __hyprland_api_get_hash();
}

} // namespace J2barABI
