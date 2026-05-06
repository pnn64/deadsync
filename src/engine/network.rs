use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::LazyLock;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// Match Simply Love / ITGmania's GrooveStats request timeout (60s).
pub const GROOVESTATS_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeouts applied to an individual `ureq::Agent`.  Every field is
/// optional: a `None` value means "do not configure this dimension"
/// and ureq's own default applies (which for global/connect/resolve
/// is "no timeout at all").
///
/// `global` covers DNS through reading the entire response body and
/// is appropriate for small, fast requests.  Long-lived requests
/// (multi-megabyte downloads on slow networks) should leave `global`
/// as `None` and rely on `connect` + `resolve` to fail fast on
/// unreachable hosts without artificially capping transfer time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentConfig {
    pub timeout: Option<Duration>,
    pub connect_timeout: Option<Duration>,
    pub resolve_timeout: Option<Duration>,
}

impl AgentConfig {
    /// Convenience constructor for a single end-to-end timeout (the
    /// historical behaviour before per-stage controls existed).
    pub const fn with_global(timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            connect_timeout: None,
            resolve_timeout: None,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self::with_global(DEFAULT_REQUEST_TIMEOUT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    Timeout,
    HttpStatus(u16),
    Request(String),
    Decode(String),
}

impl Display for NetworkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => f.write_str("request timed out"),
            Self::HttpStatus(status) => write!(f, "http status {status}"),
            Self::Request(message) => f.write_str(message),
            Self::Decode(message) => write!(f, "decode error: {message}"),
        }
    }
}

impl Error for NetworkError {}

#[inline(always)]
fn is_timeout_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("timeout") || lower.contains("timed out")
}

#[inline(always)]
fn request_error(message: String) -> NetworkError {
    if is_timeout_text(message.as_str()) {
        NetworkError::Timeout
    } else {
        NetworkError::Request(message)
    }
}

#[inline(always)]
fn ensure_success(status: u16) -> Result<(), NetworkError> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(NetworkError::HttpStatus(status))
    }
}

pub fn build_agent(config: AgentConfig) -> ureq::Agent {
    let mut builder = ureq::Agent::config_builder().timeout_global(config.timeout);
    if let Some(t) = config.connect_timeout {
        builder = builder.timeout_connect(Some(t));
    }
    if let Some(t) = config.resolve_timeout {
        builder = builder.timeout_resolve(Some(t));
    }
    builder.build().into()
}

// Reuse a single process-wide agent so score submits and leaderboard requests share
// one connection pool instead of opening fresh sockets/TLS sessions per request.
static DEFAULT_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| build_agent(AgentConfig::default()));

// Dedicated agent for GrooveStats (and BoogieStats) requests, configured with the
// longer 60s timeout used by Simply Love / ITGmania.
static GROOVESTATS_AGENT: LazyLock<ureq::Agent> =
    LazyLock::new(|| build_agent(AgentConfig::with_global(GROOVESTATS_REQUEST_TIMEOUT)));

pub fn get_agent() -> ureq::Agent {
    DEFAULT_AGENT.clone()
}

pub fn get_groovestats_agent() -> ureq::Agent {
    GROOVESTATS_AGENT.clone()
}

pub fn get_json<T>(url: &str) -> Result<T, NetworkError>
where
    T: DeserializeOwned,
{
    get_json_with(&get_agent(), url)
}

pub fn get_json_with<T>(agent: &ureq::Agent, url: &str) -> Result<T, NetworkError>
where
    T: DeserializeOwned,
{
    let response = agent
        .get(url)
        .call()
        .map_err(|error| request_error(error.to_string()))?;
    ensure_success(response.status().as_u16())?;
    response
        .into_body()
        .read_json()
        .map_err(|error| NetworkError::Decode(error.to_string()))
}

pub fn post_json<B, T>(url: &str, body: &B) -> Result<T, NetworkError>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let response = get_agent()
        .post(url)
        .header("Content-Type", "application/json")
        .send_json(body)
        .map_err(|error| request_error(error.to_string()))?;
    ensure_success(response.status().as_u16())?;
    response
        .into_body()
        .read_json()
        .map_err(|error| NetworkError::Decode(error.to_string()))
}

pub fn spawn_request<F, T>(task: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(task)
}
