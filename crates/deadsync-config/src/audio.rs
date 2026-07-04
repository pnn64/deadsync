pub const AUDIO_VOLUME_MAX: u8 = 100;
pub const MUSIC_WHEEL_SWITCH_SPEED_MIN: u8 = 1;

pub const fn clamp_audio_volume_percent(value: u8) -> u8 {
    if value > AUDIO_VOLUME_MAX {
        AUDIO_VOLUME_MAX
    } else {
        value
    }
}

pub const fn clamp_music_wheel_switch_speed(value: u8) -> u8 {
    if value < MUSIC_WHEEL_SWITCH_SPEED_MIN {
        MUSIC_WHEEL_SWITCH_SPEED_MIN
    } else {
        value
    }
}

pub fn parse_auto_audio_output_device(raw: &str) -> Option<Option<u16>> {
    parse_auto_number(raw)
}

pub fn parse_auto_audio_sample_rate_hz(raw: &str) -> Option<Option<u32>> {
    parse_auto_number(raw)
}

fn parse_auto_number<T: std::str::FromStr>(raw: &str) -> Option<Option<T>> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("auto") {
        return Some(None);
    }
    raw.parse().ok().map(Some)
}

pub fn optional_audio_output_device_value(index: Option<u16>) -> String {
    optional_auto_number_value(index)
}

pub fn optional_audio_sample_rate_hz_value(rate: Option<u32>) -> String {
    optional_auto_number_value(rate)
}

fn optional_auto_number_value<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "Auto".to_string(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_audio_volume_percent() {
        assert_eq!(clamp_audio_volume_percent(0), 0);
        assert_eq!(clamp_audio_volume_percent(80), 80);
        assert_eq!(clamp_audio_volume_percent(101), 100);
        assert_eq!(clamp_audio_volume_percent(u8::MAX), 100);
    }

    #[test]
    fn clamps_music_wheel_switch_speed() {
        assert_eq!(clamp_music_wheel_switch_speed(0), 1);
        assert_eq!(clamp_music_wheel_switch_speed(1), 1);
        assert_eq!(clamp_music_wheel_switch_speed(8), 8);
    }

    #[test]
    fn parses_auto_audio_output_device() {
        assert_eq!(parse_auto_audio_output_device("Auto"), Some(None));
        assert_eq!(parse_auto_audio_output_device(" auto "), Some(None));
        assert_eq!(parse_auto_audio_output_device(""), Some(None));
        assert_eq!(parse_auto_audio_output_device("  "), Some(None));
        assert_eq!(parse_auto_audio_output_device("2"), Some(Some(2)));
        assert_eq!(parse_auto_audio_output_device("65535"), Some(Some(65535)));
        assert_eq!(parse_auto_audio_output_device("65536"), None);
        assert_eq!(parse_auto_audio_output_device("default"), None);
    }

    #[test]
    fn parses_auto_audio_sample_rate_hz() {
        assert_eq!(parse_auto_audio_sample_rate_hz("Auto"), Some(None));
        assert_eq!(
            parse_auto_audio_sample_rate_hz(" 44100 "),
            Some(Some(44100))
        );
        assert_eq!(parse_auto_audio_sample_rate_hz("0"), Some(Some(0)));
        assert_eq!(parse_auto_audio_sample_rate_hz("fast"), None);
    }

    #[test]
    fn formats_auto_audio_values() {
        assert_eq!(optional_audio_output_device_value(None), "Auto");
        assert_eq!(optional_audio_output_device_value(Some(3)), "3");
        assert_eq!(optional_audio_sample_rate_hz_value(None), "Auto");
        assert_eq!(optional_audio_sample_rate_hz_value(Some(48_000)), "48000");
    }
}
