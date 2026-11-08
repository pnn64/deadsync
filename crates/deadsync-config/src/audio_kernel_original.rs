use super::*;
pub fn clock(value: &str) -> Result<NoteScrollClock, ()> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("rawaudio")
        || value.eq_ignore_ascii_case("raw")
        || value.eq_ignore_ascii_case("off")
        || value == "0"
        || value.eq_ignore_ascii_case("false")
    {
        Ok(NoteScrollClock::RawAudio)
    } else if value.eq_ignore_ascii_case("framestable")
        || value.eq_ignore_ascii_case("frame-stable")
        || value.eq_ignore_ascii_case("frame_stable")
        || value.eq_ignore_ascii_case("frame stable")
        || value.eq_ignore_ascii_case("smooth")
        || value.eq_ignore_ascii_case("smoothed")
        || value.eq_ignore_ascii_case("itgdestep")
        || value.eq_ignore_ascii_case("itg-destep")
        || value.eq_ignore_ascii_case("itgdestepped")
        || value.eq_ignore_ascii_case("itg-de-stepped")
        || value.eq_ignore_ascii_case("itg_de_stepped")
        || value.eq_ignore_ascii_case("itg")
        || value.eq_ignore_ascii_case("on")
        || value == "1"
        || value.eq_ignore_ascii_case("true")
    {
        Ok(NoteScrollClock::FrameStable)
    } else {
        Err(())
    }
}
pub fn anchor(value: &str) -> &'static str {
    const KEYS: &[&str] = &[
        "top-left",
        "top-right",
        "bottom-left",
        "bottom-right",
        "top-center",
        "bottom-center",
    ];
    let value = value.trim();
    KEYS.iter()
        .copied()
        .find(|&key| key.eq_ignore_ascii_case(value))
        .unwrap_or(DEFAULT_FRAME_STATS_OVERLAY_ANCHOR)
}
pub fn numeric(raw: &str) -> Option<bool> {
    raw.trim().parse::<u8>().ok().map(|n| n != 0)
}

pub fn numeric_or_default(raw: Option<&str>, default: bool) -> bool {
    raw.and_then(numeric).unwrap_or(default)
}
pub fn audio(conf: &SimpleIni, default: AudioOptions) -> AudioOptions {
    AudioOptions {
        visual_delay_seconds: conf
            .get("Options", "VisualDelaySeconds")
            .and_then(|value| value.parse().ok())
            .unwrap_or(default.visual_delay_seconds),
        note_scroll_clock: conf
            .get("Options", "NoteScrollClock")
            .and_then(|value| clock(value).ok())
            .unwrap_or(default.note_scroll_clock),
        master_volume: conf
            .get("Options", "MasterVolume")
            .and_then(|value| value.parse().ok())
            .map_or(default.master_volume, clamp_audio_volume_percent),
        menu_music: numeric_or_default(conf.get("Options", "MenuMusic"), default.menu_music),
        custom_sounds_enabled: numeric_or_default(
            conf.get("Options", "CustomSoundsEnabled"),
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
            .and_then(parse_auto_audio_output_device)
            .unwrap_or(default.output_device_index),
        sample_rate_hz: conf
            .get("Options", "AudioSampleRateHz")
            .and_then(parse_auto_audio_sample_rate_hz)
            .unwrap_or(default.sample_rate_hz),
        rate_mod_preserves_pitch: numeric_or_default(
            conf.get("Options", "RateModPreservesPitch"),
            default.rate_mod_preserves_pitch,
        ),
        enable_replaygain: numeric_or_default(
            conf.get("Options", "ReplayGain"),
            default.enable_replaygain,
        ),
        write_current_screen: conf
            .get("Options", "WriteCurrentScreen")
            .and_then(parse_bool_str)
            .unwrap_or(default.write_current_screen),
        tab_acceleration: conf
            .get("Options", "TabAcceleration")
            .and_then(parse_bool_str)
            .unwrap_or(default.tab_acceleration),
    }
}
