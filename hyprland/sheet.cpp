#define WLR_USE_UNSTABLE

#include <algorithm>
#include <any>
#include <cmath>
#include <optional>
#include <regex>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#include <hyprland/src/includes.hpp>

#define private   public
#define protected public
#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/animation/AnimationManager.hpp>
#include <hyprland/src/config/shared/animation/AnimationTree.hpp>
#include <hyprland/src/config/values/types/FloatValue.hpp>
#include <hyprland/src/config/values/types/StringValue.hpp>
#include <hyprland/src/config/values/types/Vec2Value.hpp>
#include <hyprland/src/desktop/rule/windowRule/WindowRuleApplicator.hpp>
#include <hyprland/src/desktop/state/FocusState.hpp>
#include <hyprland/src/desktop/state/WindowFadeout.hpp>
#include <hyprland/src/desktop/view/window/Window.hpp>
#include <hyprland/src/desktop/view/window/WaylandBackend.hpp>
#include <hyprland/src/desktop/view/window/WindowEffectsController.hpp>
#include <hyprland/src/desktop/view/window/WindowPresentation.hpp>
#include <hyprland/src/workspace/HLWorkspace.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/layout/LayoutManager.hpp>
#include <hyprland/src/layout/target/Target.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopManager.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/output/MonitorResources.hpp>
#include <hyprland/src/protocols/XDGShell.hpp>
#include <hyprland/src/render/OpenGL.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/render/pass/TexPassElement.hpp>
#include <hyprland/src/render/WindowRenderPresentation.hpp>
#include <hyprland/src/render/transformer/Transformer.hpp>
#include <hyprland/src/render/transformer/TransformerList.hpp>
#undef protected
#undef private

#include <hyprland/src/plugins/PluginAPI.hpp>

#include "sheet/NoFocusRestore.hpp"

using Desktop::View::IGeometric;

inline HANDLE PHANDLE = nullptr;

static struct {
    SP<Config::Values::CStringValue> windowClass;
    SP<Config::Values::CFloatValue>  parentShare;
    SP<Config::Values::CVec2Value>   minSize;
    SP<Config::Values::CFloatValue>  dip;
} configValues;

class CSheetTransformer;

// A window from its creation until it maps, in case it turns out to be a chooser. The parent is only a
// guess from focus, for clients that don't name one through xdg-foreign.
struct SPendingChooser {
    PHLWINDOWREF        chooser;
    PHLWINDOWREF        focusedParent;
    CHyprSignalListener commit;
    bool                committed = false;
    bool                isChooser = false;
};

enum class ESheetPhase {
    Opening,
    Open,
    Closing
};

// An open chooser and the window drawn scaled into it. `progress` runs from 0, the parent in its own
// box, to 1, the parent inside `into`, the chooser's box. While it opens and closes, both windows are
// drawn in the same box between the two, crossfading. In between, the parent is hidden and the
// chooser is still drawn through its transformer, at its own size scaled by `dip`, its focus
// animation: Hyprland blurs a transformed window differently, so handing the chooser back would
// change its glass the moment it came to rest.
struct SSheet {
    PHLWINDOWREF                chooser;
    PHLWINDOWREF                parent;
    CBox                        into;
    PHLANIMVAR<float>           progress;
    PHLANIMVAR<float>           dip;
    CSheetTransformer*          parentTransformer  = nullptr;
    CSheetTransformer*          chooserTransformer = nullptr;
    WP<Desktop::CWindowFadeout> chooserFadeout;
    ESheetPhase                 phase        = ESheetPhase::Opening;
    bool                        parentHidden = false;
    Sheet::CNoFocusRestore<Desktop::Types::PRIORITY_SET_PROP> parentNoFocus;
    bool                        settling     = false;
    float                       lastProgress = 0.F;
    WP<SSheet>                  self;
    // Idle-queue entries this sheet is waiting on, cancelled in cleanup so none of our functors
    // outlives the module. Entries whose work already ran cancel as no-ops.
    std::vector<uint64_t>       deferredSeqs;

    bool                        isClosing() const {
        return phase == ESheetPhase::Closing;
    }
    bool isOpen() const {
        return phase == ESheetPhase::Open;
    }

    // Animation and deferred callbacks observe the same lifetime. A removed
    // sheet cannot be mistaken for a new one allocated at the same address.
    template <class Callback>
    auto callback(Callback fn) {
        return [weak = self, fn](auto&&...) {
            if (const auto sheet = weak.lock())
                fn(*sheet);
        };
    }

    template <class Callback>
    void defer(Callback fn) {
        deferredSeqs.emplace_back(g_pEventLoopManager->doLater(callback(fn)));
    }

    static SP<SSheet> create(const PHLWINDOW& chooser, const PHLWINDOW& parent);
    void              open(const PHLWINDOW& chooser, const PHLWINDOW& parent);
    void              hideParent();
    void              showParent();
    void              damageChooser();
    void              damage();
    void              settleIfStill(float progress);
    void              onProgressUpdate();
    void              onProgressEnd();
    void              close(const PHLWINDOW& window);
    void              cleanup();
};

static std::vector<UP<SPendingChooser>> pendingChoosers;
static std::vector<SP<SSheet>>          sheets;
static std::vector<CHyprSignalListener> listeners;
static CFunctionHook*                   fadeoutCreateHook = nullptr;

static bool                             isChooserClass(const std::string& cls) {
    const auto PATTERN = configValues.windowClass->value();
    if (PATTERN.empty())
        return false;

    // an invalid regex must not throw inside the compositor
    try {
        return std::regex_match(cls, std::regex(PATTERN));
    } catch (const std::regex_error&) { return false; }
}

static SSheet* sheetFor(const PHLWINDOW& w, bool asParent) {
    const auto IT = std::ranges::find_if(sheets, [&](const auto& s) { return (asParent ? s->parent : s->chooser).get() == w.get(); });
    return IT == sheets.end() ? nullptr : IT->get();
}

static CBox lerpBox(const CBox& a, const CBox& b, float t) {
    return {a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.w + (b.w - a.w) * t, a.h + (b.h - a.h) * t};
}

// The window a sheet is heading for fades in over the first stretch of the way, and the one it's
// leaving fades out under it over the next, so what's behind them never shows through both at once
// and the fading is done well before the spring settles.
constexpr float FADE_SPAN = 0.35F;

static float    incomingAlpha(float travelled) {
    return std::clamp(travelled / FADE_SPAN, 0.F, 1.F);
}

static float outgoingAlpha(float travelled) {
    return std::clamp(2.F - travelled / FADE_SPAN, 0.F, 1.F);
}

static CBox currentBox(const PHLWINDOW& w) {
    return {w->position(IGeometric::GEOMETRIC_CURRENT), w->size(IGeometric::GEOMETRIC_CURRENT)};
}

// Where the parent is drawn once the sheet has opened: the chooser as it is now, in case it has
// moved or resized since it mapped, or where it was when it closed.
static CBox targetBox(const SSheet* sheet) {
    const auto CHOOSER = sheet->chooser.lock();
    return CHOOSER && !sheet->isClosing() ? currentBox(CHOOSER) : sheet->into;
}

static Vector2D renderOffset(const PHLWINDOW& w, const PHLMONITOR& monitor) {
    const auto PRESENTATION = w->presentation().renderPresentation();
    return PRESENTATION.workspaceOffset + PRESENTATION.floatingOffset - monitor->m_position;
}

static CBox boundsUnion(const CBox& a, const CBox& b) {
    const double X1 = std::min(a.x, b.x), Y1 = std::min(a.y, b.y);
    const double X2 = std::max(a.x + a.w, b.x + b.w), Y2 = std::max(a.y + a.h, b.y + b.h);
    return {X1, Y1, X2 - X1, Y2 - Y1};
}

// Draws a window's already rendered frame again, scaled from its own box into the sheet's current box
// and faded. Borders, shadows, subsurfaces and the blur matte are all in that frame, so they scale
// with it, and neither window is ever resized.
class CSheetTransformer : public Render::IWindowTransformer {
  public:
    CSheetTransformer(WP<SSheet> sheet, PHLWINDOWREF window, bool isParent) : m_sheet(std::move(sheet)), m_window(std::move(window)), m_isParent(isParent) {}

    // Hyprland drops inactive transformers from the window's list.
    void detach() {
        m_attached = false;
    }

    virtual bool active() const override {
        return m_attached;
    }

    // Its own box and where it is drawn, in the monitor's render pixels.
    struct SMapping {
        CBox  from;
        CBox  to;
        CBox  bounds;
        float alpha = 0.F;
    };

    std::optional<SMapping> mapping(const PHLMONITOR& monitor) const {
        const auto SHEET  = m_sheet.lock();
        const auto WINDOW = m_window.lock();
        const auto PARENT = SHEET ? SHEET->parent.lock() : nullptr;
        if (!SHEET || !WINDOW || !PARENT || !monitor)
            return std::nullopt;

        const auto OWN = currentBox(WINDOW);
        if (OWN.w <= 0 || OWN.h <= 0)
            return std::nullopt;

        const float PROGRESS  = std::clamp(SHEET->progress->value(), 0.F, 1.F);
        const float TRAVELLED = SHEET->isClosing() ? 1.F - PROGRESS : PROGRESS;
        const float ALPHA     = m_isParent == SHEET->isClosing() ? incomingAlpha(TRAVELLED) : outgoingAlpha(TRAVELLED);

        const auto  OFFSET = renderOffset(WINDOW, monitor);
        const auto  FROM   = OWN.copy().translate(OFFSET).scale(monitor->m_scale);
        auto        to     = SHEET->isOpen() ? FROM.copy().scaleFromCenter(SHEET->dip->value()) :
                                               lerpBox(currentBox(PARENT), targetBox(SHEET.get()), PROGRESS).translate(OFFSET).scale(monitor->m_scale);

        // Within two pixels of the window's own size it's drawn at that size on whole pixels, so it isn't
        // resampled as it comes to rest and looks the same once the sheet ends.
        if (std::abs(to.w - FROM.w) < 2.0 && std::abs(to.h - FROM.h) < 2.0)
            to = {FROM.x + std::round(to.x - FROM.x), FROM.y + std::round(to.y - FROM.y), FROM.w, FROM.h};

        return SMapping{FROM, to, WINDOW->getFullWindowBoundingBox().translate(OFFSET).scale(monitor->m_scale).round().expand(2), ALPHA};
    }

    static CBox mapBox(const SMapping& map, const CBox& box) {
        const auto SCALE = Vector2D{map.to.w / map.from.w, map.to.h / map.from.h};
        return {map.to.x + (box.x - map.from.x) * SCALE.x, map.to.y + (box.y - map.from.y) * SCALE.y, box.w * SCALE.x, box.h * SCALE.y};
    }

    // Hyprland only allocates and draws the transformed window within these extents.
    virtual CBox transformedExtents(const CBox& currentBox) const override {
        const auto WINDOW  = m_window.lock();
        const auto MONITOR = WINDOW ? WINDOW->m_monitor.lock() : nullptr;
        const auto MAP     = mapping(MONITOR);
        if (!MAP)
            return currentBox;

        const double SCALE = MONITOR->m_scale;
        const auto   DRAWN = mapBox(*MAP, MAP->bounds).expand(2).scale(1.0 / SCALE);
        return boundsUnion(currentBox, DRAWN).expand(4.0 / SCALE);
    }

    virtual CBox sourceBoxForOutput(const CBox&, const CBox& inputBox) const override {
        return inputBox;
    }

    virtual Render::SWindowTransformBuffer transform(Render::CRenderContext& ctx, const Render::SWindowTransformBuffer& in,
                                                     const Render::SWindowTransformContext& context) override {
        const auto MONITOR = context.monitor.lock();
        if (!MONITOR || !in.framebuffer || !in.framebuffer->getTexture())
            return in;

        const auto MAP = mapping(MONITOR);
        if (!MAP)
            return in;

        const auto OUTPUTCANVAS = Render::pixelBoxForLogical(context.outputBox.copy().expand(1.0 / MONITOR->m_scale), MONITOR->m_scale);
        if (OUTPUTCANVAS.empty())
            return in;

        const auto OUT = g_pHyprRenderer->getWorkBuffer(ctx, OUTPUTCANVAS.size());
        if (!OUT)
            return {.framebuffer = in.framebuffer, .box = in.box, .success = false};

        Render::GL::CFramebufferBindingGuard bindings{g_pHyprRenderer->glBackend()};
        auto                                 state      = ctx.saveDrawState();
        auto&                                renderData = ctx.m_data;
        g_pHyprRenderer->bindFB(ctx, OUT);
        const CRegion FULL     = CBox{{}, OUTPUTCANVAS.size()};
        renderData.damage      = FULL;
        renderData.fbSize      = OUTPUTCANVAS.size();
        renderData.transformDamage = false;
        g_pHyprRenderer->setProjectionType(ctx, Render::RPT_EXPORT);
        Render::GL::g_pHyprOpenGL->setViewport(0, 0, OUTPUTCANVAS.w, OUTPUTCANVAS.h);
        // The renderer's clear element doesn't reliably clear this buffer, and using it here cuts part
        // of a blurred bottom layer, such as a desktop widget, out of the frame.
        Render::GL::g_pHyprOpenGL->setCapStatus(GL_SCISSOR_TEST, false);
        glClearColor(0.F, 0.F, 0.F, 0.F);
        glClear(GL_COLOR_BUFFER_BIT);

        const Render::SWindowTransformBuffer RESULT = {.framebuffer = OUT, .box = OUTPUTCANVAS};
        if (MAP->alpha <= 0.F)
            return RESULT;

        // Hyprland renders the window into `in` only within its full bounding box, so nothing outside
        // that box's scaled copy needs drawing.
        const auto DRAWN = mapBox(*MAP, MAP->bounds).expand(2).round().translate(-OUTPUTCANVAS.pos());

        CTexPassElement::SRenderData data;
        data.tex = in.framebuffer->getTexture();
        data.box = mapBox(*MAP, in.box).translate(-OUTPUTCANVAS.pos());
        data.a   = MAP->alpha;
        g_pHyprRenderer->draw(ctx, data, CRegion{DRAWN}.intersect(FULL));

        return RESULT;
    }

  private:
    WP<SSheet>   m_sheet;
    PHLWINDOWREF m_window;
    bool         m_isParent;
    bool         m_attached = true;
};

static void attach(const WP<SSheet>& sheet, const PHLWINDOW& w, CSheetTransformer*& slot, bool isParent) {
    slot = w->effects().transformers()->emplace<CSheetTransformer>(sheet, PHLWINDOWREF{w}, isParent);
}

static void detach(const PHLWINDOWREF& w, CSheetTransformer*& slot) {
    if (const auto WINDOW = w.lock(); WINDOW && slot) {
        slot->detach();
        WINDOW->effects().transformers()->removeInactive();
    }
    slot = nullptr;
}

// Hyprland skips a window at alpha 0 entirely: it isn't drawn, and its surfaces get no frame callbacks,
// so a busy parent stops drawing too. Of the alphas always part of the drawn one, only the
// move-from-workspace alpha is left alone while the window stays mapped on one workspace; groups and
// monocle layouts set the layout alpha themselves. A hidden parent also can't take focus: the pointer
// passing over it would otherwise focus it, unseen, and restyle the chooser as unfocused.
void SSheet::hideParent() {
    const auto PARENT = parent.lock();
    if (!PARENT || parentHidden)
        return;

    detach(parent, parentTransformer);
    PARENT->alpha()[Desktop::View::WINDOW_ALPHA_MOVE_FROM_WORKSPACE]->setValueAndWarp(0.F);
    parentNoFocus.hide(PARENT->m_ruleApplicator->noFocus());
    parentHidden = true;
}

void SSheet::showParent() {
    if (!parentHidden)
        return;

    parentHidden = false;
    if (const auto PARENT = parent.lock()) {
        PARENT->alpha()[Desktop::View::WINDOW_ALPHA_MOVE_FROM_WORKSPACE]->setValueAndWarp(1.F);
        parentNoFocus.restore(PARENT->m_ruleApplicator->noFocus());
    }
}

void SSheet::damageChooser() {
    if (const auto CHOOSER = chooser.lock())
        g_pHyprRenderer->damageWindow(CHOOSER, true);
}

void SSheet::damage() {
    if (const auto PARENT = parent.lock())
        g_pHyprRenderer->damageWindow(PARENT, true);

    g_pHyprRenderer->damageBox(into.copy().expand(4));
}

// A spring spends its last stretch moving the scaled windows by fractions of a pixel, which only
// makes their text and the blur's grain shimmer. The sheet ends as soon as the box reaches where
// it's going, or is within half a pixel of it and moving less than that a frame, and the windows
// draw themselves from there.
void SSheet::settleIfStill(float currentProgress) {
    const auto  PARENT  = parent.lock();
    const auto  MONITOR = PARENT ? PARENT->m_monitor.lock() : nullptr;
    const float LAST    = std::exchange(lastProgress, currentProgress);
    if (!MONITOR || settling)
        return;

    const auto   FROM = currentBox(PARENT);
    const auto&  TO   = into;
    const double TRAVEL =
        MONITOR->m_scale * std::max({std::abs(TO.x - FROM.x), std::abs(TO.y - FROM.y), std::abs(TO.x + TO.w - FROM.x - FROM.w), std::abs(TO.y + TO.h - FROM.y - FROM.h)});
    const float GOAL    = isClosing() ? 0.F : 1.F;
    const bool  ARRIVED = isClosing() ? currentProgress <= GOAL : currentProgress >= GOAL;
    if (!ARRIVED && (std::abs(GOAL - currentProgress) * TRAVEL >= 0.5 || std::abs(currentProgress - LAST) * TRAVEL >= 0.5))
        return;

    // Warping a variable from inside its own update callback isn't safe, so this waits for the event loop.
    settling = true;
    defer([](SSheet& sheet) {
        sheet.settling = false;
        if (sheet.progress->isBeingAnimated())
            sheet.progress->warp();
    });
}

// The closing chooser's snapshot follows the sheet's progress rather than its own fade, so it stays
// opaque until the parent has faded back in under it.
void SSheet::onProgressUpdate() {
    const float PROGRESS = progress->value();
    if (const auto FADEOUT = chooserFadeout.lock())
        FADEOUT->m_alpha->setValueAndWarp(outgoingAlpha(1.F - PROGRESS));

    damage();
    settleIfStill(PROGRESS);

    // Once the parent has faded out it's hidden straight away, while the chooser is still moving, so
    // the chooser doesn't change under it after it has come to rest.
    if (!isClosing() && !parentHidden && outgoingAlpha(PROGRESS) <= 0.F)
        defer([](SSheet& sheet) {
            if (!sheet.isClosing())
                sheet.hideParent();
        });
}

void SSheet::cleanup() {
    progress->resetAllCallbacks();
    dip->resetAllCallbacks();
    for (const auto SEQ : deferredSeqs)
        g_pEventLoopManager->removeDoLater(SEQ);
    deferredSeqs.clear();
    // A snapshot left at rest above 0 would never finish.
    if (const auto FADEOUT = chooserFadeout.lock())
        *FADEOUT->m_alpha = 0.F;

    damage();
    detach(chooser, chooserTransformer);
    detach(parent, parentTransformer);
    showParent();
}

static void removeSheet(SSheet* sheet) {
    sheet->cleanup();
    std::erase_if(sheets, [sheet](const auto& s) { return s.get() == sheet; });
}

// Once the chooser has opened the parent is hidden; once it has closed the sheet is done. Animation
// callbacks can't free the variable that runs them, so this waits for the event loop.
void SSheet::onProgressEnd() {
    defer([](SSheet& sheet) {
        if (sheet.isClosing())
            removeSheet(&sheet);
        else {
            sheet.phase = ESheetPhase::Open;
            sheet.damage();
            sheet.hideParent();
        }
    });
}

SP<SSheet> SSheet::create(const PHLWINDOW& chooser, const PHLWINDOW& parent) {
    auto sheet  = makeShared<SSheet>();
    sheet->self = sheet;
    sheet->open(chooser, parent);
    return sheet;
}

void SSheet::open(const PHLWINDOW& window, const PHLWINDOW& parentWindow) {
    chooser = window;
    parent  = parentWindow;
    into    = {window->position(IGeometric::GEOMETRIC_GOAL), window->size(IGeometric::GEOMETRIC_GOAL)};

    Animation::mgr()->createAnimation(0.F, progress, Config::animationTree()->getAnimationPropertyConfig("hyprsheetIn"), AVARDAMAGE_NONE);
    progress->setUpdateCallback(callback([](SSheet& s) { s.onProgressUpdate(); }));
    Animation::mgr()->createAnimation(1.F, dip, Config::animationTree()->getAnimationPropertyConfig("hyprsheetIn"), AVARDAMAGE_NONE);
    dip->setUpdateCallback(callback([](SSheet& s) { s.damageChooser(); }));

    attach(self, parentWindow, parentTransformer, true);
    attach(self, window, chooserTransformer, false);

    window->positionAnimation()->warp();
    window->sizeAnimation()->warp();
    window->alpha()[Desktop::View::WINDOW_ALPHA_FADE]->warp();

    // An end callback set on a variable at rest runs straight away, so it goes on once this has started.
    *progress = 1.F;
    progress->setCallbackOnEnd(callback([](SSheet& s) { s.onProgressEnd(); }), false);
}

void SSheet::close(const PHLWINDOW& window) {
    if (parent.get() == window.get()) {
        // Hyprland snapshots the parent after this, still hidden or through its
        // transformer, so a hidden parent also closes hidden.
        phase = ESheetPhase::Closing;
        onProgressEnd();
        return;
    }
    if (isClosing())
        return;

    detach(chooser, chooserTransformer);
    if (parentHidden) {
        if (const auto PARENT = parent.lock())
            attach(self, PARENT, parentTransformer, true);
        showParent();
    }

    phase        = ESheetPhase::Closing;
    lastProgress = progress->value();
    into         = currentBox(window);
    progress->setConfig(Config::animationTree()->getAnimationPropertyConfig("hyprsheetOut"));
    *progress = 0.F;
}

static std::optional<Vector2D> chooserSizeFor(const PHLWINDOW& parent) {
    const auto SIZE = parent->size(IGeometric::GEOMETRIC_GOAL);
    if (SIZE.x <= 0 || SIZE.y <= 0)
        return std::nullopt;

    const auto SHARE = configValues.parentShare->value();
    const auto MIN   = configValues.minSize->value();
    return Vector2D{std::floor(std::max<double>(SIZE.x * SHARE, MIN.x)), std::floor(std::max<double>(SIZE.y * SHARE, MIN.y))};
}

static SP<CXDGSurfaceResource> xdgSurfaceFor(const PHLWINDOW& w) {
    const auto* BACKEND = dynamic_cast<const Desktop::View::CWaylandBackend*>(&w->backend());
    return BACKEND ? BACKEND->m_resource.lock() : nullptr;
}

// The size goes out in the chooser's first configure, so GTK draws its very first frame at it.
static void onInitialCommit(SPendingChooser* pending) {
    pending->committed = true;

    const auto CHOOSER = pending->chooser.lock();
    const auto XDG     = CHOOSER ? xdgSurfaceFor(CHOOSER) : nullptr;
    if (!XDG || !XDG->m_toplevel)
        return;

    const auto TOPLEVEL = XDG->m_toplevel.lock();
    if (!isChooserClass(TOPLEVEL->m_state.appid))
        return;

    pending->isChooser = true;

    auto parent = CHOOSER->backend().parent();
    if (!parent) {
        const auto FOCUSED = Desktop::focusState()->window();
        if (Desktop::View::validMapped(FOCUSED) && FOCUSED != CHOOSER && !FOCUSED->backend().traits().overrideRedirect && !isChooserClass(FOCUSED->metadata().appID()))
            parent = FOCUSED;
        pending->focusedParent = parent;
    }

    if (!parent)
        return;

    if (const auto SIZE = chooserSizeFor(parent))
        TOPLEVEL->setSize(*SIZE);
}

static SPendingChooser* pendingFor(const PHLWINDOW& w) {
    const auto IT = std::ranges::find_if(pendingChoosers, [&](const auto& p) { return p->chooser.get() == w.get(); });
    return IT == pendingChoosers.end() ? nullptr : IT->get();
}

static void onWindowCreate(PHLWINDOW w) {
    const auto XDG = w->backend().isX11() ? nullptr : xdgSurfaceFor(w);
    if (!XDG)
        return;

    auto pending     = makeUnique<SPendingChooser>();
    pending->chooser = w;
    auto* raw        = pending.get();
    pending->commit  = XDG->m_events.commit.listen([raw] {
        if (!raw->committed)
            onInitialCommit(raw);
    });
    pendingChoosers.emplace_back(std::move(pending));
}

static PHLWINDOW parentFor(const PHLWINDOW& chooser) {
    if (const auto PARENT = chooser->backend().parent())
        return PARENT;

    if (const auto PENDING = pendingFor(chooser))
        return PENDING->focusedParent.lock();

    return nullptr;
}

static bool sheetable(const PHLWINDOW& chooser, const PHLWINDOW& parent) {
    return Desktop::View::validMapped(parent) && parent != chooser && chooser->isFloating() && parent->m_workspace && parent->m_workspace == chooser->m_workspace &&
        !sheetFor(parent, true) && !sheetFor(parent, false);
}

// Hyprland centres a chooser only on a parent named through xdg-foreign, and not within the monitor:
// GTK won't shrink the chooser below its own minimum, which can be wider than a narrow parent.
static void onWindowOpen(PHLWINDOW w) {
    const auto PENDING = pendingFor(w);
    if (!PENDING || !PENDING->isChooser)
        return;

    const auto PARENT  = parentFor(w);
    const auto MONITOR = w->m_monitor.lock();
    if (!PARENT || !MONITOR || !sheetable(w, PARENT))
        return;

    const auto AREA    = MONITOR->logicalBoxMinusReserved();
    const auto SIZE    = w->size(IGeometric::GEOMETRIC_GOAL);
    const auto CENTER  = PARENT->position(IGeometric::GEOMETRIC_GOAL) + PARENT->size(IGeometric::GEOMETRIC_GOAL) / 2.F;
    const auto POS     = (CENTER - SIZE / 2.F).round();
    const auto CLAMPED = Vector2D{std::max(AREA.x, std::min(POS.x, AREA.x + AREA.w - SIZE.x)), std::max(AREA.y, std::min(POS.y, AREA.y + AREA.h - SIZE.y))};

    g_layoutManager->moveTarget(CLAMPED - w->position(IGeometric::GEOMETRIC_GOAL), w->layoutTarget());
}

// Runs after Hyprland has started the chooser's own open animation, which this replaces: both windows
// stay where they are, and only their drawing moves.
static void onWindowOpenLate(PHLWINDOW w) {
    const auto PENDING = pendingFor(w);
    if (!PENDING)
        return;

    const auto PARENT = PENDING->isChooser ? parentFor(w) : nullptr;
    std::erase_if(pendingChoosers, [&w](const auto& p) { return p->chooser.get() == w.get() || !p->chooser; });

    if (!PARENT || !sheetable(w, PARENT))
        return;

    sheets.emplace_back(SSheet::create(w, PARENT));
}

static void onWindowClose(PHLWINDOW w) {
    auto* sheet = sheetFor(w, true);
    if (!sheet)
        sheet = sheetFor(w, false);
    if (sheet)
        sheet->close(w);
}

typedef SP<Desktop::CWindowFadeout> (*origFadeoutCreate)(PHLWINDOW, SP<Render::IFramebuffer>, float, SP<Render::IFramebuffer>);

// The closing chooser's snapshot grows back out into the parent's box on the curve the parent grows
// back on.
static SP<Desktop::CWindowFadeout> hkFadeoutCreate(PHLWINDOW window, SP<Render::IFramebuffer> snapshot, float sourceAlpha, SP<Render::IFramebuffer> workspaceSnapshot) {
    auto  fadeout = ((origFadeoutCreate)fadeoutCreateHook->m_original)(window, snapshot, sourceAlpha, workspaceSnapshot);

    auto* SHEET = window ? sheetFor(window, false) : nullptr;
    if (!fadeout || !SHEET || !SHEET->isClosing())
        return fadeout;

    const auto PARENT = SHEET->parent.lock();
    if (!PARENT)
        return fadeout;

    const auto POUT = Config::animationTree()->getAnimationPropertyConfig("hyprsheetOut");
    const auto TO   = currentBox(PARENT);

    auto       retarget = [&POUT](auto& var, const auto& goal) {
        var->setConfig(POUT);
        var->setValueAndWarp(var->value());
        *var = goal;
    };

    retarget(fadeout->m_realPosition, TO.pos());
    retarget(fadeout->m_realSize, TO.size());
    fadeout->m_alpha->setValueAndWarp(outgoingAlpha(1.F - SHEET->progress->value()));
    SHEET->chooserFadeout = fadeout;

    return fadeout;
}

// hyprfocus dips a focused window by resizing it and back, which has a chooser lay itself out again
// at each size, so its buttons visibly change size. The chooser is kept out of hyprfocus, and once
// its sheet has opened it dips here instead, with hyprfocus's animations, by scaling its finished
// frame like the sheet does. Focus the pointer brings by passing over the chooser doesn't dip: it's
// the one window on its monitor while it's open, so a dip on every hover would only distract.
static void onWindowActive(PHLWINDOW w, Desktop::eFocusReason reason) {
    static PHLWINDOWREF lastActive;
    if (!w)
        return;

    const auto LAST = std::exchange(lastActive, PHLWINDOWREF{w});
    if (LAST.lock() == w || reason == Desktop::FOCUS_REASON_NEW_WINDOW || reason == Desktop::FOCUS_REASON_FFM)
        return;

    auto* sheet = sheetFor(w, false);
    if (!sheet || !sheet->isOpen() || sheet->dip->isBeingAnimated())
        return;

    const auto& TREE = Config::animationTree();
    if (!TREE->nodeExists("hyprfocusIn") || !TREE->nodeExists("hyprfocusOut"))
        return;

    sheet->dip->setConfig(TREE->getAnimationPropertyConfig("hyprfocusIn"));
    *sheet->dip = configValues.dip->value();
    sheet->dip->setCallbackOnEnd(sheet->callback([](SSheet& s) {
        s.dip->setConfig(Config::animationTree()->getAnimationPropertyConfig("hyprfocusOut"));
        *s.dip = 1.F;
    }));
}

// A chooser that is gone without closing would leave its parent hidden.
static void onWindowDestroy() {
    std::erase_if(pendingChoosers, [](const auto& p) { return !p->chooser; });

    std::vector<SSheet*> orphaned;
    for (const auto& s : sheets) {
        if (!s->parent || (!s->chooser && !s->isClosing()))
            orphaned.emplace_back(s.get());
    }

    for (auto* sheet : orphaned)
        removeSheet(sheet);
}

// Either window leaving the workspace the two share ends the sheet at once.
static void onWindowMoveToWorkspace(PHLWINDOW w) {
    auto* sheet = sheetFor(w, true);
    if (!sheet)
        sheet = sheetFor(w, false);

    if (sheet)
        removeSheet(sheet);
}

APICALL EXPORT std::string j2barSheetApiVersion() {
    return HYPRLAND_API_VERSION;
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO j2barSheetInit(HANDLE handle) {
    PHANDLE = handle;

    if (std::string{__hyprland_api_get_hash()} != __hyprland_api_get_client_hash()) {
        HyprlandAPI::addNotification(PHANDLE, "[hyprsheet] Version mismatch (headers ver is not equal to running hyprland ver)", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[hyprsheet] Version mismatch");
    }

    // Hyprland pairs matches with demangled names by a line count that drifts past any symbol dlsym
    // can't resolve, so this matches the mangled name.
    for (auto& fn : HyprlandAPI::findFunctionsByName(PHANDLE, "_ZN7Desktop14CWindowFadeout6createE")) {
        fadeoutCreateHook = HyprlandAPI::createFunctionHook(PHANDLE, fn.address, (void*)&hkFadeoutCreate);
        break;
    }

    if (!fadeoutCreateHook || !fadeoutCreateHook->hook()) {
        fadeoutCreateHook = nullptr;
        HyprlandAPI::addNotification(PHANDLE, "[hyprsheet] Failed to hook CWindowFadeout::create", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[hyprsheet] hook failed");
    }

    configValues.windowClass = makeShared<Config::Values::CStringValue>("plugin:hyprsheet:class", "regex for the class of chooser windows", "^(xdg-desktop-portal-gtk)$");
    configValues.parentShare = makeShared<Config::Values::CFloatValue>("plugin:hyprsheet:parent_share", "share of the parent's size a chooser takes", 0.75F,
                                                                       Config::Values::SFloatValueOptions{.min = 0.1F, .max = 1.F});
    configValues.minSize     = makeShared<Config::Values::CVec2Value>("plugin:hyprsheet:min_size", "smallest size a chooser takes", Config::VEC2{700, 450});
    configValues.dip         = makeShared<Config::Values::CFloatValue>("plugin:hyprsheet:dip", "scale an open chooser dips to when focused, with hyprfocus's animations", 0.99F,
                                                                       Config::Values::SFloatValueOptions{.min = 0.F, .max = 1.F});

    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.windowClass);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.parentShare);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.minSize);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.dip);

    // Like hyprfocus's, these nodes outlive an unload, and a reload just recreates them.
    Config::animationTree()->m_animationTree.createNode("hyprsheetIn", "windowsIn");
    Config::animationTree()->m_animationTree.createNode("hyprsheetOut", "windowsOut");

    listeners.emplace_back(Event::bus()->m_events.window.create.listen([](PHLWINDOW w) { onWindowCreate(w); }));
    listeners.emplace_back(Event::bus()->m_events.window.open.listen([](PHLWINDOW w) { onWindowOpen(w); }));
    listeners.emplace_back(Event::bus()->m_events.window.openLate.listen([](PHLWINDOW w) { onWindowOpenLate(w); }));
    listeners.emplace_back(Event::bus()->m_events.window.close.listen([](PHLWINDOW w) { onWindowClose(w); }));
    listeners.emplace_back(Event::bus()->m_events.window.destroy.listen([](PHLWINDOWREF) { onWindowDestroy(); }));
    listeners.emplace_back(Event::bus()->m_events.window.moveToWorkspace.listen([](PHLWINDOW w, PHLWORKSPACE) { onWindowMoveToWorkspace(w); }));
    listeners.emplace_back(Event::bus()->m_events.window.active.listen([](PHLWINDOW w, Desktop::eFocusReason r) { onWindowActive(w, r); }));

    return {"hyprsheet", "Draws a file chooser's parent scaled into it", "jesse", "1.0"};
}

APICALL EXPORT void j2barSheetExit() {
    listeners.clear();
    pendingChoosers.clear();

    while (!sheets.empty())
        removeSheet(sheets.back().get());
}
