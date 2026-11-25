mod common;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tohru_desktop::discord::{
    Body, Broker, Client, HttpRequest, HttpResponse, Loader, Method, Sources, Transport,
    TransportError,
};

const DISCORD: &str = env!("CARGO_BIN_EXE_tohru-discord");
const SECRET: &str = "cli-secret-DO-NOT-PRINT";

/// Answers channel metadata for channel 2 and echoes posts as message 501.
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<(Method, String, Option<String>)>>,
}

impl Transport for Fake {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.seen.lock().unwrap().push((
            request.method,
            request.path.clone(),
            request.body.clone(),
        ));
        let body = match (request.method, request.path.as_str()) {
            (Method::Get, "/channels/2") => json!({"id": "2", "guild_id": "1", "type": 0}),
            (Method::Post, _) => json!({"id": "501"}),
            (Method::Get, _) => json!([{"id": "400", "author": {"id": "9"}, "content": "héllo",
                                        "timestamp": "2026-11-22T00:00:00Z"}]),
        };
        Ok(HttpResponse {
            status: 200,
            remaining: None,
            reset_after: None,
            body: Body::Complete(body.to_string().into_bytes()),
        })
    }
}

fn private(root: &Path, command: &mut Command) -> Output {
    command
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

fn configure(root: &Path, extra: &str) -> Sources {
    let token = root.join("token");
    std::fs::write(&token, SECRET).unwrap();
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    let config = root.join("config/tohru/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        format!(
            "[assistant.discord]\nenabled = true\nread = true\npost = true\nguild_id = \"1\"\n\
             channels = \"2\"\nallowed_user_id = \"9\"\ntoken_file = \"{}\"\n{extra}",
            token.display()
        ),
    )
    .unwrap();
    Sources::new(config, None)
}

#[test]
fn status_and_receiver_report_disabled_setup_without_network() {
    let root = tempfile::tempdir().unwrap();
    let output = private(root.path(), Command::new(DISCORD).arg("status"));
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"enabled\": false, \"status\": \"disabled\"}\n"
    );
    let output = private(root.path(), Command::new(DISCORD).arg("poll"));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        json!({"event": "error", "message": "Discord is disabled."})
    );
    let output = private(
        root.path(),
        Command::new(DISCORD).args(["poll", "--settings", "{\"surprise\": 1}"]),
    );
    assert_eq!(
        stdout(&output)["message"],
        json!("Invalid Discord settings; check config.toml.")
    );
    let output = private(root.path(), Command::new(DISCORD).arg("reset-receiver"));
    assert!(output.status.success());
    assert_eq!(stdout(&output), json!({"event": "reset"}));
}

#[test]
fn status_reports_configured_setup_without_the_token() {
    let root = tempfile::tempdir().unwrap();
    configure(root.path(), "");
    let output = private(root.path(), Command::new(DISCORD).arg("status"));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains(SECRET));
    let status: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        status["status"],
        json!("configured (credentials not verified)")
    );
    assert_eq!(status["channels"], json!(["2"]));
}

#[test]
fn tools_outside_a_turn_explain_the_boundary() {
    let root = tempfile::tempdir().unwrap();
    for args in [&["read", "2"][..], &["post", "2"]] {
        let output = private(root.path(), Command::new(DISCORD).args(args));
        assert_eq!(output.status.code(), Some(1));
        assert!(
            stdout(&output)["error"]
                .as_str()
                .unwrap()
                .contains("require an active Tohru assistant turn")
        );
    }
    let output = private(
        root.path(),
        Command::new(DISCORD)
            .args(["read", "2", "--before", "1", "--after", "2"])
            .env("TOHRU_DISCORD_SOCKET", "/nonexistent"),
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn shell_tool_reaches_the_broker_without_the_token() {
    let root = tempfile::tempdir().unwrap();
    let sources = configure(root.path(), "");
    let fake = Arc::new(Fake::default());
    let client = Client::with(
        Loader::Disk(sources),
        root.path().join("state/tohru/discord"),
        Arc::clone(&fake) as Arc<dyn Transport>,
    );
    let mut broker = Broker::start(client);
    let socket = broker.socket().unwrap().to_owned();
    assert!(broker.note().contains("tohru-discord"));

    let mut child = Command::new(DISCORD)
        .args(["post", "2"])
        .env_clear()
        .env("TOHRU_DISCORD_SOCKET", &socket)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all("Hi ✓".as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        json!({"sent": true, "id": "501", "channel": "2"})
    );

    let output = Command::new(DISCORD)
        .args(["read", "2", "--limit", "5", "--before", "900"])
        .env_clear()
        .env("TOHRU_DISCORD_SOCKET", &socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("h\\u00e9llo"), "{text}");
    assert_eq!(stdout_text(&text)["next_before"], json!("400"));
    let output = Command::new(DISCORD)
        .args(["read", "2", "--limit", "51"])
        .env_clear()
        .env("TOHRU_DISCORD_SOCKET", &socket)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        json!({"error": "Discord read limit must be 1–50."})
    );

    let seen = fake.seen.lock().unwrap().clone();
    let paths: Vec<&str> = seen.iter().map(|(_, path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/channels/2",
            "/channels/2/messages",
            "/channels/2",
            "/channels/2/messages?limit=5&before=900"
        ]
    );
    let posted: Value = serde_json::from_str(seen[1].2.as_deref().unwrap()).unwrap();
    assert_eq!(posted["content"], json!("Hi ✓"));
    broker.close();
    let output = Command::new(DISCORD)
        .args(["read", "2"])
        .env_clear()
        .env("TOHRU_DISCORD_SOCKET", &socket)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output)["error"]
            .as_str()
            .unwrap()
            .contains("unavailable or cancelled")
    );
}

fn stdout_text(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}
