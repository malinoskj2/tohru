#pragma once

#include <memory>
#include <optional>
#include <utility>

namespace Focus {
// The pinned animation API exposes no callback identity. The capture lives only
// while our callback (or an in-flight copy of it) remains installed/alive.
class CCallbackOwner {
  public:
    template <class Fn>
    auto wrap(Fn fn) {
        auto token = std::make_shared<int>(0);
        m_token = token;
        return [token = std::move(token), fn = std::move(fn)](auto&&... args) {
            fn(std::forward<decltype(args)>(args)...);
        };
    }

    bool ownsCallback() const { return !m_token.expired(); }

  private:
    std::weak_ptr<int> m_token;
};

// Retained for both animation legs. Native variables are supplied by the owner;
// this also permits exercising teardown with controlled variables and layout.
template <class Position, class Config>
struct SRestoration {
    struct SGeometry {
        Position position;
        Position size;
        Config positionConfig;
        Config sizeConfig;
        bool shrink = false;
        CCallbackOwner callback = {};
    };
    struct SOpacity {
        float value;
        Config config;
        CCallbackOwner callback = {};
    };
    std::optional<SGeometry> geometry;
    std::optional<SOpacity> opacity;

    template <class PositionVar, class SizeVar, class AlphaVar, class Layout, class Release>
    void unload(PositionVar& position, SizeVar& size, AlphaVar& alpha,
                bool mapped, bool tiled, Layout recalc, Release releaseSizing) {
        if (geometry) {
            // A replacement callback belongs to another owner; preserve it.
            if (geometry->callback.ownsCallback()) {
                if (geometry->shrink)
                    size.setCallbackOnEnd(nullptr);
                else
                    position.setCallbackOnEnd(nullptr);
            }
            position.setConfig(geometry->positionConfig);
            if (geometry->shrink)
                size.setConfig(geometry->sizeConfig);
            if (mapped && tiled) {
                // Layout may have changed during the dip. Its current target wins.
                recalc();
                position.warp(false);
                if (geometry->shrink)
                    size.warp(false);
            } else {
                // Unmapped windows can still own animations, but no layout target.
                position = geometry->position;
                position.warp(false);
                if (geometry->shrink) {
                    size = geometry->size;
                    size.warp(false);
                }
            }
        }
        if (opacity) {
            if (opacity->callback.ownsCallback())
                alpha.setCallbackOnEnd(nullptr);
            alpha.setConfig(opacity->config);
            alpha = opacity->value;
            alpha.warp(false);
        }
        // A held client must never receive the inward size during teardown.
        releaseSizing();
        geometry.reset();
        opacity.reset();
    }
};
}
