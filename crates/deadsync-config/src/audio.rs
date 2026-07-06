use crate::bools::{parse_bool_str, parse_u8_bool_or_default};
use crate::defaults::{
    DEFAULT_ASSIST_TICK_VOLUME, DEFAULT_CUSTOM_SOUNDS_ENABLED, DEFAULT_ENABLE_REPLAYGAIN,
    DEFAULT_MASTER_VOLUME, DEFAULT_MENU_MUSIC, DEFAULT_MUSIC_VOLUME,
    DEFAULT_MUSIC_WHEEL_SWITCH_SPEED, DEFAULT_RATE_MOD_PRESERVES_PITCH, DEFAULT_SFX_VOLUME,
    DEFAULT_TAB_ACCELERATION, DEFAULT_VISUAL_DELAY_SECONDS, DEFAULT_WRITE_CURRENT_SCREEN,
};
use crate::ini::SimpleIni;
use crate::writer::{push_bool, push_line};

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

impl Default for AudioOptions {
    fn default() -> Self {
        Self {
            visual_delay_seconds: DEFAULT_VISUAL_DELAY_SECONDS,
            master_volume: DEFAULT_MASTER_VOLUME,
            menu_music: DEFAULT_MENU_MUSIC,
            custom_sounds_enabled: DEFAULT_CUSTOM_SOUNDS_ENABLED,
            music_volume: DEFAULT_MUSIC_VOLUME,
            music_wheel_switch_speed: DEFAULT_MUSIC_WHEEL_SWITCH_SPEED,
            sfx_volume: DEFAULT_SFX_VOLUME,
            assist_tick_volume: DEFAULT_ASSIST_TICK_VOLUME,
            output_device_index: None,
            sample_rate_hz: None,
            rate_mod_preserves_pitch: DEFAULT_RATE_MOD_PRESERVES_PITCH,
            enable_replaygain: DEFAULT_ENABLE_REPLAYGAIN,
            write_current_screen: DEFAULT_WRITE_CURRENT_SCREEN,
            tab_acceleration: DEFAULT_TAB_ACCELERATION,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioDeviceOptions<'a> {
    pub output_device_index: Option<u16>,
    pub output_mode: &'a str,
    pub sample_rate_hz: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioRuntimeOptions<L, M> {
    pub linux_audio_backend: L,
    pub output_mode: M,
}

pub fn load_audio_runtime_options<L, M>(
    conf: &SimpleIni,
    default: AudioRuntimeOptions<L, M>,
    parse_linux_backend: impl Fn(&str) -> Option<L>,
    parse_output_mode: impl Fn(&str) -> Option<M>,
) -> AudioRuntimeOptions<L, M>
where
    L: Copy,
    M: Copy,
{
    AudioRuntimeOptions {
        linux_audio_backend: conf
            .get("Options", "LinuxAudioBackend")
            .and_then(|value| parse_linux_backend(&value))
            .unwrap_or(default.linux_audio_backend),
        output_mode: conf
            .get("Options", "AudioOutputMode")
            .and_then(|value| parse_output_mode(&value))
            .unwrap_or(default.output_mode),
    }
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

pub fn push_audio_device_option_lines(content: &mut String, options: AudioDeviceOptions<'_>) {
    push_line(
        content,
        "AudioOutputDevice",
        optional_audio_output_device_value(options.output_device_index),
    );
    push_line(content, "AudioOutputMode", options.output_mode);
    push_line(
        content,
        "AudioSampleRateHz",
        optional_audio_sample_rate_hz_value(options.sample_rate_hz),
    );
}

pub fn push_audio_playback_prefix_lines(content: &mut String, options: AudioOptions) {
    push_line(content, "VisualDelaySeconds", options.visual_delay_seconds);
    push_line(content, "MasterVolume", options.master_volume);
    push_bool(content, "MenuMusic", options.menu_music);
    push_bool(
        content,
        "CustomSoundsEnabled",
        options.custom_sounds_enabled,
    );
}

pub fn push_audio_music_option_lines(content: &mut String, options: AudioOptions) {
    push_line(content, "MusicVolume", options.music_volume);
    push_line(
        content,
        "MusicWheelSwitchSpeed",
        clamp_music_wheel_switch_speed(options.music_wheel_switch_speed),
    );
    push_bool(
        content,
        "RateModPreservesPitch",
        options.rate_mod_preserves_pitch,
    );
    push_bool(content, "ReplayGain", options.enable_replaygain);
}

pub fn push_audio_tail_option_lines(content: &mut String, options: AudioOptions) {
    push_line(content, "AssistTickVolume", options.assist_tick_volume);
    push_line(content, "SFXVolume", options.sfx_volume);
    push_bool(content, "TabAcceleration", options.tab_acceleration);
}

pub fn push_audio_write_current_screen_option_lines(content: &mut String, options: AudioOptions) {
    push_bool(content, "WriteCurrentScreen", options.write_current_screen);
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

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

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

    fn parse_letter_token(raw: &str) -> Option<char> {
        match raw.trim() {
            "a" => Some('a'),
            "b" => Some('b'),
            "c" => Some('c'),
            _ => None,
        }
    }

    #[test]
    fn loads_audio_runtime_options_with_token_parsers() {
        let loaded = load_audio_runtime_options(
            &ini(r#"
                [Options]
                LinuxAudioBackend=b
                AudioOutputMode=c
                "#),
            AudioRuntimeOptions {
                linux_audio_backend: 'a',
                output_mode: 'a',
            },
            parse_letter_token,
            parse_letter_token,
        );

        assert_eq!(
            loaded,
            AudioRuntimeOptions {
                linux_audio_backend: 'b',
                output_mode: 'c',
            },
        );
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
    fn writes_audio_device_option_lines() {
        let mut content = String::new();

        push_audio_device_option_lines(
            &mut content,
            AudioDeviceOptions {
                output_device_index: Some(2),
                output_mode: "Exclusive",
                sample_rate_hz: Some(48_000),
            },
        );

        assert_eq!(
            content,
            concat!(
                "AudioOutputDevice=2\n",
                "AudioOutputMode=Exclusive\n",
                "AudioSampleRateHz=48000\n",
            ),
        );
    }

    #[test]
    fn writes_audio_playback_option_lines() {
        let mut content = String::new();
        let mut options = default_options();
        options.music_wheel_switch_speed = 0;

        push_audio_playback_prefix_lines(&mut content, options);
        push_audio_music_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "VisualDelaySeconds=0\n",
                "MasterVolume=80\n",
                "MenuMusic=1\n",
                "CustomSoundsEnabled=1\n",
                "MusicVolume=80\n",
                "MusicWheelSwitchSpeed=1\n",
                "RateModPreservesPitch=1\n",
                "ReplayGain=0\n",
            ),
        );
    }

    #[test]
    fn writes_audio_tail_option_lines() {
        let mut content = String::new();
        let mut options = default_options();
        options.assist_tick_volume = 31;
        options.sfx_volume = 42;
        options.tab_acceleration = false;
        options.write_current_screen = true;

        push_audio_tail_option_lines(&mut content, options);
        push_audio_write_current_screen_option_lines(&mut content, options);

        assert_eq!(
            content,
            concat!(
                "AssistTickVolume=31\n",
                "SFXVolume=42\n",
                "TabAcceleration=0\n",
                "WriteCurrentScreen=1\n",
            ),
        );
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
