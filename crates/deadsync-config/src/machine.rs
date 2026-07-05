use crate::writer::push_line;

pub const DEFAULT_MACHINE_NOTESKIN: &str = "cel";
pub const DEFAULT_FRAME_STATS_OVERLAY_ANCHOR: &str = "auto";
pub const DEFAULT_FRAME_STATS_OVERLAY_STYLE: &str = "detailed";
pub const SMX_LIGHT_BRIGHTNESS_MAX: u8 = 100;

pub const fn clamp_smx_light_brightness_percent(value: u8) -> u8 {
    if value > SMX_LIGHT_BRIGHTNESS_MAX {
        SMX_LIGHT_BRIGHTNESS_MAX
    } else {
        value
    }
}

pub fn normalize_machine_default_noteskin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_MACHINE_NOTESKIN.to_string();
    }
    trimmed.to_ascii_lowercase()
}

pub fn push_default_noteskin_option_line(content: &mut String, noteskin: &str) {
    push_line(content, "DefaultNoteSkin", noteskin);
}

pub fn canonical_frame_stats_overlay_anchor(value: &str) -> &'static str {
    const KEYS: &[&str] = &[
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
        "top-center",
        "bottom-center",
    ];
    let value = value.trim().to_ascii_lowercase();
    KEYS.iter()
        .copied()
        .find(|&key| key == value)
        .unwrap_or(DEFAULT_FRAME_STATS_OVERLAY_ANCHOR)
}

pub fn canonical_frame_stats_overlay_style(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case("minimal") {
        "minimal"
    } else {
        DEFAULT_FRAME_STATS_OVERLAY_STYLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_machine_default_noteskin() {
        assert_eq!(normalize_machine_default_noteskin(" Cyber "), "cyber");
        assert_eq!(
            normalize_machine_default_noteskin(""),
            DEFAULT_MACHINE_NOTESKIN
        );
        assert_eq!(
            normalize_machine_default_noteskin("   "),
            DEFAULT_MACHINE_NOTESKIN
        );
    }

    #[test]
    fn writes_default_noteskin_option_line() {
        let mut content = String::new();

        push_default_noteskin_option_line(&mut content, "cyber");

        assert_eq!(content, "DefaultNoteSkin=cyber\n");
    }

    #[test]
    fn clamps_smx_light_brightness_percent() {
        assert_eq!(clamp_smx_light_brightness_percent(0), 0);
        assert_eq!(clamp_smx_light_brightness_percent(80), 80);
        assert_eq!(clamp_smx_light_brightness_percent(101), 100);
        assert_eq!(clamp_smx_light_brightness_percent(u8::MAX), 100);
    }

    #[test]
    fn canonical_overlay_anchor_accepts_known_keys() {
        assert_eq!(
            canonical_frame_stats_overlay_anchor(" Top-Left "),
            "top-left"
        );
        assert_eq!(
            canonical_frame_stats_overlay_anchor("bottom-center"),
            "bottom-center"
        );
    }

    #[test]
    fn canonical_overlay_anchor_defaults_unknown_values() {
        assert_eq!(
            canonical_frame_stats_overlay_anchor("auto"),
            DEFAULT_FRAME_STATS_OVERLAY_ANCHOR
        );
        assert_eq!(
            canonical_frame_stats_overlay_anchor("middle"),
            DEFAULT_FRAME_STATS_OVERLAY_ANCHOR
        );
        assert_eq!(
            canonical_frame_stats_overlay_anchor(""),
            DEFAULT_FRAME_STATS_OVERLAY_ANCHOR
        );
    }

    #[test]
    fn canonical_overlay_style_accepts_minimal_only() {
        assert_eq!(canonical_frame_stats_overlay_style(" Minimal "), "minimal");
        assert_eq!(
            canonical_frame_stats_overlay_style("detailed"),
            DEFAULT_FRAME_STATS_OVERLAY_STYLE
        );
        assert_eq!(
            canonical_frame_stats_overlay_style("compact"),
            DEFAULT_FRAME_STATS_OVERLAY_STYLE
        );
    }
}
