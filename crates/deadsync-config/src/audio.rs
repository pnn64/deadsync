use crate::bools::{parse_bool_str, parse_u8_bool_or_default};
use crate::ini::SimpleIni;

pub const AUDIO_VOLUME_MAX: u8 = 100;
pub const MUSIC_WHEEL_SWITCH_SPEED_MIN: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioOptions {
    pub visual_delay_seconds: f32,
    pub master_volume: u8,
    pub menu_music: bool,
    pub custom_sounds_enabled: bool,
    pub music_volume: u8,
    pub music_wheel_switch_speed: u8,
    pub sfx_volume: u8,
    pub assist_tick_volume: u8,
    pub output_device_index: Option<u16>,
    pub sample_rate_hz: Option<u32>,
    pub rate_mod_preserves_pitch: bool,
    pub enable_replaygain: bool,
    pub write_current_screen: bool,
    pub tab_acceleration: bool,
}

pub fn load_audio_options(conf: &SimpleIni, default: AudioOptions) -> AudioOptions {
    AudioOptions {
        visual_delay_seconds: conf
            .get("Options", "VisualDelaySeconds")
            .and_then(|value| value.parse().ok())
            .unwrap_or(default.visual_delay_seconds),
        master_volume: conf
            .get("Options", "MasterVolume")
            .and_then(|value| value.parse().ok())
            .map_or(default.master_volume, clamp_audio_volume_percent),
        menu_music: parse_u8_bool_or_default(
            conf.get("Options", "MenuMusic").as_deref(),
            default.menu_music,
        ),
        custom_sounds_enabled: parse_u8_bool_or_default(
            conf.get("Options", "CustomSoundsEnabled").as_deref(),
            default.custom_sounds_enabled,
        ),
        music_volume: conf
            .get("Options", "MusicVolume")
            .and_then(|value| value.parse().ok())
            .map_or(default.music_volume, clamp_audio_volume_percent),
        music_wheel_switch_speed: conf
            .get("Options", "MusicWheelSwitchSpeed")
            .and_then(|value| value.parse::<u8>().ok())
            .map_or(
                default.music_wheel_switch_speed,
                clamp_music_wheel_switch_speed,
            ),
        sfx_volume: conf
            .get("Options", "SFXVolume")
            .and_then(|value| value.parse().ok())
            .map_or(default.sfx_volume, clamp_audio_volume_percent),
        assist_tick_volume: conf
            .get("Options", "AssistTickVolume")
            .and_then(|value| value.parse().ok())
            .map_or(default.assist_tick_volume, clamp_audio_volume_percent),
        output_device_index: conf
            .get("Options", "AudioOutputDevice")
            .and_then(|value| parse_auto_audio_output_device(&value))
            .unwrap_or(default.output_device_index),
        sample_rate_hz: conf
            .get("Options", "AudioSampleRateHz")
            .and_then(|value| parse_auto_audio_sample_rate_hz(&value))
            .unwrap_or(default.sample_rate_hz),
        rate_mod_preserves_pitch: parse_u8_bool_or_default(
            conf.get("Options", "RateModPreservesPitch").as_deref(),
            default.rate_mod_preserves_pitch,
        ),
        enable_replaygain: parse_u8_bool_or_default(
            conf.get("Options", "ReplayGain").as_deref(),
            default.enable_replaygain,
        ),
        write_current_screen: conf
            .get("Options", "WriteCurrentScreen")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.write_current_screen),
        tab_acceleration: conf
            .get("Options", "TabAcceleration")
            .and_then(|value| parse_bool_str(&value))
            .unwrap_or(default.tab_acceleration),
    }
}

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

    fn default_options() -> AudioOptions {
        AudioOptions {
            visual_delay_seconds: 0.0,
            master_volume: 80,
            menu_music: true,
            custom_sounds_enabled: true,
            music_volume: 80,
            music_wheel_switch_speed: 15,
            sfx_volume: 80,
            assist_tick_volume: 50,
            output_device_index: None,
            sample_rate_hz: None,
            rate_mod_preserves_pitch: true,
            enable_replaygain: false,
            write_current_screen: false,
            tab_acceleration: true,
        }
    }

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

    #[test]
    fn loads_audio_options_from_ini() {
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            VisualDelaySeconds=0.125
            MasterVolume=250
            MenuMusic=0
            CustomSoundsEnabled=0
            MusicVolume=72
            MusicWheelSwitchSpeed=0
            SFXVolume=101
            AssistTickVolume=17
            AudioOutputDevice=3
            AudioSampleRateHz=48000
            RateModPreservesPitch=0
            ReplayGain=1
            WriteCurrentScreen=true
            TabAcceleration=false
            "#,
        );

        let loaded = load_audio_options(&conf, default_options());

        assert_eq!(loaded.visual_delay_seconds, 0.125);
        assert_eq!(loaded.master_volume, 100);
        assert!(!loaded.menu_music);
        assert!(!loaded.custom_sounds_enabled);
        assert_eq!(loaded.music_volume, 72);
        assert_eq!(loaded.music_wheel_switch_speed, 1);
        assert_eq!(loaded.sfx_volume, 100);
        assert_eq!(loaded.assist_tick_volume, 17);
        assert_eq!(loaded.output_device_index, Some(3));
        assert_eq!(loaded.sample_rate_hz, Some(48_000));
        assert!(!loaded.rate_mod_preserves_pitch);
        assert!(loaded.enable_replaygain);
        assert!(loaded.write_current_screen);
        assert!(!loaded.tab_acceleration);
    }

    #[test]
    fn load_audio_options_keeps_defaults_for_bad_values() {
        let default = default_options();
        let mut conf = SimpleIni::new();
        conf.load_str(
            r#"
            [Options]
            VisualDelaySeconds=bad
            MasterVolume=bad
            MusicWheelSwitchSpeed=bad
            AudioOutputDevice=bad
            AudioSampleRateHz=bad
            WriteCurrentScreen=bad
            TabAcceleration=bad
            "#,
        );

        assert_eq!(load_audio_options(&conf, default), default);
    }
}
