pub mod arrowcloud;
pub mod downloads;
pub mod groovestats;
pub mod lobbies;
pub mod player_leaderboards;
pub mod runtime;
pub mod score_compat;
pub mod score_import;
pub mod srpg_shop;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnlineRequestError {
    Timeout,
    HttpStatus(u16),
    Request(String),
    Decode(String),
}

impl OnlineRequestError {
    #[inline(always)]
    pub const fn http_status(&self) -> Option<u16> {
        match self {
            Self::HttpStatus(status) => Some(*status),
            _ => None,
        }
    }
}

impl std::fmt::Display for OnlineRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => f.write_str("request timed out"),
            Self::HttpStatus(status) => write!(f, "http status {status}"),
            Self::Request(message) => f.write_str(message),
            Self::Decode(message) => write!(f, "decode error: {message}"),
        }
    }
}

impl std::error::Error for OnlineRequestError {}

pub fn boxed_request_error(
    prefix: &str,
    error: OnlineRequestError,
) -> Box<dyn std::error::Error + Send + Sync> {
    if let Some(status) = error.http_status() {
        return format!("{prefix} returned status {status}").into();
    }
    Box::new(error)
}

impl From<deadsync_net::NetworkError> for OnlineRequestError {
    fn from(error: deadsync_net::NetworkError) -> Self {
        match error {
            deadsync_net::NetworkError::Timeout => Self::Timeout,
            deadsync_net::NetworkError::HttpStatus(status) => Self::HttpStatus(status),
            deadsync_net::NetworkError::Request(message) => Self::Request(message),
            deadsync_net::NetworkError::Decode(message) => Self::Decode(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_error_reports_http_status() {
        assert_eq!(OnlineRequestError::HttpStatus(503).http_status(), Some(503));
        assert_eq!(OnlineRequestError::Timeout.http_status(), None);
        assert_eq!(
            OnlineRequestError::HttpStatus(503).to_string(),
            "http status 503"
        );
    }

    #[test]
    fn boxed_request_error_formats_http_status_with_prefix() {
        let error = boxed_request_error("API", OnlineRequestError::HttpStatus(429));
        assert_eq!(error.to_string(), "API returned status 429");

        let timeout = boxed_request_error("API", OnlineRequestError::Timeout);
        assert_eq!(timeout.to_string(), "request timed out");
    }
}
