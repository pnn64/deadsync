//! Shared types and helpers for backend score-submission UI status.

/// Why a submitted score was rejected by the backend. Distinguishing causes
/// lets the UI explain to the user that resubmitting will not change the
/// outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectReason {
    /// The backend accepted the request but considers the score invalid
    /// (validation failure, malformed payload, generic 4xx).
    InvalidScore,
    /// HTTP 401 / 403 — the API key was not accepted.
    Unauthorized,
    /// HTTP 404 — the chart hash is unknown to the backend.
    NotFound,
}

impl RejectReason {
    /// Human-readable label suitable for the evaluation footer.
    pub const fn label(self) -> &'static str {
        match self {
            Self::InvalidScore => "Invalid Score",
            Self::Unauthorized => "Unauthorized",
            Self::NotFound => "Unknown Chart",
        }
    }
}
