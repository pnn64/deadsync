use super::*;

#[test]
fn event_rate_summary_preserves_caps_and_numeric_boundaries() {
    for (latest, maximum, expected) in [
        (0, 1, "0 Hz latest / 1 Hz max"),
        (999, 1000, "999 Hz latest / 1000 Hz max"),
        (1000, 999, "1000 Hz latest / 999 Hz max"),
        (1001, 1000, ">1000 Hz latest / 1000 Hz max"),
        (u32::MAX, u32::MAX, ">1000 Hz latest / >1000 Hz max"),
    ] {
        assert_eq!(format_event_rate_summary(latest, maximum), expected);
    }
}
