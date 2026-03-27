use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentConfig {
    pub timeout: Duration,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_REQUEST_TIMEOUT,
        }
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
    ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .build()
        .into()
}

pub fn get_agent() -> ureq::Agent {
    build_agent(AgentConfig::default())
}

pub fn get_json<T>(url: &str) -> Result<T, NetworkError>
where
    T: DeserializeOwned,
{
    let response = get_agent()
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
