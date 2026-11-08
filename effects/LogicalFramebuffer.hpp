#pragma once

#include <hyprland/src/render/gl/GLFramebuffer.hpp>

// A transformed window's returned texture uses logical output orientation. This marker also
// lets successive j2bar transformers avoid undoing the native output rotation twice.
class CLogicalFramebuffer : public Render::GL::CGLFramebuffer {
  public:
    using Render::GL::CGLFramebuffer::CGLFramebuffer;
};
