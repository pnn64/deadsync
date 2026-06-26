use super::*;
use crate::assets::{FontRole, current_machine_font_key};
use deadsync_rules::scroll::ScrollSpeedSetting;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedModType {
    X,
    C,
    M,
}

impl SpeedModType {
    /// Index used by the `TypeOfSpeedMod` row's `choices` vector
    /// (`["x-mod", "c-mod", "m-mod"]`).
    #[inline(always)]
    pub fn choice_index(self) -> usize {
        match self {
            Self::X => 0,
            Self::C => 1,
            Self::M => 2,
        }
    }

    #[inline(always)]
    pub fn from_choice_index(idx: usize) -> Self {
        match idx {
            0 => Self::X,
            1 => Self::C,
            2 => Self::M,
            _ => Self::C,
        }
    }

    /// Single-letter prefix (`"X"` / `"C"` / `"M"`) used in HUD text.
    #[inline(always)]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::C => "C",
            Self::M => "M",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpeedMod {
    pub mod_type: SpeedModType,
    pub value: f32,
}

impl SpeedMod {
    /// Player-facing display string (`"1.50x"`, `"C400"`, `"M250"`).
    pub fn display(&self) -> String {
        match self.mod_type {
            SpeedModType::X => format!("{:.2}x", self.value),
            SpeedModType::C => format!("C{}", self.value as i32),
            SpeedModType::M => format!("M{}", self.value as i32),
        }
    }
}

impl From<ScrollSpeedSetting> for SpeedMod {
    fn from(setting: ScrollSpeedSetting) -> Self {
        match setting {
            ScrollSpeedSetting::XMod(mult) => Self {
                mod_type: SpeedModType::X,
                value: mult,
            },
            ScrollSpeedSetting::CMod(bpm) => Self {
                mod_type: SpeedModType::C,
                value: bpm,
            },
            ScrollSpeedSetting::MMod(bpm) => Self {
                mod_type: SpeedModType::M,
                value: bpm,
            },
        }
    }
}

#[inline(always)]
pub(super) fn scroll_speed_for_mod(speed_mod: &SpeedMod) -> ScrollSpeedSetting {
    match speed_mod.mod_type {
        SpeedModType::C => ScrollSpeedSetting::CMod(speed_mod.value),
        SpeedModType::X => ScrollSpeedSetting::XMod(speed_mod.value),
        SpeedModType::M => ScrollSpeedSetting::MMod(speed_mod.value),
    }
}

#[inline(always)]
pub(super) fn sync_profile_scroll_speed(
    profile: &mut deadsync_profile::Profile,
    speed_mod: &SpeedMod,
) {
    profile.scroll_speed = scroll_speed_for_mod(speed_mod);
}

/// Map the persisted `NoCmodAlternative` preference to the speed-mod type the
/// player should be switched to, or `None` when no auto-switch is requested.
#[inline(always)]
pub(super) fn no_cmod_alt_speed_mod_type(
    alt: deadsync_profile::NoCmodAlternative,
) -> Option<SpeedModType> {
    match alt {
        deadsync_profile::NoCmodAlternative::None => None,
        deadsync_profile::NoCmodAlternative::XMod => Some(SpeedModType::X),
        deadsync_profile::NoCmodAlternative::MMod => Some(SpeedModType::M),
    }
}

/// Convert `speed_mod` to `new_type` while preserving the on-screen scroll
/// speed. This is the same math the Type-of-Speed-Mod row applies when the
/// player flips the type by hand: it derives the target BPM implied by the
/// current mod, then re-expresses it in the new mod's units. `reference_bpm` is
/// the chart's reference BPM and `rate` the active music rate.
pub(super) fn convert_speed_mod_to_type(
    speed_mod: &SpeedMod,
    new_type: SpeedModType,
    reference_bpm: f32,
    rate: f32,
) -> SpeedMod {
    let target_bpm: f32 = match speed_mod.mod_type {
        SpeedModType::C | SpeedModType::M => speed_mod.value,
        SpeedModType::X => (reference_bpm * rate * speed_mod.value).round(),
    };
    let value = match new_type {
        SpeedModType::X => {
            let denom = reference_bpm * rate;
            let raw = if denom.is_finite() && denom > 0.0 {
                target_bpm / denom
            } else {
                1.0
            };
            round_to_step(raw, 0.05).clamp(0.05, 20.0)
        }
        SpeedModType::C | SpeedModType::M => round_to_step(target_bpm, 5.0).clamp(5.0, 2000.0),
    };
    SpeedMod {
        mod_type: new_type,
        value,
    }
}

/// Pure core of the no-cmod substitution for a single player: given the
/// player's configured `base` speed mod, their `alt` preference, whether the
/// chart is no-cmod, and the chart `reference_bpm` + music `rate`, return the
/// scroll speed they should actually play with.
///
/// The substitution applies only when all three hold: the chart is no-cmod, the
/// player is on CMod, and a non-`None` alternative is set. Otherwise the base
/// speed is returned unchanged.
pub(super) fn effective_scroll_speed_with_alt(
    base: &SpeedMod,
    alt: deadsync_profile::NoCmodAlternative,
    is_no_cmod: bool,
    reference_bpm: f32,
    rate: f32,
) -> ScrollSpeedSetting {
    match no_cmod_alt_speed_mod_type(alt) {
        Some(new_type) if is_no_cmod && base.mod_type == SpeedModType::C => scroll_speed_for_mod(
            &convert_speed_mod_to_type(base, new_type, reference_bpm, rate),
        ),
        _ => scroll_speed_for_mod(base),
    }
}

/// Resolve the scroll speed each player will actually use for the upcoming
/// play, applying the "No CMod alternative" substitution for charts tagged
/// "no cmod".
///
/// For any player who is on CMod, is about to play a no-cmod chart, and has a
/// non-`None` alternative configured, their CMod speed is converted (preserving
/// on-screen speed) to the chosen X/M type. The substitution is written into
/// the (non-persisted) `player_profiles[..].scroll_speed` snapshot as well as
/// returned, so both the arrow-scroll path (which reads the returned array) and
/// the score-validity path (which reads `player_profiles`) observe the same
/// effective speed. The persisted profile is never touched, so returning to
/// song select restores the player's real mod automatically.
pub fn apply_no_cmod_alternative(state: &mut State) -> [ScrollSpeedSetting; PLAYER_SLOTS] {
    let is_no_cmod = state.song.is_no_cmod();
    let reference_bpm = reference_bpm_for_song(
        &state.song,
        resolve_p1_chart(&state.song, &state.chart_steps_index),
    );
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    std::array::from_fn(|player_idx| {
        let effective = effective_scroll_speed_with_alt(
            &state.speed_mod[player_idx],
            state.player_profiles[player_idx].no_cmod_alternative,
            is_no_cmod,
            reference_bpm,
            rate,
        );
        state.player_profiles[player_idx].scroll_speed = effective;
        effective
    })
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

#[inline(always)]
pub(super) fn fmt_average_error_bar_intensity(value: f32) -> String {
    format!("{value:.2}x")
}

pub(super) fn average_error_bar_intensity_choices() -> Vec<String> {
    let count = ((AVERAGE_ERROR_BAR_INTENSITY_MAX - AVERAGE_ERROR_BAR_INTENSITY_MIN)
        / AVERAGE_ERROR_BAR_INTENSITY_STEP)
        .round() as usize
        + 1;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(fmt_average_error_bar_intensity(
            AVERAGE_ERROR_BAR_INTENSITY_MIN + i as f32 * AVERAGE_ERROR_BAR_INTENSITY_STEP,
        ));
    }
    out
}

#[inline(always)]
pub(super) fn fmt_average_error_bar_interval_ms(ms: u32) -> String {
    format!("{ms}ms")
}

pub(super) fn average_error_bar_interval_choices() -> Vec<String> {
    let count = ((AVERAGE_ERROR_BAR_INTERVAL_MS_MAX - AVERAGE_ERROR_BAR_INTERVAL_MS_MIN)
        / AVERAGE_ERROR_BAR_INTERVAL_MS_STEP) as usize
        + 1;
    let mut out = Vec::with_capacity(count);
    let mut ms = AVERAGE_ERROR_BAR_INTERVAL_MS_MIN;
    while ms <= AVERAGE_ERROR_BAR_INTERVAL_MS_MAX {
        out.push(fmt_average_error_bar_interval_ms(ms));
        ms += AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;
    }
    out
}

#[inline(always)]
pub(super) fn fmt_text_error_bar_threshold_ms(ms: u32) -> String {
    format!("{ms}ms")
}

pub(super) fn text_error_bar_threshold_choices() -> Vec<String> {
    let mut out = Vec::with_capacity(
        (TEXT_ERROR_BAR_THRESHOLD_MS_MAX - TEXT_ERROR_BAR_THRESHOLD_MS_MIN + 1) as usize,
    );
    for ms in TEXT_ERROR_BAR_THRESHOLD_MS_MIN..=TEXT_ERROR_BAR_THRESHOLD_MS_MAX {
        out.push(fmt_text_error_bar_threshold_ms(ms));
    }
    out
}

pub(super) fn parse_text_error_bar_threshold_ms(choice: &str) -> Option<u32> {
    choice
        .trim()
        .trim_end_matches("ms")
        .trim()
        .parse::<u32>()
        .ok()
        .map(deadsync_profile::clamp_text_error_bar_threshold_ms)
}

#[inline(always)]
pub(super) fn fmt_long_error_bar_intensity(value: f32) -> String {
    format!("{value:.2}x")
}

pub(super) fn long_error_bar_intensity_choices() -> Vec<String> {
    let count = ((LONG_ERROR_BAR_INTENSITY_MAX - LONG_ERROR_BAR_INTENSITY_MIN)
        / LONG_ERROR_BAR_INTENSITY_STEP)
        .round() as usize
        + 1;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(fmt_long_error_bar_intensity(
            LONG_ERROR_BAR_INTENSITY_MIN + i as f32 * LONG_ERROR_BAR_INTENSITY_STEP,
        ));
    }
    out
}

#[inline(always)]
pub(super) fn fmt_long_error_bar_threshold_ms(ms: u32) -> String {
    format!("{ms}ms")
}

pub(super) fn long_error_bar_threshold_choices() -> Vec<String> {
    let mut out = Vec::with_capacity(
        (LONG_ERROR_BAR_THRESHOLD_MS_MAX - LONG_ERROR_BAR_THRESHOLD_MS_MIN + 1) as usize,
    );
    for ms in LONG_ERROR_BAR_THRESHOLD_MS_MIN..=LONG_ERROR_BAR_THRESHOLD_MS_MAX {
        out.push(fmt_long_error_bar_threshold_ms(ms));
    }
    out
}

#[inline(always)]
pub(super) fn fmt_long_error_bar_min_samples(n: u32) -> String {
    format!("{n}")
}

pub(super) fn long_error_bar_min_samples_choices() -> Vec<String> {
    let mut out = Vec::with_capacity(
        (LONG_ERROR_BAR_MIN_SAMPLES_MAX - LONG_ERROR_BAR_MIN_SAMPLES_MIN + 1) as usize,
    );
    for n in LONG_ERROR_BAR_MIN_SAMPLES_MIN..=LONG_ERROR_BAR_MIN_SAMPLES_MAX {
        out.push(fmt_long_error_bar_min_samples(n));
    }
    out
}

#[inline(always)]
pub(super) fn fmt_tilt_threshold_ms(ms: u32) -> String {
    format!("{ms}ms")
}

pub(super) fn tilt_threshold_choices() -> Vec<String> {
    let mut out = Vec::with_capacity((TILT_THRESHOLD_MAX_MS - TILT_THRESHOLD_MIN_MS + 1) as usize);
    for ms in TILT_THRESHOLD_MIN_MS..=TILT_THRESHOLD_MAX_MS {
        out.push(fmt_tilt_threshold_ms(ms));
    }
    out
}

pub(super) fn parse_tilt_threshold_ms(choice: &str) -> Option<u32> {
    choice
        .trim()
        .trim_end_matches("ms")
        .trim()
        .parse::<u32>()
        .ok()
        .map(deadsync_profile::clamp_tilt_threshold_ms)
}

pub(super) fn custom_fantastic_window_choices() -> Vec<String> {
    let lo = deadsync_profile::CUSTOM_FANTASTIC_WINDOW_MIN_MS;
    let hi = deadsync_profile::CUSTOM_FANTASTIC_WINDOW_MAX_MS;
    let mut out = Vec::with_capacity((hi - lo + 1) as usize);
    for ms in lo..=hi {
        out.push(format!("{ms}ms"));
    }
    out
}

pub(super) fn crossover_cue_duration_choices() -> Vec<String> {
    let lo = deadsync_profile::CROSSOVER_CUE_DURATION_MIN_MS;
    let hi = deadsync_profile::CROSSOVER_CUE_DURATION_MAX_MS;
    let step = deadsync_profile::CROSSOVER_CUE_DURATION_STEP_MS;
    let mut out = Vec::new();
    let mut ms = lo;
    while ms <= hi {
        out.push(format!("{ms}ms"));
        ms += step;
    }
    out
}

pub(super) fn crossover_cue_quantization_choices() -> Vec<String> {
    deadsync_profile::CROSSOVER_CUE_QUANTIZATIONS
        .iter()
        .map(|q| q.to_string())
        .collect()
}

pub(super) fn resolve_p1_chart<'a>(
    song: &'a SongData,
    chart_steps_index: &[usize; PLAYER_SLOTS],
) -> Option<&'a ChartData> {
    let target_chart_type = crate::game::profile::get_session_play_style().chart_type();
    song.chart_for_steps_index(target_chart_type, chart_steps_index[0])
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
    let is_random = p1_chart
        .is_some_and(|c| matches!(c.display_bpm, Some(deadsync_chart::ChartDisplayBpm::Random)));
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
    let [lo, hi] = song.display_bpm_pair_or(chart, [120.0, 120.0]);
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
    match speed_mod.mod_type {
        SpeedModType::X => {
            lo *= speed_mod.value;
            hi *= speed_mod.value;
        }
        SpeedModType::M => {
            if hi.abs() <= f32::EPSILON {
                return None;
            }
            lo *= speed_mod.value / hi;
            hi = speed_mod.value;
        }
        SpeedModType::C => {
            lo = speed_mod.value;
            hi = speed_mod.value;
        }
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
pub(super) fn perspective_speed_mult(perspective: deadsync_profile::Perspective) -> f32 {
    match perspective {
        deadsync_profile::Perspective::Overhead => 1.0,
        deadsync_profile::Perspective::Hallway => 0.75,
        deadsync_profile::Perspective::Distant => 33.0 / 39.0,
        deadsync_profile::Perspective::Incoming => 33.0 / 43.0,
        deadsync_profile::Perspective::Space => 0.825,
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
    profile: &deadsync_profile::Profile,
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
        asset_manager.with_font(current_machine_font_key(FontRole::Header), |metrics_font| {
            let w = deadlib_present::font::measure_line_width_logical(metrics_font, text, all_fonts)
                as f32;
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
