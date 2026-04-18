use super::*;

#[derive(Clone, Debug)]
pub struct SpeedMod {
    pub mod_type: String, // "X", "C", "M"
    pub value: f32,
}

#[inline(always)]
pub(super) fn scroll_speed_for_mod(
    speed_mod: &SpeedMod,
) -> crate::game::scroll::ScrollSpeedSetting {
    match speed_mod.mod_type.as_str() {
        "C" => crate::game::scroll::ScrollSpeedSetting::CMod(speed_mod.value),
        "X" => crate::game::scroll::ScrollSpeedSetting::XMod(speed_mod.value),
        "M" => crate::game::scroll::ScrollSpeedSetting::MMod(speed_mod.value),
        _ => crate::game::scroll::ScrollSpeedSetting::default(),
    }
}

#[inline(always)]
pub(super) fn sync_profile_scroll_speed(
    profile: &mut crate::game::profile::Profile,
    speed_mod: &SpeedMod,
) {
    profile.scroll_speed = scroll_speed_for_mod(speed_mod);
}

pub(super) fn fmt_music_rate(rate: f32) -> String {
    let scaled = (rate * 100.0).round() as i32;
    let int_part = scaled / 100;
    let frac2 = (scaled % 100).abs();
    if frac2 == 0 {
        format!("{int_part}")
    } else if frac2 % 10 == 0 {
        format!("{}.{}", int_part, frac2 / 10)
    } else {
        format!("{int_part}.{frac2:02}")
    }
}

#[inline(always)]
pub(super) fn fmt_tilt_intensity(value: f32) -> String {
    format!("{value:.2}")
}

pub(super) fn tilt_intensity_choices() -> Vec<String> {
    let count =
        ((TILT_INTENSITY_MAX - TILT_INTENSITY_MIN) / TILT_INTENSITY_STEP).round() as usize + 1;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(fmt_tilt_intensity(
            TILT_INTENSITY_MIN + i as f32 * TILT_INTENSITY_STEP,
        ));
    }
    out
}

pub(super) fn custom_fantastic_window_choices() -> Vec<String> {
    let lo = crate::game::profile::CUSTOM_FANTASTIC_WINDOW_MIN_MS;
    let hi = crate::game::profile::CUSTOM_FANTASTIC_WINDOW_MAX_MS;
    let mut out = Vec::with_capacity((hi - lo + 1) as usize);
    for ms in lo..=hi {
        out.push(format!("{ms}ms"));
    }
    out
}

pub(super) fn resolve_p1_chart<'a>(
    song: &'a SongData,
    chart_steps_index: &[usize; PLAYER_SLOTS],
) -> Option<&'a ChartData> {
    let target_chart_type = crate::game::profile::get_session_play_style().chart_type();
    crate::screens::select_music::chart_for_steps_index(
        song,
        target_chart_type,
        chart_steps_index[0],
    )
}

pub(super) fn reference_bpm_for_song(song: &SongData, chart: Option<&ChartData>) -> f32 {
    let bpm = song
        .chart_display_bpm_range(chart)
        .map(|(_, hi)| hi as f32)
        .unwrap_or(song.max_bpm as f32);
    if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        120.0
    }
}

/// Translate a difficulty index (0=Beginner..4=Challenge) to a localized display name.
pub(super) fn difficulty_display_name(index: usize) -> String {
    let key = match index {
        0 => "BeginnerDifficulty",
        1 => "EasyDifficulty",
        2 => "MediumDifficulty",
        3 => "HardDifficulty",
        4 => "ChallengeDifficulty",
        _ => "EditDifficulty",
    };
    tr("SelectCourse", key).to_string()
}

pub(super) fn music_rate_display_name(state: &State) -> String {
    let p1_chart = resolve_p1_chart(&state.song, &state.chart_steps_index);
    let is_random = p1_chart.is_some_and(|c| {
        matches!(
            c.display_bpm,
            Some(crate::game::chart::ChartDisplayBpm::Random)
        )
    });
    let bpm_str = if is_random {
        "???".to_string()
    } else {
        let reference_bpm = reference_bpm_for_song(&state.song, p1_chart);
        let effective_bpm = f64::from(reference_bpm) * f64::from(state.music_rate);
        if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
            format!("{}", effective_bpm.round() as i32)
        } else {
            format!("{effective_bpm:.1}")
        }
    };
    tr_fmt("PlayerOptions", "MusicRate", &[("bpm", &bpm_str)]).replace("\\n", "\n")
}

#[inline(always)]
pub(super) fn display_bpm_pair_for_options(
    song: &SongData,
    chart: Option<&ChartData>,
    music_rate: f32,
) -> Option<(f32, f32)> {
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let (mut lo, mut hi) = song
        .chart_display_bpm_range(chart)
        .map_or((120.0_f32, 120.0_f32), |(a, b)| (a as f32, b as f32));
    if !lo.is_finite() || !hi.is_finite() || lo <= 0.0 || hi <= 0.0 {
        lo = 120.0;
        hi = 120.0;
    }
    Some((lo * rate, hi * rate))
}

#[inline(always)]
pub(super) fn speed_mod_bpm_pair(
    song: &SongData,
    chart: Option<&ChartData>,
    speed_mod: &SpeedMod,
    music_rate: f32,
) -> Option<(f32, f32)> {
    let (mut lo, mut hi) = display_bpm_pair_for_options(song, chart, music_rate)?;
    match speed_mod.mod_type.as_str() {
        "X" => {
            lo *= speed_mod.value;
            hi *= speed_mod.value;
        }
        "M" => {
            if hi.abs() <= f32::EPSILON {
                return None;
            }
            lo *= speed_mod.value / hi;
            hi = speed_mod.value;
        }
        "C" => {
            lo = speed_mod.value;
            hi = speed_mod.value;
        }
        _ => {}
    }
    if lo.is_finite() && hi.is_finite() {
        Some((lo, hi))
    } else {
        None
    }
}

#[inline(always)]
pub(super) fn format_speed_bpm_pair(lo: f32, hi: f32) -> String {
    let lo_i = lo.round() as i32;
    let hi_i = hi.round() as i32;
    if lo_i == hi_i {
        lo_i.to_string()
    } else {
        format!("{lo_i}-{hi_i}")
    }
}

#[inline(always)]
pub(super) fn perspective_speed_mult(perspective: crate::game::profile::Perspective) -> f32 {
    match perspective {
        crate::game::profile::Perspective::Overhead => 1.0,
        crate::game::profile::Perspective::Hallway => 0.75,
        crate::game::profile::Perspective::Distant => 33.0 / 39.0,
        crate::game::profile::Perspective::Incoming => 33.0 / 43.0,
        crate::game::profile::Perspective::Space => 0.825,
    }
}

#[inline(always)]
pub(super) fn speed_mod_helper_scroll_text(
    song: &SongData,
    chart: Option<&ChartData>,
    speed_mod: &SpeedMod,
    music_rate: f32,
) -> String {
    speed_mod_bpm_pair(song, chart, speed_mod, music_rate)
        .map_or_else(String::new, |(lo, hi)| format_speed_bpm_pair(lo, hi))
}

#[inline(always)]
pub(super) fn speed_mod_helper_scaled_text(
    song: &SongData,
    chart: Option<&ChartData>,
    speed_mod: &SpeedMod,
    music_rate: f32,
    profile: &crate::game::profile::Profile,
) -> String {
    let Some((mut lo, mut hi)) = speed_mod_bpm_pair(song, chart, speed_mod, music_rate) else {
        return String::new();
    };
    let mini = profile.mini_percent.clamp(-100, 150) as f32;
    let scale = ((200.0 - mini) / 200.0) * perspective_speed_mult(profile.perspective);
    lo *= scale;
    hi *= scale;
    format_speed_bpm_pair(lo, hi)
}

#[inline(always)]
pub(super) fn measure_wendy_text_width(asset_manager: &AssetManager, text: &str) -> f32 {
    let mut out_w = 1.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("wendy", |metrics_font| {
            let w = crate::engine::present::font::measure_line_width_logical(
                metrics_font,
                text,
                all_fonts,
            ) as f32;
            if w.is_finite() && w > 0.0 {
                out_w = w;
            }
        });
    });
    out_w
}

#[inline(always)]
pub(super) fn round_to_step(x: f32, step: f32) -> f32 {
    if !x.is_finite() || !step.is_finite() || step <= 0.0 {
        return x;
    }
    (x / step).round() * step
}
