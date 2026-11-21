//! Framed JSON IPC client for `greetd` (`GREETD_SOCK`).
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};
use thiserror::Error;

/// Upper bound on a single IPC payload from `greetd`.
pub const MAX_FRAME_BYTES: usize = 128 * 1024;

const IO_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("GREETD_SOCK is not set")]
    MissingSocketEnv,

    #[error("cannot connect to greetd socket at {path}")]
    Connect {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot configure greetd socket timeouts")]
    Configure {
        #[source]
        source: std::io::Error,
    },

    #[error("cannot write greetd IPC request")]
    Write {
        #[source]
        source: std::io::Error,
    },

    #[error("cannot read greetd IPC response")]
    Read {
        #[source]
        source: std::io::Error,
    },

    #[error("greetd IPC frame length {0} exceeds {MAX_FRAME_BYTES} bytes")]
    FrameTooLarge(usize),

    #[error("greetd IPC payload is invalid UTF-8")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("greetd IPC payload is invalid JSON")]
    Json(#[from] serde_json::Error),

    #[error("greetd IPC response has unexpected structure: {0}")]
    Protocol(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    CreateSession { username: String },
    PostAuthMessageResponse { response: Option<String> },
    StartSession { cmd: Vec<String>, env: Vec<String> },
    CancelSession,
}

impl Request {
    fn to_json_bytes(&self) -> Result<Vec<u8>, IpcError> {
        let mut obj = Map::new();
        match self {
            Self::CreateSession { username } => {
                obj.insert("type".into(), Value::String("create_session".into()));
                obj.insert("username".into(), Value::String(username.clone()));
            }
            Self::PostAuthMessageResponse { response } => {
                obj.insert(
                    "type".into(),
                    Value::String("post_auth_message_response".into()),
                );
                if let Some(answer) = response {
                    obj.insert("response".into(), Value::String(answer.clone()));
                }
            }
            Self::StartSession { cmd, env } => {
                obj.insert("type".into(), Value::String("start_session".into()));
                obj.insert(
                    "cmd".into(),
                    Value::Array(cmd.iter().cloned().map(Value::String).collect()),
                );
                obj.insert(
                    "env".into(),
                    Value::Array(env.iter().cloned().map(Value::String).collect()),
                );
            }
            Self::CancelSession => {
                obj.insert("type".into(), Value::String("cancel_session".into()));
            }
        }
        serde_json::to_vec(&Value::Object(obj)).map_err(IpcError::Json)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMessageType {
    Visible,
    Secret,
    Info,
    Error,
}

impl AuthMessageType {
    fn parse(kind: &str) -> Result<Self, IpcError> {
        match kind {
            "visible" => Ok(Self::Visible),
            "secret" => Ok(Self::Secret),
            "info" => Ok(Self::Info),
            "error" => Ok(Self::Error),
            _ => Err(IpcError::Protocol("unknown auth_message_type")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorType {
    AuthError,
    Error,
}

impl ErrorType {
    fn parse(kind: &str) -> Self {
        match kind {
            "auth_error" => Self::AuthError,
            _ => Self::Error,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    Success,
    AuthMessage {
        auth_message_type: AuthMessageType,
        auth_message: String,
    },
    Error {
        error_type: ErrorType,
        description: String,
    },
}

impl Response {
    fn from_json_bytes(bytes: &[u8]) -> Result<Self, IpcError> {
        let value: Value = serde_json::from_slice(bytes)?;
        let obj = value
            .as_object()
            .ok_or(IpcError::Protocol("response must be a JSON object"))?;
        let kind = obj
            .get("type")
            .and_then(Value::as_str)
            .ok_or(IpcError::Protocol("missing string `type` field"))?;

        match kind {
            "success" => Ok(Self::Success),
            "auth_message" => {
                let msg_type = obj
                    .get("auth_message_type")
                    .and_then(Value::as_str)
                    .ok_or(IpcError::Protocol("missing `auth_message_type`"))?;
                let auth_message = obj
                    .get("auth_message")
                    .and_then(Value::as_str)
                    .ok_or(IpcError::Protocol("missing `auth_message`"))?;
                Ok(Self::AuthMessage {
                    auth_message_type: AuthMessageType::parse(msg_type)?,
                    auth_message: auth_message.to_owned(),
                })
            }
            "error" => {
                let error_type = obj
                    .get("error_type")
                    .and_then(Value::as_str)
                    .map_or(ErrorType::Error, ErrorType::parse);
                let description = obj
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("Authentication error")
                    .to_owned();
                Ok(Self::Error {
                    error_type,
                    description,
                })
            }
            _ => Err(IpcError::Protocol("unknown response type")),
        }
    }
}

/// Active connection to `greetd`'s IPC socket.
pub struct Client {
    stream: UnixStream,
}

impl Client {
    /// Connect using the `GREETD_SOCK` environment variable.
    ///
    /// # Errors
    /// Returns an error when `GREETD_SOCK` is missing/empty or connection fails.
    pub fn connect_env() -> Result<Self, IpcError> {
        let path = std::env::var_os("GREETD_SOCK")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or(IpcError::MissingSocketEnv)?;
        Self::connect(&path)
    }

    /// Connect to a `greetd` UNIX domain socket at `path`.
    ///
    /// # Errors
    /// Returns an error if the socket cannot be opened or configured.
    pub fn connect(path: &Path) -> Result<Self, IpcError> {
        let stream = UnixStream::connect(path).map_err(|source| IpcError::Connect {
            path: path.to_owned(),
            source,
        })?;
        Self::from_stream(stream)
    }

    /// Wrap an existing UNIX stream (used by tests with `UnixStream::pair`).
    ///
    /// # Errors
    /// Returns an error if socket timeouts cannot be configured.
    pub fn from_stream(stream: UnixStream) -> Result<Self, IpcError> {
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|source| IpcError::Configure { source })?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|source| IpcError::Configure { source })?;
        Ok(Self { stream })
    }

    /// Send one request and read the corresponding framed response.
    ///
    /// # Errors
    /// Returns an error on serialization, framing, I/O, or protocol violations.
    pub fn send(&mut self, request: &Request) -> Result<Response, IpcError> {
        let payload = request.to_json_bytes()?;
        write_frame(&mut self.stream, &payload)?;
        let reply = read_frame(&mut self.stream)?;
        Response::from_json_bytes(&reply)
    }

    /// Initiate a login session for `username`.
    ///
    /// # Errors
    /// Propagates any transport or protocol error.
    pub fn create_session(&mut self, username: impl Into<String>) -> Result<Response, IpcError> {
        self.send(&Request::CreateSession {
            username: username.into(),
        })
    }

    /// Answer an authentication prompt from `greetd`.
    ///
    /// # Errors
    /// Propagates any transport or protocol error.
    pub fn post_auth_message_response(
        &mut self,
        response: Option<String>,
    ) -> Result<Response, IpcError> {
        self.send(&Request::PostAuthMessageResponse { response })
    }

    /// Request `greetd` to start the authenticated session once the greeter exits.
    ///
    /// # Errors
    /// Propagates any transport or protocol error.
    pub fn start_session(
        &mut self,
        cmd: Vec<String>,
        env: Vec<String>,
    ) -> Result<Response, IpcError> {
        self.send(&Request::StartSession { cmd, env })
    }

    /// Abort the active login transaction.
    ///
    /// # Errors
    /// Propagates any transport or protocol error.
    pub fn cancel_session(&mut self) -> Result<Response, IpcError> {
        self.send(&Request::CancelSession)
    }
}

/// Write a `greetd`-framed payload (native-endian `u32` length + UTF-8 JSON bytes).
///
/// # Errors
/// Returns an error if the frame exceeds `MAX_FRAME_BYTES` or writing fails.
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), IpcError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge(payload.len()));
    }
    let len = u32::try_from(payload.len()).map_err(|_| IpcError::FrameTooLarge(payload.len()))?;
    writer
        .write_all(&len.to_ne_bytes())
        .map_err(|source| IpcError::Write { source })?;
    writer
        .write_all(payload)
        .map_err(|source| IpcError::Write { source })?;
    writer.flush().map_err(|source| IpcError::Write { source })
}

/// Read a `greetd`-framed payload (native-endian `u32` length + UTF-8 JSON bytes).
///
/// # Errors
/// Returns an error if the length exceeds `MAX_FRAME_BYTES` or reading fails.
pub fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, IpcError> {
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .map_err(|source| IpcError::Read { source })?;
    let len = u32::from_ne_bytes(header) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge(len));
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|source| IpcError::Read { source })?;
    Ok(buf)
}
