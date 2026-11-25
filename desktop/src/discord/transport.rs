use std::time::Duration;

use super::MAX_BODY;

const API: &str = "https://discord.com";
const USER_AGENT: &str = "DiscordBot (https://github.com/malinoskj2/tohru, 0.1)";
const SOCKET_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: Method,
    /// Below `/api/v10`.
    pub path: String,
    pub token: String,
    pub body: Option<String>,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Complete(Vec<u8>),
    Oversized,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub remaining: Option<String>,
    pub reset_after: Option<String>,
    pub body: Body,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Discord HTTP request failed")]
    Http(#[source] Box<ureq::Error>),
    #[error("Discord test transport refused the request")]
    Refused,
}

/// One HTTP exchange with Discord's REST API, bounded by the request's timeout.
pub trait Transport: Send + Sync {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// Discord over HTTPS with the platform's root certificates.
pub struct Ureq {
    agent: ureq::Agent,
    base: String,
}

impl Ureq {
    #[must_use]
    pub fn new() -> Self {
        Self::with_base(API)
    }

    /// Another origin, such as a loopback fake in tests.
    #[must_use]
    pub fn with_base(base: &str) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(SOCKET_TIMEOUT))
            .timeout_recv_response(Some(SOCKET_TIMEOUT))
            .timeout_recv_body(Some(SOCKET_TIMEOUT))
            .user_agent(USER_AGENT)
            .build()
            .into();
        Self {
            agent,
            base: base.trim_end_matches('/').to_owned(),
        }
    }
}

impl Default for Ureq {
    fn default() -> Self {
        Self::new()
    }
}

fn http(error: ureq::Error) -> TransportError {
    TransportError::Http(Box::new(error))
}

impl Transport for Ureq {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        let url = format!("{}/api/v10{}", self.base, request.path);
        let authorization = format!("Bot {}", request.token);
        let result = match request.method {
            Method::Get => self
                .agent
                .get(&url)
                .config()
                .timeout_global(Some(request.timeout))
                .build()
                .header("Authorization", &authorization)
                .header("Content-Type", "application/json")
                .call(),
            Method::Post => self
                .agent
                .post(&url)
                .config()
                .timeout_global(Some(request.timeout))
                .build()
                .header("Authorization", &authorization)
                .header("Content-Type", "application/json")
                .send(request.body.unwrap_or_default()),
        };
        let mut response = result.map_err(http)?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let (remaining, reset_after) = (
            header("X-RateLimit-Remaining"),
            header("X-RateLimit-Reset-After"),
        );
        let status = response.status().as_u16();
        let body = match response
            .body_mut()
            .with_config()
            .limit(MAX_BODY as u64)
            .read_to_vec()
        {
            Ok(body) => Body::Complete(body),
            Err(ureq::Error::BodyExceedsLimit(_)) => Body::Oversized,
            Err(error) => return Err(http(error)),
        };
        Ok(HttpResponse {
            status,
            remaining,
            reset_after,
            body,
        })
    }
}
