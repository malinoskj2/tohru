use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use serde_json::{Map, Value, json};
use tohru_desktop::discord::{self, Client, DiscordError, Loader, Receiver, SOCKET_ENV, Sources};
use tohru_desktop::pyjson;

/// Packaged shell tool for Tohru's turn-scoped Discord broker; no credentials in argv.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Validate local setup without contacting Discord
    Status,
    Poll {
        #[arg(long, default_value = "")]
        settings: String,
    },
    ResetReceiver {
        #[arg(long, default_value = "")]
        settings: String,
    },
    Read {
        channel: String,
        /// Examine 1–50 history entries in one REST page; return only the configured author
        #[arg(long, default_value_t = 20, allow_negative_numbers = true)]
        limit: i64,
        #[arg(long, conflicts_with = "after")]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    Post {
        channel: String,
    },
}

const UNAVAILABLE: &str =
    "Discord tool unavailable or cancelled; a post may have been sent, check before retrying.";
const NO_TURN: &str = "Discord tools require an active Tohru assistant turn. Run tohru-discord status for setup feedback.";
const REPLY_LIMIT: u64 = 65_536;

fn print(value: &Value) {
    println!("{}", pyjson::dumps(value));
}

fn receiver(settings: &str, reset: bool) -> ExitCode {
    let client = Client::new(Loader::effective(Sources::from_env(), settings));
    let cancel = client.cancel_handle();
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        if signal_hook::flag::register(signal, cancel.flag()).is_err() {
            print(
                &json!({"event": "error", "message": "Cannot install Discord receiver signal handlers."}),
            );
            return ExitCode::FAILURE;
        }
    }
    let mut receiver = Receiver::new(client);
    let result = if reset {
        receiver.reset().map(|()| json!({"event": "reset"}))
    } else {
        receiver.poll()
    };
    match result {
        Ok(value) => {
            print(&value);
            ExitCode::SUCCESS
        }
        Err(
            error @ (DiscordError::Fixed(_)
            | DiscordError::Deleted { .. }
            | DiscordError::Storage(_)),
        ) => {
            print(&json!({"event": "error", "message": error.to_string()}));
            ExitCode::FAILURE
        }
    }
}

fn post_content() -> String {
    let mut raw = Vec::new();
    let _ = std::io::stdin().lock().take(2001 * 4).read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).chars().take(2001).collect()
}

fn call(request: &Value, timeout: Duration) -> Option<Value> {
    let path = std::env::var_os(SOCKET_ENV).filter(|path| !path.is_empty())?;
    let attempt = || -> std::io::Result<Option<Value>> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.write_all(format!("{}\n", pyjson::dumps_unicode(request)).as_bytes())?;
        let mut raw = Vec::new();
        BufReader::new(stream)
            .take(REPLY_LIMIT + 1)
            .read_until(b'\n', &mut raw)?;
        if raw.len() as u64 > REPLY_LIMIT {
            return Ok(None);
        }
        Ok(serde_json::from_slice(&raw).ok())
    };
    Some(
        attempt()
            .ok()
            .flatten()
            .unwrap_or_else(|| json!({"error": UNAVAILABLE})),
    )
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (request, timeout) = match cli.action {
        Action::Status => {
            print(&discord::status());
            return ExitCode::SUCCESS;
        }
        Action::Poll { settings } => return receiver(&settings, false),
        Action::ResetReceiver { settings } => return receiver(&settings, true),
        Action::Read {
            channel,
            limit,
            before,
            after,
        } => {
            let request = json!({"action": "read", "channel": channel, "limit": limit, "before": before, "after": after});
            (request, Duration::from_secs(20))
        }
        // Desktop-first posting may wait up to 300 s, then use the 15 s REST budget.
        Action::Post { channel } => {
            let mut request = Map::new();
            request.insert("action".to_owned(), json!("post"));
            request.insert("channel".to_owned(), json!(channel));
            request.insert("content".to_owned(), json!(post_content()));
            (Value::Object(request), Duration::from_secs(320))
        }
    };
    let Some(result) = call(&request, timeout) else {
        print(&json!({"error": NO_TURN}));
        return ExitCode::FAILURE;
    };
    print(&result);
    if result.get("error").is_some() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
