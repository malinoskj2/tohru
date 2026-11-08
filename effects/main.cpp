#include <stdexcept>
#include <string>
#include <string_view>

#include "Abi.hpp"

#include <hyprland/src/managers/EventManager.hpp>
#include <hyprland/src/render/Renderer.hpp>

// Hook compatibility is reviewed for this exact source revision, not a version range.
static_assert(std::string_view{GIT_COMMIT_HASH} == "efb50993780079460b0cbed1363e2166a2de1d9f",
              "Review private hooks before changing the supported Hyprland revision");

APICALL PLUGIN_DESCRIPTION_INFO j2barFocusInit(HANDLE);
APICALL void j2barFocusExit();
APICALL PLUGIN_DESCRIPTION_INFO j2barSheetInit(HANDLE);
APICALL void j2barSheetExit();
APICALL PLUGIN_DESCRIPTION_INFO j2barRecedeInit(HANDLE);
APICALL void j2barRecedeExit();
APICALL PLUGIN_DESCRIPTION_INFO j2barBarsInit(HANDLE);
APICALL void j2barBarsExit();

namespace {
HANDLE pluginHandle = nullptr;
SP<SHyprCtlCommand> capabilitiesCommand;
bool focusStarted = false;
bool sheetStarted = false;
bool recedeStarted = false;
bool barsStarted = false;

void stop() {
    // Nested transformer passes may retain bar elements whose destructors live in this DSO.
    g_pHyprRenderer->m_renderPass.clear();
    if (capabilitiesCommand) {
        HyprlandAPI::unregisterHyprCtlCommand(pluginHandle, capabilitiesCommand);
        capabilitiesCommand.reset();
    }
    if (barsStarted) {
        j2barBarsExit();
        barsStarted = false;
    }
    if (recedeStarted) {
        j2barRecedeExit();
        recedeStarted = false;
    }
    if (sheetStarted) {
        j2barSheetExit();
        sheetStarted = false;
    }
    if (focusStarted) {
        j2barFocusExit();
        focusStarted = false;
    }
}
}

APICALL EXPORT std::string PLUGIN_API_VERSION() {
    return HYPRLAND_API_VERSION;
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    pluginHandle = handle;
    // The private helper prevents weak client-hash symbol interposition by the host.
    if (!J2barABI::matchesHost())
        throw std::runtime_error("[desktop-effects] Hyprland build hash mismatch");

    try {
        // Mark before entering each component so partial initialization is unwound too.
        focusStarted = true;
        j2barFocusInit(handle);
        sheetStarted = true;
        j2barSheetInit(handle);
        recedeStarted = true;
        j2barRecedeInit(handle);
        barsStarted = true;
        j2barBarsInit(handle);
        capabilitiesCommand = HyprlandAPI::registerHyprCtlCommand(handle, SHyprCtlCommand{
            .name = "desktop-effects-capabilities",
            .exact = true,
            .fn = [handle](eHyprCtlOutputFormat, std::string) {
                return "{\"protocol\":1,\"plugin\":\"desktop-effects\",\"hyprland_hash\":\"" +
                    HyprlandAPI::getHyprlandVersion(handle).hash +
                    "\",\"capabilities\":[\"focus-dips\",\"title-bars\",\"recession\",\"sheet-transitions\"]}";
            },
        });
        if (!capabilitiesCommand)
            throw std::runtime_error("[desktop-effects] Could not register capability command");
    } catch (...) {
        stop();
        throw;
    }
    return {"desktop-effects", "Optional compositor focus, titlebar, chooser and launcher effects", "jesse", "0.1.0"};
}

APICALL EXPORT void PLUGIN_EXIT() {
    stop();
}
