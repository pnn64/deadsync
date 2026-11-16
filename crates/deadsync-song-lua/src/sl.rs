pub(super) fn gameplay_lead_in_seconds(group: &str, name: &str) -> Option<f64> {
    if !group.eq_ignore_ascii_case("ScreenGameplay") {
        return None;
    }
    [
        ("MinSecondsToStep", 6.0),
        ("MinSecondsToMusic", 2.0),
        ("MinSecondsToStepNextSong", 4.0),
    ]
    .into_iter()
    .find_map(|(metric, seconds)| name.eq_ignore_ascii_case(metric).then_some(seconds))
}
