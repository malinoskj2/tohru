#define WLR_USE_UNSTABLE

#include <algorithm>
#include <any>
#include <array>
#include <cmath>
#include <numbers>
#include <optional>
#include <ranges>
#include <regex>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#include <GLES3/gl32.h>

#include <hyprland/src/includes.hpp>

#define private   public
#define protected public
#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/animation/AnimationManager.hpp>
#include <hyprland/src/config/shared/animation/AnimationTree.hpp>
#include <hyprland/src/config/values/types/FloatValue.hpp>
#include <hyprland/src/config/values/types/StringValue.hpp>
#include <hyprland/src/desktop/Workspace.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#include <hyprland/src/desktop/view/LayerSurface.hpp>
#include <hyprland/src/desktop/view/Window.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/helpers/math/Math.hpp>
#include <hyprland/src/managers/eventLoop/EventLoopManager.hpp>
#include <hyprland/src/managers/fullscreen/FullscreenController.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/output/MonitorResources.hpp>
#include <hyprland/src/render/OpenGL.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/render/Shader.hpp>
#include <hyprland/src/render/pass/RectPassElement.hpp>
#include <hyprland/src/render/transformer/Transformer.hpp>
#undef protected
#undef private

#include <hyprland/src/plugins/PluginAPI.hpp>

#include "LogicalFramebuffer.hpp"

using Desktop::View::IGeometric;

inline HANDLE PHANDLE = nullptr;

static struct {
    SP<Config::Values::CStringValue> layerNamespace;
    SP<Config::Values::CFloatValue>  angle;
    SP<Config::Values::CFloatValue>  scale;
    SP<Config::Values::CFloatValue>  perspective;
    SP<Config::Values::CFloatValue>  shade;
    SP<Config::Values::CFloatValue>  blur;
} configValues;

// Each output pixel is traced back through the tilt to the pixel of the window's frame it shows, so
// the window is drawn with a true perspective rather than as a flat quad. `toRender` maps the
// buffers' texture coordinates to the monitor's logical orientation, in which the tilt is defined,
// and `fromRender` back. The shade darkens the window's own box from the top, as the reference's
// gradient does, and leaves its shadow alone.
constexpr const char* RECEDE_FRAG = R"GLSL(#version 300 es
precision highp float;

uniform sampler2D tex;
uniform mat3      toRender;
uniform mat3      fromRender;
uniform vec2      renderSize;
uniform vec2      origin;
uniform float     scale;
uniform float     sinAngle;
uniform float     cosAngle;
uniform float     invPerspective;
uniform vec4      windowBox;
uniform float     shade;
uniform vec3      shadeColor;

in vec2 v_texcoord;
layout(location = 0) out vec4 fragColor;

void main() {
    vec2  q     = (toRender * vec3(v_texcoord, 1.0)).xy * renderSize - origin;
    float depth = scale * (cosAngle + q.y * sinAngle * invPerspective);
    if (depth <= 0.0) {
        fragColor = vec4(0.0);
        return;
    }

    float y  = q.y / depth;
    float w  = 1.0 - scale * y * sinAngle * invPerspective;
    vec2  p  = origin + vec2(q.x * w / scale, y);
    vec2  uv = (fromRender * vec3(p / renderSize, 1.0)).xy;
    if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
        fragColor = vec4(0.0);
        return;
    }

    vec4 color = texture(tex, uv);
    vec2 local = (p - windowBox.xy) / windowBox.zw;
    if (shade > 0.0 && all(greaterThanEqual(local, vec2(0.0))) && all(lessThanEqual(local, vec2(1.0)))) {
        float ramp = local.y < 0.55 ? mix(1.0, 0.22 / 0.62, local.y / 0.55) : mix(0.22 / 0.62, 0.0, (local.y - 0.55) / 0.45);
        color.rgb  = mix(color.rgb, shadeColor * color.a, shade * ramp);
    }

    fragColor = color;
}
)GLSL";

static struct {
    SP<CShader> shader;
    bool        failed = false;
    GLint       toRender = -1, fromRender = -1, renderSize = -1, origin = -1, scale = -1, sinAngle = -1, cosAngle = -1, invPerspective = -1, windowBox = -1, shade = -1,
          shadeColor = -1;
} recedeShader;

static bool ensureShader() {
    if (recedeShader.shader)
        return true;
    if (recedeShader.failed)
        return false;

    auto shader = makeShared<CShader>();
    if (!shader->createProgram(Render::GL::g_pHyprOpenGL->m_shaders->TEXVERTSRC, RECEDE_FRAG, true)) {
        recedeShader.failed = true;
        HyprlandAPI::addNotification(PHANDLE, "[hyprrecede] Failed to compile the tilt shader", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        return false;
    }

    const auto PROGRAM            = shader->program();
    recedeShader.toRender       = glGetUniformLocation(PROGRAM, "toRender");
    recedeShader.fromRender     = glGetUniformLocation(PROGRAM, "fromRender");
    recedeShader.renderSize     = glGetUniformLocation(PROGRAM, "renderSize");
    recedeShader.origin         = glGetUniformLocation(PROGRAM, "origin");
    recedeShader.scale          = glGetUniformLocation(PROGRAM, "scale");
    recedeShader.sinAngle       = glGetUniformLocation(PROGRAM, "sinAngle");
    recedeShader.cosAngle       = glGetUniformLocation(PROGRAM, "cosAngle");
    recedeShader.invPerspective = glGetUniformLocation(PROGRAM, "invPerspective");
    recedeShader.windowBox      = glGetUniformLocation(PROGRAM, "windowBox");
    recedeShader.shade          = glGetUniformLocation(PROGRAM, "shade");
    recedeShader.shadeColor     = glGetUniformLocation(PROGRAM, "shadeColor");
    recedeShader.shader         = shader;
    return true;
}

// The reference's shade is Catppuccin Crust, #11111b.
constexpr std::array<float, 3> SHADE_COLOR = {17.F / 255.F, 17.F / 255.F, 27.F / 255.F};

// A monitor whose windows are pushed back while a matching layer is mapped on it. The three parts
// move on their own curves, as the reference's transform, shade and backdrop transitions do.
struct SRecede {
    PHLMONITORREF         monitor;
    std::vector<PHLLSREF> layers;
    PHLANIMVAR<float>     tilt;
    PHLANIMVAR<float>     shade;
    PHLANIMVAR<float>     blur;
    WP<SRecede>           self;
    // The deferred removal's place in the compositor's idle queue, cancelled before this dies so no
    // functor of ours outlives the module.
    uint64_t              removalSeq = 0;

    template <class Callback>
    auto callback(Callback fn) {
        return [weak = self, fn](auto&&...) {
            if (const auto recede = weak.lock())
                fn(*recede);
        };
    }

    bool open() const {
        return !layers.empty();
    }

    bool animating() const {
        return tilt->isBeingAnimated() || shade->isBeingAnimated() || blur->isBeingAnimated();
    }

    void damage() {
        if (const auto MONITOR = monitor.lock())
            g_pHyprRenderer->damageMonitor(MONITOR);
    }

    void retarget(float goal) {
        for (auto* var : {&tilt, &shade, &blur}) {
            **var = goal;
        }
    }
};

static std::vector<SP<SRecede>>         recedes;
static std::vector<CHyprSignalListener> listeners;

static SRecede*                         recedeFor(const PHLMONITOR& monitor) {
    if (!monitor)
        return nullptr;
    const auto IT = std::ranges::find_if(recedes, [&](const auto& r) { return r->monitor.get() == monitor.get(); });
    return IT == recedes.end() ? nullptr : IT->get();
}

static bool matchesNamespace(const std::string& ns) {
    static std::string pattern;
    static std::regex  regex;
    static bool        valid = false;

    const auto         CONFIGURED = configValues.layerNamespace->value();
    if (CONFIGURED.empty())
        return false;

    // an invalid regex must not throw inside the compositor
    if (CONFIGURED != pattern) {
        pattern = CONFIGURED;
        try {
            regex = std::regex(pattern);
            valid = true;
        } catch (const std::regex_error&) { valid = false; }
    }

    return valid && std::regex_match(ns, regex);
}

// The CSS transform the reference animates, `perspective(P) rotateX(A) scale(S)` about the bottom
// centre, at `t` of the way from none. Like a browser, it interpolates the perspective's inverse,
// so the tilt has depth from the first frame. Everything is in the monitor's render pixels.
struct SProjection {
    Vector2D origin;
    double   scale          = 1.0;
    double   sinAngle       = 0.0;
    double   cosAngle       = 1.0;
    double   invPerspective = 0.0;

    Vector2D apply(const Vector2D& p) const {
        const auto   Q = p - origin;
        const double W = 1.0 - scale * Q.y * sinAngle * invPerspective;
        return origin + Vector2D{scale * Q.x, scale * Q.y * cosAngle} / W;
    }

    CBox apply(const CBox& box) const {
        const std::array CORNERS = {apply({box.x, box.y}), apply({box.x + box.w, box.y}), apply({box.x, box.y + box.h}), apply({box.x + box.w, box.y + box.h})};
        const auto [MINX, MAXX]  = std::ranges::minmax(CORNERS | std::views::transform([](const auto& c) { return c.x; }));
        const auto [MINY, MAXY]  = std::ranges::minmax(CORNERS | std::views::transform([](const auto& c) { return c.y; }));
        return {MINX, MINY, MAXX - MINX, MAXY - MINY};
    }
};

// The windows lean back about the bottom centre of the area they tile, the work area, or the whole
// monitor under a fullscreen window, so the tiles of a workspace tilt as one plane.
static std::optional<SProjection> projectionFor(const SRecede& recede) {
    const auto MONITOR = recede.monitor.lock();
    if (!MONITOR)
        return std::nullopt;

    const float T = recede.tilt->value();
    if (std::abs(T) < 1e-4F)
        return std::nullopt;

    const bool FULLSCREEN = MONITOR->m_activeWorkspace && Fullscreen::controller()->hasFullscreen(MONITOR->m_activeWorkspace);
    const auto AREA       = FULLSCREEN ? CBox{MONITOR->m_position, MONITOR->m_size} : MONITOR->logicalBoxMinusReserved();
    const auto ANGLE      = T * configValues.angle->value() * std::numbers::pi / 180.0;

    SProjection projection;
    projection.origin         = (Vector2D{AREA.x + AREA.w / 2.0, AREA.y + AREA.h} - MONITOR->m_position) * MONITOR->m_scale;
    projection.scale          = std::max(1e-3, 1.0 + T * (configValues.scale->value() - 1.0));
    projection.sinAngle       = std::sin(ANGLE);
    projection.cosAngle       = std::cos(ANGLE);
    projection.invPerspective = T / std::max(1.0, sc<double>(configValues.perspective->value()) * MONITOR->m_scale);
    return projection;
}

static Vector2D renderOffset(const PHLWINDOW& w, const PHLMONITOR& monitor) {
    const auto WORKSPACE = w->m_workspace;
    return (w->m_pinned || !WORKSPACE ? Vector2D{} : WORKSPACE->m_renderOffset->value()) + w->m_floatingOffset - monitor->m_position;
}

using SMat3 = std::array<double, 9>;

// OpenGL ES requires a column-major matrix with transpose=false.
static std::array<GLfloat, 9> toGL(const SMat3& m) {
    std::array<GLfloat, 9> out;
    for (size_t row = 0; row < 3; ++row)
        for (size_t column = 0; column < 3; ++column)
            out[column * 3 + row] = sc<GLfloat>(m[row * 3 + column]);
    return out;
}

// Draws a window's rendered frame again, tilted and shaded. Hyprland calls it for the frame and,
// for a window it blurs, again for the matte that says where the blur goes, which must be tilted
// alike but not shaded.
class CRecedeTransformer : public Render::IWindowTransformer {
  public:
    CRecedeTransformer(PHLWINDOWREF window) : m_window(std::move(window)) {}

    virtual void preWindowRender(CSurfacePassElement::SRenderData* pRenderData) override {
        m_nextIsFrame = true;
    }

    virtual SP<Render::IFramebuffer> transform(SP<Render::IFramebuffer> in) override {
        const bool IS_FRAME = std::exchange(m_nextIsFrame, false);
        const auto WINDOW   = m_window.lock();
        const auto MONITOR  = g_pHyprRenderer->m_renderData.pMonitor.lock();
        if (!WINDOW || !MONITOR || !in || !in->getTexture())
            return in;

        const auto* RECEDE = recedeFor(MONITOR);
        // The backdrop blurs the composed desktop once. Reapplying each translucent window's
        // own live blur after tilting gives neighbouring terminals separate blur boundaries.
        // Hyprland samples the matte's red channel, so black suppresses that extra blur.
        if (!IS_FRAME && RECEDE && RECEDE->blur->value() > 1e-4F && configValues.blur->value() > 0.F) {
            const auto GUARD = g_pHyprRenderer->bindTempFB(in);
            Render::GL::g_pHyprOpenGL->setCapStatus(GL_SCISSOR_TEST, false);
            glClearColor(0.F, 0.F, 0.F, 1.F);
            glClear(GL_COLOR_BUFFER_BIT);
            return in;
        }

        const auto  PROJ   = RECEDE ? projectionFor(*RECEDE) : std::nullopt;
        const float SHADE  = RECEDE && IS_FRAME ? std::clamp(RECEDE->shade->value(), 0.F, 1.F) * configValues.shade->value() : 0.F;
        if (!PROJ && SHADE <= 0.F)
            return in;

        if (!ensureShader())
            return in;

        const auto TEX = in->getTexture();
        auto& output = IS_FRAME ? m_frame : m_matte;
        if (!output)
            output = makeShared<CLogicalFramebuffer>("j2bar-recede-logical");
        const auto FORMAT = MONITOR->useFP16() ? in->m_drmFormat : DRM_FORMAT_ARGB8888;
        if (!output->alloc(MONITOR->m_transformedSize.x, MONITOR->m_transformedSize.y, FORMAT))
            return in;
        output->setImageDescription(TEX->m_imageDescription);
        const auto OUT = output;
        auto sourceMapping = Mat3x3::identity();
        if (!dynamic_cast<CLogicalFramebuffer*>(in.get()))
            sourceMapping.translate({0.5, 0.5}).transform(Math::wlTransformToHyprutils(MONITOR->m_transform)).translate({-0.5, -0.5});

        const auto  PROJECTION = PROJ.value_or(SProjection{});
        const auto  OFFSET     = renderOffset(WINDOW, MONITOR);
        const auto  BOX        = CBox{WINDOW->position(IGeometric::GEOMETRIC_CURRENT), WINDOW->size(IGeometric::GEOMETRIC_CURRENT)}.translate(OFFSET).scale(MONITOR->m_scale);
        const auto  BOUNDS     = WINDOW->getFullWindowBoundingBox().translate(OFFSET).scale(MONITOR->m_scale).expand(2);
        const CBox  FULL       = {{}, MONITOR->m_transformedSize};
        const auto  DRAWN      = PROJECTION.apply(BOUNDS).expand(2).round().intersection(FULL);

        const auto  GUARD = g_pHyprRenderer->bindTempFB(OUT);
        auto&       gl    = Render::GL::g_pHyprOpenGL;
        const auto PROJECTION_TYPE = g_pHyprRenderer->m_renderData.projectionType;
        const auto FB_SIZE = g_pHyprRenderer->m_renderData.fbSize;
        const auto TRANSFORM_DAMAGE = g_pHyprRenderer->m_renderData.transformDamage;
        g_pHyprRenderer->m_renderData.fbSize = OUT->m_size;
        g_pHyprRenderer->setProjectionType(Render::RPT_EXPORT);
        g_pHyprRenderer->m_renderData.transformDamage = false;
        const auto PROJECTION_GUARD = Render::CScopeGuard([PROJECTION_TYPE, FB_SIZE, TRANSFORM_DAMAGE] {
            g_pHyprRenderer->m_renderData.fbSize = FB_SIZE;
            g_pHyprRenderer->setProjectionType(PROJECTION_TYPE);
            g_pHyprRenderer->m_renderData.transformDamage = TRANSFORM_DAMAGE;
        });
        // The renderer's clear element doesn't reliably clear this buffer.
        gl->setCapStatus(GL_SCISSOR_TEST, false);
        glClearColor(0.F, 0.F, 0.F, 0.F);
        glClear(GL_COLOR_BUFFER_BIT);
        if (DRAWN.empty())
            return OUT;

        glActiveTexture(GL_TEXTURE0);
        TEX->bind();
        TEX->setTexParameter(GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        TEX->setTexParameter(GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
        TEX->setTexParameter(GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        TEX->setTexParameter(GL_TEXTURE_MAG_FILTER, GL_LINEAR);

        auto shader = gl->useShader(recedeShader.shader).lock();
        if (!shader)
            return in;

        // Only the quad's texture coordinates matter: it covers the whole buffer.
        static constexpr std::array<GLfloat, 9> FULLSCREEN = {2.F, 0.F, 0.F, 0.F, 2.F, 0.F, -1.F, -1.F, 1.F};
        shader->setUniformMatrix3fv(SHADER_PROJ, 1, GL_FALSE, FULLSCREEN);
        shader->setUniformInt(SHADER_TEX, 0);
        static constexpr SMat3 IDENTITY = {1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0};
        const auto SOURCE_MAPPING = sourceMapping.getMatrix();
        SMat3 source;
        std::ranges::copy(SOURCE_MAPPING, source.begin());
        glUniformMatrix3fv(recedeShader.toRender, 1, GL_FALSE, toGL(IDENTITY).data());
        glUniformMatrix3fv(recedeShader.fromRender, 1, GL_FALSE, toGL(source).data());
        glUniform2f(recedeShader.renderSize, FULL.w, FULL.h);
        glUniform2f(recedeShader.origin, PROJECTION.origin.x, PROJECTION.origin.y);
        glUniform1f(recedeShader.scale, PROJECTION.scale);
        glUniform1f(recedeShader.sinAngle, PROJECTION.sinAngle);
        glUniform1f(recedeShader.cosAngle, PROJECTION.cosAngle);
        glUniform1f(recedeShader.invPerspective, PROJECTION.invPerspective);
        glUniform4f(recedeShader.windowBox, BOX.x, BOX.y, std::max(1.0, BOX.w), std::max(1.0, BOX.h));
        glUniform1f(recedeShader.shade, SHADE);
        glUniform3f(recedeShader.shadeColor, SHADE_COLOR[0], SHADE_COLOR[1], SHADE_COLOR[2]);

        gl->setViewport(0, 0, OUT->m_size.x, OUT->m_size.y);
        glBindVertexArray(shader->getUniformLocation(SHADER_SHADER_VAO));
        gl->scissor(DRAWN, false);
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
        gl->scissor(nullptr);
        glBindVertexArray(0);

        return OUT;
    }

  private:
    PHLWINDOWREF m_window;
    bool         m_nextIsFrame = true;
    SP<Render::IFramebuffer> m_frame;
    SP<Render::IFramebuffer> m_matte;
};

struct SAttached {
    PHLWINDOWREF        window;
    CRecedeTransformer* transformer = nullptr;
};

static std::vector<SAttached> attached;

static void                   attach(const PHLWINDOW& w) {
    if (!Desktop::View::validMapped(w) || !recedeFor(w->m_monitor.lock()) ||
        std::ranges::any_of(attached, [&](const auto& a) { return a.window.get() == w.get(); }))
        return;

    auto transformer = makeUnique<CRecedeTransformer>(PHLWINDOWREF{w});
    attached.emplace_back(SAttached{.window = w, .transformer = transformer.get()});
    w->m_transformers.emplace_back(std::move(transformer));
}

// Even a no-op transformer changes Hyprland's rendering path, including output orientation.
// Only windows belonging to a receding monitor may use it. Reconcile before building a render
// pass so windows moved between outputs and newly opened windows follow their current monitor.
static void syncAttachments() {
    std::erase_if(attached, [](const auto& a) {
        const auto WINDOW = a.window.lock();
        if (!WINDOW)
            return true;
        const auto MONITOR = WINDOW->m_monitor.lock();
        if (Desktop::View::validMapped(WINDOW) && recedeFor(MONITOR))
            return false;
        std::erase_if(WINDOW->m_transformers, [&a](const auto& t) { return t.get() == a.transformer; });
        if (MONITOR)
            g_pHyprRenderer->damageMonitor(MONITOR);
        return true;
    });
    for (const auto& w : Desktop::windowState()->windows()) {
        attach(w);
    }
}

// Hyprland blurs a transformed window differently, so no window keeps a transformer once nothing
// recedes.
static void detachAll() {
    for (const auto& a : attached) {
        if (const auto WINDOW = a.window.lock())
            std::erase_if(WINDOW->m_transformers, [&a](const auto& t) { return t.get() == a.transformer; });
    }
    attached.clear();
}

static void removeRecede(SRecede* recede) {
    for (auto* var : {&recede->tilt, &recede->shade, &recede->blur}) {
        (*var)->resetAllCallbacks();
    }
    if (recede->removalSeq != 0)
        g_pEventLoopManager->removeDoLater(std::exchange(recede->removalSeq, 0));
    recede->damage();
    std::erase_if(recedes, [recede](const auto& r) { return r.get() == recede; });

    syncAttachments();
}

// Animation callbacks can't free the variables that run them, so this waits for the event loop.
// The three parts end together, so one deferred removal is all a recede ever needs.
static void onPartEnd(SRecede& recede) {
    if (recede.removalSeq != 0)
        return;

    recede.removalSeq = g_pEventLoopManager->doLater(recede.callback([](SRecede& r) {
        r.removalSeq = 0;
        if (!r.open() && !r.animating())
            removeRecede(&r);
    }));
}

static SRecede* createRecede(const PHLMONITOR& monitor) {
    auto recede     = makeShared<SRecede>();
    recede->self    = recede;
    recede->monitor = monitor;

    const auto& TREE = Config::animationTree();
    Animation::mgr()->createAnimation(0.F, recede->tilt, TREE->getAnimationPropertyConfig("hyprrecedeTilt"), AVARDAMAGE_NONE);
    Animation::mgr()->createAnimation(0.F, recede->shade, TREE->getAnimationPropertyConfig("hyprrecedeShade"), AVARDAMAGE_NONE);
    Animation::mgr()->createAnimation(0.F, recede->blur, TREE->getAnimationPropertyConfig("hyprrecedeBlur"), AVARDAMAGE_NONE);

    for (auto* var : {&recede->tilt, &recede->shade, &recede->blur}) {
        (*var)->setUpdateCallback(recede->callback([](SRecede& r) { r.damage(); }));
    }

    recedes.emplace_back(recede);
    return recede.get();
}

static void onLayerOpened(PHLLS ls) {
    if (!ls || !matchesNamespace(ls->m_namespace))
        return;

    const auto MONITOR = ls->m_monitor.lock();
    if (!MONITOR)
        return;

    auto* recede = recedeFor(MONITOR);
    if (!recede)
        recede = createRecede(MONITOR);

    recede->layers.emplace_back(ls);
    syncAttachments();
    recede->retarget(1.F);

    // An end callback set on a variable at rest runs straight away, so they go on once moving.
    for (auto* var : {&recede->tilt, &recede->shade, &recede->blur}) {
        (*var)->setCallbackOnEnd(recede->callback([](SRecede& r) { onPartEnd(r); }), false);
    }
}

static void onLayerClosed(PHLLS ls) {
    for (const auto& recede : recedes) {
        if (std::erase_if(recede->layers, [&](const auto& l) { return !l || l.get() == ls.get(); }) && !recede->open())
            recede->retarget(0.F);
    }
}

// The blur covers everything below the layers, wallpaper included, the way the reference's
// backdrop does, and fades in as a whole rather than growing in radius.
static void onRenderStage(eRenderStage stage) {
    if (stage != RENDER_POST_WINDOWS)
        return;

    const auto MONITOR = g_pHyprRenderer->m_renderData.pMonitor.lock();
    const auto* RECEDE = MONITOR ? recedeFor(MONITOR) : nullptr;
    if (!RECEDE)
        return;

    const float ALPHA = std::clamp(RECEDE->blur->value(), 0.F, 1.F) * configValues.blur->value();
    if (ALPHA <= 0.F)
        return;

    CRectPassElement::SRectData data;
    data.box   = {{}, MONITOR->m_transformedSize};
    data.color = CHyprColor(0, 0, 0, 0);
    data.blur  = true;
    data.blurA = ALPHA;
    g_pHyprRenderer->m_renderPass.add(makeUnique<CRectPassElement>(data));
}

// A window damages the box it occupies, but receded, it's drawn somewhere else, so whatever damage
// falls on a window also covers where that part of it is drawn. Damage outside the box doesn't
// reach the window's drawing, and while the windows move the whole monitor is damaged anyway.
static void onRenderPre(PHLMONITOR monitor) {
    if (!recedes.empty())
        syncAttachments();
    auto* recede = recedeFor(monitor);
    if (!recede || recede->animating())
        return;

    const auto PROJ = projectionFor(*recede);
    auto&      ring = monitor->m_damage;
    if (!PROJ || ring.m_current.empty())
        return;

    CRegion extra;
    for (const auto& a : attached) {
        const auto WINDOW = a.window.lock();
        if (!WINDOW || !WINDOW->m_isMapped)
            continue;

        const auto BOUNDS = WINDOW->getFullWindowBoundingBox().translate(renderOffset(WINDOW, monitor)).scale(monitor->m_scale);
        if (auto hit = ring.m_current.copy().intersect(BOUNDS); !hit.empty())
            extra.add(PROJ->apply(hit.getExtents()).expand(2).round());
    }

    // This frame renders anyway, so it needs no other scheduled.
    if (!extra.empty())
        ring.damage(extra);
}

static void onWindowOpen(PHLWINDOW w) {
    if (!recedes.empty())
        attach(w);
}

static void onWindowDestroy() {
    std::erase_if(attached, [](const auto& a) { return !a.window; });
}

static void onMonitorGone(PHLMONITOR monitor) {
    if (auto* recede = recedeFor(monitor))
        removeRecede(recede);
}

APICALL EXPORT std::string j2barRecedeApiVersion() {
    return HYPRLAND_API_VERSION;
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO j2barRecedeInit(HANDLE handle) {
    PHANDLE = handle;

    if (std::string{__hyprland_api_get_hash()} != __hyprland_api_get_client_hash()) {
        HyprlandAPI::addNotification(PHANDLE, "[hyprrecede] Version mismatch (headers ver is not equal to running hyprland ver)", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[hyprrecede] Version mismatch");
    }

    configValues.layerNamespace = makeShared<Config::Values::CStringValue>("plugin:hyprrecede:namespace", "regex for the namespace of layers that push windows back", "^j2bar-launcher-backdrop$");
    configValues.angle          = makeShared<Config::Values::CFloatValue>("plugin:hyprrecede:angle", "degrees the windows lean back", 16.F,
                                                                          Config::Values::SFloatValueOptions{.min = 0.F, .max = 60.F});
    configValues.scale          = makeShared<Config::Values::CFloatValue>("plugin:hyprrecede:scale", "scale of receded windows", 0.86F,
                                                                          Config::Values::SFloatValueOptions{.min = 0.1F, .max = 1.F});
    configValues.perspective    = makeShared<Config::Values::CFloatValue>("plugin:hyprrecede:perspective", "distance to the viewer in logical pixels", 1400.F,
                                                                          Config::Values::SFloatValueOptions{.min = 100.F, .max = 100000.F});
    configValues.shade          = makeShared<Config::Values::CFloatValue>("plugin:hyprrecede:shade", "opacity of the shade at the top of receded windows", 0.62F,
                                                                          Config::Values::SFloatValueOptions{.min = 0.F, .max = 1.F});
    configValues.blur           = makeShared<Config::Values::CFloatValue>("plugin:hyprrecede:blur", "opacity of the blur over receded windows", 1.F,
                                                                          Config::Values::SFloatValueOptions{.min = 0.F, .max = 1.F});

    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.layerNamespace);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.angle);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.scale);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.perspective);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.shade);
    HyprlandAPI::addConfigValueV2(PHANDLE, configValues.blur);

    // Like hyprfocus's, these nodes outlive an unload, and a reload just recreates them.
    Config::animationTree()->m_animationTree.createNode("hyprrecede", "global");
    Config::animationTree()->m_animationTree.createNode("hyprrecedeTilt", "hyprrecede");
    Config::animationTree()->m_animationTree.createNode("hyprrecedeShade", "hyprrecede");
    Config::animationTree()->m_animationTree.createNode("hyprrecedeBlur", "hyprrecede");

    listeners.emplace_back(Event::bus()->m_events.layer.opened.listen([](PHLLS ls) { onLayerOpened(ls); }));
    listeners.emplace_back(Event::bus()->m_events.layer.closed.listen([](PHLLS ls) { onLayerClosed(ls); }));
    listeners.emplace_back(Event::bus()->m_events.window.open.listen([](PHLWINDOW w) { onWindowOpen(w); }));
    listeners.emplace_back(Event::bus()->m_events.window.destroy.listen([](PHLWINDOWREF) { onWindowDestroy(); }));
    listeners.emplace_back(Event::bus()->m_events.monitor.removed.listen([](PHLMONITOR m) { onMonitorGone(m); }));
    listeners.emplace_back(Event::bus()->m_events.monitor.destroyMon.listen([](PHLMONITOR m) { onMonitorGone(m); }));
    listeners.emplace_back(Event::bus()->m_events.render.pre.listen([](PHLMONITOR m) { onRenderPre(m); }));
    listeners.emplace_back(Event::bus()->m_events.render.stage.listen([](eRenderStage s) { onRenderStage(s); }));

    return {"hyprrecede", "Tilts and blurs the windows behind a modal layer", "jesse", "1.0"};
}

APICALL EXPORT void j2barRecedeExit() {
    listeners.clear();

    while (!recedes.empty())
        removeRecede(recedes.back().get());
    detachAll();

    if (recedeShader.shader) {
        Render::GL::g_pHyprOpenGL->makeEGLCurrent();
        recedeShader.shader->destroy();
        recedeShader.shader.reset();
    }
}
