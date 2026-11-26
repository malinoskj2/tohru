mod common;

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tohru_desktop::shortcuts::portal::{
    Action, BUS_NAME, GLOBAL, MATCH_RULES, PATH, PORTAL, PortalOwner, PortalState, REQUEST, SESSION,
};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

type Options = HashMap<String, OwnedValue>;

/// What the fake portal saw and how it answers the next request.
#[derive(Default)]
struct Script {
    version: u32,
    response: u32,
    delay: Duration,
    grants: Vec<&'static str>,
    previous: Vec<&'static str>,
    preferred: HashMap<String, String>,
    session_closed: Vec<String>,
    request_closed: Vec<String>,
    binds: usize,
    configures: usize,
}

fn records(actions: &[&str]) -> Vec<(String, HashMap<String, Value<'static>>)> {
    actions
        .iter()
        .map(|action| {
            let properties = HashMap::from([
                ("description".to_owned(), Value::from(action.to_string())),
                (
                    "trigger_description".to_owned(),
                    Value::from(format!("Backend {action}")),
                ),
            ]);
            (action.to_string(), properties)
        })
        .collect()
}

fn token(options: &Options, key: &str) -> String {
    options
        .get(key)
        .and_then(|value| String::try_from(value.try_clone().ok()?).ok())
        .unwrap_or_default()
}

struct Answer {
    options: Options,
    results: HashMap<String, Value<'static>>,
    code: u32,
    delay: Duration,
}

/// The `Response` a portal request will receive, or None for an unknown method.
fn answer(
    member: &str,
    message: &zbus::Message,
    sender: &str,
    script: &mut Script,
) -> Option<Answer> {
    let body = message.body();
    let mut results: HashMap<String, Value<'static>> = HashMap::new();
    let (mut code, mut delay) = (0_u32, Duration::from_millis(1));
    let options = match member {
        "CreateSession" => {
            let (options,): (Options,) = body.deserialize().unwrap();
            let session = format!(
                "{PATH}/session/{sender}/{}",
                token(&options, "session_handle_token")
            );
            results.insert("session_handle".to_owned(), Value::from(session));
            options
        }
        "ListShortcuts" => {
            let (_, options): (OwnedObjectPath, Options) = body.deserialize().unwrap();
            results.insert(
                "shortcuts".to_owned(),
                Value::from(records(&script.previous)),
            );
            options
        }
        "BindShortcuts" => {
            let (_, choices, _, options): (
                OwnedObjectPath,
                Vec<(String, Options)>,
                String,
                Options,
            ) = body.deserialize().unwrap();
            script.binds += 1;
            script.preferred = choices
                .iter()
                .filter_map(|(action, properties)| {
                    let trigger = token(properties, "preferred_trigger");
                    (!trigger.is_empty()).then(|| (action.clone(), trigger))
                })
                .collect();
            results.insert("shortcuts".to_owned(), Value::from(records(&script.grants)));
            code = script.response;
            delay = script.delay;
            options
        }
        _ => return None,
    };
    Some(Answer {
        options,
        results,
        code,
        delay,
    })
}

/// Answers `GlobalShortcuts` calls on a private bus the way xdg-desktop-portal does.
fn fake_portal(server: Connection, script: Arc<Mutex<Script>>) {
    // Subscribed before returning: zbus only queues calls for streams that already exist.
    let messages = MessageIterator::from(&server);
    std::thread::spawn(move || {
        for message in messages {
            let Ok(message) = message else { break };
            if message.message_type() != MessageType::MethodCall {
                continue;
            }
            let header = message.header();
            let interface = header
                .interface()
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default();
            let member = header
                .member()
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default();
            let path = header
                .path()
                .map(|path| path.as_str().to_owned())
                .unwrap_or_default();
            let sender = header
                .sender()
                .map(|name| name.trim_start_matches(':').replace('.', "_"))
                .unwrap_or_default();
            let mut script = script.lock().unwrap();
            match (interface.as_str(), member.as_str()) {
                ("org.freedesktop.DBus.Properties", "Get") => {
                    server.reply(&header, &Value::from(script.version)).unwrap();
                    continue;
                }
                (SESSION, "Close") => script.session_closed.push(path),
                (REQUEST, "Close") => script.request_closed.push(path),
                (GLOBAL, "ConfigureShortcuts") => script.configures += 1,
                _ => {}
            }
            if interface != GLOBAL || member == "ConfigureShortcuts" {
                server.reply(&header, &()).unwrap();
                continue;
            }
            let Some(Answer {
                options,
                results,
                code,
                delay,
            }) = answer(&member, &message, &sender, &mut script)
            else {
                server
                    .reply_dbus_error(
                        &header,
                        zbus::fdo::Error::UnknownMethod("unknown".to_owned()),
                    )
                    .unwrap();
                continue;
            };
            let request = format!(
                "{PATH}/request/{sender}/{}",
                token(&options, "handle_token")
            );
            server
                .reply(
                    &header,
                    &OwnedObjectPath::try_from(request.clone()).unwrap(),
                )
                .unwrap();
            let signaller = server.clone();
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                let _ = signaller.emit_signal(
                    None::<&str>,
                    request.as_str(),
                    REQUEST,
                    "Response",
                    &(code, results),
                );
            });
        }
    });
}

struct Bus {
    daemon: Child,
    address: String,
    _directory: tempfile::TempDir,
}

const BUS_CONFIG: &str = r#"<busconfig>
  <type>session</type>
  <listen>unix:dir=DIRECTORY</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

impl Bus {
    /// A private bus daemon with its own config; None when dbus-daemon is not installed.
    fn start() -> Option<Self> {
        let daemon = common::which("dbus-daemon")?;
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("bus.conf");
        std::fs::write(
            &config,
            BUS_CONFIG.replace("DIRECTORY", &directory.path().display().to_string()),
        )
        .unwrap();
        let mut daemon = Command::new(daemon)
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(!address.trim().is_empty(), "private bus did not start");
        Some(Self {
            daemon,
            address: address.trim().to_owned(),
            _directory: directory,
        })
    }

    fn connect(&self) -> Connection {
        zbus::blocking::connection::Builder::address(self.address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(2))
            .build()
            .unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

struct Harness {
    _bus: Bus,
    server: Connection,
    script: Arc<Mutex<Script>>,
    owner: Arc<PortalOwner>,
    actions: Arc<Mutex<Vec<Action>>>,
}

impl Harness {
    fn start(configure: impl FnOnce(&mut Script)) -> Option<Self> {
        let bus = Bus::start()?;
        let server = bus.connect();
        server.request_name(PORTAL).unwrap();
        let mut script = Script {
            version: 2,
            grants: vec!["launcher", "session", "assistant"],
            delay: Duration::from_millis(10),
            ..Script::default()
        };
        configure(&mut script);
        let script = Arc::new(Mutex::new(script));
        fake_portal(server.clone(), Arc::clone(&script));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&actions);
        let runner = Box::new(move |action| {
            recorded.lock().unwrap().push(action);
            true
        });
        let preferred = BTreeMap::from([(Action::Launcher, "<Super>space".to_owned())]);
        let owner = PortalOwner::new(bus.connect(), preferred, runner, Duration::from_millis(300));
        owner.listen();
        owner.subscribe().unwrap();
        owner.open_session().unwrap();
        Some(Self {
            _bus: bus,
            server,
            script,
            owner,
            actions,
        })
    }

    fn signal(&self, member: &str, session: &str, action: &str) {
        let body = (
            OwnedObjectPath::try_from(session).unwrap(),
            action,
            1_u64,
            HashMap::<String, Value<'_>>::new(),
        );
        self.server
            .emit_signal(None::<&str>, PATH, GLOBAL, member, &body)
            .unwrap();
        settle();
    }

    fn actions(&self) -> Vec<Action> {
        self.actions.lock().unwrap().clone()
    }
}

fn settle() {
    std::thread::sleep(Duration::from_millis(60));
}

macro_rules! harness {
    ($configure:expr) => {
        match Harness::start($configure) {
            Some(harness) => harness,
            None => {
                eprintln!("dbus-daemon unavailable; no private portal test");
                return;
            }
        }
    };
}

#[test]
fn no_bind_until_explicit_setup_then_triggers_and_configure() {
    let harness = harness!(|_| {});
    assert_eq!(harness.script.lock().unwrap().binds, 0);
    assert_eq!(harness.owner.state(), PortalState::NotConfigured);
    harness.owner.setup();
    assert_eq!(harness.owner.state(), PortalState::Granted);
    assert_eq!(
        harness.script.lock().unwrap().preferred["launcher"],
        "<Super>space"
    );
    let snapshot = harness.owner.snapshot();
    assert_eq!(snapshot["bindings"][0]["shortcut"], "Backend launcher");
    assert_eq!(snapshot["configure_available"], true);
    harness.owner.configure().unwrap();
    assert_eq!(harness.script.lock().unwrap().configures, 1);
    assert_eq!(harness.owner.state(), PortalState::ConfigurationRequested);
}

#[test]
fn denied_cancelled_partial_without_a_prompt_loop() {
    let harness = harness!(|script| script.response = 2);
    harness.owner.setup();
    assert_eq!(harness.owner.state(), PortalState::Denied);
    assert!(harness.owner.bound_actions().is_empty());
    harness.script.lock().unwrap().response = 1;
    harness.owner.setup();
    assert_eq!(harness.owner.state(), PortalState::Cancelled);
    {
        let mut script = harness.script.lock().unwrap();
        script.response = 0;
        script.grants = vec!["launcher"];
    }
    harness.owner.setup();
    assert_eq!(harness.owner.state(), PortalState::Partial);
    assert_eq!(harness.owner.bound_actions(), ["launcher"]);
    settle();
    assert_eq!(harness.script.lock().unwrap().binds, 3);
}

#[test]
fn delayed_response_times_out_and_closes_the_request() {
    let harness = harness!(|script| script.delay = Duration::from_secs(1));
    harness.owner.setup();
    assert_eq!(harness.owner.state(), PortalState::RequestTimeout);
    assert!(!harness.script.lock().unwrap().request_closed.is_empty());
    assert_eq!(harness.owner.open_requests(), 0);
    assert!(!harness.owner.pending());
}

#[test]
fn unknown_session_action_and_ungranted_actions_never_dispatch() {
    let harness = harness!(|script| script.grants = vec!["launcher"]);
    harness.owner.setup();
    let session = harness.owner.session();
    for (path, action) in [
        ("/wrong", "launcher"),
        (session.as_str(), "unknown"),
        (session.as_str(), "assistant"),
    ] {
        harness.signal("Activated", path, action);
    }
    assert!(harness.actions().is_empty());
    harness.signal("Activated", &session, "launcher");
    harness.signal("Activated", &session, "launcher");
    assert_eq!(harness.actions(), [Action::Launcher]);
    harness.signal("Deactivated", &session, "launcher");
    harness.signal("Activated", &session, "launcher");
    assert_eq!(harness.actions(), [Action::Launcher, Action::Launcher]);
}

#[test]
fn changes_revocation_close_and_portal_loss() {
    let harness = harness!(|_| {});
    harness.owner.setup();
    let session = OwnedObjectPath::try_from(harness.owner.session()).unwrap();
    let changed = |actions: &[&str]| {
        harness
            .server
            .emit_signal(
                None::<&str>,
                PATH,
                GLOBAL,
                "ShortcutsChanged",
                &(session.clone(), records(actions)),
            )
            .unwrap();
        settle();
    };
    changed(&["session"]);
    assert_eq!(harness.owner.bound_actions(), ["session"]);
    changed(&[]);
    assert_eq!(harness.owner.state(), PortalState::Revoked);

    harness.owner.setup();
    let session = harness.owner.session();
    harness
        .server
        .emit_signal(None::<&str>, session.as_str(), SESSION, "Closed", &())
        .unwrap();
    settle();
    assert_eq!(harness.owner.state(), PortalState::Revoked);
    assert!(harness.owner.bound_actions().is_empty());

    harness.owner.setup();
    let session = harness.owner.session();
    harness.owner.close_session();
    assert!(
        harness
            .script
            .lock()
            .unwrap()
            .session_closed
            .contains(&session)
    );

    harness.server.release_name(PORTAL).unwrap();
    settle();
    assert_eq!(harness.owner.state(), PortalState::PortalUnavailable);
    assert!(harness.owner.bound_actions().is_empty());
}

#[test]
fn version_one_cannot_configure_and_previous_grants_stay_inactive() {
    let harness = harness!(|script| {
        script.version = 1;
        script.previous = vec!["launcher"];
    });
    assert_eq!(harness.owner.state(), PortalState::PreviousGrants);
    assert!(!harness.owner.registered());
    harness.signal("Activated", &harness.owner.session(), "launcher");
    assert!(harness.actions().is_empty());
    assert!(harness.owner.configure().is_err());
    harness.owner.setup();
    assert_eq!(harness.owner.snapshot()["configure_available"], false);
}

#[test]
fn portal_restart_clears_grants_and_never_prompts_by_itself() {
    let harness = harness!(|_| {});
    harness.owner.setup();
    assert!(harness.owner.registered());
    let binds = harness.script.lock().unwrap().binds;
    harness.server.release_name(PORTAL).unwrap();
    settle();
    assert!(!harness.owner.registered());
    assert!(harness.owner.bound_actions().is_empty());
    harness.server.request_name(PORTAL).unwrap();
    settle();
    assert_eq!(harness.owner.state(), PortalState::Connecting);
    harness.owner.connect_portal();
    assert_eq!(harness.script.lock().unwrap().binds, binds);
    assert!(!harness.owner.registered());
    harness.owner.setup();
    assert_eq!(harness.script.lock().unwrap().binds, binds + 1);
}

#[test]
fn a_second_owner_does_not_queue_for_the_bus_name() {
    let Some(bus) = Bus::start() else { return };
    let flags = zbus::fdo::RequestNameFlags::DoNotQueue.into();
    let (owner, rival) = (bus.connect(), bus.connect());
    let first = owner.request_name_with_flags(BUS_NAME, flags).unwrap();
    let second = rival.request_name_with_flags(BUS_NAME, flags);
    assert_eq!(first, zbus::fdo::RequestNameReply::PrimaryOwner);
    assert!(matches!(second, Err(zbus::Error::NameTaken)));
}

#[test]
fn disabled_and_compositor_modes_never_contact_a_portal() {
    for mode in ["disabled", "compositor"] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("tohru")).unwrap();
        std::fs::write(
            directory.path().join("tohru/config.toml"),
            format!("[shortcuts]\nmode=\"{mode}\"\n"),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_tohru-shortcuts"))
            .env("XDG_CONFIG_HOME", directory.path())
            .env("TOHRU_MANAGED_CONFIG", "")
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("disabled"));
    }
}

#[test]
fn match_rules_cover_owner_changes_and_portal_signals() {
    assert!(
        MATCH_RULES
            .iter()
            .any(|rule| rule.contains("NameOwnerChanged"))
    );
    assert!(
        MATCH_RULES
            .iter()
            .any(|rule| rule.contains("sender='org.freedesktop.portal.Desktop'"))
    );
}

#[test]
fn service_answers_status_and_setup_over_its_private_socket() {
    let Some(bus) = Bus::start() else { return };
    let server = bus.connect();
    server.request_name(PORTAL).unwrap();
    let script = Script {
        version: 2,
        grants: vec!["launcher", "session", "assistant"],
        ..Script::default()
    };
    let script = Arc::new(Mutex::new(script));
    fake_portal(server.clone(), Arc::clone(&script));

    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    std::fs::create_dir_all(root.path().join("config/tohru")).unwrap();
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::write(
        root.path().join("config/tohru/config.toml"),
        "[shortcuts]\nmode=\"portal\"\nsession_trigger=\"<Super>Escape\"\n",
    )
    .unwrap();
    let environment = |command: &mut Command| {
        command
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("WAYLAND_DISPLAY", "private-test")
            .env("XDG_SESSION_ID", "private-test")
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .env("TOHRU_MANAGED_CONFIG", "")
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address);
    };
    let mut service = Command::new(env!("CARGO_BIN_EXE_tohru-shortcuts"));
    environment(&mut service);
    let mut service = Service(service.stderr(Stdio::piped()).spawn().unwrap());
    let status = |operation: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tohru-portability"));
        environment(&mut command);
        let output = command.args(["shortcuts", operation]).output().unwrap();
        serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
    };

    let first =
        common::wait_for(|| status("status").filter(|report| report["state"] == "not-configured"));
    assert_eq!(first["owner"], "Tohru actions via desktop portal");
    assert_eq!(first["setup_available"], true);
    assert_eq!(script.lock().unwrap().binds, 0);
    status("setup").unwrap();
    let granted =
        common::wait_for(|| status("status").filter(|report| report["state"] == "granted"));
    assert_eq!(granted["bindings"].as_array().unwrap().len(), 3);
    assert_eq!(script.lock().unwrap().preferred["session"], "<Super>Escape");

    let mut duplicate = Command::new(env!("CARGO_BIN_EXE_tohru-shortcuts"));
    environment(&mut duplicate);
    let duplicate = duplicate.output().unwrap();
    assert_eq!(duplicate.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&duplicate.stderr)
            .contains("another Tohru portal owner is running")
    );

    let pid = rustix::process::Pid::from_raw(i32::try_from(service.0.id()).unwrap()).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
    assert!(service.0.wait().unwrap().success());
    let socket =
        common::session_dir(&runtime, "private-test", "private-test").join("shortcuts.sock");
    assert!(!socket.exists());
    assert!(!script.lock().unwrap().session_closed.is_empty());
}

struct Service(Child);

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
