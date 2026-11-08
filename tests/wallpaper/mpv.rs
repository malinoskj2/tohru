//! Fake SDK transport only: production wrapper/lifetime code runs without loading libmpv/GL.
use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;

#[derive(Default)]
struct Fixture {
    options: Vec<(String, String)>,
    sequence: Vec<&'static str>,
    events: VecDeque<(c_int, c_int, u64, Option<bool>)>,
    last: Option<Box<Payload>>,
    hold_pause: bool,
    fail_option: bool,
    need_frame: bool,
    renders: Vec<(i32, i32, bool)>,
    end_file: Option<(c_int, c_int)>,
}
struct Payload {
    event: Event,
    property: Property,
    flag: c_int,
    end: EndFile,
}
thread_local! {static FIXTURE:RefCell<Fixture>=RefCell::new(Fixture::default());}
fn fixture<T>(f: impl FnOnce(&mut Fixture) -> T) -> T {
    FIXTURE.with(|state| f(&mut state.borrow_mut()))
}
fn reset() {
    fixture(|f| *f = Fixture::default());
}

#[unsafe(no_mangle)]
extern "C" fn mpv_client_api_version() -> c_ulong {
    0x0002_0005
}
#[unsafe(no_mangle)]
extern "C" fn mpv_create() -> *mut Handle {
    fixture(|f| f.sequence.push("create-core"));
    Box::into_raw(Box::new(0u8)).cast()
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_set_option_string(
    _: *mut Handle,
    name: *const c_char,
    value: *const c_char,
) -> c_int {
    // SAFETY: actual wrapper passes valid terminated option strings for this call.
    let name = unsafe { CStr::from_ptr(name) }.to_str().unwrap().to_owned();
    // SAFETY: same option value lifetime supplied by the actual wrapper.
    let value = unsafe { CStr::from_ptr(value) }
        .to_str()
        .unwrap()
        .to_owned();
    fixture(|f| {
        f.options.push((name, value));
        if f.fail_option { -5 } else { 0 }
    })
}
#[unsafe(no_mangle)]
extern "C" fn mpv_initialize(_: *mut Handle) -> c_int {
    fixture(|f| f.sequence.push("initialize"));
    0
}
#[unsafe(no_mangle)]
extern "C" fn mpv_set_wakeup_callback(
    _: *mut Handle,
    _: Option<unsafe extern "C" fn(*mut c_void)>,
    _: *mut c_void,
) {
}
#[unsafe(no_mangle)]
extern "C" fn mpv_observe_property(_: *mut Handle, user: u64, _: *const c_char, _: c_int) -> c_int {
    fixture(|f| f.events.push_back((22, 0, user, Some(true))));
    0
}
#[unsafe(no_mangle)]
extern "C" fn mpv_command_async(_: *mut Handle, user: u64, _: *const *const c_char) -> c_int {
    fixture(|f| {
        f.sequence.push("load");
        f.events.push_back((5, 0, user, None));
        f.events.push_back((8, 0, 0, None));
    });
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_set_property_async(
    _: *mut Handle,
    user: u64,
    _: *const c_char,
    _: c_int,
    value: *mut c_void,
) -> c_int {
    // SAFETY: actual wrapper passes an int FLAG alive during this call; fixture copies it like SDK.
    let value = unsafe { *value.cast::<c_int>() } != 0;
    fixture(|f| {
        f.sequence.push(if value { "pause" } else { "unpause" });
        if !f.hold_pause {
            f.events.push_back((4, 0, user, None));
            f.events.push_back((22, 0, 2, Some(value)));
        }
    });
    0
}
#[unsafe(no_mangle)]
extern "C" fn mpv_wait_event(_: *mut Handle, _: f64) -> *const Event {
    fixture(|f| {
        let (id, error, user, value) = f.events.pop_front().unwrap_or((0, 0, 0, None));
        let mut payload = Box::new(Payload {
            event: Event {
                id,
                error,
                user,
                data: ptr::null_mut(),
            },
            property: Property {
                name: ptr::null(),
                format: 3,
                data: ptr::null_mut(),
            },
            flag: c_int::from(value.unwrap_or(false)),
            end: EndFile {
                reason: 0,
                error: 0,
            },
        });
        if value.is_some() {
            payload.property.data = ptr::from_mut(&mut payload.flag).cast();
            payload.event.data = ptr::from_mut(&mut payload.property).cast();
        }
        if let (7, Some((reason, error))) = (id, f.end_file) {
            payload.end = EndFile { reason, error };
            payload.event.data = ptr::from_mut(&mut payload.end).cast();
        }
        f.last = Some(payload);
        ptr::from_ref(&f.last.as_ref().unwrap().event)
    })
}
#[unsafe(no_mangle)]
extern "C" fn mpv_get_time_us(_: *mut Handle) -> i64 {
    1000
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_terminate_destroy(handle: *mut Handle) {
    fixture(|f| f.sequence.push("terminate-core"));
    // SAFETY: pointer originated as Box<u8> in fixture create and is consumed once by actual Drop.
    drop(unsafe { Box::from_raw(handle.cast::<u8>()) });
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_render_context_create(
    result: *mut *mut RenderContext,
    _: *mut Handle,
    _: *mut Param,
) -> c_int {
    fixture(|f| f.sequence.push("create-render"));
    // SAFETY: actual wrapper provides a valid output pointer to one context pointer.
    unsafe { *result = Box::into_raw(Box::new(0u8)).cast() };
    0
}
#[unsafe(no_mangle)]
extern "C" fn mpv_render_context_set_update_callback(
    _: *mut RenderContext,
    _: Option<unsafe extern "C" fn(*mut c_void)>,
    _: *mut c_void,
) {
}
#[unsafe(no_mangle)]
extern "C" fn mpv_render_context_update(_: *mut RenderContext) -> u64 {
    fixture(|f| u64::from(std::mem::take(&mut f.need_frame)))
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_render_context_get_info(_: *mut RenderContext, param: Param) -> c_int {
    assert_eq!(param.kind, 11);
    // SAFETY: actual wrapper provides a live FrameInfo for NEXT_FRAME_INFO.
    unsafe {
        *param.data.cast::<FrameInfo>() = FrameInfo {
            flags: 1,
            target_time: 2000,
        }
    };
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_render_context_render(_: *mut RenderContext, params: *mut Param) -> c_int {
    let mut fbo = (0, 0);
    let mut skip = false;
    for index in 0..5 {
        // SAFETY: actual wrapper provides five typed parameters terminated at the fifth.
        let address = unsafe { params.add(index) };
        // SAFETY: the pointer above remains inside the actual wrapper's live parameter array.
        let param = unsafe { &*address };
        match param.kind {
            0 => break,
            3 => {
                // SAFETY: OPENGL_FBO parameter points to the actual wrapper's Framebuffer.
                let framebuffer = unsafe { &*param.data.cast::<Framebuffer>() };
                fbo = (framebuffer.width, framebuffer.height);
            }
            13 => {
                // SAFETY: SKIP_RENDERING parameter points to an int flag.
                skip = unsafe { *param.data.cast::<c_int>() } != 0;
            }
            _ => {}
        }
    }
    fixture(|f| f.renders.push((fbo.0, fbo.1, skip)));
    0
}
#[unsafe(no_mangle)]
extern "C" fn mpv_render_context_report_swap(_: *mut RenderContext) {
    fixture(|f| f.sequence.push("swap"));
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mpv_render_context_free(context: *mut RenderContext) {
    fixture(|f| f.sequence.push("free-render"));
    // SAFETY: pointer originated as one Box<u8> in fixture render create; actual Drop consumes it once.
    drop(unsafe { Box::from_raw(context.cast::<u8>()) });
}
unsafe extern "C" fn get_proc(_: *mut c_void, _: *const c_char) -> *mut c_void {
    ptr::null_mut()
}
fn player() -> (Player, super::super::wake::Receiver) {
    let (wake, receiver) = Wake::pair().unwrap();
    // SAFETY: fake SDK symbols access no actual GL/display and create only owned test allocations.
    let player = unsafe {
        Player::new(
            c"/chosen/video.mp4",
            GlInit {
                get_proc,
                context: ptr::null_mut(),
            },
            ptr::null_mut(),
            wake,
        )
    }
    .unwrap();
    (player, receiver)
}
#[test]
fn startup_is_paused_muted_local_only_and_retirement_orders_render_before_core() {
    reset();
    let (mut player, _receiver) = player();
    fixture(|f| {
        for pair in [
            ("pause", "yes"),
            ("audio", "no"),
            ("loop-file", "inf"),
            ("config", "no"),
            ("load-scripts", "no"),
            ("access-references", "no"),
            ("autoload-files", "no"),
            ("demuxer-max-bytes", "32MiB"),
            ("demuxer-max-back-bytes", "8MiB"),
        ] {
            assert!(f.options.contains(&(pair.0.into(), pair.1.into())));
        }
        assert_eq!(
            f.sequence,
            vec!["create-core", "initialize", "create-render", "load"]
        );
    });
    player.events(Instant::now()).unwrap();
    assert_eq!(player.paused(), Some(true));
    drop(player);
    fixture(|f| assert!(f.sequence.ends_with(&["free-render", "terminate-core"])));
}
#[test]
fn pause_requests_are_bounded_acknowledged_and_safety_supersession_retires() {
    reset();
    let (mut player, _) = player();
    let now = Instant::now();
    player.pause(false, now).unwrap();
    fixture(|f| assert!(!f.sequence.contains(&"unpause")));
    player.events(now).unwrap();
    fixture(|f| f.hold_pause = true);
    player.pause(false, now).unwrap();
    for _ in 0..100 {
        player.pause(false, now).unwrap();
    }
    fixture(|f| assert_eq!(f.sequence.iter().filter(|s| **s == "unpause").count(), 1));
    assert!(matches!(
        player.pause(true, now),
        Err(Error::SupersededUnpause)
    ));
    assert!(matches!(
        player.pause(false, now + Duration::from_millis(501)),
        Err(Error::PauseTimeout)
    ));
}
#[test]
fn actual_ack_changes_observed_state_and_render_parameters_keep_geometry_skip_and_timing() {
    reset();
    let (mut player, _) = player();
    let now = Instant::now();
    player.events(now).unwrap();
    player.pause(false, now).unwrap();
    assert_eq!(player.paused(), Some(true));
    player.events(now).unwrap();
    assert_eq!(player.paused(), Some(false));
    fixture(|f| f.need_frame = true);
    assert!(player.update());
    assert!(!player.update());
    assert_eq!(player.delay().unwrap(), Duration::from_millis(1));
    assert!(matches!(player.render((0, 100), false), Err(Error::Size)));
    player.render((1600, 900), false).unwrap();
    player.swapped();
    player.render((900, 1600), true).unwrap();
    fixture(|f| assert_eq!(f.renders, vec![(1600, 900, false), (900, 1600, true)]));
}
#[test]
fn failed_initialization_drops_owned_core_and_backend_termination_is_not_success() {
    reset();
    fixture(|f| f.fail_option = true);
    let (wake, _) = Wake::pair().unwrap();
    assert!(
        // SAFETY: fake SDK allocates Rust fixtures only, with no actual display or GL context.
        unsafe {
            Player::new(
                c"/chosen/video.mp4",
                GlInit {
                    get_proc,
                    context: ptr::null_mut(),
                },
                ptr::null_mut(),
                wake,
            )
        }
        .is_err()
    );
    fixture(|f| assert_eq!(f.sequence, vec!["create-core", "terminate-core"]));
    reset();
    let (mut player, _) = player();
    fixture(|f| f.events.push_front((7, -13, 0, None)));
    assert!(matches!(
        player.events(Instant::now()),
        Err(Error::Ended { .. })
    ));
}
#[test]
fn callback_signal_coalesces_without_work_or_unbounded_queue() {
    let (wake, receiver) = Wake::pair().unwrap();
    for _ in 0..10000 {
        wake.notify();
    }
    receiver.drain().unwrap();
    wake.notify();
    receiver.drain().unwrap();
}

#[test]
fn startup_pause_deadlines_and_terminal_request_remain_truthful() {
    reset();
    let (mut player, _) = player();
    assert!(player.deadline().is_some());
    let now = Instant::now();
    player.events(now).unwrap();
    assert!(player.deadline().is_none());
    fixture(|f| f.hold_pause = true);
    player.pause(false, now).unwrap();
    assert_eq!(player.deadline(), Some(now + Duration::from_millis(500)));
    fixture(|f| f.events.push_front((22, 0, 2, Some(false))));
    player.events(now).unwrap();
    player.terminal_pause().unwrap();
    // Enqueueing the one final safety request is not a pause acknowledgment.
    assert_eq!(player.paused(), Some(false));
    fixture(|f| assert!(f.sequence.ends_with(&["unpause", "pause"])));
}

#[test]
fn native_end_file_reports_actual_decode_error_and_preserves_unknown_reason() {
    reset();
    let (mut player, _) = player();
    fixture(|f| {
        f.end_file = Some((4, -13));
        f.events.push_front((7, 0, 0, None));
    });
    assert!(matches!(
        player.events(Instant::now()),
        Err(Error::Api {
            operation: "media decode/open",
            code: -13
        })
    ));
    fixture(|f| {
        f.end_file = Some((0, 0));
        f.events.push_front((7, 0, 0, None));
    });
    assert!(matches!(
        player.events(Instant::now()),
        Err(Error::Ended { reason: 0 })
    ));
    fixture(|f| {
        f.end_file = None;
        f.events.push_front((7, 0, 0, None));
    });
    assert!(matches!(
        player.events(Instant::now()),
        Err(Error::Ended { reason: -1 })
    ));
}
