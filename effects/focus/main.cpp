#define WLR_USE_UNSTABLE

#include <unistd.h>
#include <vector>

#include <hyprland/src/includes.hpp>
#include <any>
#include <regex>
#include <sstream>

#define private public
#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/config/ConfigValue.hpp>
#include <hyprland/src/config/shared/animation/AnimationTree.hpp>
#include <hyprland/src/helpers/AnimatedVariable.hpp>
#include <hyprland/src/animation/AnimationManager.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopManager.hpp>
#include <hyprland/src/managers/fullscreen/FullscreenController.hpp>
#include <hyprland/src/layout/LayoutManager.hpp>
#include <hyprland/src/config/ConfigManager.hpp>
#include <hyprland/src/config/values/types/BoolValue.hpp>
#include <hyprland/src/config/values/types/FloatValue.hpp>
#include <hyprland/src/config/values/types/StringValue.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/desktop/state/FocusState.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#undef private

#include "globals.hpp"

#include <hyprutils/string/VarList.hpp>
#include <hyprutils/animation/BezierCurve.hpp>
using namespace Hyprutils::String;
using namespace Hyprutils::Animation;

static struct {
    SP<Config::Values::CBoolValue>   enable;
    SP<Config::Values::CBoolValue>   animateFloating;
    SP<Config::Values::CBoolValue>   onlyOnMonitorChange;
    SP<Config::Values::CStringValue> keyboardFocusAnimation;
    SP<Config::Values::CStringValue> mouseFocusAnimation;
    SP<Config::Values::CFloatValue>  fadeOpacity;
    SP<Config::Values::CFloatValue>  shrinkPercentage;
    SP<Config::Values::CFloatValue>  slideHeight;
    SP<Config::Values::CStringValue> windowClass;
} configValues;

// Do NOT change this function.
APICALL EXPORT std::string j2barFocusApiVersion() {
    return HYPRLAND_API_VERSION;
}

// Shrink is done purely at render time. While a window shrinks and grows back we
//  - drop Hyprland's sendWindowSize for it, so the client is never configured to the shrunk size and
//    doesn't relayout (Firefox visibly rescales its page otherwise), and
//  - set m_animatingIn, which makes the renderer scale the buffer into the smaller box instead of
//    cropping it (same path Hyprland uses for popin).
struct SHeldWindow {
    PHLWINDOWREF window;
    bool         setAnimatingIn = false;
};
static std::vector<SHeldWindow> heldWindows;
static CFunctionHook*           g_pSendWindowSizeHook = nullptr;

// Held globally and dropped in PLUGIN_EXIT: dlclose can't actually unload this .so (it has
// STB_GNU_UNIQUE symbols), so a function-local static listener would outlive an unload and keep
// animating alongside the next loaded copy.
static Hyprutils::Signal::CHyprSignalListener g_focusListener;

static bool isHeld(const Desktop::View::CWindow* w) {
    return std::ranges::any_of(heldWindows, [w](const auto& e) { return e.window.get() == w; });
}

typedef void (*origSendWindowSize)(Desktop::View::CWindow*, bool);
static void hkSendWindowSize(Desktop::View::CWindow* thisptr, bool force) {
    if (isHeld(thisptr))
        return;

    (*(origSendWindowSize)g_pSendWindowSizeHook->m_original)(thisptr, force);
}

static void holdClientResize(PHLWINDOW w) {
    std::erase_if(heldWindows, [](const auto& e) { return !e.window; });

    if (isHeld(w.get()))
        return;

    heldWindows.emplace_back(SHeldWindow{w, !w->m_animatingIn});
    w->m_animatingIn = true;
}

static void releaseClientResize(PHLWINDOW w) {
    const auto it = std::ranges::find_if(heldWindows, [&w](const auto& e) { return e.window.get() == w.get(); });
    if (it == heldWindows.end())
        return;

    if (it->setAnimatingIn)
        w->m_animatingIn = false;
    heldWindows.erase(it);

    // no-op unless the layout changed the size while sends were held
    if (w->m_isMapped)
        w->sendWindowSize();
}

static bool isMouseReason(Desktop::eFocusReason r) {
    return r == Desktop::FOCUS_REASON_CLICK || r == Desktop::FOCUS_REASON_FFM;
}

static void onFocusChange(PHLWINDOW window, Desktop::eFocusReason reason) {
    if (!window)
        return;

    if (!configValues.enable->value())
        return;

    if (!configValues.animateFloating->value() && window->m_isFloating)
        return;

    static PHLWINDOWREF lastWindow;

    if (lastWindow == window)
        return;

    if (configValues.onlyOnMonitorChange->value() && lastWindow && lastWindow->m_monitor == window->m_monitor)
        return;

    lastWindow = window;

    // Mapping can focus before popin starts; either opening callback can clear m_animatingIn.
    // Wait for both geometry animations before taking over their goals and callbacks.
    if (reason == Desktop::FOCUS_REASON_NEW_WINDOW || window->m_firstMap || window->m_animatingIn ||
        window->positionAnimation()->isBeingAnimated() || window->sizeAnimation()->isBeingAnimated())
        return;

    const auto CLASS = configValues.windowClass->value();
    if (!CLASS.empty()) {
        // an invalid regex must not throw inside the compositor
        try {
            if (!std::regex_match(window->m_class, std::regex(CLASS)))
                return;
        } catch (const std::regex_error&) { return; }
    }

    const auto mode = isMouseReason(reason)
        ? configValues.mouseFocusAnimation->value()
        : configValues.keyboardFocusAnimation->value();

    // mode is a comma-separated list, e.g. "flash,shrink"
    const auto has = [&mode](const std::string& m) { return std::ranges::contains(CVarList(mode, 0, ','), m); };

    const auto PIN  = Config::animationTree()->getAnimationPropertyConfig("hyprfocusIn");
    const auto POUT = Config::animationTree()->getAnimationPropertyConfig("hyprfocusOut");

    if (has("flash")) {
        const auto ORIGINAL = window->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE)->goal();
        window->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE)->setConfig(PIN);
        *window->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE) = configValues.fadeOpacity->value();

        window->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE)->setCallbackOnEnd([w = PHLWINDOWREF{window}, POUT, ORIGINAL](WP<CBaseAnimatedVariable> pav) {
            if (!w)
                return;
            w->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE)->setConfig(POUT);
            *w->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE) = ORIGINAL;

            w->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE)->setCallbackOnEnd(nullptr);
        });
    }

    if (has("shrink") && g_pSendWindowSizeHook) {
        const auto ORIGINAL = CBox{window->positionAnimation()->goal(), window->sizeAnimation()->goal()};

        holdClientResize(window);

        window->positionAnimation()->setConfig(PIN);
        window->sizeAnimation()->setConfig(PIN);

        auto box = ORIGINAL.copy().scaleFromCenter(configValues.shrinkPercentage->value());

        *window->positionAnimation() = box.pos();
        *window->sizeAnimation()     = box.size();

        window->sizeAnimation()->setCallbackOnEnd([w = PHLWINDOWREF{window}, POUT, ORIGINAL](WP<CBaseAnimatedVariable> pav) {
            if (!w)
                return;
            w->sizeAnimation()->setConfig(POUT);
            w->positionAnimation()->setConfig(POUT);

            if (w->m_isFloating || Fullscreen::controller()->isFullscreen(w.lock())) {
                *w->positionAnimation() = ORIGINAL.pos();
                *w->sizeAnimation()     = ORIGINAL.size();
            } else
                w->layoutTarget()->recalc();

            // keep the render-only scaling through the grow-back as well
            if (w->sizeAnimation()->isBeingAnimated())
                w->sizeAnimation()->setCallbackOnEnd([w](WP<CBaseAnimatedVariable> pav) {
                    if (w)
                        releaseClientResize(w.lock());
                });
            else {
                w->sizeAnimation()->setCallbackOnEnd(nullptr);
                releaseClientResize(w.lock());
            }
        });
    } else if (has("slide")) {
        const auto ORIGINAL = window->positionAnimation()->goal();

        window->positionAnimation()->setConfig(PIN);

        *window->positionAnimation() = ORIGINAL - Vector2D{0.F, configValues.slideHeight->value()};

        window->positionAnimation()->setCallbackOnEnd([w = PHLWINDOWREF{window}, POUT, ORIGINAL](WP<CBaseAnimatedVariable> pav) {
            if (!w)
                return;
            w->positionAnimation()->setConfig(POUT);

            if (w->m_isFloating || Fullscreen::controller()->isFullscreen(w.lock()))
                *w->positionAnimation() = ORIGINAL;
            else
                w->layoutTarget()->recalc();

            w->positionAnimation()->setCallbackOnEnd(nullptr);
        });
    }
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO j2barFocusInit(HANDLE handle) {
    PHANDLE = handle;

    const std::string HASH        = __hyprland_api_get_hash();
    const std::string CLIENT_HASH = __hyprland_api_get_client_hash();

    if (HASH != CLIENT_HASH) {
        HyprlandAPI::addNotification(PHANDLE, "[hyprwinwrap] Failure in initialization: Version mismatch (headers ver is not equal to running hyprland ver)",
                                     CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[hww] Version mismatch");
    }

    for (auto& fn : HyprlandAPI::findFunctionsByName(PHANDLE, "sendWindowSize")) {
        if (!fn.demangled.contains("CWindow::sendWindowSize"))
            continue;

        g_pSendWindowSizeHook = HyprlandAPI::createFunctionHook(PHANDLE, fn.address, (void*)&hkSendWindowSize);
        break;
    }

    if (!g_pSendWindowSizeHook || !g_pSendWindowSizeHook->hook()) {
        g_pSendWindowSizeHook = nullptr;
        HyprlandAPI::addNotification(PHANDLE, "[j2bar-hyprland] Render-only focus dip hook unavailable", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[j2bar-hyprland] Render-only focus dip hook unavailable");
    }

    g_focusListener = Event::bus()->m_events.window.active.listen([&](PHLWINDOW w, Desktop::eFocusReason r) { onFocusChange(w, r); });

    configValues.enable = makeShared<Config::Values::CBoolValue>("plugin:hyprfocus:enable", "enable or disable the plugin", true);
    configValues.animateFloating = makeShared<Config::Values::CBoolValue>("plugin:hyprfocus:animate_floating", "animate floating windows", true);
    configValues.onlyOnMonitorChange = makeShared<Config::Values::CBoolValue>("plugin:hyprfocus:only_on_monitor_change", "whether to fire the animation only on monitor change", false);
    configValues.keyboardFocusAnimation = makeShared<Config::Values::CStringValue>("plugin:hyprfocus:keyboard_focus_animation", "animation for keyboard focus changes", "flash");
    configValues.mouseFocusAnimation = makeShared<Config::Values::CStringValue>("plugin:hyprfocus:mouse_focus_animation", "animation for mouse focus changes", "none");
    configValues.fadeOpacity = makeShared<Config::Values::CFloatValue>("plugin:hyprfocus:fade_opacity", "fade opacity", 0.8F, Config::Values::SFloatValueOptions{.min = 0.F, .max = 1.F});
    configValues.shrinkPercentage = makeShared<Config::Values::CFloatValue>("plugin:hyprfocus:shrink_percentage", "shrink percentage", 0.95F, Config::Values::SFloatValueOptions{.min = 0.F, .max = 1.F});
    configValues.slideHeight = makeShared<Config::Values::CFloatValue>("plugin:hyprfocus:slide_height", "slide height", 20.F, Config::Values::SFloatValueOptions{.min = 0.F, .max = 150.F});

    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.enable);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.animateFloating);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.onlyOnMonitorChange);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.keyboardFocusAnimation);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.mouseFocusAnimation);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.fadeOpacity);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.shrinkPercentage);
    configValues.windowClass = makeShared<Config::Values::CStringValue>("plugin:hyprfocus:class", "only animate windows whose class matches this regex (empty = all)", "");

    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.slideHeight);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.windowClass);

    // this will not be cleaned up after we are unloaded but it doesn't really matter,
    // as if we create this again it will just overwrite the old one.
    Config::animationTree()->m_animationTree.createNode("hyprfocusIn", "windowsIn");
    Config::animationTree()->m_animationTree.createNode("hyprfocusOut", "windowsOut");

    return {"hyprfocus", "Flashfocus for Hyprland", "Vaxry", "1.0"};
}

APICALL EXPORT void j2barFocusExit() {
    g_focusListener.reset();

    // reset the callbacks to avoid crashes; unmapped windows too, since one heading into a fadeout
    // still animates its geometry and would call into unloaded memory when its animation ends
    for (const auto& w : Desktop::windowState()->windows()) {
        if (!w)
            continue;

        w->sizeAnimation()->setCallbackOnEnd(nullptr);
        w->positionAnimation()->setCallbackOnEnd(nullptr);
        w->alpha(Desktop::View::WINDOW_ALPHA_ACTIVE)->setCallbackOnEnd(nullptr);
    }

    // hooks are removed after this returns, so let the held sends through now
    for (const auto& w : Desktop::windowState()->windows()) {
        if (isHeld(w.get()))
            releaseClientResize(w);
    }
}
