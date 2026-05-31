pub mod arrowcloud;
pub mod downloads;
pub mod groovestats;
pub mod lobbies;

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
}
