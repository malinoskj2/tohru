//! Private libmpv0.41 client/render boundary. No executable, IPC server or codec code.
#![expect(
    unsafe_code,
    reason = "owned libmpv client/render ABI and documented callbacks"
)]
use std::ffi::{CStr, c_char, c_int, c_ulong, c_void};
use std::marker::PhantomData;
use std::mem::offset_of;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::Wake;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("libmpv client ABI must be2.5 or newer within major2")]
    Abi,
    #[error("libmpv allocation failed")]
    Allocation,
    #[error("libmpv {operation} failed with code{code}")]
    Api {
        operation: &'static str,
        code: c_int,
    },
    #[error("libmpv event queue exceeded its bounded drain budget")]
    EventOverflow,
    #[error("video backend shut down")]
    Shutdown,
    #[error("video ended (reason{reason})")]
    Ended { reason: c_int },
    #[error("video startup exceeded10 seconds")]
    StartupTimeout,
    #[error("video pause acknowledgment exceeded500 milliseconds")]
    PauseTimeout,
    #[error("unsafe pending unpause was superseded; retire the backend")]
    SupersededUnpause,
    #[error("video framebuffer size is invalid")]
    Size,
}

#[repr(C)]
struct Handle {
    _private: [u8; 0],
}
#[repr(C)]
struct RenderContext {
    _private: [u8; 0],
}
#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
pub(super) struct GlInit {
    pub get_proc: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub context: *mut c_void,
}
#[repr(C)]
struct Framebuffer {
    id: c_int,
    width: c_int,
    height: c_int,
    format: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Event {
    id: c_int,
    error: c_int,
    user: u64,
    data: *mut c_void,
}
#[repr(C)]
struct Property {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}
#[repr(C)]
#[derive(Default)]
struct FrameInfo {
    flags: u64,
    target_time: i64,
}
/// Only the stable reason/error prefix of `mpv_event_end_file` is read.
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

// Checked independently against the pinned SDK with tests/wallpaper/mpv-abi.c.
const _: () = {
    assert!(size_of::<Param>() == 2 * size_of::<*mut c_void>());
    assert!(offset_of!(Param, data) == size_of::<*mut c_void>());
    assert!(size_of::<GlInit>() == 2 * size_of::<*mut c_void>());
    assert!(size_of::<Framebuffer>() == 16);
    assert!(offset_of!(Event, data) == 16);
    assert!(size_of::<Event>() == 24);
    assert!(offset_of!(Property, data) == 16);
    assert!(size_of::<Property>() == 24);
    assert!(size_of::<FrameInfo>() == 16);
    assert!(size_of::<EndFile>() == 8 && offset_of!(EndFile, error) == 4);
};

#[cfg_attr(not(test), link(name = "mpv"))]
unsafe extern "C" {
    fn mpv_client_api_version() -> c_ulong;
    fn mpv_create() -> *mut Handle;
    fn mpv_set_option_string(
        handle: *mut Handle,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;
    fn mpv_initialize(handle: *mut Handle) -> c_int;
    fn mpv_set_wakeup_callback(
        handle: *mut Handle,
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        data: *mut c_void,
    );
    fn mpv_observe_property(
        handle: *mut Handle,
        user: u64,
        name: *const c_char,
        format: c_int,
    ) -> c_int;
    fn mpv_command_async(handle: *mut Handle, user: u64, arguments: *const *const c_char) -> c_int;
    fn mpv_set_property_async(
        handle: *mut Handle,
        user: u64,
        name: *const c_char,
        format: c_int,
        value: *mut c_void,
    ) -> c_int;
    fn mpv_wait_event(handle: *mut Handle, timeout: f64) -> *const Event;
    fn mpv_get_time_us(handle: *mut Handle) -> i64;
    fn mpv_terminate_destroy(handle: *mut Handle);
    fn mpv_render_context_create(
        result: *mut *mut RenderContext,
        handle: *mut Handle,
        params: *mut Param,
    ) -> c_int;
    fn mpv_render_context_set_update_callback(
        context: *mut RenderContext,
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        data: *mut c_void,
    );
    fn mpv_render_context_update(context: *mut RenderContext) -> u64;
    fn mpv_render_context_get_info(context: *mut RenderContext, param: Param) -> c_int;
    fn mpv_render_context_render(context: *mut RenderContext, params: *mut Param) -> c_int;
    fn mpv_render_context_report_swap(context: *mut RenderContext);
    fn mpv_render_context_free(context: *mut RenderContext);
}

fn check(code: c_int, operation: &'static str) -> Result<(), Error> {
    if code < 0 {
        Err(Error::Api { operation, code })
    } else {
        Ok(())
    }
}

fn param<T>(kind: c_int, value: &mut T) -> Param {
    Param {
        kind,
        data: ptr::from_mut(value).cast(),
    }
}
fn end() -> Param {
    Param {
        kind: 0,
        data: ptr::null_mut(),
    }
}

unsafe extern "C" fn wake(data: *mut c_void) {
    // SAFETY: the stable Box<Wake> outlives context free AND final core termination.
    let wake = unsafe { &*data.cast::<Wake>() };
    wake.notify();
}

/// Thread-affine wrapper: the outer EGL owner must make its original context
/// current for construction, every render method and Drop. No handle escapes.
pub(super) struct Player {
    handle: NonNull<Handle>,
    context: Option<NonNull<RenderContext>>,
    callback: Box<Wake>,
    loaded: bool,
    observed_pause: Option<bool>,
    pending_pause: Option<(u64, bool, Instant)>,
    next_request: u64,
    started: Instant,
    _affinity: PhantomData<Rc<()>>,
}

impl Player {
    /// # Safety
    /// `init` resolves the current live EGL context, and display is its live
    /// `wl_display`. Both must outlive this wrapper. This same GL context must be
    /// current on this thread for every render method and Drop, including errors.
    pub(super) unsafe fn new(
        path: &CStr,
        mut init: GlInit,
        display: *mut c_void,
        notify: Wake,
    ) -> Result<Self, Error> {
        // SAFETY: no pointers; reads the loaded library's ABI version.
        let version = unsafe { mpv_client_api_version() };
        if version >> 16 != 2 || version & 0xffff < 5 {
            return Err(Error::Abi);
        }
        // SAFETY: creates one independent core; null is handled below.
        let handle = NonNull::new(unsafe { mpv_create() }).ok_or(Error::Allocation)?;
        let mut player = Self {
            handle,
            context: None,
            callback: Box::new(notify),
            loaded: false,
            observed_pause: None,
            pending_pause: None,
            next_request: 10,
            started: Instant::now(),
            _affinity: PhantomData,
        };
        for (name, value) in [
            (c"config", c"no"),
            (c"load-scripts", c"no"),
            (c"ytdl", c"no"),
            (c"osc", c"no"),
            (c"input-default-bindings", c"no"),
            (c"input-terminal", c"no"),
            (c"terminal", c"no"),
            (c"access-references", c"no"),
            (c"load-unsafe-playlists", c"no"),
            (c"autoload-files", c"no"),
            (c"demuxer-max-bytes", c"32MiB"),
            (c"demuxer-max-back-bytes", c"8MiB"),
            (c"vo", c"libmpv"),
            (c"audio", c"no"),
            (c"sub", c"no"),
            (c"mute", c"yes"),
            (c"pause", c"yes"),
            (c"loop-file", c"inf"),
            (c"panscan", c"1.0"),
            (c"hwdec", c"auto"),
            (c"osd-level", c"0"),
        ] {
            check(
                // SAFETY: owned fresh core and valid strings, before initialize/render.
                unsafe { mpv_set_option_string(handle.as_ptr(), name.as_ptr(), value.as_ptr()) },
                name.to_str().unwrap_or("startup option"),
            )?;
        }
        // SAFETY: sole owned fresh core, configured paused and with no audio.
        check(unsafe { mpv_initialize(handle.as_ptr()) }, "initialize")?;
        check(
            // SAFETY: live core; fixed property name and MPV_FORMAT_FLAG=3.
            unsafe { mpv_observe_property(handle.as_ptr(), 2, c"pause".as_ptr(), 3) },
            "observe pause",
        )?;
        let mut params = [
            Param {
                kind: 1,
                data: c"opengl".as_ptr().cast_mut().cast(),
            },
            param(2, &mut init),
            Param {
                kind: 9,
                data: display,
            },
            end(),
        ];
        let mut context = ptr::null_mut();
        check(
            // SAFETY: caller guarantees current GL/context/display lifetime; terminated typed parameter array.
            unsafe {
                mpv_render_context_create(&raw mut context, handle.as_ptr(), params.as_mut_ptr())
            },
            "create render context",
        )?;
        player.context = Some(NonNull::new(context).ok_or(Error::Allocation)?);
        let callback = ptr::from_mut(player.callback.as_mut()).cast();
        // SAFETY: stable boxed callback retained through final core termination; wake only signals the owner.
        unsafe { mpv_set_wakeup_callback(handle.as_ptr(), Some(wake), callback) };
        // SAFETY: just-created context and same stable callback; no render API is called from the callback.
        unsafe { mpv_render_context_set_update_callback(context, Some(wake), callback) };
        let arguments = [
            c"loadfile".as_ptr(),
            path.as_ptr(),
            c"replace".as_ptr(),
            ptr::null(),
        ];
        check(
            // SAFETY: null-terminated owned strings copied by this asynchronous API before it returns.
            unsafe { mpv_command_async(handle.as_ptr(), 1, arguments.as_ptr()) },
            "load local file",
        )?;
        Ok(player)
    }

    pub(super) fn events(&mut self, now: Instant) -> Result<(), Error> {
        for _ in 0..256 {
            // SAFETY: sole owner, zero timeout, every borrowed event is consumed before the next wait.
            let event = unsafe { mpv_wait_event(self.handle.as_ptr(), 0.0) };
            // SAFETY: SDK guarantees nonnull event valid until the next wait/core destroy.
            let event = unsafe { *event };
            match event.id {
                0 => {
                    if !self.loaded && now.duration_since(self.started) > Duration::from_secs(10) {
                        return Err(Error::StartupTimeout);
                    }
                    return Ok(());
                }
                1 => return Err(Error::Shutdown),
                7 => {
                    if event.data.is_null() {
                        return Err(Error::Ended { reason: -1 });
                    }
                    // SAFETY: END_FILE payload begins with the SDK's reason/error int pair.
                    let end = unsafe { &*event.data.cast::<EndFile>() };
                    if end.reason == 4 && end.error < 0 {
                        return Err(Error::Api {
                            operation: "media decode/open",
                            code: end.error,
                        });
                    }
                    return Err(Error::Ended { reason: end.reason });
                }
                4 => {
                    check(event.error, "set pause reply")?;
                    if self
                        .pending_pause
                        .is_some_and(|(id, _, _)| id == event.user)
                    {
                        let (_, paused, _) =
                            self.pending_pause.take().ok_or(Error::PauseTimeout)?;
                        self.observed_pause = Some(paused);
                    }
                }
                5 => check(event.error, "load command reply")?,
                8 => self.loaded = true,
                22 if event.user == 2 && !event.data.is_null() => {
                    // SAFETY: property-change pause observer payload has the SDK property layout.
                    let value = unsafe { &*event.data.cast::<Property>() };
                    self.observed_pause = if value.format == 3 && !value.data.is_null() {
                        // SAFETY: MPV_FORMAT_FLAG is an int pointer valid until the next event read.
                        Some(unsafe { *value.data.cast::<c_int>() } != 0)
                    } else {
                        None
                    };
                }
                24 => return Err(Error::EventOverflow),
                _ => {}
            }
        }
        Err(Error::EventOverflow)
    }

    pub(super) fn pause(&mut self, paused: bool, now: Instant) -> Result<(), Error> {
        if let Some((_, pending, since)) = self.pending_pause {
            if paused && !pending {
                return Err(Error::SupersededUnpause);
            }
            if now.duration_since(since) > Duration::from_millis(500) {
                return Err(Error::PauseTimeout);
            }
            return Ok(());
        }
        // Startup stays paused until file loaded AND the actual pause observation arrives.
        if (!paused && (!self.loaded || self.observed_pause.is_none()))
            || self.observed_pause == Some(paused)
        {
            return Ok(());
        }
        let mut value = c_int::from(paused);
        self.next_request = self.next_request.wrapping_add(1).max(10);
        let id = self.next_request;
        check(
            // SAFETY: SDK's async setter copies FLAG data during this call and is render-thread safe.
            unsafe {
                mpv_set_property_async(
                    self.handle.as_ptr(),
                    id,
                    c"pause".as_ptr(),
                    3,
                    ptr::from_mut(&mut value).cast(),
                )
            },
            "enqueue pause",
        )?;
        self.pending_pause = Some((id, paused, now));
        Ok(())
    }

    /// Last-resort bounded client-only request if EGL cannot be made current.
    /// This does NOT acknowledge pause or make it safe to free render resources.
    pub(super) fn terminal_pause(&mut self) -> Result<(), Error> {
        let mut paused = 1;
        check(
            // SAFETY: FLAG copied by the render-thread-safe async client API; one final request only.
            unsafe {
                mpv_set_property_async(
                    self.handle.as_ptr(),
                    9,
                    c"pause".as_ptr(),
                    3,
                    ptr::from_mut(&mut paused).cast(),
                )
            },
            "enqueue terminal pause",
        )
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        let startup = (!self.loaded).then(|| self.started + Duration::from_secs(10));
        let pause = self
            .pending_pause
            .map(|(_, _, since)| since + Duration::from_millis(500));
        startup.into_iter().chain(pause).min()
    }

    pub(super) fn paused(&self) -> Option<bool> {
        self.observed_pause
    }

    pub(super) fn update(&self) -> bool {
        let Some(context) = self.context else {
            return false;
        };
        // SAFETY: sole thread-affine context, original GL context current by outer owner contract.
        unsafe { mpv_render_context_update(context.as_ptr()) & 1 != 0 }
    }

    pub(super) fn delay(&self) -> Result<Duration, Error> {
        let context = self.context.ok_or(Error::Allocation)?;
        let mut info = FrameInfo::default();
        check(
            // SAFETY: original current GL context and correctly typed NEXT_FRAME_INFO output storage.
            unsafe { mpv_render_context_get_info(context.as_ptr(), param(11, &mut info)) },
            "frame timing",
        )?;
        // SAFETY: owned live core; time query is documented safe for render API threads.
        let now = unsafe { mpv_get_time_us(self.handle.as_ptr()) };
        let micros = info.target_time.saturating_sub(now).max(0);
        Ok(Duration::from_micros(u64::try_from(micros).unwrap_or(0)))
    }

    pub(super) fn render(&self, size: (u32, u32), skip: bool) -> Result<(), Error> {
        let (Ok(width), Ok(height)) = (c_int::try_from(size.0), c_int::try_from(size.1)) else {
            return Err(Error::Size);
        };
        if width <= 0 || height <= 0 {
            return Err(Error::Size);
        }
        let context = self.context.ok_or(Error::Allocation)?;
        let mut fbo = Framebuffer {
            id: 0,
            width,
            height,
            format: 0,
        };
        let (mut flip, mut block, mut skip) = (1, 0, c_int::from(skip));
        let mut params = [
            param(3, &mut fbo),
            param(4, &mut flip),
            param(12, &mut block),
            param(13, &mut skip),
            end(),
        ];
        check(
            // SAFETY: same current GL context/default framebuffer, validated dimensions, typed terminated params.
            unsafe { mpv_render_context_render(context.as_ptr(), params.as_mut_ptr()) },
            "render frame",
        )
    }

    pub(super) fn swapped(&self) {
        if let Some(context) = self.context {
            // SAFETY: outer owner reports only an actual successful swap on this context.
            unsafe { mpv_render_context_report_swap(context.as_ptr()) };
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(context) = self.context.take() {
            // SAFETY: original GL context is current per constructor's ongoing outer-owner contract.
            unsafe {
                mpv_render_context_set_update_callback(context.as_ptr(), None, ptr::null_mut());
            };
            // SAFETY: context is sole owned and freed before its core or EGL/Wayland dependencies.
            unsafe { mpv_render_context_free(context.as_ptr()) };
        }
        // SAFETY: owned sole client; callback Box stays alive until the final core termination below returns.
        unsafe { mpv_set_wakeup_callback(self.handle.as_ptr(), None, ptr::null_mut()) };
        // SAFETY: no concurrent client API calls; this waits until the core/foreign callbacks have terminated.
        unsafe { mpv_terminate_destroy(self.handle.as_ptr()) };
    }
}

#[cfg(test)]
#[path = "../../tests/wallpaper/mpv.rs"]
mod tests;
