// Frozen from 6c782aee5 for differential tests and paired measurements.
pub fn parse_legacy_present_mode<P>(raw: &str, balanced_policy: P, unhinged_policy: P) -> Option<P>
where
    P: Copy,
{
    match raw.trim().to_ascii_lowercase().as_str() {
        "balanced" => Some(balanced_policy),
        "unhinged" | "maxfps" | "max_fps" | "max-fps" => Some(unhinged_policy),
        _ => None,
    }
}
