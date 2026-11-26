mod common;

use std::collections::VecDeque;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use serde_json::{Value, json};
use tohru_desktop::assistant::chat::discord_chat_with;
use tohru_desktop::assistant::cli::{Cli, Command};
use tohru_desktop::discord::{
    Body, Cancel, Client, DesktopOutcome, DiscordError, HttpRequest, HttpResponse, Loader, Method,
    Receiver, Sources, Transport, TransportError,
};

const SECRET: &str = "dm-secret-DO-NOT-PRINT";

/// Discord REST as a queue of canned replies.
#[derive(Default)]
struct Fake {
    replies: Mutex<VecDeque<Value>>,
    seen: Mutex<Vec<(Method, String)>>,
}

impl Fake {
    fn push(&self, body: Value) {
        self.replies.lock().unwrap().push_back(body);
    }

    fn page(&self, messages: Value) {
        self.push(metadata());
        self.push(metadata());
        self.push(messages);
    }

    fn posts(&self) -> usize {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, path)| {
                *method == Method::Post && path.as_str() == "/channels/7/messages"
            })
            .count()
    }
}

impl Transport for Fake {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.seen
            .lock()
            .unwrap()
            .push((request.method, request.path.clone()));
        let body = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected request {}", request.path));
        Ok(HttpResponse {
            status: 200,
            remaining: None,
            reset_after: None,
            body: Body::Complete(body.to_string().into_bytes()),
        })
    }
}

fn metadata() -> Value {
    json!({"id": "7", "type": 1, "recipients": [{"id": "9"}]})
}

fn message(ident: &str, content: &str) -> Value {
    json!({"id": ident, "channel_id": "7", "type": 0, "author": {"id": "9"},
           "content": content, "timestamp": "2026-11-22T00:00:00Z"})
}

fn acknowledged(
    _: &str,
    _: Duration,
    verify: &dyn Fn() -> Result<(), DiscordError>,
    _: &Cancel,
) -> Result<DesktopOutcome, DiscordError> {
    verify().map(|()| DesktopOutcome::Acknowledged)
}

fn unavailable(
    _: &str,
    _: Duration,
    verify: &dyn Fn() -> Result<(), DiscordError>,
    _: &Cancel,
) -> Result<DesktopOutcome, DiscordError> {
    verify().map(|()| DesktopOutcome::Unavailable)
}

fn unacknowledged(
    _: &str,
    _: Duration,
    verify: &dyn Fn() -> Result<(), DiscordError>,
    _: &Cancel,
) -> Result<DesktopOutcome, DiscordError> {
    verify().map(|()| DesktopOutcome::Unacknowledged)
}

/// Logs each provider run; with the revoke marker present it turns DM reception off mid-turn.
const FAKE_CLAUDE: &str = r#"if [ "$1 $2" = "auth status" ]; then echo '{"loggedIn": true}'; exit 0; fi
input=$(cat)
{
    printf 'ARGV %s\n' "$*"
    printf 'TOKEN %s\n' "${TOHRU_DISCORD_BOT_TOKEN-unset}"
    printf 'SOCKET %s\n' "${TOHRU_DISCORD_SOCKET-unset}"
    printf 'STDIN %s\n' "$input"
} >> "$FAKE_LOG"
if [ -e "$FAKE_REVOKE" ]; then
    sed -i 's/^receive = true/receive = false/' "$FAKE_CONFIG"
fi
echo '{"type": "system", "subtype": "init", "session_id": "s-1"}'
echo '{"type": "result", "subtype": "success", "result": "Fake reply", "session_id": "s-1"}'
"#;

struct Case {
    post: bool,
    revoke: bool,
    desktop: Option<tohru_desktop::discord::Notifier>,
    acknowledged: bool,
}

fn write_config(path: &Path, token: &Path, case: &Case) {
    std::fs::write(
        path,
        format!(
            "[assistant.discord]\nenabled = true\nread = true\npost = {}\nreceive = true\n\
             destination = \"dm\"\nallowed_user_id = \"9\"\ndesktop_first = {}\n\
             token_file = \"{}\"\n",
            case.post,
            case.desktop.is_some(),
            token.display()
        ),
    )
    .unwrap();
}

fn chat_args() -> tohru_desktop::assistant::cli::ChatArgs {
    let cli = Cli::try_parse_from([
        "tohru-assistant",
        "chat",
        "--provider",
        "claude",
        "--discord-incoming",
        "101",
        "--orca",
        "false",
        "--memory",
        "off",
    ])
    .unwrap();
    match cli.command {
        Command::Chat(args) => *args,
        _ => unreachable!(),
    }
}

fn cases() -> [Case; 6] {
    [
        Case {
            post: true,
            revoke: false,
            desktop: None,
            acknowledged: false,
        },
        Case {
            post: false,
            revoke: false,
            desktop: None,
            acknowledged: false,
        },
        Case {
            post: true,
            revoke: true,
            desktop: None,
            acknowledged: false,
        },
        Case {
            post: true,
            revoke: false,
            desktop: Some(acknowledged),
            acknowledged: true,
        },
        Case {
            post: true,
            revoke: false,
            desktop: Some(unavailable),
            acknowledged: false,
        },
        Case {
            post: true,
            revoke: false,
            desktop: Some(unacknowledged),
            acknowledged: false,
        },
    ]
}

#[test]
#[expect(
    unsafe_code,
    reason = "the assistant reads its private roots from the environment; this is the only test in its binary"
)]
fn incoming_dm_reaches_the_provider_and_replies_under_the_live_policy() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    common::script(&bin, "claude", FAKE_CLAUDE);
    let token = root.join("token");
    std::fs::write(&token, SECRET).unwrap();
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    let config = root.join("config/tohru/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    let log = root.join("provider.log");
    let revoke = root.join("revoke");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let vars: [(&str, PathBuf); 11] = [
        ("PATH", PathBuf::from(path)),
        ("HOME", root.join("home")),
        ("XDG_CONFIG_HOME", root.join("config")),
        ("XDG_STATE_HOME", root.join("state")),
        ("XDG_DATA_HOME", root.join("data")),
        ("XDG_CACHE_HOME", root.join("cache")),
        ("TOHRU_ASSISTANT_DATA", root.join("data/tohru")),
        ("TOHRU_ASSISTANT_SCRATCH", root.join("scratch")),
        ("FAKE_LOG", log.clone()),
        ("FAKE_REVOKE", revoke.clone()),
        ("FAKE_CONFIG", config.clone()),
    ];
    for (key, value) in vars {
        // SAFETY: set before the assistant starts any thread; no other test shares this process.
        unsafe { std::env::set_var(key, value) };
    }
    // SAFETY: as above.
    unsafe { std::env::remove_var("TOHRU_DISCORD_BOT_TOKEN") };

    for (index, case) in cases().iter().enumerate() {
        write_config(&config, &token, case);
        std::fs::write(&log, "").unwrap();
        let _ = std::fs::remove_file(&revoke);
        let fake = Arc::new(Fake::default());
        let client = || {
            let client = Client::with(
                Loader::Disk(Sources::new(config.clone(), None)),
                root.join("state/tohru/discord"),
                Arc::clone(&fake) as Arc<dyn Transport>,
            );
            match case.desktop {
                Some(notify) => client.with_notifier(notify),
                None => client,
            }
        };
        let mut receiver = Receiver::new(client());
        receiver.reset().unwrap();
        fake.page(json!([message("100", "old history")]));
        assert_eq!(receiver.poll().unwrap(), json!({"event": "idle"}));
        fake.page(json!([message("101", "Hello")]));
        assert_eq!(
            receiver.poll().unwrap(),
            json!({"event": "incoming", "id": "101"})
        );
        // The claim refetches the message, so the provider sees its current text.
        fake.push(metadata());
        fake.push(metadata());
        fake.push(message("101", "Fresh authorized input"));
        let sends = case.post && !case.revoke && !case.acknowledged;
        if sends {
            fake.page(json!({"id": "200"}));
        }
        if case.revoke {
            std::fs::write(&revoke, "").unwrap();
        }
        let code = discord_chat_with(&mut chat_args(), client()).unwrap();
        assert_eq!(code, u8::from(case.revoke), "case {index}");
        assert_eq!(fake.posts(), usize::from(sends), "case {index}");
        let provider = std::fs::read_to_string(&log).unwrap();
        assert!(
            provider.contains("Fresh authorized input"),
            "case {index}: {provider}"
        );
        assert!(!provider.contains("STDIN Hello"), "case {index}");
        assert!(provider.contains("TOKEN unset"), "case {index}");
        assert!(provider.contains("SOCKET unset"), "case {index}");
        assert!(provider.contains("--tools"), "case {index}");
        assert!(!provider.contains(SECRET), "case {index}");
    }
}
