//! Shared types and helpers for backend score-submission UI status.

use std::time::Duration;

/// Maximum number of attempts before the backoff schedule saturates. For
/// *auto-retryable* statuses (e.g. `TimedOut`) this is also the auto-retry
/// budget. For *manual-only* statuses (e.g. `NetworkError`, `ServerError`)
/// the cooldown caps at `submit_retry_delay_secs(MAX)` and stays there for
/// subsequent failures.
///
/// Shared between every score-submission backend (GrooveStats, ArrowCloud,
/// …) so the player sees a uniform retry cadence across services.
pub const SUBMIT_RETRY_MAX_ATTEMPTS: u8 = 5;

/// Exponential backoff schedule used by every score-submission backend.
/// `attempt` is 1-based: 1 → 2s, 2 → 4s, 3 → 8s, 4 → 16s, 5 → 32s.
/// Total auto budget ≈ 62s.
#[inline(always)]
pub const fn submit_retry_delay_secs(attempt: u8) -> u64 {
    1u64 << attempt
}

/// Convert a remaining `Duration` into whole seconds, rounding **up** so the
/// UI countdown shows the configured delay (e.g., a freshly-armed 16s gate
/// reads "16s" instead of "15s" due to subsecond truncation).
#[inline]
pub fn duration_to_ceil_secs(remaining: Duration) -> u32 {
    let secs = remaining.as_secs();
    let bumped = secs.saturating_add(if remaining.subsec_nanos() > 0 { 1 } else { 0 });
    bumped.min(u32::MAX as u64) as u32
}

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
