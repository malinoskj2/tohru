//! Video-only EGL targets; all handles remain on the owning thread.
#![expect(
    unsafe_code,
    reason = "EGL platform/display/window and OpenGL symbol ABI"
)]
use std::ffi::{CStr, c_char, c_void};
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use khronos_egl as egl;
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use wayland_client::{Connection, Proxy};
use wayland_egl::WlEglSurface;

use super::mpv::GlInit;
use super::wayland::Viewport;

type Api = egl::DynamicInstance<egl::EGL1_5>;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("loading EGL1.5")]
    Load(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("EGL {stage}")]
    Api {
        stage: &'static str,
        #[source]
        source: egl::Error,
    },
    #[error("no compatible OpenGL EGL window configuration")]
    Config,
    #[error("Wayland EGL window library is unavailable")]
    WindowLibrary,
    #[error("creating Wayland EGL window")]
    Window(#[from] wayland_egl::Error),
    #[error("video target requires positive bounded dimensions")]
    Size,
    #[error("EGL owner was closed")]
    Closed,
}
fn at<T>(stage: &'static str, result: Result<T, egl::Error>) -> Result<T, Error> {
    result.map_err(|source| Error::Api { stage, source })
}

struct DisplayInner {
    api: Api,
    display: egl::Display,
    config: Option<egl::Config>,
    context: Option<egl::Context>,
    connection: Connection,
    fatal: Arc<AtomicBool>,
}
pub(super) struct Display {
    inner: Option<Box<DisplayInner>>,
}

impl Display {
    pub(super) fn new(connection: Connection, fatal: Arc<AtomicBool>) -> Result<Rc<Self>, Error> {
        // SAFETY: trusted packaged EGL loader; its library remains owned by DisplayInner through all handles/callbacks.
        let api = unsafe { Api::load_required() }.map_err(|error| Error::Load(Box::new(error)))?;
        let native = connection.backend().display_ptr().cast();
        // SAFETY: this owned system-backend Wayland connection outlives the EGL display; platform is WAYLAND_KHR.
        let display = at("get Wayland display", unsafe {
            api.get_platform_display(0x31d8, native, &[egl::ATTRIB_NONE])
        })?;
        at("initialize", api.initialize(display))?;
        let mut owner = Self {
            inner: Some(Box::new(DisplayInner {
                api,
                display,
                config: None,
                context: None,
                connection,
                fatal,
            })),
        };
        let inner = owner.inner.as_mut().ok_or(Error::Closed)?;
        at("bind OpenGL", inner.api.bind_api(egl::OPENGL_API))?;
        let attributes = [
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_BIT,
            egl::NONE,
        ];
        let config = at(
            "choose window configuration",
            inner.api.choose_first_config(display, &attributes),
        )?
        .ok_or(Error::Config)?;
        inner.config = Some(config);
        let attributes = [
            egl::CONTEXT_MAJOR_VERSION,
            3,
            egl::CONTEXT_MINOR_VERSION,
            3,
            egl::CONTEXT_OPENGL_PROFILE_MASK,
            egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
            egl::NONE,
        ];
        inner.context = Some(at(
            "create OpenGL3.3 context",
            inner.api.create_context(display, config, None, &attributes),
        )?);
        Ok(Rc::new(owner))
    }
    fn inner(&self) -> Result<&DisplayInner, Error> {
        self.inner.as_deref().ok_or(Error::Closed)
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };
        let result = inner
            .api
            .make_current(inner.display, None, None, None)
            .and_then(|()| {
                inner.context.map_or(Ok(()), |context| {
                    inner.api.destroy_context(inner.display, context)
                })
            })
            .and_then(|()| inner.api.terminate(inner.display));
        if result.is_err() {
            inner.fatal.store(true, Ordering::Release);
            // A failed foreign teardown cannot prove the driver stopped referencing
            // its library/Wayland display. Retain the entire owner, report fatal,
            // and require the containing owned role to exit before reuse.
            std::mem::forget(inner);
        }
    }
}

struct TargetInner {
    surface: egl::Surface,
    // Field order: EGL window before viewport/layer, display/connection last.
    window: WlEglSurface,
    viewport: Option<Rc<Viewport>>,
    layer: LayerSurface,
    display: Rc<Display>,
}
pub(super) struct Target {
    inner: Option<Box<TargetInner>>,
}

impl Target {
    pub(super) fn new(
        display: Rc<Display>,
        layer: LayerSurface,
        viewport: Option<Rc<Viewport>>,
        logical: (u32, u32),
        physical: (u32, u32),
        scale: i32,
    ) -> Result<Rc<Self>, Error> {
        let (w, h) = dimensions(physical)?;
        if !wayland_egl::is_available() {
            return Err(Error::WindowLibrary);
        }
        let window = WlEglSurface::new(layer.wl_surface().id(), w, h)?;
        let d = display.inner()?;
        let config = d.config.ok_or(Error::Config)?;
        // SAFETY: window wraps this display's live wl_surface and stays alive until after EGL surface destruction.
        let surface = at("create window surface", unsafe {
            d.api
                .create_window_surface(d.display, config, window.ptr().cast_mut(), None)
        })?;
        let target = Rc::new(Self {
            inner: Some(Box::new(TargetInner {
                surface,
                window,
                viewport,
                layer,
                display,
            })),
        });
        target.resize(logical, physical, scale)?;
        target.current()?;
        // Explicit frame callbacks pace commits; EGL must not wait for an occluded background frame.
        let inner = target.inner()?;
        let d = inner.display.inner()?;
        at(
            "disable blocking swap interval",
            d.api.swap_interval(d.display, 0),
        )?;
        Ok(target)
    }
    fn inner(&self) -> Result<&TargetInner, Error> {
        self.inner.as_deref().ok_or(Error::Closed)
    }

    pub(super) fn current(&self) -> Result<(), Error> {
        let inner = self.inner()?;
        let d = inner.display.inner()?;
        at(
            "make original context current",
            d.api.make_current(
                d.display,
                Some(inner.surface),
                Some(inner.surface),
                d.context,
            ),
        )
    }
    pub(super) fn resize(
        &self,
        logical: (u32, u32),
        physical: (u32, u32),
        scale: i32,
    ) -> Result<(), Error> {
        let inner = self.inner()?;
        let (w, h) = dimensions(physical)?;
        let (lw, lh) = dimensions(logical)?;
        if let Some(viewport) = &inner.viewport {
            inner.layer.wl_surface().set_buffer_scale(1);
            viewport.destination(lw, lh);
        } else {
            if scale < 1 {
                return Err(Error::Size);
            }
            inner.layer.wl_surface().set_buffer_scale(scale);
        }
        inner.window.resize(w, h, 0, 0);
        Ok(())
    }
    pub(super) fn swap(&self, physical: (u32, u32)) -> Result<(), Error> {
        let inner = self.inner()?;
        let d = inner.display.inner()?;
        let (w, h) = dimensions(physical)?;
        inner.layer.wl_surface().damage_buffer(0, 0, w, h);
        at(
            "swap video buffer",
            d.api.swap_buffers(d.display, inner.surface),
        )
    }
    pub(super) fn gl_init(&self) -> Result<GlInit, Error> {
        let d = self.inner()?.display.inner()?;
        Ok(GlInit {
            get_proc,
            context: ptr::from_ref(d).cast_mut().cast(),
        })
    }
    pub(super) fn wayland_display(&self) -> Result<*mut c_void, Error> {
        Ok(self
            .inner()?
            .display
            .inner()?
            .connection
            .backend()
            .display_ptr()
            .cast())
    }
}

fn dimensions(size: (u32, u32)) -> Result<(i32, i32), Error> {
    if size.0 == 0 || size.1 == 0 || u64::from(size.0) * u64::from(size.1) > 32 * 1024 * 1024 {
        return Err(Error::Size);
    }
    let (Ok(w), Ok(h)) = (i32::try_from(size.0), i32::try_from(size.1)) else {
        return Err(Error::Size);
    };
    Ok((w, h))
}

unsafe extern "C" fn get_proc(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: Target owns the stable DisplayInner box until after libmpv render/core termination.
    let display = unsafe { &*context.cast::<DisplayInner>() };
    // SAFETY: libmpv supplies a terminated symbol name during GL resolution on its rendering owner thread.
    let name = unsafe { CStr::from_ptr(name) };
    let Ok(name) = name.to_str() else {
        return ptr::null_mut();
    };
    display
        .api
        .get_proc_address(name)
        .map_or(ptr::null_mut(), |function| {
            function as *const () as *mut c_void
        })
}

impl Drop for Target {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };
        let result = inner.display.inner().and_then(|d| {
            at(
                "unbind target for teardown",
                d.api.make_current(d.display, None, None, None),
            )?;
            at(
                "destroy video surface",
                d.api.destroy_surface(d.display, inner.surface),
            )
        });
        if result.is_err() {
            if let Ok(display) = inner.display.inner() {
                display.fatal.store(true, Ordering::Release);
            }
            // Retain native window/layer/display/library when driver ownership is unproven.
            std::mem::forget(inner);
        }
    }
}
