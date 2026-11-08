// Frozen from e001ee23bfe8df93aec868d6294eb6de895883df for paired behavioral and allocation checks.
use super::*;
#[inline(always)]
pub(super) fn format_hz(hz: u32) -> String {
    if hz > MAX_DISPLAY_HZ {
        return format!(">{MAX_DISPLAY_HZ} Hz");
    }
    format!("{hz} Hz")
}

#[inline(always)]
pub(super) fn format_event_rate_summary(latest_hz: u32, max_hz: u32) -> String {
    format!(
        "{} latest / {} max",
        format_hz(latest_hz),
        format_hz(max_hz)
    )
}
