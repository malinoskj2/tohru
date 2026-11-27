use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const API: &str = "https://api.anthropic.com";
const VERSION: &str = "2023-06-01";
const KEY_LIMIT: u64 = 64 * 1024;

/// The API key from `ANTHROPIC_API_KEY`, else from `key_file`, else from
/// `$XDG_CONFIG_HOME/tohru/anthropic-key`: the bare key on one line, or a dotenv-style file
/// with an `ANTHROPIC_API_KEY=` line. Never logged.
#[must_use]
pub fn api_key(key_file: &str) -> Option<String> {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY")
        && !key.trim().is_empty()
    {
        return Some(key.trim().to_owned());
    }
    if !key_file.is_empty() {
        return key_from_file(Path::new(key_file));
    }
    let path = default_key_file();
    let (key, warning) = fallback_key(&path);
    if let Some(warning) = warning {
        eprintln!("tohru-assistant: {warning}");
    }
    key
}

#[must_use]
pub fn default_key_file() -> PathBuf {
    crate::paths::xdg(crate::paths::Xdg::Config).join("tohru/anthropic-key")
}

/// The key in the default file, quietly absent when the file is, with a warning naming the
/// path and mode when others can read it.
fn fallback_key(path: &Path) -> (Option<String>, Option<String>) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::metadata(path) else {
        return (None, None);
    };
    let mode = metadata.permissions().mode() & 0o777;
    let key = key_from_file(path);
    let warning = (key.is_some() && mode & 0o077 != 0).then(|| {
        format!(
            "{} is readable by group or others (mode {mode:03o}); chmod 600 it",
            path.display()
        )
    });
    (key, warning)
}

fn key_from_file(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(KEY_LIMIT)
        .read_to_string(&mut text)
        .ok()?;
    parse_key(&text)
}

fn parse_key(text: &str) -> Option<String> {
    let unquote = |value: &str| {
        value
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_owned()
    };
    for line in text.lines() {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some(value) = line.strip_prefix("ANTHROPIC_API_KEY=") {
            let value = unquote(value);
            return (!value.is_empty()).then_some(value);
        }
    }
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    match lines.as_slice() {
        [only] if !only.contains('=') && !only.contains(char::is_whitespace) => {
            Some((*only).to_owned())
        }
        _ => None,
    }
}

/// One streaming Messages call over a pooled HTTPS connection that `warm` can open early.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    key: String,
    base: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Http(String),
    Status(u16),
    Refused,
    Stopped,
    Malformed,
}

impl Client {
    #[must_use]
    pub fn new(key: String) -> Self {
        Self::with_base(key, API)
    }

    #[must_use]
    pub fn with_base(key: String, base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(5)))
            .user_agent("tohru-assistant")
            .build()
            .into();
        Self {
            agent,
            key,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// Opens the TLS connection the next call reuses; failures are left to that call.
    pub fn warm(&self) {
        let _ = self
            .agent
            .get(format!("{}/v1/models?limit=1", self.base))
            .config()
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .header("x-api-key", &self.key)
            .header("anthropic-version", VERSION)
            .call()
            .and_then(|mut response| response.body_mut().read_to_vec());
    }

    /// Streams one reply, passing each text delta to `on_text`. `system` blocks marked `true`
    /// end a cached prefix.
    pub fn stream(
        &self,
        request: &Request<'_>,
        on_text: &mut dyn FnMut(&str),
        stopped: &dyn Fn() -> bool,
    ) -> Result<String, Failure> {
        let system: Vec<Value> = request
            .system
            .iter()
            .filter(|(text, _)| !text.is_empty())
            .map(|(text, cached)| {
                if *cached {
                    json!({"type": "text", "text": text, "cache_control": {"type": "ephemeral"}})
                } else {
                    json!({"type": "text", "text": text})
                }
            })
            .collect();
        let body = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "stream": true,
            "system": system,
            "messages": [{"role": "user", "content": request.user}],
            "thinking": {"type": request.thinking},
            "output_config": {"effort": request.effort},
        });
        let deadline = Instant::now() + request.timeout;
        let mut response = self
            .agent
            .post(format!("{}/v1/messages", self.base))
            .config()
            .timeout_global(Some(request.timeout))
            .build()
            .header("x-api-key", &self.key)
            .header("anthropic-version", VERSION)
            .header("content-type", "application/json")
            .send(body.to_string())
            .map_err(|error| Failure::Http(error.to_string()))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Failure::Status(status));
        }
        let reader = BufReader::new(response.body_mut().as_reader());
        let mut text = String::new();
        let mut finished = false;
        for line in reader.lines() {
            if stopped() || Instant::now() > deadline {
                return Err(Failure::Stopped);
            }
            let line = line.map_err(|error| Failure::Http(error.to_string()))?;
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<Value>(data.trim()) else {
                continue;
            };
            match event["type"].as_str() {
                Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                    let delta = event["delta"]["text"].as_str().unwrap_or("");
                    text.push_str(delta);
                    on_text(delta);
                }
                Some("message_delta") if event["delta"]["stop_reason"] == "refusal" => {
                    return Err(Failure::Refused);
                }
                Some("message_stop") => {
                    finished = true;
                    break;
                }
                Some("error") => return Err(Failure::Malformed),
                _ => {}
            }
        }
        if finished {
            Ok(text)
        } else {
            Err(Failure::Malformed)
        }
    }
}

pub struct Request<'a> {
    pub model: &'a str,
    pub effort: &'a str,
    pub thinking: &'a str,
    pub system: &'a [(&'a str, bool)],
    pub user: &'a str,
    pub max_tokens: u32,
    pub timeout: Duration,
}

/// Reads `{"route": "...", "text": "..."}` as it streams: the route once it is known, and the
/// text decoded up to the last complete character.
#[derive(Default)]
pub struct RouteText {
    raw: String,
    route: Option<String>,
    sent: usize,
}

impl RouteText {
    /// Adds streamed output; returns newly decoded text once the route is known.
    pub fn push(&mut self, chunk: &str) -> String {
        self.raw.push_str(chunk);
        if self.route.is_none() {
            self.route =
                field_string(&self.raw, "route").and_then(|(value, done)| done.then_some(value));
        }
        if self.route.is_none() {
            return String::new();
        }
        let Some((text, _)) = field_string(&self.raw, "text") else {
            return String::new();
        };
        let fresh: String = text.chars().skip(self.sent).collect();
        self.sent += fresh.chars().count();
        fresh
    }

    #[must_use]
    pub fn route(&self) -> Option<&str> {
        self.route.as_deref()
    }
}

/// The decoded value of a string field in partial JSON and whether its closing quote arrived.
fn field_string(raw: &str, name: &str) -> Option<(String, bool)> {
    let key = format!("\"{name}\"");
    let at = raw.find(&key)? + key.len();
    let rest = raw[at..].trim_start().strip_prefix(':')?.trim_start();
    let mut chars = rest.strip_prefix('"')?.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some((out, true)),
            '\\' => {
                let Some(escape) = chars.next() else {
                    return Some((out, false));
                };
                match escape {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' | 'f' => out.push(' '),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if hex.len() < 4 {
                            return Some((out, false));
                        }
                        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            Some(decoded) => out.push(decoded),
                            None => out.push('\u{fffd}'),
                        }
                    }
                    other => out.push(other),
                }
            }
            c => out.push(c),
        }
    }
    Some((out, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_come_from_a_bare_or_dotenv_file() {
        assert_eq!(parse_key("sk-test-1\n"), Some("sk-test-1".into()));
        assert_eq!(
            parse_key("ANTHROPIC_MODEL=x\nANTHROPIC_API_KEY=\"sk-test-2\"\n"),
            Some("sk-test-2".into())
        );
        assert_eq!(
            parse_key("export ANTHROPIC_API_KEY=sk-3"),
            Some("sk-3".into())
        );
        assert_eq!(parse_key("OTHER=1\nMORE=2\n"), None);
        assert_eq!(parse_key("ANTHROPIC_API_KEY=\n"), None);
    }

    #[test]
    fn the_default_key_file_is_optional_and_warns_when_others_can_read_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("anthropic-key");
        assert_eq!(fallback_key(&path), (None, None));

        std::fs::write(&path, "sk-fallback-1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fallback_key(&path), (Some("sk-fallback-1".into()), None));

        std::fs::write(&path, "ANTHROPIC_API_KEY=sk-fallback-2\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (key, warning) = fallback_key(&path);
        assert_eq!(key, Some("sk-fallback-2".into()));
        let warning = warning.unwrap();
        assert!(warning.contains(&path.display().to_string()));
        assert!(warning.contains("644"));
        assert!(!warning.contains("sk-fallback"));
    }

    #[test]
    fn route_text_streams_once_the_route_is_known() {
        let mut stream = RouteText::default();
        assert_eq!(stream.push("{\"ro"), "");
        assert_eq!(stream.push("ute\": \"ans"), "");
        assert_eq!(stream.push("wer\", \"text\": \"It's"), "It's");
        assert_eq!(stream.route(), Some("answer"));
        assert_eq!(stream.push(" \\\"fine\\\"\\u00e9"), " \"fine\"é");
        assert_eq!(stream.push("\\"), "");
        assert_eq!(stream.push("n.\"}"), "\n.");
    }

    #[test]
    fn text_before_the_route_waits_for_it() {
        let mut stream = RouteText::default();
        assert_eq!(stream.push("{\"text\":\"Later"), "");
        assert_eq!(stream.push(" on\",\"route\":\"home\"}"), "Later on");
        assert_eq!(stream.route(), Some("home"));
    }

    #[test]
    fn a_loopback_stream_yields_text_and_reuses_the_warm_connection() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut requests = 0;
            let mut buffer = Vec::new();
            while requests < 2 {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).unwrap();
                buffer.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buffer).to_string();
                let Some(head) = text.find("\r\n\r\n") else {
                    continue;
                };
                let length = text[..head]
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if buffer.len() < head + 4 + length {
                    continue;
                }
                buffer.drain(..head + 4 + length);
                requests += 1;
                let body = if requests == 1 {
                    "{}".to_owned()
                } else {
                    [
                        "event: message_start\ndata: {\"type\":\"message_start\"}\n\n",
                        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
                        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
                        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
                    ]
                    .concat()
                };
                write!(socket, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        let client = Client::with_base("sk-test".into(), &base);
        client.warm();
        let mut seen = Vec::new();
        let text = client
            .stream(
                &Request {
                    model: "claude-sonnet-5-5",
                    effort: "low",
                    thinking: "between_tools",
                    system: &[("persona", true)],
                    user: "hi",
                    max_tokens: 64,
                    timeout: Duration::from_secs(5),
                },
                &mut |delta| seen.push(delta.to_owned()),
                &|| false,
            )
            .unwrap();
        assert_eq!(text, "Hello");
        assert_eq!(seen, ["Hel", "lo"]);
        assert_eq!(
            server.join().unwrap(),
            2,
            "both requests share one connection"
        );
    }
}
