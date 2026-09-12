// Frozen from b63f6a0c9d58926bb863e2aae5cbc7e94690d476; only test visibility is adapted.

pub(super) fn parse_player_speed_option(text: &str) -> Option<(&'static str, f32)> {
    let compact: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    if let Some(value) = compact
        .strip_suffix('x')
        .and_then(|raw| raw.parse::<f32>().ok())
    {
        return Some(("xmod", value));
    }
    for (prefix, key) in [("ca", "camod"), ("c", "cmod"), ("m", "mmod"), ("a", "amod")] {
        if let Some(value) = compact
            .strip_prefix(prefix)
            .and_then(|raw| raw.parse::<f32>().ok())
            .or_else(|| {
                compact
                    .strip_suffix(prefix)
                    .and_then(|raw| raw.parse::<f32>().ok())
            })
        {
            return Some((key, value));
        }
    }
    None
}
