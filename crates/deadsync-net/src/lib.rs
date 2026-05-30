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
pub fn is_timeout_message(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("timeout") || lower.contains("timed out")
}

#[inline(always)]
pub fn request_error(message: String) -> NetworkError {
    if is_timeout_message(message.as_str()) {
        NetworkError::Timeout
    } else {
        NetworkError::Request(message)
    }
}

pub fn error_from_ureq(error: ureq::Error) -> NetworkError {
    match error {
        ureq::Error::StatusCode(status) => NetworkError::HttpStatus(status),
        other => request_error(other.to_string()),
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

pub fn read_json_body<T>(response: ureq::http::Response<ureq::Body>) -> Result<T, NetworkError>
where
    T: DeserializeOwned,
{
    response
        .into_body()
        .read_json()
        .map_err(|error| NetworkError::Decode(error.to_string()))
}

pub fn read_text_body_or_empty(response: ureq::http::Response<ureq::Body>) -> String {
    response.into_body().read_to_string().unwrap_or_default()
}

pub fn build_agent(config: AgentConfig) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .build()
        .into()
}

// Reuse a single process-wide agent so score submits and leaderboard requests share
// one connection pool instead of opening fresh sockets/TLS sessions per request.
static DEFAULT_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| build_agent(AgentConfig::default()));

// Dedicated agent for GrooveStats (and BoogieStats) requests, configured with the
// longer 60s timeout used by Simply Love / ITGmania.
static GROOVESTATS_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    build_agent(AgentConfig {
        timeout: GROOVESTATS_REQUEST_TIMEOUT,
    })
});

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
    let response = agent.get(url).call().map_err(error_from_ureq)?;
    ensure_success(response.status().as_u16())?;
    read_json_body(response)
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
        .map_err(error_from_ureq)?;
    ensure_success(response.status().as_u16())?;
    read_json_body(response)
}

pub fn spawn_request<F, T>(task: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_timeout_message_accepts_common_timeout_text() {
        assert!(is_timeout_message("request timed out"));
        assert!(is_timeout_message("Timeout reading body"));
        assert!(!is_timeout_message("connection refused"));
    }

    #[test]
    fn request_error_maps_timeout_messages() {
        assert_eq!(
            request_error("timed out while connecting".to_string()),
            NetworkError::Timeout
        );
        assert_eq!(
            request_error("connection refused".to_string()),
            NetworkError::Request("connection refused".to_string())
        );
    }

    #[test]
    fn error_from_ureq_preserves_http_status() {
        assert_eq!(
            error_from_ureq(ureq::Error::StatusCode(404)),
            NetworkError::HttpStatus(404)
        );
    }

    #[test]
    fn read_json_body_decodes_response() {
        let response = ureq::http::Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(
                ureq::Body::builder()
                    .mime_type("application/json")
                    .data(r#"{"message":"ok"}"#),
            )
            .expect("response");

        #[derive(serde::Deserialize)]
        struct Payload {
            message: String,
        }

        let payload: Payload = read_json_body(response).expect("decode json");
        assert_eq!(payload.message, "ok");
    }

    #[test]
    fn read_text_body_or_empty_reads_response_text() {
        let response = ureq::http::Response::builder()
            .status(200)
            .header("content-type", "text/plain")
            .body(ureq::Body::builder().mime_type("text/plain").data("ok"))
            .expect("response");

        assert_eq!(read_text_body_or_empty(response), "ok");
    }
}
