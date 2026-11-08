// Frozen from 8a36344a9 for strict grammar and allocation comparisons.
use super::{BreakdownStyle, DefaultSyncOffset, VersionOverlaySide};
pub fn breakdown(s: &str) -> Result<BreakdownStyle, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "sl" => Ok(BreakdownStyle::Sl),
        "sn" => Ok(BreakdownStyle::Sn),
        _ => Err(()),
    }
}
pub fn sync(s: &str) -> Result<DefaultSyncOffset, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "null" | "none" | "off" => Ok(DefaultSyncOffset::Null),
        "itg" => Ok(DefaultSyncOffset::Itg),
        _ => Err(()),
    }
}
pub fn side(s: &str) -> Result<VersionOverlaySide, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "left" | "l" | "lhs" => Ok(VersionOverlaySide::Left),
        "right" | "r" | "rhs" => Ok(VersionOverlaySide::Right),
        _ => Err(()),
    }
}
