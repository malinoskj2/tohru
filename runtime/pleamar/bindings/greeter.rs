//! Tohru's greeter authority (`TOHRU_GREETER=1`).
//!
//! Mirrors `tohru_lock`'s password isolation for pre-session login over `greetd`
//! (`GREETD_SOCK`): secret input stays in native memory, is transmitted directly
//! to `greetd` on a worker thread, and is volatile-zeroed regardless of outcome.
//! Luau sees only masked bullet counts, prompts, error text, and numeric state.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[cfg(feature = "luau")]
use mlua::{Lua, Table};
use tohru_shell::greeter::GreeterSnapshot;
use tohru_shell::greeter::discovery::{SessionEntry, build_session_launch};
use tohru_shell::greeter::ipc::{AuthMessageType, Client, ErrorType, Response};
use tohru_shell::greeter::state::{LastState, save as save_state};

use crate::scene::ToRender;

/// `greeter_auth`, as the scene reads it.
pub const IDLE: u8 = 0;
pub const CHECKING: u8 = 1;
pub const ACCEPTED: u8 = 2;
pub const REJECTED: u8 = 3;
pub const UNAVAILABLE: u8 = 4;
pub const PROMPT_VISIBLE: u8 = 5;
pub const PROMPT_INFO: u8 = 6;
pub const PROMPT_SECRET: u8 = 7;

/// Maximum time after `start_session` succeeds before the greeter exits anyway
/// so `greetd` is never left waiting for the greeter session to end.
pub const EXIT_GRACE: Duration = Duration::from_millis(650);

static RENDER_TX: OnceLock<Mutex<Option<Sender<ToRender>>>> = OnceLock::new();
static CHECKING_NOW: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<GreeterState>> = OnceLock::new();

struct GreeterState {
    snapshot: GreeterSnapshot,
    selected_user: String,
    selected_session: String,
    pending_client: Option<Client>,
    wallpaper_light: Option<bool>,
}

impl GreeterState {
    fn load() -> Self {
        let snapshot = GreeterSnapshot::load_live();
        let wallpaper_light = snapshot
            .config
            .wallpaper
            .as_deref()
            .and_then(sample_wallpaper_light);
        let selected_user = snapshot.selected_user.clone();
        let selected_session = snapshot.selected_session.clone();
        Self {
            snapshot,
            selected_user,
            selected_session,
            pending_client: None,
            wallpaper_light,
        }
    }

    fn current_session(&self) -> Option<SessionEntry> {
        self.snapshot
            .sessions
            .iter()
            .find(|s| s.id == self.selected_session)
            .or_else(|| self.snapshot.sessions.first())
            .cloned()
    }
}

fn state() -> &'static Mutex<GreeterState> {
    STATE.get_or_init(|| Mutex::new(GreeterState::load()))
}

/// Whether this process is running as the dedicated `greetd` greeter (`TOHRU_GREETER=1`).
pub fn greeter() -> bool {
    static GREETER: OnceLock<bool> = OnceLock::new();
    *GREETER.get_or_init(|| std::env::var("TOHRU_GREETER").as_deref() == Ok("1"))
}

/// Record the render channel at startup so Luau-initiated calls (`submit_visible`, `cancel`)
/// can report state updates cleanly.
pub fn set_render_tx(tx: Sender<ToRender>) {
    let slot = RENDER_TX.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap() = Some(tx);
}

fn render_tx() -> Option<Sender<ToRender>> {
    RENDER_TX.get()?.lock().unwrap().clone()
}

/// Overwrites a buffer that held a password before it is freed.
pub fn zero(text: &mut String) {
    // SAFETY: zero bytes are valid UTF-8.
    unsafe { text.as_bytes_mut() }
        .iter_mut()
        .for_each(|b| unsafe { std::ptr::write_volatile(b, 0) });
    text.clear();
}

/// Bullet mask sent to Luau while typing into a secret field.
pub fn masked(text: &str) -> String {
    "•".repeat(text.chars().count())
}

fn report_state(tx: &Sender<ToRender>, code: u8) {
    let _ = tx.send(ToRender::ExternalFact(
        crate::scene::intern("greeter_auth"),
        code.to_string(),
    ));
}

fn report_text(tx: &Sender<ToRender>, name: &'static str, value: String) {
    let _ = tx.send(ToRender::Text(crate::scene::intern(name), value));
}

/// Sample five relative points on a static wallpaper image to decide light/dark ink (`pk`).
fn sample_wallpaper_light(path: &Path) -> Option<bool> {
    let img = image::open(path).ok()?.to_rgb8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let points = [
        (0.08, 0.13),
        (0.33, 0.11),
        (0.58, 0.15),
        (0.16, 0.48),
        (0.47, 0.56),
    ];
    let mut sum = 0.0f64;
    for (fx, fy) in points {
        let x = ((fx * w as f64) as u32).min(w - 1);
        let y = ((fy * h as f64) as u32).min(h - 1);
        let p = img.get_pixel(x, y).0;
        sum += (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]))
            / 255.0;
    }
    Some(sum / points.len() as f64 > 0.55)
}

enum AuthInput {
    Secret(String),
    Visible(Option<String>),
}

/// Called directly from `render.rs` when the user submits a secret input while `TOHRU_GREETER=1`.
pub fn submit_secret(mut password: String, to_render: Sender<ToRender>) {
    if STARTED.load(SeqCst) || password.is_empty() || CHECKING_NOW.swap(true, SeqCst) {
        zero(&mut password);
        if !STARTED.load(SeqCst) {
            report_state(&to_render, IDLE);
        }
        return;
    }
    start_worker(AuthInput::Secret(password), to_render);
}

fn submit_visible(response: Option<String>) {
    let Some(to_render) = render_tx() else { return };
    if STARTED.load(SeqCst) || CHECKING_NOW.swap(true, SeqCst) {
        return;
    }
    start_worker(AuthInput::Visible(response), to_render);
}

fn start_worker(input: AuthInput, to_render: Sender<ToRender>) {
    report_state(&to_render, CHECKING);
    let _ = std::thread::Builder::new()
        .name("greeter-auth".into())
        .spawn(move || {
            run_auth_round(input, &to_render, || Client::connect_env());
            CHECKING_NOW.store(false, SeqCst);
        });
}

fn run_auth_round(
    mut input: AuthInput,
    to_render: &Sender<ToRender>,
    connect: impl FnOnce() -> Result<Client, tohru_shell::greeter::ipc::IpcError>,
) {
    let (username, session, existing_client) = {
        let mut guard = state().lock().unwrap();
        (
            guard.selected_user.clone(),
            guard.current_session(),
            guard.pending_client.take(),
        )
    };

    if username.trim().is_empty() {
        zero_input(&mut input);
        report_text(to_render, "greeter_error", "Enter a username".into());
        report_state(to_render, REJECTED);
        return;
    }

    let mut client = match existing_client {
        Some(mut client) => {
            let reply = send_answer(&mut client, &mut input);
            return handle_response(client, reply, &username, session.as_ref(), to_render);
        }
        None => match connect() {
            Ok(client) => client,
            Err(err) => {
                zero_input(&mut input);
                eprintln!("greeter · cannot reach greetd: {err}");
                report_text(
                    to_render,
                    "greeter_error",
                    "Login service unavailable".into(),
                );
                report_state(to_render, UNAVAILABLE);
                return;
            }
        },
    };

    match client.create_session(username.clone()) {
        Ok(Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message,
        }) => match &input {
            AuthInput::Secret(pw) if !pw.is_empty() => {
                let reply = send_answer(&mut client, &mut input);
                handle_response(client, reply, &username, session.as_ref(), to_render);
            }
            _ => {
                zero_input(&mut input);
                state().lock().unwrap().pending_client = Some(client);
                report_text(to_render, "greeter_prompt", auth_message);
                report_state(to_render, PROMPT_SECRET);
            }
        },
        Ok(other) => {
            zero_input(&mut input);
            handle_response(client, Ok(other), &username, session.as_ref(), to_render);
        }
        Err(err) => {
            zero_input(&mut input);
            eprintln!("greeter · create_session failed: {err}");
            report_text(
                to_render,
                "greeter_error",
                "Login service unavailable".into(),
            );
            report_state(to_render, UNAVAILABLE);
        }
    }
}

fn send_answer(
    client: &mut Client,
    input: &mut AuthInput,
) -> Result<Response, tohru_shell::greeter::ipc::IpcError> {
    match input {
        AuthInput::Secret(pw) => {
            let reply = client.post_auth_message_response(Some(pw.clone()));
            zero(pw);
            reply
        }
        AuthInput::Visible(answer) => client.post_auth_message_response(answer.take()),
    }
}

fn zero_input(input: &mut AuthInput) {
    if let AuthInput::Secret(pw) = input {
        zero(pw);
    }
}

fn handle_response(
    mut client: Client,
    reply: Result<Response, tohru_shell::greeter::ipc::IpcError>,
    username: &str,
    session: Option<&SessionEntry>,
    to_render: &Sender<ToRender>,
) {
    match reply {
        Ok(Response::Success) => {
            let Some(session) = session else {
                let _ = client.cancel_session();
                report_text(
                    to_render,
                    "greeter_error",
                    "No desktop session available".into(),
                );
                report_state(to_render, UNAVAILABLE);
                return;
            };
            let Some(launch) = build_session_launch(session) else {
                let _ = client.cancel_session();
                report_text(to_render, "greeter_error", "Invalid session command".into());
                report_state(to_render, UNAVAILABLE);
                return;
            };
            match client.start_session(launch.cmd, launch.env) {
                Ok(Response::Success) => {
                    if let Some(dir) = std::env::var_os("TOHRU_GREETER_STATE_DIR")
                        .filter(|v| !v.is_empty())
                        .map(std::path::PathBuf::from)
                    {
                        let _ = save_state(
                            &dir,
                            &LastState {
                                last_user: Some(username.to_owned()),
                                last_session: Some(session.id.clone()),
                            },
                        );
                    }
                    STARTED.store(true, SeqCst);
                    report_text(to_render, "greeter_error", String::new());
                    report_state(to_render, ACCEPTED);
                    std::thread::spawn(|| {
                        std::thread::sleep(EXIT_GRACE);
                        crate::quit_from_scene();
                    });
                }
                Ok(Response::Error { description, .. }) => {
                    let _ = client.cancel_session();
                    report_text(to_render, "greeter_error", description);
                    report_state(to_render, UNAVAILABLE);
                }
                Ok(Response::AuthMessage { auth_message, .. }) => {
                    let _ = client.cancel_session();
                    report_text(to_render, "greeter_error", auth_message);
                    report_state(to_render, UNAVAILABLE);
                }
                Err(err) => {
                    eprintln!("greeter · start_session failed: {err}");
                    report_text(to_render, "greeter_error", "Could not start session".into());
                    report_state(to_render, UNAVAILABLE);
                }
            }
        }
        Ok(Response::AuthMessage {
            auth_message_type,
            auth_message,
        }) => {
            state().lock().unwrap().pending_client = Some(client);
            report_text(to_render, "greeter_prompt", auth_message);
            let code = match auth_message_type {
                AuthMessageType::Secret => PROMPT_SECRET,
                AuthMessageType::Visible => PROMPT_VISIBLE,
                AuthMessageType::Info | AuthMessageType::Error => PROMPT_INFO,
            };
            report_state(to_render, code);
        }
        Ok(Response::Error {
            error_type,
            description,
        }) => {
            let _ = client.cancel_session();
            let message = if description.trim().is_empty() {
                "Incorrect password".into()
            } else {
                description
            };
            report_text(to_render, "greeter_error", message);
            report_state(
                to_render,
                match error_type {
                    ErrorType::AuthError => REJECTED,
                    ErrorType::Error => UNAVAILABLE,
                },
            );
        }
        Err(err) => {
            eprintln!("greeter · IPC failed: {err}");
            report_text(
                to_render,
                "greeter_error",
                "Login service unavailable".into(),
            );
            report_state(to_render, UNAVAILABLE);
        }
    }
}

fn cancel_pending() {
    let client = state().lock().unwrap().pending_client.take();
    if let Some(mut client) = client {
        let _ = std::thread::Builder::new()
            .name("greeter-cancel".into())
            .spawn(move || {
                let _ = client.cancel_session();
            });
    }
}

/// Install the `tohru_greeter` Luau global (only when `TOHRU_GREETER=1`).
#[cfg(feature = "luau")]
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let api = lua.create_table()?;

    api.set(
        "init",
        lua.create_function(|lua, ()| {
            let guard = state().lock().unwrap();
            let out = lua.create_table()?;

            let cfg = lua.create_table()?;
            cfg.set("style", guard.snapshot.config.style.name())?;
            cfg.set(
                "wallpaper",
                guard
                    .snapshot
                    .config
                    .wallpaper
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
            )?;
            cfg.set(
                "preferred_output",
                guard.snapshot.config.preferred_output.clone(),
            )?;
            cfg.set(
                "allow_power_actions",
                guard.snapshot.config.allow_power_actions,
            )?;
            cfg.set(
                "warnings",
                lua.create_sequence_from(guard.snapshot.config.warnings.clone())?,
            )?;
            out.set("config", cfg)?;

            let users = lua.create_table()?;
            for (index, user) in guard.snapshot.users.iter().enumerate() {
                let row: Table = lua.create_table()?;
                row.set("name", user.name.as_str())?;
                row.set("display_name", user.display_name.as_str())?;
                row.set("home", user.home.to_string_lossy().into_owned())?;
                row.set(
                    "avatar",
                    user.avatar
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned()),
                )?;
                users.raw_set(index + 1, row)?;
            }
            out.set("users", users)?;

            let sessions = lua.create_table()?;
            for (index, session) in guard.snapshot.sessions.iter().enumerate() {
                let row: Table = lua.create_table()?;
                row.set("id", session.id.as_str())?;
                row.set("name", session.name.as_str())?;
                row.set("kind", session.kind.as_str())?;
                sessions.raw_set(index + 1, row)?;
            }
            out.set("sessions", sessions)?;

            out.set("selected_user", guard.selected_user.as_str())?;
            out.set("selected_session", guard.selected_session.as_str())?;
            out.set("host", guard.snapshot.hostname.as_str())?;
            out.set("wallpaper_light", guard.wallpaper_light)?;
            Ok(out)
        })?,
    )?;

    api.set(
        "select_user",
        lua.create_function(|_, username: String| {
            let trimmed = username.trim().to_owned();
            let changed = {
                let mut guard = state().lock().unwrap();
                let changed = guard.selected_user != trimmed;
                guard.selected_user = trimmed;
                changed
            };
            if changed {
                cancel_pending();
            }
            Ok(())
        })?,
    )?;

    api.set(
        "select_session",
        lua.create_function(|_, id: String| {
            let mut guard = state().lock().unwrap();
            guard.selected_session = id.trim().to_owned();
            Ok(())
        })?,
    )?;

    api.set(
        "submit_visible",
        lua.create_function(|_, value: Option<String>| {
            submit_visible(value);
            Ok(())
        })?,
    )?;

    api.set(
        "cancel",
        lua.create_function(|_, ()| {
            cancel_pending();
            Ok(())
        })?,
    )?;

    api.set(
        "finish",
        lua.create_function(|_, ()| {
            if STARTED.load(SeqCst) {
                crate::quit_from_scene();
            }
            Ok(())
        })?,
    )?;

    lua.globals().set("tohru_greeter", api)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use tohru_shell::greeter::config::GreeterConfig;
    use tohru_shell::greeter::discovery::SessionKind;
    use tohru_shell::greeter::ipc::{read_frame, write_frame};

    #[test]
    fn tohru_greeter_zero_and_mask() {
        let mut s = String::from("hunter2é");
        assert_eq!(masked(&s), "••••••••");
        zero(&mut s);
        assert!(s.is_empty());
    }

    #[test]
    fn tohru_greeter_auth_round_handles_rejection_and_success() {
        let _ = STATE.set(Mutex::new(GreeterState {
            snapshot: GreeterSnapshot {
                config: GreeterConfig::default(),
                users: Vec::new(),
                sessions: vec![SessionEntry {
                    id: "hyprland".into(),
                    name: "Hyprland".into(),
                    exec: "start-hyprland".into(),
                    desktop_names: vec!["Hyprland".into()],
                    kind: SessionKind::Wayland,
                }],
                selected_user: "jesse".into(),
                selected_session: "hyprland".into(),
                hostname: "home".into(),
            },
            selected_user: "jesse".into(),
            selected_session: "hyprland".into(),
            pending_client: None,
            wallpaper_light: None,
        }));
        STARTED.store(false, SeqCst);

        let (c1, mut s1) = UnixStream::pair().unwrap();
        let server1 = std::thread::spawn(move || {
            let _ = read_frame(&mut s1).unwrap();
            write_frame(
                &mut s1,
                br#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#,
            )
            .unwrap();
            let _ = read_frame(&mut s1).unwrap();
            write_frame(
                &mut s1,
                br#"{"type":"error","error_type":"auth_error","description":"Bad password"}"#,
            )
            .unwrap();
            let cancel = read_frame(&mut s1).unwrap();
            assert!(String::from_utf8_lossy(&cancel).contains("cancel_session"));
            write_frame(&mut s1, br#"{"type":"success"}"#).unwrap();
        });

        let (tx, rx) = std::sync::mpsc::channel();
        run_auth_round(AuthInput::Secret("wrong".into()), &tx, || {
            Client::from_stream(c1)
        });
        server1.join().unwrap();

        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.iter().any(
            |e| matches!(e, ToRender::ExternalFact(n, v) if *n == "greeter_auth" && v == "3")
        ));
        assert!(!STARTED.load(SeqCst));
    }
}
