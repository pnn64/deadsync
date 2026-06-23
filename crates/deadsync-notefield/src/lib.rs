use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, TexturedMeshVertex};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_core::timing::beat_to_note_row;
use deadsync_noteskin::NoteAnimPart;
use deadsync_rules::judgment::{JudgeGrade, TimingWindow};
use deadsync_rules::note::{MineResult, Note, NoteCountStat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::StreamSegment;
use deadsync_rules::timing::{self, TimeSignatureSegment, WindowCounts, default_time_signature};
use glam::{Mat4 as Matrix4, Vec3 as Vector3};
use std::sync::Arc;

const HOLD_BODY_LEGACY_SEGMENT_LIMIT: usize = 512;
const HOLD_BODY_SEGMENT_SAFETY_MAX: usize = 65_536;
const BUMPY_Z_MAGNITUDE: f32 = 40.0;
const BUMPY_Z_ANGLE_DIVISOR: f32 = 16.0;
const BEAT_OFFSET_HEIGHT: f32 = 15.0;
const BEAT_PI_HEIGHT: f32 = 2.0;
const BLINK_MOD_FREQUENCY: f32 = 0.3333;
const CENTER_LINE_Y: f32 = 160.0;
const DRUNK_COLUMN_FREQUENCY: f32 = 0.2;
const DRUNK_OFFSET_FREQUENCY: f32 = 10.0;
const DRUNK_ARROW_MAGNITUDE: f32 = 0.5;
const FADE_DIST_Y: f32 = 40.0;
const TORNADO_X_OFFSET_FREQUENCY: f32 = 6.0;
const TIPSY_TIMER_FREQUENCY: f32 = 1.2;
const TIPSY_COLUMN_FREQUENCY: f32 = 1.8;
const TIPSY_ARROW_MAGNITUDE: f32 = 0.4;
const ARROW_EFFECT_PIXEL_SIZE: f32 = 64.0;
pub const COLUMN_CUE_Y_OFFSET: f32 = 80.0;
const COLUMN_CUE_FADE_TIME: f32 = 0.15;
const CROSSOVER_CUE_HEIGHT_REDUCTION: f32 = 270.0;
const COLUMN_FLASH_DEFAULT_Y_OFFSET: f32 = 80.0;
const COLUMN_FLASH_COMPACT_Y_OFFSET: f32 = 70.0;
const COLUMN_FLASH_COMPACT_HEIGHT_TRIM: f32 = 270.0;
const COLUMN_FLASH_DEFAULT_FADE: f32 = 0.333;
const COLUMN_FLASH_COMPACT_FADE: f32 = 0.2;
const COLUMN_FLASH_NORMAL_ALPHA: f32 = 0.66;
const COLUMN_FLASH_DIMMED_ALPHA: f32 = 0.3;
const ERROR_BAR_SEG_ALPHA_BASE: f32 = 0.3;
const BOOST_MOD_MIN_CLAMP: f32 = -400.0;
const BOOST_MOD_MAX_CLAMP: f32 = 400.0;
const BRAKE_MOD_MIN_CLAMP: f32 = -400.0;
const BRAKE_MOD_MAX_CLAMP: f32 = 400.0;
const WAVE_MOD_MAGNITUDE: f32 = 20.0;
const WAVE_MOD_HEIGHT: f32 = 38.0;
const EXPAND_MULTIPLIER_FREQUENCY: f32 = 3.0;
const EXPAND_MULTIPLIER_SCALE_FROM_LOW: f32 = -1.0;
const EXPAND_MULTIPLIER_SCALE_FROM_HIGH: f32 = 1.0;
const EXPAND_MULTIPLIER_SCALE_TO_LOW: f32 = 0.75;
const EXPAND_MULTIPLIER_SCALE_TO_HIGH: f32 = 1.75;
const EXPAND_SPEED_SCALE_FROM_LOW: f32 = 0.0;
const EXPAND_SPEED_SCALE_FROM_HIGH: f32 = 1.0;
const EXPAND_SPEED_SCALE_TO_LOW: f32 = 1.0;
const MAX_NOTES_AFTER: usize = 64;
const FANTASTIC_BLUE_RGBA: [f32; 4] = rgba8_const(0x21, 0xcc, 0xe8);
const EXCELLENT_RGBA: [f32; 4] = rgba8_const(0xe2, 0x9c, 0x18);
const GREAT_RGBA: [f32; 4] = rgba8_const(0x66, 0xc9, 0x55);
const DECENT_RGBA: [f32; 4] = rgba8_const(0xb4, 0x5c, 0xff);
const WAY_OFF_RGBA: [f32; 4] = rgba8_const(0xc9, 0x85, 0x5e);
const FA_PLUS_WHITE_RGBA: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

const fn rgba8_const(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

pub struct BuiltNotefield {
    pub layout_center_x: f32,
    pub field_actors: Vec<Arc<[Actor]>>,
    pub judgment_actors: Option<Vec<Arc<[Actor]>>>,
    pub combo_actors: Option<Vec<Arc<[Actor]>>>,
}

impl BuiltNotefield {
    pub fn empty(layout_center_x: f32) -> Self {
        Self {
            layout_center_x,
            field_actors: Vec::new(),
            judgment_actors: None,
            combo_actors: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldAnimParts {
    pub head: NoteAnimPart,
    pub body: NoteAnimPart,
    pub topcap: NoteAnimPart,
    pub bottomcap: NoteAnimPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapReplacementHead {
    pub is_roll: bool,
    pub part: NoteAnimPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditBeatBarInfo {
    pub frame: u32,
    pub measure_index: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TornadoBounds {
    pub min_x: f32,
    pub max_x: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoteAlphaParams {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisualEffectParams {
    pub tiny: f32,
    pub pulse_inner: f32,
    pub pulse_outer: f32,
    pub pulse_offset: f32,
    pub pulse_period: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub dizzy: f32,
    pub bumpy: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelYParams {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub boomerang: f32,
    pub expand: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoteXParams {
    pub screen_height: f32,
    pub tornado: f32,
    pub drunk: f32,
    pub flip: f32,
    pub invert: f32,
    pub beat: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMiniIndicatorPosition {
    Default,
    UnderUpArrow,
}

#[derive(Clone, Copy, Debug)]
pub struct ZmodLayoutParams {
    pub judgment_height: f32,
    pub has_error_bar: bool,
    pub has_judgment_texture: bool,
    pub error_bar_up: bool,
    pub has_measure_counter: bool,
    pub measure_counter_up: bool,
    pub broken_run: bool,
    pub mini_indicator_position: LayoutMiniIndicatorPosition,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HudLayoutOffsets {
    pub judgment_extra_y: f32,
    pub combo_extra_y: f32,
    pub error_bar_extra_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HudLayoutParams {
    pub zmod: ZmodLayoutParams,
    pub has_judgment_texture: bool,
    pub error_bar_up: bool,
    pub error_bar_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZmodLayoutYs {
    pub combo_y: f32,
    pub measure_counter_y: Option<f32>,
    pub subtractive_scoring_y: f32,
    pub subtractive_scoring_addx: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudLayoutYs {
    pub judgment_y: f32,
    pub error_bar_y: f32,
    pub error_bar_max_h: f32,
    pub zmod_layout: ZmodLayoutYs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorScoreType {
    Itg,
    Ex,
    HardEx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorSize {
    Default,
    Large,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorMode {
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorColorStyle {
    Default,
    Detailed,
    Combo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniIndicatorSubtractiveDisplay {
    CountThenPercent,
    Points,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MiniIndicatorProgress {
    pub kept_percent: f64,
    pub lost_percent: f64,
    pub pace_percent: f64,
    pub current_score_percent: f64,
    pub current_possible_ratio: f64,
    pub current_possible_dp: i32,
    pub actual_dp: i32,
    pub white_count: u32,
    pub white_10ms_count: u32,
    pub w2: u32,
    pub w3: u32,
    pub w4: u32,
    pub w5: u32,
    pub miss: u32,
    pub let_go: u32,
    pub mines_hit: u32,
    pub judged_any: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZmodMeasureCounterText {
    Break(i32),
    Ratio { current: i32, total: i32 },
    Total(i32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZmodMiniIndicatorText {
    Percent(f64),
    SignedPercent { value: f64, negative: bool },
    NegativeInt(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct ZmodMiniIndicatorParams {
    pub mode: MiniIndicatorMode,
    pub color_style: MiniIndicatorColorStyle,
    pub subtractive_display: MiniIndicatorSubtractiveDisplay,
    pub score_type: MiniIndicatorScoreType,
    pub combo_color: [f32; 4],
    pub is_failing: bool,
    pub life: f32,
    pub rival_score_percent: f64,
    pub target_score_percent: f64,
    pub stream_completion: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZmodMiniIndicatorOutput {
    pub text: ZmodMiniIndicatorText,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZmodComboColorStyle {
    None,
    Rainbow,
    RainbowScroll,
    Glow,
    Solid,
}

#[derive(Clone, Copy, Debug)]
pub struct ZmodComboColorParams {
    pub style: ZmodComboColorStyle,
    pub full_combo_mode: bool,
    pub combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub quint_active: bool,
    pub elapsed_s: f32,
}

#[inline(always)]
pub const fn zmod_combo_quint_active(show_fa_plus_window: bool, counts: WindowCounts) -> bool {
    show_fa_plus_window
        && counts.w0 > 0
        && counts.w1 == 0
        && counts.w2 == 0
        && counts.w3 == 0
        && counts.w4 == 0
        && counts.w5 == 0
        && counts.miss == 0
}

#[inline(always)]
pub const fn zmod_resolved_mini_indicator_mode(
    mode: MiniIndicatorMode,
    subtractive_scoring: bool,
    pacemaker: bool,
) -> MiniIndicatorMode {
    if !matches!(mode, MiniIndicatorMode::None) {
        mode
    } else if subtractive_scoring {
        MiniIndicatorMode::SubtractiveScoring
    } else if pacemaker {
        MiniIndicatorMode::Pacemaker
    } else {
        MiniIndicatorMode::None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct JudgmentTiltParams {
    pub enabled: bool,
    pub grade: JudgeGrade,
    pub time_error_ms: f32,
    pub min_threshold_ms: f32,
    pub max_threshold_ms: f32,
    pub multiplier: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TapJudgmentRowsParams {
    pub grade: JudgeGrade,
    pub window: Option<TimingWindow>,
    pub time_error_ms: f32,
    pub frame_rows: usize,
    pub show_fa_plus_window: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub custom_fantastic_window: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayModsAttackMode {
    Off,
    #[default]
    On,
    Random,
}

pub const DISPLAY_TURN_MIRROR: u16 = 1 << 0;
pub const DISPLAY_TURN_LEFT: u16 = 1 << 1;
pub const DISPLAY_TURN_RIGHT: u16 = 1 << 2;
pub const DISPLAY_TURN_LR_MIRROR: u16 = 1 << 3;
pub const DISPLAY_TURN_UD_MIRROR: u16 = 1 << 4;
pub const DISPLAY_TURN_SHUFFLE: u16 = 1 << 5;
pub const DISPLAY_TURN_BLENDER: u16 = 1 << 6;
pub const DISPLAY_TURN_RANDOM: u16 = 1 << 7;

#[derive(Clone, Copy, Debug)]
pub struct GameplayModsTextParams<'a> {
    pub speed: ScrollSpeedSetting,
    pub noteskin: &'a str,
    pub insert_mask: u8,
    pub remove_mask: u8,
    pub holds_mask: u8,
    pub turn_bits: u16,
    pub attack_mode: GameplayModsAttackMode,
    pub mini_percent: i16,
    pub spacing_percent: i16,
    pub visual_delay_ms: i16,
    pub average_error_bar_active: bool,
    pub avg_error_bar_intensity_centi: i16,
    pub avg_error_bar_interval_ms: u16,
    pub accel: [i16; 5],
    pub visual: [i16; 9],
    pub appearance: [i16; 5],
    pub scroll: [i16; 5],
    pub perspective_tilt: i16,
    pub perspective_skew: i16,
    pub dark: i16,
    pub blind: i16,
    pub cover: i16,
    pub disabled_timing_windows: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum FieldPlacement {
    P1,
    P2,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewOverride {
    pub field_zoom: Option<f32>,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub force_center_1player: bool,
    pub center_receptors_y: bool,
    pub receptor_y: Option<f32>,
    pub edit_beat_bars: bool,
    pub hide_display_mods: bool,
    pub hide_combo: bool,
}

#[derive(Clone, Copy, Default)]
pub struct ProxyCaptureRequests {
    pub note_field: bool,
    pub judgment: bool,
    pub combo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnFlashLayout {
    pub y_offset: f32,
    pub height_trim: f32,
    pub fade: f32,
}

fn valid_edit_time_signature(sig: TimeSignatureSegment) -> TimeSignatureSegment {
    if sig.numerator > 0 && sig.denominator > 0 {
        sig
    } else {
        default_time_signature()
    }
}

fn edit_time_signature_at(segments: &[TimeSignatureSegment], index: usize) -> TimeSignatureSegment {
    if segments.is_empty() {
        default_time_signature()
    } else {
        valid_edit_time_signature(segments[index])
    }
}

fn edit_time_signature_count(segments: &[TimeSignatureSegment]) -> usize {
    segments.len().max(1)
}

fn edit_bar_step_rows(sig: TimeSignatureSegment) -> i32 {
    (beat_to_note_row(sig.denominator as f32 / 4.0) / 4).max(1)
}

fn edit_measure_frequency(sig: TimeSignatureSegment) -> i32 {
    sig.numerator.saturating_mul(4).max(1)
}

fn edit_measure_bars_in_segment(start_row: i32, end_row: i32, sig: TimeSignatureSegment) -> i64 {
    if end_row <= start_row {
        return 0;
    }
    let step = i64::from(edit_bar_step_rows(sig));
    let freq = i64::from(edit_measure_frequency(sig));
    let bars = (i64::from(end_row) - i64::from(start_row) - 1) / step + 1;
    (bars - 1) / freq + 1
}

fn edit_measure_index_before_segment(
    segments: &[TimeSignatureSegment],
    segment_index: usize,
) -> i64 {
    let mut measure_index = 0;
    for i in 0..segment_index {
        let sig = edit_time_signature_at(segments, i);
        let next_sig = edit_time_signature_at(segments, i + 1);
        measure_index += edit_measure_bars_in_segment(
            beat_to_note_row(sig.beat),
            beat_to_note_row(next_sig.beat),
            sig,
        );
    }
    measure_index
}

fn edit_time_signature_index_at_row(segments: &[TimeSignatureSegment], row: i32) -> usize {
    if segments.is_empty() {
        return 0;
    }

    let mut index = 0;
    for (i, sig) in segments.iter().enumerate() {
        if beat_to_note_row(sig.beat) <= row {
            index = i;
        } else {
            break;
        }
    }
    index
}

pub fn edit_beat_bar_info_for_row(
    row: i32,
    segments: &[TimeSignatureSegment],
) -> Option<EditBeatBarInfo> {
    if row < 0 {
        return None;
    }

    let segment_index = edit_time_signature_index_at_row(segments, row);
    let sig = edit_time_signature_at(segments, segment_index);
    let segment_start_row = beat_to_note_row(sig.beat);
    if row < segment_start_row {
        return None;
    }

    let step_rows = edit_bar_step_rows(sig);
    let local_rows = row - segment_start_row;
    if local_rows % step_rows != 0 {
        return None;
    }

    let bars_drawn = local_rows / step_rows;
    let measure_frequency = edit_measure_frequency(sig);
    let is_measure = bars_drawn % measure_frequency == 0;
    let frame = if is_measure {
        0
    } else if bars_drawn % 4 == 0 {
        1
    } else if bars_drawn % 2 == 0 {
        2
    } else {
        3
    };
    let measure_index = is_measure.then(|| {
        edit_measure_index_before_segment(segments, segment_index)
            + i64::from(bars_drawn / measure_frequency)
    });

    Some(EditBeatBarInfo {
        frame,
        measure_index,
    })
}

fn edit_bar_gcd(a: i32, b: i32) -> i32 {
    let mut a = i64::from(a).abs();
    let mut b = i64::from(b).abs();
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a.clamp(1, i64::from(i32::MAX)) as i32
}

pub fn edit_bar_candidate_step_rows(segments: &[TimeSignatureSegment]) -> i32 {
    let mut step = edit_bar_step_rows(edit_time_signature_at(segments, 0));
    for i in 0..edit_time_signature_count(segments) {
        let sig = edit_time_signature_at(segments, i);
        step = edit_bar_gcd(step, edit_bar_step_rows(sig));
        step = edit_bar_gcd(step, beat_to_note_row(sig.beat));
    }
    step.max(1)
}

pub fn edit_bar_scroll_speed(
    scroll_speed: ScrollSpeedSetting,
    reference_bpm: f32,
    music_rate: f32,
) -> f32 {
    match scroll_speed {
        ScrollSpeedSetting::XMod(multiplier) => multiplier,
        ScrollSpeedSetting::MMod(_) => scroll_speed.beat_multiplier(reference_bpm, music_rate),
        ScrollSpeedSetting::CMod(_) => 4.0,
    }
    .max(0.0)
}

#[inline(always)]
pub fn beat_scroll_travel(
    note_displayed_beat: f32,
    current_displayed_beat: f32,
    displayed_speed_percent: f32,
) -> f32 {
    (note_displayed_beat - current_displayed_beat)
        * ScrollSpeedSetting::ARROW_SPACING
        * displayed_speed_percent
}

#[inline(always)]
pub fn edit_beat_scroll_travel(note_beat: f32, current_beat: f32) -> f32 {
    (note_beat - current_beat) * ScrollSpeedSetting::ARROW_SPACING
}

pub fn scaled_edit_bar_alpha(scroll_speed: f32, visible_at: f32, full_at: f32) -> f32 {
    ((scroll_speed - visible_at) / (full_at - visible_at)).clamp(0.0, 1.0)
}

#[inline(always)]
pub fn sm_scale(v: f32, in0: f32, in1: f32, out0: f32, out1: f32) -> f32 {
    let denom = in1 - in0;
    if denom.abs() < 1e-6 {
        return out1;
    }
    ((v - in0) / denom).mul_add(out1 - out0, out0)
}

#[inline(always)]
pub fn quantize_step(v: f32, step: f32) -> f32 {
    ((v + step * 0.5) / step).trunc() * step
}

#[inline(always)]
pub fn quantize_centi_i32(value: f64) -> i32 {
    (if value.is_finite() { value } else { 0.0 } * 100.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

#[inline(always)]
pub fn quantize_centi_u32(value: f64) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 100.0).round()).clamp(0.0, u32::MAX as f64) as u32
}

#[inline(always)]
pub fn mod_percent_key(level: f32) -> i16 {
    let value = if level.is_finite() { level } else { 0.0 };
    (value * 100.0)
        .round()
        .clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

#[inline(always)]
pub fn clamp_rounded_i16(value: f32) -> i16 {
    let value = if value.is_finite() { value } else { 0.0 };
    value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn trim_float2(value: f32) -> String {
    let mut out = format!("{value:.2}");
    if out.contains('.') {
        while out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
    }
    out
}

fn format_speed_mod_for_display(speed: ScrollSpeedSetting) -> String {
    match speed {
        ScrollSpeedSetting::XMod(mult) => {
            if (mult - 1.0).abs() <= 0.000_1 {
                "1x".to_string()
            } else {
                let mut out = trim_float2(mult);
                out.push('x');
                out
            }
        }
        ScrollSpeedSetting::CMod(bpm) => {
            if (bpm - bpm.round()).abs() <= 0.000_1 {
                format!("C{}", bpm.round() as i32)
            } else {
                let mut out = String::from("C");
                out.push_str(&trim_float2(bpm));
                out
            }
        }
        ScrollSpeedSetting::MMod(bpm) => {
            if (bpm - bpm.round()).abs() <= 0.000_1 {
                format!("m{}", bpm.round() as i32)
            } else {
                let mut out = String::from("m");
                out.push_str(&trim_float2(bpm));
                out
            }
        }
    }
}

fn append_mod_part(parts: &mut Vec<String>, percent: i16, name: &str) {
    if percent == 0 {
        return;
    }
    if percent == 100 {
        parts.push(name.to_string());
    } else {
        parts.push(format!("{percent}% {name}"));
    }
}

fn append_mini_part(parts: &mut Vec<String>, mini_percent: i16) {
    if mini_percent != 0 {
        parts.push(format!("{mini_percent}% Mini"));
    }
}

fn append_spacing_part(parts: &mut Vec<String>, spacing_percent: i16) {
    if spacing_percent != 0 {
        parts.push(format!("{spacing_percent}% Spacing"));
    }
}

fn append_average_error_bar_part(parts: &mut Vec<String>, params: GameplayModsTextParams<'_>) {
    if !params.average_error_bar_active {
        return;
    }
    let zoom = trim_float2(params.avg_error_bar_intensity_centi as f32 / 100.0);
    parts.push(format!(
        "ErrorBar{}x(Avg:{}ms)",
        zoom, params.avg_error_bar_interval_ms
    ));
}

fn push_display_mod_option(out: &mut String, option: &str) {
    for ch in option.chars() {
        out.push(if ch == ' ' { '\u{00A0}' } else { ch });
    }
}

fn join_display_mod_parts(parts: &[String]) -> String {
    let mut out =
        String::with_capacity(parts.iter().map(String::len).sum::<usize>() + parts.len() * 2);
    for (idx, part) in parts.iter().enumerate() {
        if idx != 0 {
            out.push_str(", ");
        }
        push_display_mod_option(&mut out, part);
    }
    out
}

fn append_perspective_parts(parts: &mut Vec<String>, tilt: i16, skew: i16) {
    if tilt == 0 && skew == 0 {
        parts.push("Overhead".to_string());
        return;
    }
    if skew == 0 {
        if tilt > 0 {
            append_mod_part(parts, tilt, "Distant");
        } else {
            append_mod_part(parts, -tilt, "Hallway");
        }
        return;
    }
    if skew == tilt {
        append_mod_part(parts, skew, "Space");
        return;
    }
    if skew == -tilt {
        append_mod_part(parts, skew, "Incoming");
        return;
    }
    append_mod_part(parts, skew, "Skew");
    append_mod_part(parts, tilt, "Tilt");
}

fn append_turn_parts(parts: &mut Vec<String>, bits: u16) {
    for (bit, name) in [
        (DISPLAY_TURN_MIRROR, "Mirror"),
        (DISPLAY_TURN_LEFT, "Left"),
        (DISPLAY_TURN_RIGHT, "Right"),
        (DISPLAY_TURN_LR_MIRROR, "LR-Mirror"),
        (DISPLAY_TURN_UD_MIRROR, "UD-Mirror"),
        (DISPLAY_TURN_SHUFFLE, "Shuffle"),
        (DISPLAY_TURN_BLENDER, "Blender"),
        (DISPLAY_TURN_RANDOM, "Random"),
    ] {
        if bits & bit != 0 {
            parts.push(name.to_string());
        }
    }
}

fn attack_mode_name(mode: GameplayModsAttackMode) -> Option<&'static str> {
    match mode {
        GameplayModsAttackMode::Off => Some("NoAttacks"),
        GameplayModsAttackMode::On => None,
        GameplayModsAttackMode::Random => Some("RandomAttacks"),
    }
}

fn push_transform_parts(parts: &mut Vec<String>, insert_mask: u8, remove_mask: u8, holds_mask: u8) {
    if (remove_mask & (1 << 2)) != 0 {
        parts.push("NoHolds".to_string());
    }
    if (holds_mask & (1 << 3)) != 0 {
        parts.push("NoRolls".to_string());
    }
    if (remove_mask & (1 << 1)) != 0 {
        parts.push("NoMines".to_string());
    }
    if (remove_mask & (1 << 0)) != 0 {
        parts.push("Little".to_string());
    }
    if (insert_mask & (1 << 0)) != 0 {
        parts.push("Wide".to_string());
    }
    if (insert_mask & (1 << 1)) != 0 {
        parts.push("Big".to_string());
    }
    if (insert_mask & (1 << 2)) != 0 {
        parts.push("Quick".to_string());
    }
    if (insert_mask & (1 << 3)) != 0 {
        parts.push("BMRize".to_string());
    }
    if (insert_mask & (1 << 4)) != 0 {
        parts.push("Skippy".to_string());
    }
    if (insert_mask & (1 << 7)) != 0 {
        parts.push("Mines".to_string());
    }
    if (insert_mask & (1 << 5)) != 0 {
        parts.push("Echo".to_string());
    }
    if (insert_mask & (1 << 6)) != 0 {
        parts.push("Stomp".to_string());
    }
    if (holds_mask & (1 << 0)) != 0 {
        parts.push("Planted".to_string());
    }
    if (holds_mask & (1 << 1)) != 0 {
        parts.push("Floored".to_string());
    }
    if (holds_mask & (1 << 2)) != 0 {
        parts.push("Twister".to_string());
    }
    if (holds_mask & (1 << 4)) != 0 {
        parts.push("HoldsToRolls".to_string());
    }
    if (remove_mask & (1 << 3)) != 0 {
        parts.push("NoJumps".to_string());
    }
    if (remove_mask & (1 << 4)) != 0 {
        parts.push("NoHands".to_string());
    }
    if (remove_mask & (1 << 6)) != 0 {
        parts.push("NoLifts".to_string());
    }
    if (remove_mask & (1 << 7)) != 0 {
        parts.push("NoFakes".to_string());
    }
    if (remove_mask & (1 << 5)) != 0 {
        parts.push("NoQuads".to_string());
    }
}

fn disabled_timing_windows_name(bits: u8) -> Option<String> {
    if bits == 0 {
        return None;
    }
    let mut text = String::from("No ");
    let mut first = true;
    for i in 0..5 {
        if bits & (1 << i) == 0 {
            continue;
        }
        if first {
            first = false;
        } else {
            text.push('/');
        }
        text.push('W');
        text.push(char::from(b'1' + i as u8));
    }
    Some(text)
}

pub fn gameplay_mods_text(params: GameplayModsTextParams<'_>) -> String {
    let mut parts = Vec::with_capacity(32);
    parts.push(format_speed_mod_for_display(params.speed));

    for (percent, name) in
        params
            .accel
            .into_iter()
            .zip(["Boost", "Brake", "Wave", "Expand", "Boomerang"])
    {
        append_mod_part(&mut parts, percent, name);
    }
    for (percent, name) in params.visual.into_iter().zip([
        "Drunk",
        "Dizzy",
        "Confusion",
        "Flip",
        "Invert",
        "Tornado",
        "Tipsy",
        "Bumpy",
        "Beat",
    ]) {
        append_mod_part(&mut parts, percent, name);
    }
    append_mini_part(&mut parts, params.mini_percent);
    append_spacing_part(&mut parts, params.spacing_percent);
    for (percent, name) in
        params
            .appearance
            .into_iter()
            .zip(["Hidden", "Sudden", "Stealth", "Blink", "RandomVanish"])
    {
        append_mod_part(&mut parts, percent, name);
    }
    for (percent, name) in
        params
            .scroll
            .into_iter()
            .zip(["Reverse", "Split", "Alternate", "Cross", "Centered"])
    {
        append_mod_part(&mut parts, percent, name);
    }
    append_mod_part(&mut parts, params.dark, "Dark");
    append_mod_part(&mut parts, params.blind, "Blind");
    append_mod_part(&mut parts, params.cover, "Hide BG");

    if let Some(name) = attack_mode_name(params.attack_mode) {
        parts.push(name.to_string());
    }
    append_turn_parts(&mut parts, params.turn_bits);
    push_transform_parts(
        &mut parts,
        params.insert_mask,
        params.remove_mask,
        params.holds_mask,
    );
    append_perspective_parts(&mut parts, params.perspective_tilt, params.perspective_skew);
    parts.push(params.noteskin.to_string());
    if params.visual_delay_ms != 0 {
        parts.push(format!("{}ms VisualDelay", params.visual_delay_ms));
    }
    append_average_error_bar_part(&mut parts, params);
    if let Some(disabled_windows) = disabled_timing_windows_name(params.disabled_timing_windows) {
        parts.push(disabled_windows);
    }

    join_display_mod_parts(&parts)
}

#[inline(always)]
pub fn beat_factor(song_beat: f32) -> f32 {
    let accel_time = 0.2_f32;
    let total_time = 0.5_f32;
    let mut beat = song_beat + accel_time;
    let even_beat = (beat as i32 % 2) != 0;
    if beat < 0.0 {
        return 0.0;
    }
    beat -= beat.trunc();
    beat += 1.0;
    beat -= beat.trunc();
    if beat >= total_time {
        return 0.0;
    }
    let mut factor = if beat < accel_time {
        let t = sm_scale(beat, 0.0, accel_time, 0.0, 1.0);
        t * t
    } else {
        let t = sm_scale(beat, accel_time, total_time, 1.0, 0.0);
        1.0 - (1.0 - t) * (1.0 - t)
    };
    if even_beat {
        factor *= -1.0;
    }
    factor * 20.0
}

#[inline(always)]
pub fn mod_divisor(value: f32) -> f32 {
    if value.abs() > 0.001 {
        value
    } else if value.is_sign_negative() {
        -0.001
    } else {
        0.001
    }
}

#[inline(always)]
pub fn bumpy_angle(y: f32, offset: f32, period: f32) -> f32 {
    let offset = if offset.is_finite() { offset } else { 0.0 };
    let period = if period.is_finite() { period } else { 0.0 };
    let divisor = mod_divisor(period.mul_add(BUMPY_Z_ANGLE_DIVISOR, BUMPY_Z_ANGLE_DIVISOR));
    (y + 100.0 * offset) / divisor
}

#[inline(always)]
pub fn apply_accel_y_with_peak(
    raw_y: f32,
    elapsed: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> (f32, bool) {
    if raw_y < 0.0 {
        return (raw_y, true);
    }
    let mut y = raw_y;
    if accel.boost > f32::EPSILON {
        let new_y = y * 1.5 / ((y + effect_height / 1.2) / effect_height);
        let adjust = (accel.boost * (new_y - y)).clamp(BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
        y += adjust;
    }
    if accel.brake > f32::EPSILON {
        let scale = sm_scale(y, 0.0, effect_height, 0.0, 1.0);
        let new_y = y * scale;
        let adjust = (accel.brake * (new_y - y)).clamp(BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
        y += adjust;
    }
    if accel.wave > f32::EPSILON {
        y += accel.wave * WAVE_MOD_MAGNITUDE * (y / WAVE_MOD_HEIGHT).sin();
    }
    let mut before_boomerang_peak = true;
    if accel.boomerang > f32::EPSILON {
        let peak_at_y = screen_height * 0.75;
        before_boomerang_peak = y < peak_at_y;
        y = (-y * y / screen_height) + 1.5 * y;
    }
    if accel.expand > f32::EPSILON {
        let seconds = elapsed.rem_euclid((std::f32::consts::PI * 2.0).max(f32::EPSILON));
        let multiplier = sm_scale(
            (seconds * EXPAND_MULTIPLIER_FREQUENCY).cos(),
            EXPAND_MULTIPLIER_SCALE_FROM_LOW,
            EXPAND_MULTIPLIER_SCALE_FROM_HIGH,
            EXPAND_MULTIPLIER_SCALE_TO_LOW,
            EXPAND_MULTIPLIER_SCALE_TO_HIGH,
        );
        y *= sm_scale(
            accel.expand,
            EXPAND_SPEED_SCALE_FROM_LOW,
            EXPAND_SPEED_SCALE_FROM_HIGH,
            EXPAND_SPEED_SCALE_TO_LOW,
            multiplier,
        );
    }
    (y, before_boomerang_peak)
}

#[inline(always)]
pub fn apply_accel_y(
    raw_y: f32,
    elapsed: f32,
    effect_height: f32,
    screen_height: f32,
    accel: AccelYParams,
) -> f32 {
    apply_accel_y_with_peak(raw_y, elapsed, effect_height, screen_height, accel).0
}

#[inline(always)]
pub fn note_world_z_for_bumpy(y: f32, bumpy: f32, offset: f32, period: f32) -> f32 {
    if bumpy.abs() <= f32::EPSILON || !bumpy.is_finite() {
        return 0.0;
    }
    bumpy * BUMPY_Z_MAGNITUDE * bumpy_angle(y, offset, period).sin()
}

#[inline(always)]
pub fn itg_actor_rotation_z(deg: f32) -> f32 {
    -deg
}

#[inline(always)]
pub fn visual_hold_body_needs_z_buffer(params: VisualEffectParams) -> bool {
    // ITGmania ArrowEffects::NeedZBuffer checks global Bumpy but not BumpyN.
    signed_effect_active(params.bumpy)
}

#[inline(always)]
pub fn visual_use_legacy_hold_sprites(
    col_bumpy: f32,
    drunk: f32,
    tornado: f32,
    beat: f32,
    pulse_outer: f32,
) -> bool {
    col_bumpy.abs() <= f32::EPSILON
        && !signed_effect_active(drunk)
        && !signed_effect_active(tornado)
        && !signed_effect_active(beat)
        && pulse_outer.abs() <= f32::EPSILON
}

#[inline(always)]
pub fn visual_tiny_zoom(params: VisualEffectParams) -> f32 {
    let tiny = params.tiny;
    if tiny.abs() <= f32::EPSILON || !tiny.is_finite() {
        return 1.0;
    }
    0.5_f32.powf(tiny)
}

#[inline(always)]
pub fn visual_pulse_active(params: VisualEffectParams) -> bool {
    params.pulse_inner.abs() > f32::EPSILON || params.pulse_outer.abs() > f32::EPSILON
}

#[inline(always)]
pub fn visual_pulse_inner_zoom(params: VisualEffectParams) -> f32 {
    if !visual_pulse_active(params) {
        return 1.0;
    }
    let inner = if params.pulse_inner.is_finite() {
        params.pulse_inner.mul_add(0.5, 1.0)
    } else {
        1.0
    };
    if inner.abs() <= f32::EPSILON {
        0.01
    } else {
        inner
    }
}

#[inline(always)]
pub fn visual_pulse_zoom_for_y(y: f32, params: VisualEffectParams) -> f32 {
    if !visual_pulse_active(params) {
        return 1.0;
    }
    let outer = if params.pulse_outer.is_finite() {
        params.pulse_outer
    } else {
        0.0
    };
    let offset = if params.pulse_offset.is_finite() {
        params.pulse_offset
    } else {
        0.0
    };
    let period = if params.pulse_period.is_finite() {
        params.pulse_period
    } else {
        0.0
    };
    let divisor = mod_divisor(0.4 * ARROW_EFFECT_PIXEL_SIZE * (1.0 + period));
    ((y + 100.0 * offset) / divisor)
        .sin()
        .mul_add(outer * 0.5, visual_pulse_inner_zoom(params))
}

#[inline(always)]
pub fn visual_arrow_effect_zoom(y: f32, params: VisualEffectParams) -> f32 {
    visual_tiny_zoom(params) * visual_pulse_zoom_for_y(y, params)
}

#[inline(always)]
pub fn visual_confusion_rotation_deg(song_beat: f32, params: VisualEffectParams) -> f32 {
    let mut itg_rotation = 0.0;
    if params.confusion_offset.abs() > f32::EPSILON {
        itg_rotation += params.confusion_offset * (180.0 / std::f32::consts::PI);
    }
    if params.confusion.abs() > f32::EPSILON {
        let confusion = (song_beat * params.confusion).rem_euclid(std::f32::consts::TAU);
        itg_rotation += confusion * (-180.0 / std::f32::consts::PI);
    }
    itg_actor_rotation_z(itg_rotation)
}

#[inline(always)]
pub fn visual_dizzy_rotation_deg(
    note_beat: f32,
    song_beat: f32,
    params: VisualEffectParams,
) -> f32 {
    if params.dizzy.abs() <= f32::EPSILON {
        return 0.0;
    }
    let dizzy = ((note_beat - song_beat) * params.dizzy) % std::f32::consts::TAU;
    dizzy * (180.0 / std::f32::consts::PI)
}

#[inline(always)]
pub fn visual_note_rotation_z(
    note_beat: f32,
    song_beat: f32,
    is_hold_head: bool,
    params: VisualEffectParams,
) -> f32 {
    let mut rotation = visual_confusion_rotation_deg(song_beat, params);
    if params.dizzy.abs() > f32::EPSILON && !is_hold_head {
        rotation += itg_actor_rotation_z(visual_dizzy_rotation_deg(note_beat, song_beat, params));
    }
    rotation
}

#[inline(always)]
pub fn visual_effect_params_for_col(
    mut params: VisualEffectParams,
    local_col: usize,
    tiny_cols: &[f32],
    confusion_offset_cols: &[f32],
    bumpy_cols: &[f32],
) -> VisualEffectParams {
    params.tiny += tiny_cols.get(local_col).copied().unwrap_or(0.0);
    params.confusion_offset += confusion_offset_cols
        .get(local_col)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0);
    params.bumpy += bumpy_cols.get(local_col).copied().unwrap_or(0.0);
    params
}

#[inline(always)]
pub fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline(always)]
pub const fn column_flash_layout(compact: bool) -> ColumnFlashLayout {
    if compact {
        ColumnFlashLayout {
            y_offset: COLUMN_FLASH_COMPACT_Y_OFFSET,
            height_trim: COLUMN_FLASH_COMPACT_HEIGHT_TRIM,
            fade: COLUMN_FLASH_COMPACT_FADE,
        }
    } else {
        ColumnFlashLayout {
            y_offset: COLUMN_FLASH_DEFAULT_Y_OFFSET,
            height_trim: 0.0,
            fade: COLUMN_FLASH_DEFAULT_FADE,
        }
    }
}

#[inline(always)]
pub fn column_flash_height(screen_height: f32, layout: ColumnFlashLayout) -> f32 {
    (screen_height - layout.y_offset - layout.height_trim).max(0.0)
}

#[inline(always)]
pub fn column_flash_reverse_bottom_y(
    layout: ColumnFlashLayout,
    lane_width: f32,
    notefield_offset_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    layout.y_offset * 3.0 + receptor_reverse_y + lane_width * 0.5 + notefield_offset_y
}

#[inline(always)]
pub fn column_flash_reverse_top_y(
    layout: ColumnFlashLayout,
    lane_width: f32,
    flash_height: f32,
    notefield_offset_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    column_flash_reverse_bottom_y(layout, lane_width, notefield_offset_y, receptor_reverse_y)
        - flash_height
}

#[inline(always)]
pub fn column_flash_alpha_at(
    started_at: f32,
    current_time: f32,
    duration: f32,
    base_alpha: f32,
) -> f32 {
    let elapsed = current_time - started_at;
    if !elapsed.is_finite() || elapsed < 0.0 || elapsed >= duration || duration <= 0.0 {
        return 0.0;
    }
    let t = (elapsed / duration).clamp(0.0, 1.0);
    base_alpha * (1.0 - t * t)
}

#[inline(always)]
pub const fn column_flash_base_alpha(dimmed: bool) -> f32 {
    if dimmed {
        COLUMN_FLASH_DIMMED_ALPHA
    } else {
        COLUMN_FLASH_NORMAL_ALPHA
    }
}

#[inline(always)]
pub fn column_flash_alpha(started_at: f32, current_time: f32, duration: f32, dimmed: bool) -> f32 {
    column_flash_alpha_at(
        started_at,
        current_time,
        duration,
        column_flash_base_alpha(dimmed),
    )
}

#[inline(always)]
pub fn column_flash_color(grade: JudgeGrade, blue_fantastic: bool, alpha: f32) -> [f32; 4] {
    let mut rgba = match grade {
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                FANTASTIC_BLUE_RGBA
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }
        JudgeGrade::Excellent => [0.88, 0.61, 0.09, 1.0],
        JudgeGrade::Great => [0.40, 0.79, 0.33, 1.0],
        JudgeGrade::Decent => [0.70, 0.36, 1.00, 1.0],
        JudgeGrade::WayOff => [0.78, 0.52, 0.36, 1.0],
        JudgeGrade::Miss => [1.0, 0.0, 0.0, 1.0],
    };
    rgba[3] = alpha;
    rgba
}

#[inline(always)]
pub fn field_effect_height(screen_height: f32, tilt: f32) -> f32 {
    screen_height + tilt.abs() * 200.0
}

#[inline(always)]
pub fn signed_effect_active(value: f32) -> bool {
    value.is_finite() && value.abs() > f32::EPSILON
}

#[inline(always)]
pub fn itg_actor_glow_alpha(alpha: f32) -> f32 {
    if alpha.is_finite() {
        alpha.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[inline(always)]
pub const fn hold_glow_color(alpha: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, alpha]
}

#[inline(always)]
pub fn column_cue_height(screen_height: f32) -> f32 {
    (screen_height - COLUMN_CUE_Y_OFFSET).max(0.0)
}

#[inline(always)]
pub fn crossover_cue_height(screen_height: f32) -> f32 {
    (screen_height - COLUMN_CUE_Y_OFFSET - CROSSOVER_CUE_HEIGHT_REDUCTION).max(0.0)
}

#[inline(always)]
pub fn column_cue_reverse_bottom_y(
    lane_width: f32,
    notefield_offset_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    COLUMN_CUE_Y_OFFSET * 3.0 + receptor_reverse_y + lane_width * 0.5 + notefield_offset_y
}

#[inline(always)]
pub fn column_cue_reverse_top_y(
    lane_width: f32,
    cue_height: f32,
    notefield_offset_y: f32,
    receptor_reverse_y: f32,
) -> f32 {
    column_cue_reverse_bottom_y(lane_width, notefield_offset_y, receptor_reverse_y) - cue_height
}

#[inline(always)]
pub fn column_cue_alpha(elapsed_real: f32, duration_real: f32) -> f32 {
    if !elapsed_real.is_finite() || !duration_real.is_finite() {
        return 0.0;
    }
    if elapsed_real < 0.0 || elapsed_real > duration_real {
        return 0.0;
    }
    if duration_real <= COLUMN_CUE_FADE_TIME * 2.0 {
        return 0.0;
    }
    if elapsed_real < COLUMN_CUE_FADE_TIME {
        let t = (elapsed_real / COLUMN_CUE_FADE_TIME).clamp(0.0, 1.0);
        return 1.0 - (1.0 - t) * (1.0 - t);
    }
    if elapsed_real > duration_real - COLUMN_CUE_FADE_TIME {
        let t = ((elapsed_real - (duration_real - COLUMN_CUE_FADE_TIME)) / COLUMN_CUE_FADE_TIME)
            .clamp(0.0, 1.0);
        return 1.0 - t * t;
    }
    1.0
}

#[inline(always)]
pub fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if !age.is_finite() || age < 0.0 {
        return 0.0;
    }
    if multi_tick {
        if age < 0.03 {
            1.0
        } else if age < dur {
            1.0 - (age - 0.03) / (dur - 0.03).max(0.000_001)
        } else {
            0.0
        }
    } else if age < dur {
        1.0
    } else {
        0.0
    }
}

#[inline(always)]
pub fn error_bar_flash_alpha(now: f32, started_at: Option<f32>, dur: f32) -> f32 {
    let Some(t0) = started_at else {
        return ERROR_BAR_SEG_ALPHA_BASE;
    };
    let age = now - t0;
    if !age.is_finite() || age < 0.0 || age >= dur {
        return ERROR_BAR_SEG_ALPHA_BASE;
    }
    let t = (age / dur).clamp(0.0, 1.0);
    1.0 - (1.0 - ERROR_BAR_SEG_ALPHA_BASE) * t
}

#[inline(always)]
pub fn error_bar_boundaries_s(
    windows_s: [f32; 5],
    w0_s: Option<f32>,
    show_fa_plus_window: bool,
    max_window_ix: usize,
) -> ([f32; 6], usize) {
    let mut out = [0.0_f32; 6];
    let mut len: usize = 0;
    let base_end = max_window_ix.min(4) + 1; // 1..=5
    for wi in 1..=base_end {
        if show_fa_plus_window && wi == 1 {
            if let Some(w0) = w0_s
                && len < out.len()
            {
                out[len] = w0;
                len += 1;
            }
            if len < out.len() {
                out[len] = windows_s[0];
                len += 1;
            }
        } else if len < out.len() {
            out[len] = windows_s[wi - 1];
            len += 1;
        }
    }
    (out, len)
}

#[inline(always)]
pub fn stream_segment_index_exclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure >= s.end as f32)
}

#[inline(always)]
pub fn stream_segment_index_inclusive_end(segs: &[StreamSegment], curr_measure: f32) -> usize {
    if curr_measure.is_nan() {
        return segs.len();
    }
    segs.partition_point(|s| curr_measure > s.end as f32)
}

pub fn zmod_broken_run_end(segs: &[StreamSegment], start_index: usize) -> (usize, bool) {
    let Some(first) = segs.get(start_index).copied() else {
        return (0, false);
    };
    if first.is_break {
        return (first.end, false);
    }

    let last_index = segs.len().saturating_sub(1);
    let mut end = first.end;
    let mut broken = false;

    for i in (start_index + 1)..segs.len() {
        let seg = segs[i];
        let len = seg.end - seg.start;
        if seg.is_break {
            if len < 4 && i != last_index {
                end += len;
                broken = true;
                continue;
            }
            break;
        }

        broken = true;
        end += len;
        if !segs[i - 1].is_break {
            end += 1;
        }
    }

    (end, broken)
}

pub fn zmod_broken_run_segment(
    segs: &[StreamSegment],
    curr_measure: f32,
) -> Option<(usize, usize, bool)> {
    for (i, seg) in segs.iter().copied().enumerate() {
        if seg.is_break {
            if curr_measure < seg.end as f32 {
                return Some((i, seg.end, false));
            }
            continue;
        }
        let (end, broken) = zmod_broken_run_end(segs, i);
        if curr_measure < end as f32 {
            return Some((i, end, broken));
        }
    }
    None
}

pub fn zmod_run_timer_index(segs: &[StreamSegment], curr_measure: f32) -> Option<usize> {
    let i = stream_segment_index_inclusive_end(segs, curr_measure);
    if i < segs.len() { Some(i) } else { None }
}

pub fn zmod_measure_counter_text(
    curr_beat_floor: f32,
    curr_measure: f32,
    segs: &[StreamSegment],
    stream_index_unshifted: usize,
    is_lookahead: bool,
    lookahead: u8,
    multiplier: f32,
) -> Option<ZmodMeasureCounterText> {
    if segs.is_empty() {
        return None;
    }

    let mut stream_index = stream_index_unshifted as isize;
    let beat_div4 = curr_beat_floor / 4.0;

    if curr_measure < 0.0 {
        if !is_lookahead {
            let first = segs[0];
            if !first.is_break {
                let v = ((-beat_div4) + (1.0 * multiplier)).floor() as i32;
                return Some(ZmodMeasureCounterText::Break(v));
            }
            let len = (first.end - first.start) as i32;
            let v_unscaled = (-beat_div4).floor() as i32 + 1 + len;
            let v = ((v_unscaled as f32) * multiplier).floor() as i32;
            return Some(ZmodMeasureCounterText::Break(v));
        }
        if !segs[0].is_break {
            stream_index -= 1;
        }
    }

    let seg = stream_index
        .try_into()
        .ok()
        .and_then(|i: usize| segs.get(i).copied())?;

    let segment_start = seg.start as f32;
    let segment_end = seg.end as f32;
    let seg_len = ((segment_end - segment_start) * multiplier).floor() as i32;
    let curr_count = (((beat_div4 - segment_start) * multiplier).floor() as i32) + 1;

    if seg.is_break {
        if lookahead == 0 {
            return None;
        }
        if is_lookahead {
            Some(ZmodMeasureCounterText::Break(seg_len))
        } else {
            let remaining = seg_len - curr_count + 1;
            Some(ZmodMeasureCounterText::Break(remaining))
        }
    } else if !is_lookahead && curr_count != 0 {
        Some(ZmodMeasureCounterText::Ratio {
            current: curr_count,
            total: seg_len,
        })
    } else {
        Some(ZmodMeasureCounterText::Total(seg_len))
    }
}

pub fn zmod_broken_run_counter_text(
    curr_measure: f32,
    segs: &[StreamSegment],
    broken_index: usize,
    broken_end: usize,
) -> Option<ZmodMeasureCounterText> {
    let seg0 = segs.get(broken_index).copied()?;
    if seg0.is_break {
        return None;
    }
    let curr_count = (curr_measure - (seg0.start as f32)).floor() as i32 + 1;
    let len = (broken_end - seg0.start) as i32;
    if curr_measure < 0.0 {
        // BrokenRunCounter.lua special-cases negative time.
        let first = segs[0];
        if first.is_break {
            let first_len = (first.end - first.start) as i32;
            let v = (-curr_measure).floor() as i32 + 1 + first_len;
            Some(ZmodMeasureCounterText::Break(v))
        } else {
            let v = (-curr_measure).floor() as i32 + 1;
            Some(ZmodMeasureCounterText::Break(v))
        }
    } else if curr_count != 0 {
        Some(ZmodMeasureCounterText::Ratio {
            current: curr_count,
            total: len,
        })
    } else {
        Some(ZmodMeasureCounterText::Total(len))
    }
}

#[inline(always)]
pub const fn timing_window_from_num(n: usize) -> TimingWindow {
    match n {
        0 => TimingWindow::W0,
        1 => TimingWindow::W1,
        2 => TimingWindow::W2,
        3 => TimingWindow::W3,
        4 => TimingWindow::W4,
        _ => TimingWindow::W5,
    }
}

#[inline(always)]
pub const fn error_bar_color_for_window(
    window: TimingWindow,
    show_fa_plus_window: bool,
) -> [f32; 4] {
    match window {
        TimingWindow::W0 => FANTASTIC_BLUE_RGBA,
        TimingWindow::W1 => {
            if show_fa_plus_window {
                FA_PLUS_WHITE_RGBA
            } else {
                FANTASTIC_BLUE_RGBA
            }
        }
        TimingWindow::W2 => EXCELLENT_RGBA,
        TimingWindow::W3 => GREAT_RGBA,
        TimingWindow::W4 => DECENT_RGBA,
        TimingWindow::W5 => WAY_OFF_RGBA,
    }
}

#[inline(always)]
pub fn zmod_percent_from_points(points: i32, total: i32) -> f64 {
    if total <= 0 {
        return 0.0;
    }
    ((f64::from(points.max(0)) / f64::from(total)) * 10000.0).floor() / 100.0
}

#[inline(always)]
pub fn zmod_subtractive_counter_state(
    progress: &MiniIndicatorProgress,
    score_type: MiniIndicatorScoreType,
) -> (u32, bool) {
    let forced_percent = progress.w3 > 0
        || progress.w4 > 0
        || progress.w5 > 0
        || progress.miss > 0
        || progress.let_go > 0
        || progress.mines_hit > 0;
    match score_type {
        MiniIndicatorScoreType::Itg => (progress.w2, forced_percent || progress.w2 > 10),
        MiniIndicatorScoreType::Ex => (
            progress.white_count,
            forced_percent || progress.w2 > 0 || progress.white_count > 10,
        ),
        MiniIndicatorScoreType::HardEx => (
            progress.white_10ms_count,
            forced_percent || progress.w2 > 0 || progress.white_10ms_count > 10,
        ),
    }
}

#[inline(always)]
pub fn zmod_subtractive_points(
    progress: &MiniIndicatorProgress,
    score_type: MiniIndicatorScoreType,
) -> u32 {
    match score_type {
        MiniIndicatorScoreType::Itg => progress
            .current_possible_dp
            .saturating_sub(progress.actual_dp)
            .max(0) as u32,
        MiniIndicatorScoreType::Ex => progress
            .white_count
            .saturating_add(progress.w2.saturating_mul(3))
            .saturating_add(progress.w3.saturating_mul(5))
            .saturating_add(
                progress
                    .w4
                    .saturating_add(progress.w5)
                    .saturating_add(progress.miss)
                    .saturating_mul(7),
            )
            .saturating_add(progress.let_go.saturating_mul(2))
            .saturating_add(progress.mines_hit.saturating_mul(2)),
        MiniIndicatorScoreType::HardEx => progress
            .white_10ms_count
            .saturating_add(progress.w2.saturating_mul(5))
            .saturating_add(
                progress
                    .w3
                    .saturating_add(progress.w4)
                    .saturating_add(progress.w5)
                    .saturating_add(progress.miss)
                    .saturating_mul(7),
            )
            .saturating_add(progress.let_go.saturating_mul(2))
            .saturating_add(progress.mines_hit.saturating_mul(2)),
    }
}

#[inline(always)]
pub const fn zmod_mini_indicator_zoom(size: MiniIndicatorSize) -> f32 {
    match size {
        MiniIndicatorSize::Default => 0.35,
        MiniIndicatorSize::Large => 0.5,
    }
}

#[inline(always)]
pub fn zmod_rival_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace)).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace)).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

#[inline(always)]
pub fn zmod_pacemaker_color(pace: f64, rival_pace: f64) -> [f32; 4] {
    let r = (1.0 - (pace - rival_pace) / 100.0).clamp(0.0, 1.0) as f32;
    let g = (0.5 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    let b = (1.0 - (rival_pace - pace) / 100.0).clamp(0.0, 1.0) as f32;
    [r, g, b, 1.0]
}

#[inline(always)]
fn zmod_indicator_score_color_style(
    score_percent: f64,
    style: MiniIndicatorColorStyle,
) -> [f32; 4] {
    match style {
        MiniIndicatorColorStyle::Default => zmod_indicator_default_color(score_percent),
        MiniIndicatorColorStyle::Detailed => zmod_indicator_detailed_color(score_percent),
        MiniIndicatorColorStyle::Combo => zmod_indicator_default_color(score_percent),
    }
}

#[inline(always)]
fn zmod_mini_indicator_score_color(
    score_percent: f64,
    params: ZmodMiniIndicatorParams,
) -> [f32; 4] {
    match params.color_style {
        MiniIndicatorColorStyle::Combo => params.combo_color,
        style => zmod_indicator_score_color_style(score_percent, style),
    }
}

pub fn zmod_mini_indicator_output(
    progress: &MiniIndicatorProgress,
    params: ZmodMiniIndicatorParams,
) -> Option<ZmodMiniIndicatorOutput> {
    if params.mode == MiniIndicatorMode::None || !progress.judged_any {
        return None;
    }

    match params.mode {
        MiniIndicatorMode::SubtractiveScoring => {
            if params.subtractive_display == MiniIndicatorSubtractiveDisplay::Points {
                let points = zmod_subtractive_points(progress, params.score_type);
                let score = progress.kept_percent.clamp(0.0, 100.0);
                return Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::NegativeInt(points),
                    color: zmod_mini_indicator_score_color(score, params),
                });
            }

            let (count, entered_percent_mode) =
                zmod_subtractive_counter_state(progress, params.score_type);
            if !(entered_percent_mode || params.is_failing || params.life <= 0.0) && count > 0 {
                let color = if params.color_style == MiniIndicatorColorStyle::Combo {
                    params.combo_color
                } else {
                    rgba8(0xff, 0x55, 0xcc)
                };
                return Some(ZmodMiniIndicatorOutput {
                    text: ZmodMiniIndicatorText::NegativeInt(count),
                    color,
                });
            }

            let score = progress.kept_percent.clamp(0.0, 100.0);
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: progress.lost_percent.clamp(0.0, 100.0),
                    negative: true,
                },
                color: zmod_mini_indicator_score_color(score, params),
            })
        }
        MiniIndicatorMode::PredictiveScoring => {
            let score = progress.kept_percent.clamp(0.0, 100.0);
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(score),
                color: zmod_mini_indicator_score_color(score, params),
            })
        }
        MiniIndicatorMode::PaceScoring => {
            let pace = progress.pace_percent.clamp(0.0, 100.0);
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(pace),
                color: zmod_mini_indicator_score_color(pace, params),
            })
        }
        MiniIndicatorMode::RivalScoring => {
            let pace = progress.current_score_percent.clamp(0.0, 100.0);
            let rival_score = params.rival_score_percent.clamp(0.0, 100.0);
            let rival_pace =
                (progress.current_possible_ratio * 10000.0 * rival_score).floor() / 10000.0;
            let diff = (pace - rival_pace).abs();
            let color = if params.color_style == MiniIndicatorColorStyle::Combo {
                params.combo_color
            } else {
                zmod_rival_color(pace, rival_pace)
            };
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: diff,
                    negative: pace < rival_pace,
                },
                color,
            })
        }
        MiniIndicatorMode::Pacemaker => {
            let pace = (progress.current_score_percent.clamp(0.0, 100.0) * 100.0).floor();
            let target_ratio = (params.target_score_percent / 100.0).clamp(0.0, 1.0);
            let rival_pace =
                (progress.current_possible_ratio * 1_000_000.0 * target_ratio).floor() / 100.0;
            let (value, negative) = if pace < rival_pace {
                (((rival_pace - pace).floor() / 100.0).max(0.0), true)
            } else {
                (((pace - rival_pace).floor() / 100.0).max(0.0), false)
            };
            let color = if params.color_style == MiniIndicatorColorStyle::Combo {
                params.combo_color
            } else {
                zmod_pacemaker_color(pace, rival_pace)
            };
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent { value, negative },
                color,
            })
        }
        MiniIndicatorMode::StreamProg => {
            let completion = params.stream_completion?;
            let color = if completion >= 0.9 {
                [
                    0.0,
                    1.0,
                    ((completion - 0.9) * 10.0).clamp(0.0, 1.0) as f32,
                    1.0,
                ]
            } else if completion >= 0.5 {
                [
                    ((0.9 - completion) * 10.0 / 4.0).clamp(0.0, 1.0) as f32,
                    1.0,
                    0.0,
                    1.0,
                ]
            } else {
                [
                    1.0,
                    ((completion - 0.2) * 10.0 / 3.0).clamp(0.0, 1.0) as f32,
                    0.0,
                    1.0,
                ]
            };
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent((completion * 100.0).clamp(0.0, 100.0)),
                color,
            })
        }
        MiniIndicatorMode::None => None,
    }
}

#[inline(always)]
pub fn zmod_combo_glow_color(color1: [f32; 4], color2: [f32; 4], elapsed: f32) -> [f32; 4] {
    let effect_period = 0.8_f32;
    let through = (elapsed / effect_period).fract();
    let anim_t = ((through * 2.0 * std::f32::consts::PI).sin() + 1.0) * 0.5;
    [
        color1[0] + (color2[0] - color1[0]) * anim_t,
        color1[1] + (color2[1] - color1[1]) * anim_t,
        color1[2] + (color2[2] - color1[2]) * anim_t,
        1.0,
    ]
}

#[inline(always)]
fn rgba8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

#[inline(always)]
pub fn zmod_combo_glow_pair(grade: JudgeGrade, quint: bool) -> ([f32; 4], [f32; 4]) {
    if quint && matches!(grade, JudgeGrade::Fantastic) {
        return (rgba8(0xf7, 0xc0, 0xfe), rgba8(0xe9, 0x28, 0xff));
    }
    match grade {
        JudgeGrade::Fantastic => (rgba8(0xc8, 0xff, 0xff), rgba8(0x6b, 0xf0, 0xff)),
        JudgeGrade::Excellent => (rgba8(0xfd, 0xff, 0xc9), rgba8(0xfd, 0xdb, 0x85)),
        JudgeGrade::Great => (rgba8(0xc9, 0xff, 0xc9), rgba8(0x94, 0xfe, 0xc1)),
        _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
    }
}

#[inline(always)]
pub fn zmod_combo_solid_color(grade: JudgeGrade, quint: bool) -> [f32; 4] {
    if quint && matches!(grade, JudgeGrade::Fantastic) {
        return rgba8(0xe9, 0x28, 0xff);
    }
    match grade {
        JudgeGrade::Fantastic => rgba8(0x21, 0xcc, 0xe8),
        JudgeGrade::Excellent => rgba8(0xe2, 0x9c, 0x18),
        JudgeGrade::Great => rgba8(0x66, 0xc9, 0x55),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

#[inline(always)]
pub fn zmod_indicator_default_color(score_percent: f64) -> [f32; 4] {
    if score_percent >= 96.0 {
        rgba8(0x21, 0xcc, 0xe8) // Fantastic
    } else if score_percent >= 89.0 {
        rgba8(0xe2, 0x9c, 0x18) // Excellent
    } else if score_percent >= 80.0 {
        rgba8(0x66, 0xc9, 0x55) // Great
    } else if score_percent >= 68.0 {
        rgba8(0xb4, 0x5c, 0xff) // Decent
    } else {
        rgba8(0xff, 0x30, 0x30) // Miss
    }
}

#[inline(always)]
pub fn zmod_indicator_detailed_color(score_percent: f64) -> [f32; 4] {
    if score_percent >= 99.0 {
        rgba8(0xff, 0x00, 0xff)
    } else if score_percent >= 98.0 {
        rgba8(0x25, 0x6e, 0xce)
    } else if score_percent >= 96.0 {
        [1.0, 1.0, 1.0, 1.0]
    } else if score_percent >= 94.0 {
        rgba8(0xfd, 0xa3, 0x07)
    } else if score_percent >= 90.0 {
        rgba8(0x79, 0xa9, 0x01)
    } else if score_percent >= 85.0 {
        rgba8(0xb9, 0x32, 0xe2)
    } else {
        [1.0, 0.0, 0.0, 1.0]
    }
}

#[inline(always)]
pub fn zmod_combo_rainbow_color(elapsed: f32, scroll: bool, combo: u32) -> [f32; 4] {
    let speed = if scroll { 0.45 } else { 0.35 };
    let offset = if scroll { combo as f32 * 0.013 } else { 0.0 };
    let hue = (elapsed * speed + offset).fract();
    let h6 = hue * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let q = 1.0 - f;
    match i.rem_euclid(6) {
        0 => [1.0, f, 0.0, 1.0],
        1 => [q, 1.0, 0.0, 1.0],
        2 => [0.0, 1.0, f, 1.0],
        3 => [0.0, q, 1.0, 1.0],
        4 => [f, 0.0, 1.0, 1.0],
        _ => [1.0, 0.0, q, 1.0],
    }
}

#[inline(always)]
fn zmod_combo_grade(params: ZmodComboColorParams) -> Option<JudgeGrade> {
    if params.full_combo_mode {
        params.full_combo_grade
    } else {
        params.current_combo_grade
    }
}

#[inline(always)]
fn zmod_full_combo_rainbow_active(grade: Option<JudgeGrade>) -> bool {
    matches!(
        grade,
        Some(JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great)
    )
}

pub fn zmod_resolved_combo_color(params: ZmodComboColorParams) -> [f32; 4] {
    match params.style {
        ZmodComboColorStyle::None => [1.0, 1.0, 1.0, 1.0],
        ZmodComboColorStyle::Rainbow => {
            if params.full_combo_mode && !zmod_full_combo_rainbow_active(params.full_combo_grade) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                zmod_combo_rainbow_color(params.elapsed_s, false, params.combo)
            }
        }
        ZmodComboColorStyle::RainbowScroll => {
            if params.full_combo_mode && !zmod_full_combo_rainbow_active(params.full_combo_grade) {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                zmod_combo_rainbow_color(params.elapsed_s, true, params.combo)
            }
        }
        ZmodComboColorStyle::Glow => {
            if let Some(grade) = zmod_combo_grade(params) {
                let (color1, color2) = zmod_combo_glow_pair(
                    grade,
                    params.quint_active && grade == JudgeGrade::Fantastic,
                );
                zmod_combo_glow_color(color1, color2, params.elapsed_s)
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        }
        ZmodComboColorStyle::Solid => zmod_static_combo_color(params),
    }
}

pub fn zmod_static_combo_color(params: ZmodComboColorParams) -> [f32; 4] {
    if let Some(grade) = zmod_combo_grade(params) {
        zmod_combo_solid_color(grade, params.quint_active && grade == JudgeGrade::Fantastic)
    } else {
        [1.0, 1.0, 1.0, 1.0]
    }
}

pub fn zmod_stream_prog_completion_for_beat(
    total_stream_measures: f64,
    segs: &[StreamSegment],
    beat_floor: f32,
) -> Option<f64> {
    if total_stream_measures <= 0.0 || segs.is_empty() {
        return None;
    }
    if !beat_floor.is_finite() {
        return Some(0.0);
    }
    let upper_beat = (beat_floor as i32).saturating_add(1).max(0);
    if upper_beat <= 0 {
        return Some(0.0);
    }
    let mut completed_stream_beats: i64 = 0;
    for seg in segs {
        let start_beat = (seg.start as i32).saturating_mul(4);
        if start_beat >= upper_beat {
            break;
        }
        if seg.is_break {
            continue;
        }
        let end_beat = (seg.end as i32).saturating_mul(4);
        let lo = start_beat.max(0);
        let hi = upper_beat.min(end_beat);
        if hi > lo {
            completed_stream_beats += i64::from(hi - lo);
        }
    }
    let completed_stream_measures = (completed_stream_beats as f64) / 4.0;
    Some((completed_stream_measures / total_stream_measures).clamp(0.0, 1.0))
}

#[inline(always)]
pub fn error_bar_text_scalable_zoom(abs_ms: f32, scale_start_ms: f32, w2_ms: f32) -> f32 {
    let ms = if abs_ms.is_finite() {
        abs_ms
    } else {
        timing::FA_PLUS_W010_MS
    };
    let scale_start_ms = if scale_start_ms.is_finite() && scale_start_ms > 0.0 {
        scale_start_ms
    } else {
        timing::FA_PLUS_W010_MS
    };
    let w1_ms = scale_start_ms + (timing::FA_PLUS_W0_MS - timing::FA_PLUS_W010_MS).max(0.001);
    let w2_ms = if w2_ms.is_finite() && w2_ms > w1_ms {
        w2_ms
    } else {
        w1_ms
    };
    let mut scale1 = 1.0;
    let mut scale2 = 1.0;
    if scale_start_ms < ms && ms <= w1_ms {
        scale1 = (ms - scale_start_ms) / (w1_ms - scale_start_ms);
    } else if w1_ms < ms && ms <= w2_ms && w2_ms > w1_ms {
        scale2 = (ms - w1_ms) / (w2_ms - w1_ms);
    }
    0.15 + scale1 * 0.2 + scale2 * 0.1
}

#[inline(always)]
pub fn player_metric_y(
    center_y: f32,
    notefield_offset_y: f32,
    reverse_percent: f32,
    normal_offset: f32,
    reverse_offset: f32,
) -> f32 {
    sm_scale(
        reverse_percent,
        0.0,
        1.0,
        center_y + normal_offset + notefield_offset_y,
        center_y + reverse_offset + notefield_offset_y,
    )
}

#[inline(always)]
fn rage_frustum(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32) -> Matrix4 {
    let a = (r + l) / (r - l);
    let bb = (t + b) / (t - b);
    let c = -(zf + zn) / (zf - zn);
    let d = -(2.0 * zf * zn) / (zf - zn);
    // Match ITGmania's RageDisplay::GetFrustumMatrix (OpenGL-style frustum matrix).
    //
    // Note: `glam::Mat4::from_cols_array` takes elements in column-major order.
    Matrix4::from_cols_array(&[
        2.0 * zn / (r - l),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 * zn / (t - b),
        0.0,
        0.0,
        a,
        bb,
        c,
        -1.0,
        0.0,
        0.0,
        d,
        0.0,
    ])
}

pub fn notefield_view_proj(
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    center_y: f32,
    tilt: f32,
    skew: f32,
    reverse: bool,
) -> Option<Matrix4> {
    if !screen_w.is_finite() || !screen_h.is_finite() || screen_w <= 0.0 || screen_h <= 0.0 {
        return None;
    }

    let half_w = 0.5 * screen_w;
    let half_h = 0.5 * screen_h;

    // ITGmania: Player::PushPlayerMatrix -> LoadMenuPerspective(45, w, h, vanish_x, center_y)
    let fov_deg = 45.0_f32;
    let theta = (0.5 * fov_deg).to_radians();
    let tan_theta = theta.tan();
    if !tan_theta.is_finite() || tan_theta.abs() < 1e-6 {
        return None;
    }
    let dist = half_w / tan_theta;
    if !dist.is_finite() || dist <= 0.0 {
        return None;
    }

    let vanish_x = sm_scale(skew, 0.1, 1.0, playfield_center_x, half_w);
    let vanish_y = center_y;

    let near = 1.0_f32;
    let far = dist + 1000.0_f32;

    // Match RageDisplay::LoadMenuPerspective exactly (ITGmania).
    let mut vp_x = sm_scale(vanish_x, 0.0, screen_w, screen_w, 0.0);
    let mut vp_y = sm_scale(vanish_y, 0.0, screen_h, screen_h, 0.0);
    vp_x -= half_w;
    vp_y -= half_h;
    let l = (vp_x - half_w) / dist;
    let r = (vp_x + half_w) / dist;
    let b = (vp_y + half_h) / dist;
    let t = (vp_y - half_h) / dist;
    let proj = rage_frustum(l, r, b, t, near, far);

    let eye = Vector3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Vector3::new(-vp_x + half_w, -vp_y + half_h, 0.0);
    let view = Matrix4::look_at_rh(eye, at, Vector3::Y);

    // ITGmania: PlayerNoteFieldPositioner applies tilt/zoom/y_offset on the NoteField actor.
    let reverse_mult = if reverse { -1.0 } else { 1.0 };
    let tilt = tilt.clamp(-1.0, 1.0);
    let tilt_deg = (-30.0 * tilt) * reverse_mult;
    let tilt_abs = tilt.abs();
    let tilt_scale = 1.0 - 0.1 * tilt_abs;
    let y_offset_screen = if tilt > 0.0 {
        -45.0 * tilt
    } else {
        20.0 * tilt
    } * reverse_mult;
    // Screen y-down to world y-up.
    let y_offset_world = -y_offset_screen;

    let pivot_x = playfield_center_x - half_w;
    let pivot_y = half_h - center_y;
    // Convert our world coords (centered, y-up) back into the SM-style screen
    // coords (top-left, y-down) expected by the menu perspective camera.
    let world_to_screen = Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        0.0, -1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        half_w, half_h, 0.0, 1.0,
    ]);
    let field = Matrix4::from_translation(Vector3::new(0.0, y_offset_world, 0.0))
        * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
        * Matrix4::from_rotation_x(tilt_deg.to_radians())
        * Matrix4::from_scale(Vector3::new(tilt_scale, tilt_scale, 1.0))
        * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0));

    Some((proj * view) * world_to_screen * field)
}

#[inline(always)]
pub fn judgment_tilt_rotation_deg(params: JudgmentTiltParams) -> f32 {
    if !params.enabled || params.grade == JudgeGrade::Miss {
        return 0.0;
    }
    if !params.time_error_ms.is_finite() || !params.multiplier.is_finite() {
        return 0.0;
    }
    let max_ms = params.max_threshold_ms.max(params.min_threshold_ms);
    let active_ms = params.time_error_ms.abs().min(max_ms) - params.min_threshold_ms;
    if active_ms <= 0.0 {
        return 0.0;
    }
    let dir = if params.time_error_ms < 0.0 {
        1.0
    } else {
        -1.0
    };
    dir * active_ms * 0.3 * params.multiplier
}

#[inline(always)]
pub fn judgment_actor_zoom(
    mini: f32,
    judgment_back: bool,
    _perspective_tilt: f32,
    _perspective_skew: f32,
) -> f32 {
    if judgment_back {
        ((2.0 - mini) * 0.5).clamp(0.35, 1.0)
    } else {
        combo_actor_zoom(mini)
    }
}

#[inline(always)]
pub fn combo_actor_zoom(mini: f32) -> f32 {
    0.5_f32.powf(mini).min(1.0)
}

#[inline(always)]
pub fn effective_mini_value(mini_percent: f32, fallback_mini_percent: f32, big_effect: f32) -> f32 {
    let mut mini = if mini_percent.is_finite() {
        mini_percent
    } else {
        fallback_mini_percent
    };
    if big_effect > f32::EPSILON {
        // ITG _fallback/ArrowCloud map Effect Big to mod,-100% mini.
        mini -= 100.0;
    }
    mini.clamp(-100.0, 150.0) / 100.0
}

#[inline(always)]
pub fn average_error_bar_mini_scale(mini: f32) -> f32 {
    (1.1 - 0.545 * mini).max(0.0)
}

#[inline(always)]
pub fn tap_judgment_rows(params: TapJudgmentRowsParams) -> (usize, Option<usize>) {
    if params.frame_rows < 7 {
        return match params.grade {
            JudgeGrade::Fantastic => (0, None),
            JudgeGrade::Excellent => (1, None),
            JudgeGrade::Great => (2, None),
            JudgeGrade::Decent => (3, None),
            JudgeGrade::WayOff => (4, None),
            JudgeGrade::Miss => (5, None),
        };
    }

    match params.grade {
        JudgeGrade::Fantastic => {
            if tap_judgment_split_15_10ms_active(params) {
                // zmod SplitWhites keeps the 15ms blue base, then overlays the
                // white Fantastic art at half alpha for the 10ms-15ms slice.
                (0, Some(1))
            } else if params.show_fa_plus_window {
                if tap_judgment_is_blue_fantastic(params) {
                    (0, None)
                } else {
                    (1, None)
                }
            } else {
                (0, None)
            }
        }
        JudgeGrade::Excellent => (2, None),
        JudgeGrade::Great => (3, None),
        JudgeGrade::Decent => (4, None),
        JudgeGrade::WayOff => (5, None),
        JudgeGrade::Miss => (6, None),
    }
}

#[inline(always)]
fn tap_judgment_split_15_10ms_active(params: TapJudgmentRowsParams) -> bool {
    params.show_fa_plus_window
        && params.split_15_10ms
        && !params.custom_fantastic_window
        && params.grade == JudgeGrade::Fantastic
        && params.time_error_ms.abs() > timing::FA_PLUS_W010_MS
        && params.time_error_ms.abs() <= timing::FA_PLUS_W0_MS
}

#[inline(always)]
fn tap_judgment_is_blue_fantastic(params: TapJudgmentRowsParams) -> bool {
    if params.grade != JudgeGrade::Fantastic {
        return false;
    }
    if !params.show_fa_plus_window {
        return true;
    }
    if params.fa_plus_10ms_blue_window && !params.split_15_10ms && !params.custom_fantastic_window {
        return params.time_error_ms.abs() <= timing::FA_PLUS_W010_MS;
    }
    params.window == Some(TimingWindow::W0)
}

#[inline(always)]
pub fn held_miss_zoom(elapsed: f32, mini: f32) -> (f32, f32) {
    let mini_scale = (1.0 - mini * 0.5).max(0.0);
    if elapsed < 0.1 {
        let t = (elapsed / 0.1).clamp(0.0, 1.0);
        let ease_t = 1.0 - (1.0 - t).powi(2);
        let zoom_x = 0.8 + (0.75 - 0.8) * ease_t;
        return (zoom_x * mini_scale, 0.75 * mini_scale);
    }
    if elapsed < 0.3 {
        return (0.75 * mini_scale, 0.75 * mini_scale);
    }
    let t = ((elapsed - 0.3) / 0.2).clamp(0.0, 1.0);
    let zoom = 0.75 * mini_scale * (1.0 - t.powi(2));
    (zoom, zoom)
}

#[inline(always)]
pub fn hud_y(
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
    reverse: bool,
    centered_percent: f32,
) -> f32 {
    let base_y = if reverse { reverse_y } else { normal_y };
    sm_scale(centered_percent, 0.0, 1.0, base_y, centered_y)
}

#[inline(always)]
pub fn zmod_layout_ys(
    judgment_y: f32,
    combo_y_base: f32,
    reverse: bool,
    params: ZmodLayoutParams,
) -> ZmodLayoutYs {
    let mut top_y = judgment_y - params.judgment_height * 0.5;
    let mut bottom_y = judgment_y + params.judgment_height * 0.5;

    if params.has_error_bar {
        if !params.has_judgment_texture {
            // Error bar replaces judgment; no top/bottom adjustment.
        } else if params.error_bar_up {
            top_y -= 15.0;
        } else {
            bottom_y += 15.0;
        }
    }

    let mut measure_counter_y = None;
    if params.has_measure_counter {
        if params.measure_counter_up {
            let mut y = top_y - 8.0;
            top_y -= 20.0;
            if params.broken_run {
                y -= 16.0;
            }
            measure_counter_y = Some(y);
        } else {
            measure_counter_y = Some(bottom_y + 8.0);
            bottom_y += 21.0;
        }
    }

    // Zmod: HideLookahead is not implemented in deadsync, so we always take the normal branch.
    let (subtractive_scoring_y, subtractive_scoring_addx) = match params.mini_indicator_position {
        LayoutMiniIndicatorPosition::Default => {
            if params.has_measure_counter && params.measure_counter_up {
                let y = bottom_y + 8.0;
                bottom_y += 16.0;
                (y, 0.0)
            } else {
                let y = top_y - 8.0;
                top_y -= 16.0;
                (y, 0.0)
            }
        }
        LayoutMiniIndicatorPosition::UnderUpArrow => {
            if params.has_measure_counter && params.measure_counter_up {
                let y = top_y + 16.0;
                top_y -= 16.0;
                (y, -60.0)
            } else {
                let y = top_y - 8.0;
                top_y -= 16.0;
                (y, 0.0)
            }
        }
    };

    let combo_y = if reverse {
        combo_y_base.min(top_y - 20.0)
    } else {
        combo_y_base.max(bottom_y + 20.0)
    };

    ZmodLayoutYs {
        combo_y,
        measure_counter_y,
        subtractive_scoring_y,
        subtractive_scoring_addx,
    }
}

#[inline(always)]
pub fn hud_layout_ys(
    judgment_y_base: f32,
    combo_y_base: f32,
    reverse: bool,
    offsets: HudLayoutOffsets,
    params: HudLayoutParams,
) -> HudLayoutYs {
    let mut zmod_layout = zmod_layout_ys(judgment_y_base, combo_y_base, reverse, params.zmod);
    zmod_layout.combo_y += offsets.combo_extra_y;
    let judgment_y = judgment_y_base + offsets.judgment_extra_y;
    let (error_bar_y, error_bar_max_h) = if !params.has_judgment_texture {
        (judgment_y_base + offsets.error_bar_extra_y, 30.0)
    } else if params.error_bar_up {
        (
            judgment_y_base - params.error_bar_offset + offsets.error_bar_extra_y,
            10.0,
        )
    } else {
        (
            judgment_y_base + params.error_bar_offset + offsets.error_bar_extra_y,
            10.0,
        )
    };
    HudLayoutYs {
        judgment_y,
        error_bar_y,
        error_bar_max_h,
        zmod_layout,
    }
}

#[inline(always)]
pub fn compute_invert_distances(col_offsets: &[f32], out: &mut [f32]) {
    let num_cols = col_offsets.len();
    if num_cols == 0 {
        return;
    }
    let num_sides = if num_cols > 4 { 2 } else { 1 };
    let cols_per_side = (num_cols / num_sides).max(1);
    for i in 0..num_cols {
        let side = i / cols_per_side;
        let on_side = i % cols_per_side;
        let left_mid = (cols_per_side - 1) / 2;
        let right_mid = cols_per_side.div_ceil(2);
        let (first, last) = if on_side <= left_mid {
            (0, left_mid)
        } else if on_side >= right_mid {
            (right_mid, cols_per_side - 1)
        } else {
            (on_side / 2, on_side / 2)
        };
        let new_on_side = if first == last {
            0
        } else {
            sm_scale(
                on_side as f32,
                first as f32,
                last as f32,
                last as f32,
                first as f32,
            )
            .round() as usize
        };
        let new_col = side * cols_per_side + new_on_side.min(num_cols.saturating_sub(1));
        out[i] = col_offsets[new_col] - col_offsets[i];
    }
}

#[inline(always)]
pub fn compute_tornado_bounds(col_offsets: &[f32], out: &mut [TornadoBounds]) {
    let num_cols = col_offsets.len();
    let width = if num_cols > 4 { 2 } else { 3 };
    for (i, bounds) in out.iter_mut().take(num_cols).enumerate() {
        let start = i.saturating_sub(width);
        let end = (i + width).min(num_cols.saturating_sub(1));
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in &col_offsets[start..=end] {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
        }
        *bounds = TornadoBounds { min_x, max_x };
    }
}

#[inline(always)]
pub fn tipsy_y_extra(local_col: usize, elapsed: f32, tipsy: f32) -> f32 {
    if !signed_effect_active(tipsy) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = elapsed * TIPSY_TIMER_FREQUENCY + col * TIPSY_COLUMN_FREQUENCY;
    tipsy * angle.cos() * ScrollSpeedSetting::ARROW_SPACING * TIPSY_ARROW_MAGNITUDE
}

#[inline(always)]
pub fn beat_x_extra(y: f32, beat_factor: f32, beat: f32) -> f32 {
    if !signed_effect_active(beat) {
        return 0.0;
    }
    let shift =
        beat_factor * (y / BEAT_OFFSET_HEIGHT + std::f32::consts::PI / BEAT_PI_HEIGHT).sin();
    beat * shift
}

#[inline(always)]
pub fn drunk_x_extra(
    local_col: usize,
    y: f32,
    elapsed: f32,
    screen_height: f32,
    drunk: f32,
) -> f32 {
    if !signed_effect_active(drunk) {
        return 0.0;
    }
    let col = local_col as f32;
    let angle = elapsed + col * DRUNK_COLUMN_FREQUENCY + y * DRUNK_OFFSET_FREQUENCY / screen_height;
    drunk * angle.cos() * ScrollSpeedSetting::ARROW_SPACING * DRUNK_ARROW_MAGNITUDE
}

#[inline(always)]
pub fn tornado_x_extra(
    y: f32,
    base_x: f32,
    bounds: TornadoBounds,
    screen_height: f32,
    tornado: f32,
) -> f32 {
    if !signed_effect_active(tornado) {
        return 0.0;
    }
    let position_between = sm_scale(base_x, bounds.min_x, bounds.max_x, -1.0, 1.0).clamp(-1.0, 1.0);
    let radians = position_between.acos() + y * TORNADO_X_OFFSET_FREQUENCY / screen_height;
    let adjusted = sm_scale(radians.cos(), -1.0, 1.0, bounds.min_x, bounds.max_x);
    (adjusted - base_x) * tornado
}

#[inline(always)]
pub fn note_x_extra(
    local_col: usize,
    y: f32,
    elapsed: f32,
    beat_factor: f32,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
    params: NoteXParams,
) -> f32 {
    let mut x = 0.0;
    let base_x = col_offsets[local_col];
    if signed_effect_active(params.tornado) {
        x += tornado_x_extra(
            y,
            base_x,
            tornado_bounds[local_col],
            params.screen_height,
            params.tornado,
        );
    }
    if signed_effect_active(params.drunk) {
        x += drunk_x_extra(local_col, y, elapsed, params.screen_height, params.drunk);
    }
    if signed_effect_active(params.flip) {
        let mirrored = col_offsets[col_offsets.len().saturating_sub(1) - local_col];
        x += (mirrored - base_x) * params.flip;
    }
    if signed_effect_active(params.invert) {
        x += invert_distances[local_col] * params.invert;
    }
    if signed_effect_active(params.beat) {
        x += beat_x_extra(y, beat_factor, params.beat);
    }
    x
}

#[inline(always)]
pub fn note_x_offset(
    local_col: usize,
    y: f32,
    elapsed: f32,
    beat_factor: f32,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
    move_x_cols: &[f32],
    params: NoteXParams,
    tiny: f32,
) -> f32 {
    let base = col_offsets[local_col]
        + note_x_extra(
            local_col,
            y,
            elapsed,
            beat_factor,
            col_offsets,
            invert_distances,
            tornado_bounds,
            params,
        );
    base * tiny_spacing_scale(tiny) + move_col_extra(move_x_cols, local_col)
}

#[inline(always)]
pub fn receptor_row_center(
    playfield_center_x: f32,
    local_col: usize,
    receptor_y_lane: f32,
    elapsed: f32,
    beat_factor: f32,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
    move_x_cols: &[f32],
    move_y_cols: &[f32],
    params: NoteXParams,
    tiny: f32,
    tipsy: f32,
) -> [f32; 2] {
    [
        playfield_center_x
            + note_x_offset(
                local_col,
                0.0,
                elapsed,
                beat_factor,
                col_offsets,
                invert_distances,
                tornado_bounds,
                move_x_cols,
                params,
                tiny,
            ),
        receptor_y_lane
            + tipsy_y_extra(local_col, elapsed, tipsy)
            + move_col_extra(move_y_cols, local_col),
    ]
}

#[inline(always)]
pub fn hold_indicator_column_x(
    playfield_center_x: f32,
    local_col: usize,
    elapsed: f32,
    beat_factor: f32,
    col_offsets: &[f32],
    invert_distances: &[f32],
    tornado_bounds: &[TornadoBounds],
    move_x_cols: &[f32],
    params: NoteXParams,
    tiny: f32,
) -> f32 {
    playfield_center_x
        + note_x_offset(
            local_col,
            0.0,
            elapsed,
            beat_factor,
            col_offsets,
            invert_distances,
            tornado_bounds,
            move_x_cols,
            params,
            tiny,
        )
}

#[inline(always)]
pub fn appearance_note_alpha(
    y_no_reverse: f32,
    elapsed: f32,
    mini: f32,
    appearance: NoteAlphaParams,
) -> f32 {
    if y_no_reverse < 0.0 {
        return 1.0;
    }
    let zoom = (1.0 - mini * 0.5).abs().max(0.01);
    let center_line = CENTER_LINE_Y / zoom;
    let hidden_sudden = appearance.hidden * appearance.sudden;
    let hidden_end = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, -1.0, -1.25)
        + center_line * appearance.hidden_offset;
    let hidden_start = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, -0.25)
        + center_line * appearance.hidden_offset;
    let sudden_end = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 0.0, 0.25)
        + center_line * appearance.sudden_offset;
    let sudden_start = center_line
        + FADE_DIST_Y * sm_scale(hidden_sudden, 0.0, 1.0, 1.0, 1.25)
        + center_line * appearance.sudden_offset;
    let mut visible_adjust = 0.0;
    if appearance.hidden > f32::EPSILON {
        visible_adjust += appearance.hidden
            * sm_scale(y_no_reverse, hidden_start, hidden_end, 0.0, -1.0).clamp(-1.0, 0.0);
    }
    if appearance.sudden > f32::EPSILON {
        visible_adjust += appearance.sudden
            * sm_scale(y_no_reverse, sudden_start, sudden_end, -1.0, 0.0).clamp(-1.0, 0.0);
    }
    if appearance.stealth > f32::EPSILON {
        visible_adjust -= appearance.stealth;
    }
    if appearance.blink > f32::EPSILON {
        let blink = quantize_step((elapsed * 10.0).sin(), BLINK_MOD_FREQUENCY);
        visible_adjust += sm_scale(blink, 0.0, 1.0, -1.0, 0.0);
    }
    if appearance.random_vanish > f32::EPSILON {
        let dist = (y_no_reverse - center_line).abs();
        visible_adjust += sm_scale(dist, 80.0, 160.0, -1.0, 0.0) * appearance.random_vanish;
    }
    (1.0 + visible_adjust).clamp(0.0, 1.0)
}

#[inline(always)]
pub fn appearance_note_glow(
    y_no_reverse: f32,
    elapsed: f32,
    mini: f32,
    appearance: NoteAlphaParams,
) -> f32 {
    let percent_visible = appearance_note_alpha(y_no_reverse, elapsed, mini, appearance);
    sm_scale((percent_visible - 0.5).abs(), 0.0, 0.5, 1.3, 0.0).max(0.0)
}

#[inline(always)]
pub fn appearance_note_actor_alpha(
    y_no_reverse: f32,
    elapsed: f32,
    mini: f32,
    appearance: NoteAlphaParams,
) -> f32 {
    if appearance_note_alpha(y_no_reverse, elapsed, mini, appearance) > 0.5 {
        1.0
    } else {
        0.0
    }
}

#[inline(always)]
pub fn appearance_needs_rows(appearance: NoteAlphaParams) -> bool {
    appearance.hidden > f32::EPSILON
        || appearance.sudden > f32::EPSILON
        || appearance.random_vanish > f32::EPSILON
}

#[inline(always)]
pub fn tiny_spacing_scale(tiny: f32) -> f32 {
    if tiny.abs() <= f32::EPSILON || !tiny.is_finite() {
        return 1.0;
    }
    0.5_f32.powf(tiny).min(1.0)
}

#[inline(always)]
pub fn move_col_extra(values: &[f32], local_col: usize) -> f32 {
    values
        .get(local_col)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        * ScrollSpeedSetting::ARROW_SPACING
}

#[inline(always)]
pub fn default_column_x(local_col: usize, num_cols: usize) -> f32 {
    (local_col as f32 - num_cols.saturating_sub(1) as f32 * 0.5) * ScrollSpeedSetting::ARROW_SPACING
}

#[inline(always)]
pub fn fill_lane_col_offsets(
    out: &mut [f32],
    column_xs: Option<&[i32]>,
    num_cols: usize,
    spacing_mult: f32,
    field_zoom: f32,
) {
    for (i, col_offset) in out.iter_mut().take(num_cols).enumerate() {
        let col_x = column_xs
            .and_then(|xs| xs.get(i))
            .map_or_else(|| default_column_x(i, num_cols), |x| *x as f32);
        *col_offset = col_x * spacing_mult * field_zoom;
    }
}

#[inline(always)]
pub fn translated_uv_rect(mut uv: [f32; 4], translate: [f32; 2]) -> [f32; 4] {
    uv[0] += translate[0];
    uv[1] += translate[1];
    uv[2] += translate[0];
    uv[3] += translate[1];
    uv
}

#[inline(always)]
pub fn maybe_flip_uv_vert(mut uv: [f32; 4], flip: bool) -> [f32; 4] {
    if flip {
        (uv[1], uv[3]) = (uv[3], uv[1]);
    }
    uv
}

#[inline(always)]
pub const fn maybe_mirror_uv_horiz_for_reverse_flipped(
    uv: [f32; 4],
    lane_reverse: bool,
    body_flipped: bool,
) -> [f32; 4] {
    if lane_reverse && body_flipped {
        [uv[2], uv[1], uv[0], uv[3]]
    } else {
        uv
    }
}

#[inline(always)]
pub const fn top_cap_rotation_deg(lane_reverse: bool, body_flipped: bool) -> f32 {
    if lane_reverse && body_flipped {
        180.0
    } else {
        0.0
    }
}

#[inline(always)]
pub fn scale_effect_size(logical_size: [f32; 2], field_zoom: f32, effect_zoom: f32) -> [f32; 2] {
    let zoom = field_zoom * effect_zoom;
    [logical_size[0] * zoom, logical_size[1] * zoom]
}

#[inline(always)]
pub fn scale_sprite_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
    let width = size[0].max(0) as f32;
    let height = size[1].max(0) as f32;
    if height <= 0.0 || target_arrow_px <= 0.0 {
        [width, height]
    } else {
        let scale = target_arrow_px / height;
        [width * scale, target_arrow_px]
    }
}

#[inline(always)]
pub fn scale_cap_to_arrow(size: [i32; 2], target_arrow_px: f32) -> [f32; 2] {
    let width = size[0].max(0) as f32;
    let height = size[1].max(0) as f32;
    if width <= 0.0 || target_arrow_px <= 0.0 {
        [width, height]
    } else {
        let scale = target_arrow_px / width;
        [target_arrow_px, height * scale]
    }
}

#[inline(always)]
pub fn offset_center(
    center: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
) -> [f32; 2] {
    let [sin_r, cos_r] = local_offset_rot_sin_cos;
    let offset = [
        local_offset[0] * cos_r - local_offset[1] * sin_r,
        local_offset[0] * sin_r + local_offset[1] * cos_r,
    ];
    [center[0] + offset[0], center[1] + offset[1]]
}

#[inline(always)]
pub fn hold_tail_cap_bounds(
    body_tail_y: f32,
    cap_height: f32,
    rendered_body_top: Option<f32>,
    rendered_body_bottom: Option<f32>,
) -> Option<(f32, f32)> {
    let default_bounds = (body_tail_y, body_tail_y + cap_height);
    let rb = match (rendered_body_top, rendered_body_bottom) {
        (Some(t), Some(b)) if b > t + 0.5 => b,
        _ => return Some(default_bounds),
    };

    let dist = body_tail_y - rb;
    if dist < -2.0 || dist > cap_height + 2.0 {
        return Some(default_bounds);
    }

    Some((rb, rb + cap_height))
}

#[inline(always)]
pub fn clipped_hold_body_bounds(
    body_top: f32,
    body_bottom: f32,
    natural_top: f32,
    natural_bottom: f32,
) -> Option<(f32, f32)> {
    let clipped_top = body_top.max(natural_top);
    let clipped_bottom = body_bottom.min(natural_bottom);
    (clipped_bottom > clipped_top).then_some((clipped_top, clipped_bottom))
}

#[inline(always)]
pub fn hold_body_bottom_for_tail_cap(body_bottom: f32, y_tail: f32, cap_height: f32) -> f32 {
    if cap_height <= f32::EPSILON {
        return body_bottom;
    }
    let mut bottom = body_bottom.min(y_tail + 1.0);
    if bottom >= y_tail - 1.0 {
        bottom = y_tail + 1.0;
    }
    bottom
}

#[inline(always)]
pub fn hold_draw_span(y_head: f32, y_tail: f32, screen_height: f32) -> Option<(f32, f32)> {
    let mut top = y_head.min(y_tail);
    let mut bottom = y_head.max(y_tail);
    if bottom < -200.0 || top > screen_height + 200.0 {
        return None;
    }
    top = top.max(-400.0);
    bottom = bottom.min(screen_height + 400.0);
    (bottom >= top).then_some((top, bottom))
}

#[inline(always)]
pub fn hold_body_segment_budget(visible_span: f32, segment_height: f32) -> (usize, bool) {
    let estimated = if visible_span <= f32::EPSILON || segment_height <= f32::EPSILON {
        1
    } else {
        (visible_span / segment_height).ceil() as usize
    };
    let max_segments = estimated
        .saturating_add(2)
        .clamp(2048, HOLD_BODY_SEGMENT_SAFETY_MAX);
    (max_segments, estimated <= HOLD_BODY_LEGACY_SEGMENT_LIMIT)
}

#[inline(always)]
pub fn hold_strip_row_3d(
    center: [f32; 3],
    forward: [f32; 2],
    half_width: f32,
    u0: f32,
    u1: f32,
    v: f32,
    color: [f32; 4],
) -> [TexturedMeshVertex; 2] {
    let len = forward[0].hypot(forward[1]).max(f32::EPSILON);
    let nx = -forward[1] / len * half_width;
    let ny = forward[0] / len * half_width;
    [
        TexturedMeshVertex {
            pos: [center[0] + nx, center[1] + ny, center[2]],
            uv: [u0, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
        TexturedMeshVertex {
            pos: [center[0] - nx, center[1] - ny, center[2]],
            uv: [u1, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
    ]
}

#[inline(always)]
pub fn hold_strip_row_from_positions(
    left: [f32; 3],
    right: [f32; 3],
    u0: f32,
    u1: f32,
    v: f32,
    color: [f32; 4],
) -> [TexturedMeshVertex; 2] {
    [
        TexturedMeshVertex {
            pos: left,
            uv: [u0, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
        TexturedMeshVertex {
            pos: right,
            uv: [u1, v],
            tex_matrix_scale: [1.0, 1.0],
            color,
        },
    ]
}

#[inline(always)]
pub fn hold_strip_quad(
    top: [TexturedMeshVertex; 2],
    bottom: [TexturedMeshVertex; 2],
) -> [TexturedMeshVertex; 6] {
    [top[0], top[1], bottom[1], top[0], bottom[1], bottom[0]]
}

#[inline(always)]
pub fn hold_strip_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
    blend: BlendMode,
    depth_test: bool,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0; 4],
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices,
        geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test,
        visible: true,
        blend,
        z,
    }
}

#[inline(always)]
pub fn hold_strip_glow_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
    depth_test: bool,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0, 1.0, 1.0, 0.0],
        glow: [1.0, 1.0, 1.0, 1.0],
        vertices,
        geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test,
        visible: true,
        blend: BlendMode::Alpha,
        z,
    }
}

#[inline(always)]
pub fn actor_with_world_z(mut actor: Actor, world_z: f32) -> Actor {
    if world_z.abs() <= f32::EPSILON {
        return actor;
    }
    match &mut actor {
        Actor::Sprite { world_z: z, .. } | Actor::TexturedMesh { world_z: z, .. } => *z = world_z,
        _ => {}
    }
    actor
}

pub fn share_actor_range(actors: &mut Vec<Actor>, start: usize) -> Option<Vec<Arc<[Actor]>>> {
    if start >= actors.len() {
        return None;
    }
    let children = Arc::<[Actor]>::from(actors.drain(start..).collect::<Vec<_>>());
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Arc::clone(&children),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
    Some(vec![children])
}

#[inline(always)]
pub const fn tap_part_for_note_type(note_type: NoteType) -> NoteAnimPart {
    match note_type {
        NoteType::Fake => NoteAnimPart::Fake,
        NoteType::Lift => NoteAnimPart::Lift,
        _ => NoteAnimPart::Tap,
    }
}

#[inline(always)]
pub const fn mine_part() -> NoteAnimPart {
    NoteAnimPart::Mine
}

#[inline(always)]
pub const fn hold_parts_for_note_type(note_type: NoteType) -> HoldAnimParts {
    match note_type {
        NoteType::Roll => HoldAnimParts {
            head: NoteAnimPart::RollHead,
            body: NoteAnimPart::RollBody,
            topcap: NoteAnimPart::RollTopCap,
            bottomcap: NoteAnimPart::RollBottomCap,
        },
        _ => HoldAnimParts {
            head: NoteAnimPart::HoldHead,
            body: NoteAnimPart::HoldBody,
            topcap: NoteAnimPart::HoldTopCap,
            bottomcap: NoteAnimPart::HoldBottomCap,
        },
    }
}

#[inline(always)]
const fn hold_head_part_for_roll(is_roll: bool) -> NoteAnimPart {
    if is_roll {
        NoteAnimPart::RollHead
    } else {
        NoteAnimPart::HoldHead
    }
}

#[inline(always)]
pub const fn tap_replacement_head(
    note_type: NoteType,
    same_row_has_hold: bool,
    same_row_has_roll: bool,
    draw_hold_same_row: bool,
    draw_roll_same_row: bool,
    tap_same_row_means_hold: bool,
) -> Option<TapReplacementHead> {
    match tap_replacement_roll(
        note_type,
        same_row_has_hold,
        same_row_has_roll,
        draw_hold_same_row,
        draw_roll_same_row,
        tap_same_row_means_hold,
    ) {
        Some(is_roll) => Some(TapReplacementHead {
            is_roll,
            part: hold_head_part_for_roll(is_roll),
        }),
        None => None,
    }
}

#[inline(always)]
const fn tap_replacement_roll(
    note_type: NoteType,
    same_row_has_hold: bool,
    same_row_has_roll: bool,
    draw_hold_same_row: bool,
    draw_roll_same_row: bool,
    tap_same_row_means_hold: bool,
) -> Option<bool> {
    if !matches!(note_type, NoteType::Tap | NoteType::Lift) {
        return None;
    }
    if same_row_has_hold && same_row_has_roll {
        if draw_hold_same_row && draw_roll_same_row {
            Some(!tap_same_row_means_hold)
        } else if draw_hold_same_row {
            Some(false)
        } else if draw_roll_same_row {
            Some(true)
        } else {
            None
        }
    } else if same_row_has_hold && draw_hold_same_row {
        Some(false)
    } else if same_row_has_roll && draw_roll_same_row {
        Some(true)
    } else {
        None
    }
}

#[inline(always)]
pub fn bottom_cap_uv_window(
    v_base0: f32,
    v_base1: f32,
    draw_height: f32,
    cap_span: f32,
    anchor_to_top: bool,
) -> Option<(f32, f32)> {
    if cap_span <= f32::EPSILON || draw_height <= f32::EPSILON {
        return None;
    }
    let tex_add = if anchor_to_top {
        0.0
    } else {
        (1.0 - draw_height / cap_span).clamp(0.0, 1.0)
    };
    let v_span = v_base1 - v_base0;
    let t0 = tex_add;
    let t1 = (draw_height / cap_span) + tex_add;
    Some((v_base0 + v_span * t0, v_base0 + v_span * t1))
}

#[inline(always)]
pub fn hold_segment_pose(top: [f32; 2], bottom: [f32; 2]) -> ([f32; 2], f32, f32) {
    let dx = bottom[0] - top[0];
    let dy = bottom[1] - top[1];
    let length = dx.hypot(dy);
    let rotation_deg = if length <= f32::EPSILON {
        0.0
    } else {
        dx.atan2(dy).to_degrees()
    };
    (
        [(top[0] + bottom[0]) * 0.5, (top[1] + bottom[1]) * 0.5],
        length,
        rotation_deg,
    )
}

#[inline(always)]
pub fn song_time_ns_to_seconds(time_ns: SongTimeNs) -> f32 {
    (time_ns as f64 * 1.0e-9) as f32
}

#[inline(always)]
pub fn song_time_ns_delta_seconds(lhs: SongTimeNs, rhs: SongTimeNs) -> f32 {
    ((lhs as i128 - rhs as i128) as f64 * 1.0e-9) as f32
}

#[inline(always)]
pub fn note_itg_row(note: &Note) -> i32 {
    // ITG's TrackMap rows are BeatToNoteRow(beat). Dead Sync keeps a separate
    // dense row_index for gameplay row bookkeeping.
    beat_to_note_row(note.beat)
}

#[inline(always)]
pub fn lane_window_bounds_by_note_row(
    note_indices: &[usize],
    notes: &[Note],
    min_row: i32,
    max_row: i32,
) -> (usize, usize) {
    if max_row < 0 {
        return (0, 0);
    }
    let min_row = min_row.max(0);
    (
        note_indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) < min_row),
        note_indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) <= max_row),
    )
}

#[inline(always)]
pub fn lane_hold_window_bounds_by_note_row(
    hold_indices: &[usize],
    notes: &[Note],
    min_row: i32,
    max_row: i32,
) -> (usize, usize) {
    let (mut start, end) = lane_window_bounds_by_note_row(hold_indices, notes, min_row, max_row);
    let min_row = min_row.max(0);
    while start > 0 {
        let prev_note_index = hold_indices[start - 1];
        let prev_end_row = notes[prev_note_index]
            .hold
            .as_ref()
            .map_or(note_itg_row(&notes[prev_note_index]), |hold| {
                beat_to_note_row(hold.end_beat)
            });
        if prev_end_row < min_row {
            break;
        }
        start -= 1;
    }
    (start, end)
}

#[inline(always)]
pub fn for_each_visible_note_index(
    note_indices: &[usize],
    notes: &[Note],
    visible_row_range: Option<(i32, i32)>,
    mut visit: impl FnMut(usize),
) {
    if let Some((min_row, max_row)) = visible_row_range {
        let (start, end) = lane_window_bounds_by_note_row(note_indices, notes, min_row, max_row);
        for &note_index in &note_indices[start..end] {
            visit(note_index);
        }
        return;
    }
    for &note_index in note_indices {
        visit(note_index);
    }
}

#[inline(always)]
pub fn for_each_visible_hold_index(
    hold_indices: &[usize],
    notes: &[Note],
    visible_row_range: Option<(i32, i32)>,
    mut visit: impl FnMut(usize),
) {
    if let Some((min_row, max_row)) = visible_row_range {
        let (start, end) =
            lane_hold_window_bounds_by_note_row(hold_indices, notes, min_row, max_row);
        for &note_index in &hold_indices[start..end] {
            visit(note_index);
        }
        return;
    }
    for &note_index in hold_indices {
        visit(note_index);
    }
}

#[inline(always)]
pub fn hold_overlaps_visible_window(
    note_index: usize,
    notes: &[Note],
    visible_row_range: Option<(i32, i32)>,
) -> bool {
    if let Some((min_row, max_row)) = visible_row_range {
        let hold_end_row = notes[note_index]
            .hold
            .as_ref()
            .map_or(note_itg_row(&notes[note_index]), |hold| {
                beat_to_note_row(hold.end_beat)
            });
        return max_row >= 0
            && hold_end_row >= min_row.max(0)
            && note_itg_row(&notes[note_index]) <= max_row;
    }
    true
}

#[inline(always)]
fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    let ix = stats
        .partition_point(|stat| stat.beat <= beat)
        .saturating_sub(1);
    stats[ix]
}

#[inline(always)]
fn note_count_range(stats: &[NoteCountStat], low: f32, high: f32) -> usize {
    let low = note_count_at(stats, low);
    let high = note_count_at(stats, high);
    high.notes_upper.saturating_sub(low.notes_lower)
}

#[inline(always)]
pub fn find_first_displayed_beat(
    current_beat: f32,
    draw_distance_after_targets: f32,
    note_count_stats: &[NoteCountStat],
    mut y_offset_for_beat: impl FnMut(f32) -> f32,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance_after_targets.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let has_cache = !note_count_stats.is_empty();
    let mut low = if has_cache { 0.0 } else { high - 4.0 };
    let mut first = low;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if y_offset_for_beat(mid) < -draw_distance_after_targets
            || (has_cache
                && note_count_range(note_count_stats, mid, current_beat) > MAX_NOTES_AFTER)
        {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(first)
}

#[inline(always)]
pub fn find_last_displayed_beat(
    current_beat: f32,
    draw_distance_before_targets: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    mut y_offset_for_beat: impl FnMut(f32) -> (f32, bool),
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance_before_targets.is_finite() {
        return None;
    }
    let mut search_distance = 10.0;
    let mut last = current_beat + search_distance;
    for _ in 0..20 {
        let (y_offset, before_peak) = y_offset_for_beat(last);
        if boomerang && !before_peak {
            last += search_distance;
        } else if y_offset > draw_distance_before_targets {
            last -= search_distance;
        } else {
            last += search_distance;
        }
        search_distance *= 0.5;
    }
    if displayed_speed_percent < 0.75 {
        last = last.min(current_beat + 16.0);
    }
    Some(last)
}

#[inline(always)]
pub const fn mine_hides_after_resolution(mine_result: Option<MineResult>) -> bool {
    // ITG hides mines once they have received any final mine judgment, not
    // only after a hit explosion.
    mine_result.is_some()
}

#[cfg(test)]
mod tests {
    use deadlib_present::actors::{Actor, SizeSpec};
    use deadlib_render::BlendMode;
    use deadsync_noteskin::NoteAnimPart;
    use std::sync::Arc;

    use super::{
        AccelYParams, BuiltNotefield, COLUMN_CUE_Y_OFFSET, DISPLAY_TURN_MIRROR,
        DISPLAY_TURN_RANDOM, DISPLAY_TURN_UD_MIRROR, GameplayModsAttackMode,
        GameplayModsTextParams, HudLayoutOffsets, HudLayoutParams, JudgmentTiltParams,
        LayoutMiniIndicatorPosition, MiniIndicatorColorStyle, MiniIndicatorMode,
        MiniIndicatorProgress, MiniIndicatorScoreType, MiniIndicatorSize,
        MiniIndicatorSubtractiveDisplay, NoteAlphaParams, NoteXParams, TapJudgmentRowsParams,
        TapReplacementHead, TornadoBounds, VisualEffectParams, ZmodComboColorParams,
        ZmodComboColorStyle, ZmodLayoutParams, ZmodMeasureCounterText, ZmodMiniIndicatorOutput,
        ZmodMiniIndicatorParams, ZmodMiniIndicatorText, actor_with_world_z, appearance_needs_rows,
        appearance_note_actor_alpha, appearance_note_alpha, appearance_note_glow,
        append_average_error_bar_part, append_mini_part, append_perspective_parts,
        append_turn_parts, apply_accel_y, apply_accel_y_with_peak, average_error_bar_mini_scale,
        beat_factor, beat_scroll_travel, beat_x_extra, bottom_cap_uv_window, bumpy_angle,
        clamp_rounded_i16, clipped_hold_body_bounds, column_cue_alpha, column_cue_height,
        column_cue_reverse_top_y, column_flash_alpha, column_flash_alpha_at, column_flash_color,
        column_flash_height, column_flash_layout, column_flash_reverse_top_y, combo_actor_zoom,
        compute_invert_distances, compute_tornado_bounds, crossover_cue_height, default_column_x,
        disabled_timing_windows_name, drunk_x_extra, edit_beat_bar_info_for_row,
        edit_beat_scroll_travel, effective_mini_value, error_bar_boundaries_s,
        error_bar_color_for_window, error_bar_flash_alpha, error_bar_text_scalable_zoom,
        error_bar_tick_alpha, field_effect_height, fill_lane_col_offsets,
        find_first_displayed_beat, find_last_displayed_beat, for_each_visible_hold_index,
        for_each_visible_note_index, gameplay_mods_text, held_miss_zoom,
        hold_body_bottom_for_tail_cap, hold_body_segment_budget, hold_draw_span, hold_glow_color,
        hold_head_part_for_roll, hold_indicator_column_x, hold_overlaps_visible_window,
        hold_parts_for_note_type, hold_segment_pose, hold_strip_actor, hold_strip_glow_actor,
        hold_strip_row_3d, hold_tail_cap_bounds, hud_layout_ys, hud_y, itg_actor_glow_alpha,
        itg_actor_rotation_z, join_display_mod_parts, judgment_actor_zoom,
        judgment_tilt_rotation_deg, maybe_mirror_uv_horiz_for_reverse_flipped,
        mine_hides_after_resolution, mine_part, mod_divisor, mod_percent_key, move_col_extra,
        note_itg_row, note_world_z_for_bumpy, note_x_extra, note_x_offset, notefield_view_proj,
        offset_center, player_metric_y, push_transform_parts, quantize_centi_i32,
        quantize_centi_u32, quantize_step, receptor_row_center, rgba8, scale_cap_to_arrow,
        scale_effect_size, scale_sprite_to_arrow, share_actor_range, signed_effect_active,
        sm_scale, smoothstep01, song_time_ns_delta_seconds, song_time_ns_to_seconds,
        stream_segment_index_exclusive_end, stream_segment_index_inclusive_end, tap_judgment_rows,
        tap_part_for_note_type, tap_replacement_head, timing_window_from_num, tiny_spacing_scale,
        tipsy_y_extra, top_cap_rotation_deg, tornado_x_extra, translated_uv_rect,
        visual_arrow_effect_zoom, visual_confusion_rotation_deg, visual_effect_params_for_col,
        visual_hold_body_needs_z_buffer, visual_note_rotation_z, visual_pulse_inner_zoom,
        visual_pulse_zoom_for_y, visual_tiny_zoom, visual_use_legacy_hold_sprites,
        zmod_broken_run_counter_text, zmod_broken_run_end, zmod_broken_run_segment,
        zmod_combo_glow_color, zmod_combo_glow_pair, zmod_combo_quint_active,
        zmod_combo_rainbow_color, zmod_combo_solid_color, zmod_indicator_default_color,
        zmod_indicator_detailed_color, zmod_layout_ys, zmod_measure_counter_text,
        zmod_mini_indicator_output, zmod_mini_indicator_zoom, zmod_pacemaker_color,
        zmod_percent_from_points, zmod_resolved_combo_color, zmod_resolved_mini_indicator_mode,
        zmod_rival_color, zmod_run_timer_index, zmod_static_combo_color,
        zmod_stream_prog_completion_for_beat, zmod_subtractive_counter_state,
        zmod_subtractive_points,
    };
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_rules::judgment::{JudgeGrade, TimingWindow};
    use deadsync_rules::note::{HoldData, MineResult, Note, NoteCountStat};
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::stream::StreamSegment;
    use deadsync_rules::timing::{self, TimeSignatureSegment};

    fn test_note_at_beat(beat: f32) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Tap,
            row_index: beat_to_note_row(beat).max(0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn test_note_at_dense_row(beat: f32, row_index: usize) -> Note {
        let mut note = test_note_at_beat(beat);
        note.row_index = row_index;
        note
    }

    fn test_hold_at_beat(beat: f32, end_beat: f32) -> Note {
        let mut note = test_note_at_beat(beat);
        note.note_type = NoteType::Hold;
        note.hold = Some(HoldData {
            end_row_index: beat_to_note_row(end_beat).max(0) as usize,
            end_beat,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: note.row_index,
            last_held_beat: beat,
        });
        note
    }

    #[test]
    fn edit_beat_bar_labels_default_measure_indices() {
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(0.0), &[])
                .and_then(|info| info.measure_index),
            Some(0)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(1.0), &[])
                .and_then(|info| info.measure_index),
            None
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(4.0), &[])
                .and_then(|info| info.measure_index),
            Some(1)
        );
    }

    #[test]
    fn edit_beat_bar_labels_follow_time_signature_segments() {
        let segments = [
            TimeSignatureSegment {
                beat: 0.0,
                numerator: 3,
                denominator: 4,
            },
            TimeSignatureSegment {
                beat: 6.0,
                numerator: 4,
                denominator: 4,
            },
        ];

        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(0.0), &segments)
                .and_then(|info| info.measure_index),
            Some(0)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(3.0), &segments)
                .and_then(|info| info.measure_index),
            Some(1)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(6.0), &segments)
                .and_then(|info| info.measure_index),
            Some(2)
        );
    }

    #[test]
    fn beat_measure_travel_applies_mini_once_like_notes() {
        let raw = beat_scroll_travel(12.0, 8.0, 1.25);
        let field_zoom = 0.75;
        let player_speed = 5.0;
        let expected = (12.0 - 8.0) * ScrollSpeedSetting::ARROW_SPACING * 1.25;

        assert!((raw - expected).abs() <= 0.001);

        let note_y = raw * field_zoom * player_speed;
        let old_measure_y = raw * field_zoom * field_zoom * player_speed;

        assert!((note_y - expected * field_zoom * player_speed).abs() <= 0.001);
        assert!(
            (note_y - old_measure_y).abs() > 100.0,
            "double-applying field zoom would drift measure lines away from notes"
        );
    }

    #[test]
    fn edit_beat_travel_uses_step_editor_spacing() {
        let edit_raw = edit_beat_scroll_travel(44.0, 40.0);
        let displayed_raw = beat_scroll_travel(42.0, 40.0, 0.5);

        assert!((edit_raw - 4.0 * ScrollSpeedSetting::ARROW_SPACING).abs() <= 0.001);
        assert!((displayed_raw - ScrollSpeedSetting::ARROW_SPACING).abs() <= 0.001);
        assert!(
            (edit_raw - displayed_raw).abs() > 100.0,
            "ITG's step editor ignores displayed beat and speed segments"
        );
    }

    #[test]
    fn translated_uv_rect_offsets_all_edges() {
        assert_eq!(
            translated_uv_rect([0.1, 0.2, 0.3, 0.4], [0.5, -0.1]),
            [0.6, 0.1, 0.8, 0.3]
        );
    }

    #[test]
    fn reverse_flipped_cap_uv_only_mirrors_when_both_flags_are_enabled() {
        let uv = [0.125, 0.25, 0.75, 0.875];
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, true, true),
            [0.75, 0.25, 0.125, 0.875]
        );
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, true, false),
            uv
        );
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, false, true),
            uv
        );
    }

    #[test]
    fn reverse_flipped_top_cap_rotation_matches_itg_parity_path() {
        assert!((top_cap_rotation_deg(true, true) - 180.0).abs() <= f32::EPSILON);
        assert!((top_cap_rotation_deg(true, false) - 0.0).abs() <= f32::EPSILON);
        assert!((top_cap_rotation_deg(false, true) - 0.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn sprite_scale_uses_height_as_arrow_target() {
        assert_eq!(scale_sprite_to_arrow([32, 64], 128.0), [64.0, 128.0]);
        assert_eq!(scale_sprite_to_arrow([32, 0], 128.0), [32.0, 0.0]);
    }

    #[test]
    fn cap_scale_uses_width_as_arrow_target() {
        assert_eq!(scale_cap_to_arrow([32, 16], 64.0), [64.0, 32.0]);
        assert_eq!(scale_cap_to_arrow([0, 16], 64.0), [0.0, 16.0]);
    }

    #[test]
    fn effect_size_applies_field_and_effect_zoom() {
        assert_eq!(scale_effect_size([64.0, 32.0], 1.25, 2.0), [160.0, 80.0]);
    }

    #[test]
    fn offset_center_applies_rotated_local_offset() {
        let center = offset_center(
            [10.0, 20.0],
            [3.0, 4.0],
            [
                std::f32::consts::FRAC_PI_2.sin(),
                std::f32::consts::FRAC_PI_2.cos(),
            ],
        );
        assert!((center[0] - 6.0).abs() <= 1e-6);
        assert!((center[1] - 23.0).abs() <= 1e-6);
    }

    #[test]
    fn default_column_x_centers_lanes() {
        assert_eq!(default_column_x(0, 4), -96.0);
        assert_eq!(default_column_x(3, 4), 96.0);
        assert_eq!(default_column_x(0, 1), 0.0);
    }

    #[test]
    fn fill_lane_col_offsets_uses_noteskin_columns_when_present() {
        let mut out = [0.0; 4];
        fill_lane_col_offsets(&mut out, Some(&[-100, -20, 20, 100]), 4, 1.5, 0.5);
        assert_eq!(out, [-75.0, -15.0, 15.0, 75.0]);
    }

    #[test]
    fn compute_invert_distances_mirrors_sides() {
        let cols = [-96.0, -32.0, 32.0, 96.0];
        let mut out = [0.0; 4];
        compute_invert_distances(&cols, &mut out);
        assert_eq!(out, [64.0, -64.0, 64.0, -64.0]);
    }

    #[test]
    fn compute_tornado_bounds_uses_neighbor_window() {
        let cols = [-160.0, -96.0, -32.0, 32.0, 96.0, 160.0];
        let mut out = [TornadoBounds::default(); 6];
        compute_tornado_bounds(&cols, &mut out);
        assert_eq!(
            out[0],
            TornadoBounds {
                min_x: -160.0,
                max_x: -32.0
            }
        );
        assert_eq!(
            out[3],
            TornadoBounds {
                min_x: -96.0,
                max_x: 160.0
            }
        );
    }

    #[test]
    fn sm_scale_interpolates_and_handles_degenerate_inputs() {
        assert!((sm_scale(0.25, 0.0, 1.0, 100.0, 200.0) - 125.0).abs() <= 1e-6);
        assert_eq!(sm_scale(0.25, 1.0, 1.0, 100.0, 200.0), 200.0);
    }

    #[test]
    fn quantize_step_rounds_to_nearest_step() {
        assert!((quantize_step(0.24, 0.5) - 0.0).abs() <= 1e-6);
        assert!((quantize_step(0.26, 0.5) - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn quantize_centi_keys_round_and_sanitize_inputs() {
        assert_eq!(quantize_centi_i32(1.234), 123);
        assert_eq!(quantize_centi_i32(-1.235), -124);
        assert_eq!(quantize_centi_i32(f64::NAN), 0);
        assert_eq!(quantize_centi_u32(1.235), 124);
        assert_eq!(quantize_centi_u32(-1.0), 0);
        assert_eq!(quantize_centi_u32(f64::INFINITY), 0);
    }

    #[test]
    fn mod_and_i16_keys_round_sanitize_and_clamp() {
        assert_eq!(mod_percent_key(1.234), 123);
        assert_eq!(mod_percent_key(-1.235), -124);
        assert_eq!(mod_percent_key(f32::NAN), 0);
        assert_eq!(mod_percent_key(1000.0), i16::MAX);
        assert_eq!(clamp_rounded_i16(12.5), 13);
        assert_eq!(clamp_rounded_i16(f32::NAN), 0);
        assert_eq!(clamp_rounded_i16(f32::NEG_INFINITY), 0);
        assert_eq!(clamp_rounded_i16(40_000.0), i16::MAX);
    }

    fn empty_mods_params() -> GameplayModsTextParams<'static> {
        GameplayModsTextParams {
            speed: ScrollSpeedSetting::XMod(1.0),
            noteskin: "devcel-2024",
            insert_mask: 0,
            remove_mask: 0,
            holds_mask: 0,
            turn_bits: 0,
            attack_mode: GameplayModsAttackMode::On,
            mini_percent: 0,
            spacing_percent: 0,
            visual_delay_ms: 0,
            average_error_bar_active: false,
            avg_error_bar_intensity_centi: 100,
            avg_error_bar_interval_ms: 100,
            accel: [0; 5],
            visual: [0; 9],
            appearance: [0; 5],
            scroll: [0; 5],
            perspective_tilt: 0,
            perspective_skew: 0,
            dark: 0,
            blind: 0,
            cover: 0,
            disabled_timing_windows: 0,
        }
    }

    #[test]
    fn display_mods_mini_keeps_full_percent() {
        let mut parts = Vec::new();
        append_mini_part(&mut parts, 100);
        assert_eq!(parts, vec!["100% Mini".to_string()]);
    }

    #[test]
    fn display_mods_keep_spaces_inside_one_option_atomic() {
        let text =
            join_display_mod_parts(&["devcel-2024".to_string(), "-4ms VisualDelay".to_string()]);

        assert_eq!(text, "devcel-2024, -4ms\u{00A0}VisualDelay");
    }

    #[test]
    fn display_mods_append_average_error_bar_config() {
        let mut params = empty_mods_params();
        params.average_error_bar_active = true;
        params.avg_error_bar_intensity_centi = 175;
        params.avg_error_bar_interval_ms = 300;

        let mut parts = Vec::new();
        append_average_error_bar_part(&mut parts, params);

        assert_eq!(parts, vec!["ErrorBar1.75x(Avg:300ms)".to_string()]);
    }

    #[test]
    fn display_mods_skip_average_error_bar_config_when_inactive() {
        let mut parts = Vec::new();
        append_average_error_bar_part(&mut parts, empty_mods_params());

        assert!(parts.is_empty());
    }

    #[test]
    fn display_mods_append_all_active_turns_in_itg_order() {
        let mut parts = Vec::new();
        append_turn_parts(&mut parts, DISPLAY_TURN_MIRROR | DISPLAY_TURN_RANDOM);
        assert_eq!(parts, vec!["Mirror".to_string(), "Random".to_string()]);
    }

    #[test]
    fn display_mods_use_simply_love_turn_names() {
        let mut parts = Vec::new();
        append_turn_parts(&mut parts, DISPLAY_TURN_UD_MIRROR);
        assert_eq!(parts, vec!["UD-Mirror".to_string()]);
    }

    #[test]
    fn display_mods_transform_order_matches_itg() {
        let mut parts = Vec::new();
        push_transform_parts(
            &mut parts,
            (1 << 0) | (1 << 1) | (1 << 7),
            (1 << 0) | (1 << 1),
            1 << 3,
        );
        assert_eq!(
            parts,
            vec![
                "NoRolls".to_string(),
                "NoMines".to_string(),
                "Little".to_string(),
                "Wide".to_string(),
                "Big".to_string(),
                "Mines".to_string(),
            ]
        );
    }

    #[test]
    fn display_mods_use_itg_disabled_timing_window_names() {
        assert_eq!(
            disabled_timing_windows_name((1 << 3) | (1 << 4)),
            Some("No W4/W5".to_string())
        );
        assert_eq!(
            disabled_timing_windows_name((1 << 0) | (1 << 1)),
            Some("No W1/W2".to_string())
        );
    }

    #[test]
    fn display_mods_perspective_names_match_itg_rules() {
        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, 0, 0);
        assert_eq!(parts, vec!["Overhead".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, -100, 100);
        assert_eq!(parts, vec!["Incoming".to_string()]);
    }

    #[test]
    fn gameplay_mods_text_formats_full_option_list() {
        let mut params = empty_mods_params();
        params.speed = ScrollSpeedSetting::CMod(725.0);
        params.attack_mode = GameplayModsAttackMode::Off;
        params.turn_bits = DISPLAY_TURN_MIRROR;
        params.remove_mask = (1 << 0) | (1 << 1);
        params.mini_percent = 100;
        params.visual_delay_ms = -4;
        params.average_error_bar_active = true;
        params.avg_error_bar_intensity_centi = 175;
        params.avg_error_bar_interval_ms = 300;
        params.disabled_timing_windows = (1 << 3) | (1 << 4);

        assert_eq!(
            gameplay_mods_text(params),
            "C725, 100%\u{00A0}Mini, NoAttacks, Mirror, NoMines, Little, Overhead, devcel-2024, -4ms\u{00A0}VisualDelay, ErrorBar1.75x(Avg:300ms), No\u{00A0}W4/W5"
        );
    }

    #[test]
    fn beat_factor_pulses_early_in_each_beat() {
        assert_eq!(beat_factor(-0.25), 0.0);
        assert_eq!(beat_factor(0.3), 0.0);
        assert!(beat_factor(0.0) > 0.0);
        assert!(beat_factor(1.0) < 0.0);
    }

    #[test]
    fn mod_divisor_preserves_sign_near_zero() {
        assert_eq!(mod_divisor(2.0), 2.0);
        assert_eq!(mod_divisor(0.0), 0.001);
        assert_eq!(mod_divisor(-0.0), -0.001);
    }

    #[test]
    fn bumpy_angle_sanitizes_non_finite_options() {
        assert!((bumpy_angle(16.0, f32::NAN, f32::NAN) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn accel_y_brake_matches_itg_pre_scroll_order() {
        let raw_y = ScrollSpeedSetting::ARROW_SPACING;
        let effect_height = 480.0;
        let scroll_speed = 2.0;
        let accel = AccelYParams {
            brake: 1.0,
            ..AccelYParams::default()
        };
        let itg_order = apply_accel_y(raw_y, 0.0, effect_height, 480.0, accel) * scroll_speed;
        let pre_scaled_order =
            apply_accel_y(raw_y * scroll_speed, 0.0, effect_height, 480.0, accel);
        let expected_itg_order = raw_y * (raw_y / effect_height) * scroll_speed;

        assert!(itg_order < pre_scaled_order);
        assert!((itg_order - expected_itg_order).abs() <= 0.001);
    }

    #[test]
    fn accel_y_reports_boomerang_peak_side() {
        let accel = AccelYParams {
            boomerang: 1.0,
            ..AccelYParams::default()
        };
        assert!(apply_accel_y_with_peak(100.0, 0.0, 480.0, 480.0, accel).1);
        assert!(!apply_accel_y_with_peak(400.0, 0.0, 480.0, 480.0, accel).1);
    }

    #[test]
    fn note_world_z_for_bumpy_uses_itg_sine_formula() {
        let z = note_world_z_for_bumpy(8.0 * std::f32::consts::PI, 1.0, 0.0, 0.0);
        assert!((z - 40.0).abs() <= 0.0001);

        let z = note_world_z_for_bumpy(-2.0 * std::f32::consts::PI, 1.0, 0.0, -1.25);
        assert!((z - 40.0).abs() <= 0.0001);

        assert_eq!(note_world_z_for_bumpy(8.0, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(note_world_z_for_bumpy(8.0, f32::NAN, 0.0, 0.0), 0.0);
    }

    #[test]
    fn itg_actor_rotation_z_converts_to_world_space() {
        assert_eq!(itg_actor_rotation_z(90.0), -90.0);
    }

    #[test]
    fn visual_hold_z_buffer_ignores_column_bumpy_like_itg() {
        assert!(!visual_hold_body_needs_z_buffer(VisualEffectParams {
            bumpy: 0.0,
            ..VisualEffectParams::default()
        }));
        assert!(visual_hold_body_needs_z_buffer(VisualEffectParams {
            bumpy: 1.0,
            ..VisualEffectParams::default()
        }));
    }

    #[test]
    fn visual_legacy_hold_sprites_disable_for_dynamic_effects() {
        assert!(visual_use_legacy_hold_sprites(0.0, 0.0, 0.0, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.1, 0.0, 0.0, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.1, 0.0, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.0, -0.1, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.0, 0.0, 0.1, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.0, 0.0, 0.0, 0.1));
        assert!(!visual_use_legacy_hold_sprites(
            0.0,
            0.0,
            0.0,
            0.0,
            f32::NAN
        ));
    }

    #[test]
    fn visual_effect_params_for_col_applies_column_mods() {
        let params = visual_effect_params_for_col(
            VisualEffectParams {
                tiny: 0.25,
                confusion_offset: 0.5,
                bumpy: 0.75,
                ..VisualEffectParams::default()
            },
            1,
            &[9.0, -0.5],
            &[9.0, f32::NAN],
            &[9.0, 0.25],
        );
        assert!((params.tiny + 0.25).abs() <= 1e-6);
        assert!((params.confusion_offset - 0.5).abs() <= 1e-6);
        assert!((params.bumpy - 1.0).abs() <= 1e-6);

        let params = visual_effect_params_for_col(params, 2, &[0.0], &[0.0, 0.0, 0.25], &[0.0]);
        assert!((params.tiny + 0.25).abs() <= 1e-6);
        assert!((params.confusion_offset - 0.75).abs() <= 1e-6);
        assert!((params.bumpy - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn visual_pulse_outer_zoom_matches_itg_formula() {
        let params = VisualEffectParams {
            pulse_outer: 1.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_pulse_zoom_for_y(0.0, params) - 1.0).abs() <= 1e-6);
        assert!(
            (visual_pulse_zoom_for_y(0.4 * 64.0 * std::f32::consts::FRAC_PI_2, params) - 1.5).abs()
                <= 1e-6
        );
    }

    #[test]
    fn visual_pulse_inner_zero_clamps_like_itg() {
        let params = VisualEffectParams {
            pulse_inner: -2.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_pulse_inner_zoom(params) - 0.01).abs() <= 1e-6);
    }

    #[test]
    fn visual_tiny_zoom_matches_itg_power_formula() {
        assert!(
            (visual_tiny_zoom(VisualEffectParams {
                tiny: 2.0,
                ..VisualEffectParams::default()
            }) - 0.5_f32.powf(2.0))
            .abs()
                <= 1e-6
        );
        assert!(
            (visual_tiny_zoom(VisualEffectParams {
                tiny: -0.5,
                ..VisualEffectParams::default()
            }) - 0.5_f32.powf(-0.5))
            .abs()
                <= 1e-6
        );
    }

    #[test]
    fn visual_arrow_effect_zoom_combines_tiny_and_pulse() {
        let params = VisualEffectParams {
            tiny: 1.0,
            pulse_outer: 1.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_arrow_effect_zoom(0.0, params) - 0.5).abs() <= 1e-6);

        let doubled = VisualEffectParams {
            tiny: -1.0,
            ..VisualEffectParams::default()
        };
        let base = scale_effect_size([64.0, 64.0], 1.25, 1.0);
        let scaled = scale_effect_size([64.0, 64.0], 1.25, visual_arrow_effect_zoom(0.0, doubled));
        assert!((scaled[0] - base[0] * 2.0).abs() <= 1e-6);
        assert!((scaled[1] - base[1] * 2.0).abs() <= 1e-6);
    }

    #[test]
    fn note_x_offset_applies_tiny_and_column_move_after_effects() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let move_x = [0.0, 0.5, 0.0, 0.0];
        let params = NoteXParams {
            screen_height: 480.0,
            drunk: 1.0,
            ..NoteXParams::default()
        };
        let offset = note_x_offset(
            1,
            0.0,
            1.0,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            &move_x,
            params,
            1.0,
        );
        let base = col_offsets[1]
            + note_x_extra(1, 0.0, 1.0, 0.0, &col_offsets, &invert, &tornado, params);
        assert!((offset - (base * 0.5 + 32.0)).abs() <= 1e-6);
    }

    #[test]
    fn receptor_row_center_uses_zero_travel_x_and_tipsy_y() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let params = NoteXParams {
            screen_height: 480.0,
            drunk: 1.0,
            ..NoteXParams::default()
        };
        let center = receptor_row_center(
            320.0,
            2,
            240.0,
            1.25,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            &[0.0; 4],
            &[0.0, 0.0, -0.25, 0.0],
            params,
            0.0,
            1.0,
        );
        let expected_x = 320.0
            + note_x_offset(
                2,
                0.0,
                1.25,
                0.0,
                &col_offsets,
                &invert,
                &tornado,
                &[0.0; 4],
                params,
                0.0,
            );
        let expected_y = 240.0 + tipsy_y_extra(2, 1.25, 1.0) - 16.0;
        assert!((center[0] - expected_x).abs() <= 1e-6);
        assert!((center[1] - expected_y).abs() <= 1e-6);
    }

    #[test]
    fn hold_indicator_column_x_uses_zero_travel_note_offset() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let move_x = [0.0, -0.5, 0.0, 0.0];
        let params = NoteXParams {
            screen_height: 480.0,
            invert: 1.0,
            ..NoteXParams::default()
        };
        let x = hold_indicator_column_x(
            320.0,
            1,
            0.75,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            &move_x,
            params,
            0.5,
        );
        let expected = 320.0
            + note_x_offset(
                1,
                0.0,
                0.75,
                0.0,
                &col_offsets,
                &invert,
                &tornado,
                &move_x,
                params,
                0.5,
            );
        assert!((x - expected).abs() <= 1e-6);
    }

    #[test]
    fn visual_confusion_offset_converts_static_rotation_to_actor_space() {
        let params = VisualEffectParams {
            confusion_offset: std::f32::consts::FRAC_PI_2,
            ..VisualEffectParams::default()
        };
        assert!((visual_confusion_rotation_deg(0.0, params) + 90.0).abs() <= 1e-6);
    }

    #[test]
    fn visual_note_rotation_converts_confusion_and_dizzy_to_actor_space() {
        let params = VisualEffectParams {
            confusion: 1.5,
            ..VisualEffectParams::default()
        };
        let rotation = visual_note_rotation_z(12.0, 3.5, true, params);
        let itg_expected = (3.5_f32 * params.confusion).rem_euclid(std::f32::consts::TAU)
            * (-180.0 / std::f32::consts::PI);
        assert!((rotation + itg_expected).abs() <= 1e-6);

        let params = VisualEffectParams {
            dizzy: 2.0,
            ..VisualEffectParams::default()
        };
        let rotation = visual_note_rotation_z(6.75, 3.5, false, params);
        let itg_expected =
            ((6.75 - 3.5) * params.dizzy) % std::f32::consts::TAU * (180.0 / std::f32::consts::PI);
        assert!((rotation + itg_expected).abs() <= 1e-6);
    }

    #[test]
    fn visual_negative_dizzy_rotates_notes_like_itgmania() {
        let params = VisualEffectParams {
            dizzy: -0.5,
            ..VisualEffectParams::default()
        };
        let rotation = visual_note_rotation_z(70.0, 68.0, false, params);
        let itg_expected =
            ((70.0 - 68.0) * params.dizzy) % std::f32::consts::TAU * (180.0 / std::f32::consts::PI);

        assert!(rotation.abs() > 1.0);
        assert!((rotation + itg_expected).abs() <= 1e-6);
    }

    #[test]
    fn smoothstep01_clamps_and_eases() {
        assert_eq!(smoothstep01(-1.0), 0.0);
        assert_eq!(smoothstep01(0.0), 0.0);
        assert_eq!(smoothstep01(1.0), 1.0);
        assert_eq!(smoothstep01(2.0), 1.0);
        assert!((smoothstep01(0.5) - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn reverse_column_cue_bounds_match_simply_love() {
        let lane_width = 64.0;
        let screen_height = 480.0;
        let receptor_reverse_y = 145.0;
        let cue_height = column_cue_height(screen_height);
        let top = column_cue_reverse_top_y(lane_width, cue_height, 0.0, receptor_reverse_y);
        let bottom = top + cue_height;

        assert!((cue_height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
        assert!((crossover_cue_height(screen_height) - 130.0).abs() <= 1e-6);
        assert!((COLUMN_CUE_Y_OFFSET - 80.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_default_layout_matches_original_simply_love() {
        let lane_width = 64.0;
        let screen_height = 480.0;
        let receptor_reverse_y = 145.0;
        let layout = column_flash_layout(false);
        let height = column_flash_height(screen_height, layout);
        let top = column_flash_reverse_top_y(layout, lane_width, height, 0.0, receptor_reverse_y);
        let bottom = top + height;

        assert!((layout.y_offset - 80.0).abs() <= 1e-6);
        assert!((layout.fade - 0.333).abs() <= 1e-6);
        assert!((height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_compact_layout_matches_chris_reference() {
        let lane_width = 64.0;
        let screen_height = 480.0;
        let receptor_reverse_y = 145.0;
        let layout = column_flash_layout(true);
        let height = column_flash_height(screen_height, layout);
        let top = column_flash_reverse_top_y(layout, lane_width, height, 0.0, receptor_reverse_y);
        let bottom = top + height;

        assert!((layout.y_offset - 70.0).abs() <= 1e-6);
        assert!((layout.height_trim - 270.0).abs() <= 1e-6);
        assert!((layout.fade - 0.2).abs() <= 1e-6);
        assert!((height - 140.0).abs() <= 1e-6);
        assert!((top - 247.0).abs() <= 1e-6);
        assert!((bottom - 387.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_alpha_at_decays_quadratically() {
        assert!((column_flash_alpha_at(1.0, 1.0, 0.5, 0.66) - 0.66).abs() <= 1e-6);
        assert!((column_flash_alpha_at(1.0, 1.25, 0.5, 0.66) - 0.495).abs() <= 1e-6);
        assert_eq!(column_flash_alpha_at(1.0, 1.5, 0.5, 0.66), 0.0);
    }

    #[test]
    fn column_flash_alpha_at_rejects_invalid_inputs() {
        assert_eq!(column_flash_alpha_at(1.0, 0.9, 0.5, 0.66), 0.0);
        assert_eq!(column_flash_alpha_at(1.0, 1.1, 0.0, 0.66), 0.0);
        assert_eq!(column_flash_alpha_at(1.0, f32::NAN, 0.5, 0.66), 0.0);
    }

    #[test]
    fn column_flash_alpha_matches_brightness_options() {
        let normal = column_flash_alpha(0.0, 0.0, 0.5, false);
        let dimmed = column_flash_alpha(0.0, 0.0, 0.5, true);

        assert!((normal - 0.66).abs() <= 1e-6);
        assert!((dimmed - 0.3).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_colors_match_reference_palette() {
        assert_eq!(
            column_flash_color(JudgeGrade::Miss, false, 0.3),
            [1.0, 0.0, 0.0, 0.3]
        );
        assert_eq!(
            column_flash_color(JudgeGrade::Decent, false, 0.3),
            [0.70, 0.36, 1.00, 0.3]
        );
        assert_eq!(
            column_flash_color(JudgeGrade::Fantastic, false, 0.3),
            [1.0, 1.0, 1.0, 0.3]
        );
    }

    #[test]
    fn field_effect_height_adds_tilt_margin() {
        assert_eq!(field_effect_height(480.0, 0.0), 480.0);
        assert_eq!(field_effect_height(480.0, -0.5), 580.0);
    }

    #[test]
    fn signed_effect_active_rejects_zero_epsilon_and_nan() {
        assert!(!signed_effect_active(0.0));
        assert!(!signed_effect_active(f32::EPSILON));
        assert!(!signed_effect_active(f32::NAN));
        assert!(signed_effect_active(-0.01));
    }

    #[test]
    fn tipsy_y_extra_matches_itg_column_wave() {
        assert_eq!(tipsy_y_extra(0, 0.0, 0.0), 0.0);
        assert!((tipsy_y_extra(0, 0.0, -1.0) + 25.6).abs() <= 1e-6);
    }

    #[test]
    fn beat_x_extra_uses_beat_factor_wave() {
        assert_eq!(beat_x_extra(0.0, 20.0, 0.0), 0.0);
        assert!((beat_x_extra(0.0, 20.0, 1.0) - 20.0).abs() <= 1e-6);
    }

    #[test]
    fn drunk_x_extra_uses_column_and_y_phase() {
        assert_eq!(drunk_x_extra(0, 0.0, 0.0, 480.0, 0.0), 0.0);
        assert!((drunk_x_extra(0, 0.0, 0.0, 480.0, -1.0) + 32.0).abs() <= 1e-6);
    }

    #[test]
    fn tornado_x_extra_scales_toward_bound_arc() {
        let bounds = TornadoBounds {
            min_x: -96.0,
            max_x: 96.0,
        };
        assert_eq!(tornado_x_extra(0.0, 0.0, bounds, 480.0, 0.0), 0.0);
        assert!((tornado_x_extra(0.0, 0.0, bounds, 480.0, 1.0) - 0.0).abs() <= 1e-4);
        assert!(tornado_x_extra(80.0, -96.0, bounds, 480.0, 1.0) > 0.0);
    }

    #[test]
    fn note_x_extra_flip_moves_to_mirrored_column() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let delta = note_x_extra(
            0,
            64.0,
            0.0,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            NoteXParams {
                screen_height: 480.0,
                flip: 1.0,
                tornado: 0.0,
                drunk: 0.0,
                invert: 0.0,
                beat: 0.0,
            },
        );
        assert!((delta - 192.0).abs() <= 1e-6);
    }

    #[test]
    fn note_x_extra_keeps_negative_position_mods_active_like_itg() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let delta = note_x_extra(
            0,
            0.0,
            0.0,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            NoteXParams {
                screen_height: 480.0,
                tornado: 0.0,
                drunk: -1.0,
                flip: -0.5,
                invert: 0.0,
                beat: 0.0,
            },
        );

        assert!((delta + 128.0).abs() <= 1e-6);
        assert!((tipsy_y_extra(0, 0.0, -1.0) + 25.6).abs() <= 1e-6);
    }

    #[test]
    fn appearance_blink_alpha_matches_itg_boolean_behavior() {
        let partial = appearance_note_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                blink: 0.3,
                ..NoteAlphaParams::default()
            },
        );
        let full = appearance_note_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                blink: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        assert!((partial - full).abs() <= 1e-6);
    }

    #[test]
    fn appearance_stealth_glow_matches_itg_visibility_curve() {
        let glow = appearance_note_glow(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                stealth: 0.25,
                ..NoteAlphaParams::default()
            },
        );
        assert!((glow - 0.65).abs() <= 1e-6);
    }

    #[test]
    fn appearance_note_actor_alpha_matches_itg_visibility_gate() {
        let half_visible = appearance_note_actor_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                stealth: 0.5,
                ..NoteAlphaParams::default()
            },
        );
        let mostly_visible = appearance_note_actor_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                stealth: 0.25,
                ..NoteAlphaParams::default()
            },
        );
        assert_eq!(half_visible, 0.0);
        assert_eq!(mostly_visible, 1.0);
    }

    #[test]
    fn appearance_needs_rows_only_for_y_varying_effects() {
        assert!(!appearance_needs_rows(NoteAlphaParams::default()));
        assert!(appearance_needs_rows(NoteAlphaParams {
            hidden: 1.0,
            ..NoteAlphaParams::default()
        }));
        assert!(appearance_needs_rows(NoteAlphaParams {
            sudden: 1.0,
            ..NoteAlphaParams::default()
        }));
        assert!(appearance_needs_rows(NoteAlphaParams {
            random_vanish: 1.0,
            ..NoteAlphaParams::default()
        }));
        assert!(!appearance_needs_rows(NoteAlphaParams {
            blink: 1.0,
            stealth: 1.0,
            ..NoteAlphaParams::default()
        }));
    }

    #[test]
    fn appearance_sudden_offset_shifts_fade_band_like_itg() {
        let base = appearance_note_alpha(
            180.0,
            0.0,
            0.0,
            NoteAlphaParams {
                sudden: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        let shifted = appearance_note_alpha(
            180.0,
            0.0,
            0.0,
            NoteAlphaParams {
                sudden: 1.0,
                sudden_offset: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        assert!(shifted > base);
    }

    #[test]
    fn tiny_spacing_scale_sanitizes_and_shrinks() {
        assert_eq!(tiny_spacing_scale(0.0), 1.0);
        assert_eq!(tiny_spacing_scale(f32::NAN), 1.0);
        assert!((tiny_spacing_scale(1.0) - 0.5).abs() <= 1e-6);
        assert_eq!(tiny_spacing_scale(-1.0), 1.0);
    }

    #[test]
    fn move_col_extra_scales_finite_columns() {
        assert_eq!(move_col_extra(&[0.0, 0.5], 1), 32.0);
        assert_eq!(move_col_extra(&[f32::NAN], 0), 0.0);
        assert_eq!(move_col_extra(&[], 4), 0.0);
    }

    #[test]
    fn itg_actor_glow_alpha_clamps_like_itg_vertex_color() {
        assert_eq!(itg_actor_glow_alpha(1.3), 1.0);
        assert_eq!(itg_actor_glow_alpha(0.65), 0.65);
        assert_eq!(itg_actor_glow_alpha(f32::NAN), 0.0);
    }

    #[test]
    fn hold_glow_color_uses_white_with_alpha() {
        assert_eq!(hold_glow_color(0.25), [1.0, 1.0, 1.0, 0.25]);
    }

    #[test]
    fn column_cue_alpha_fades_in_and_out() {
        assert!((column_cue_alpha(0.0, 1.0) - 0.0).abs() <= 1e-6);
        assert!((column_cue_alpha(0.075, 1.0) - 0.75).abs() <= 1e-6);
        assert!((column_cue_alpha(0.15, 1.0) - 1.0).abs() <= 1e-6);
        assert!((column_cue_alpha(0.5, 1.0) - 1.0).abs() <= 1e-6);
        assert!((column_cue_alpha(0.925, 1.0) - 0.75).abs() <= 1e-6);
        assert!((column_cue_alpha(1.0, 1.0) - 0.0).abs() <= 1e-6);
    }

    #[test]
    fn column_cue_alpha_rejects_invalid_ranges() {
        assert_eq!(column_cue_alpha(-0.1, 1.0), 0.0);
        assert_eq!(column_cue_alpha(1.1, 1.0), 0.0);
        assert_eq!(column_cue_alpha(0.1, 0.3), 0.0);
        assert_eq!(column_cue_alpha(f32::NAN, 1.0), 0.0);
        assert_eq!(column_cue_alpha(0.1, f32::INFINITY), 0.0);
    }

    #[test]
    fn error_bar_tick_alpha_matches_tick_modes() {
        assert_eq!(error_bar_tick_alpha(-0.1, 0.5, false), 0.0);
        assert_eq!(error_bar_tick_alpha(0.2, 0.5, false), 1.0);
        assert_eq!(error_bar_tick_alpha(0.5, 0.5, false), 0.0);

        assert_eq!(error_bar_tick_alpha(0.02, 0.5, true), 1.0);
        assert!((error_bar_tick_alpha(0.265, 0.5, true) - 0.5).abs() <= 1e-6);
        assert_eq!(error_bar_tick_alpha(0.5, 0.5, true), 0.0);
    }

    #[test]
    fn error_bar_flash_alpha_falls_back_to_base_alpha() {
        assert!((error_bar_flash_alpha(1.0, None, 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.0, Some(1.2), 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.6, Some(1.0), 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(f32::NAN, Some(1.0), 0.5) - 0.3).abs() <= 1e-6);
    }

    #[test]
    fn error_bar_flash_alpha_fades_from_full_to_base() {
        assert!((error_bar_flash_alpha(1.0, Some(1.0), 0.5) - 1.0).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.25, Some(1.0), 0.5) - 0.65).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.49, Some(1.0), 0.5) - 0.314).abs() <= 1e-6);
    }

    #[test]
    fn error_bar_boundaries_insert_fa_plus_split() {
        let windows = [0.015, 0.0225, 0.045, 0.09, 0.135];
        let (bounds, len) = error_bar_boundaries_s(windows, Some(0.010), true, 0);

        assert_eq!(len, 2);
        assert!((bounds[0] - 0.010).abs() <= 1e-6);
        assert!((bounds[1] - windows[0]).abs() <= 1e-6);
    }

    #[test]
    fn error_bar_boundaries_clamp_to_max_window() {
        let windows = [0.015, 0.0225, 0.045, 0.09, 0.135];
        let (bounds, len) = error_bar_boundaries_s(windows, None, false, 99);

        assert_eq!(len, 5);
        assert_eq!(&bounds[..len], &windows);
    }

    #[test]
    fn stream_segment_indices_handle_boundaries_and_nan() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 4,
                is_break: false,
            },
            StreamSegment {
                start: 4,
                end: 8,
                is_break: true,
            },
        ];

        assert_eq!(stream_segment_index_exclusive_end(&segs, 4.0), 1);
        assert_eq!(stream_segment_index_inclusive_end(&segs, 4.0), 0);
        assert_eq!(
            stream_segment_index_exclusive_end(&segs, f32::NAN),
            segs.len()
        );
        assert_eq!(
            stream_segment_index_inclusive_end(&segs, f32::NAN),
            segs.len()
        );
        assert_eq!(zmod_run_timer_index(&segs, 9.0), None);
    }

    #[test]
    fn zmod_broken_run_merges_short_breaks_and_adjacent_streams() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 10,
                is_break: true,
            },
            StreamSegment {
                start: 10,
                end: 14,
                is_break: false,
            },
            StreamSegment {
                start: 14,
                end: 20,
                is_break: true,
            },
        ];

        assert_eq!(zmod_broken_run_end(&segs, 0), (14, true));
        assert_eq!(zmod_broken_run_segment(&segs, 9.0), Some((0, 14, true)));
        assert_eq!(zmod_broken_run_segment(&segs, 15.0), Some((3, 20, false)));
        assert_eq!(zmod_broken_run_segment(&segs, 21.0), None);
    }

    #[test]
    fn zmod_measure_counter_text_describes_current_and_lookahead_segments() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 12,
                is_break: true,
            },
            StreamSegment {
                start: 12,
                end: 20,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 0, false, 2, 1.0),
            Some(ZmodMeasureCounterText::Ratio {
                current: 4,
                total: 8
            })
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 1, true, 2, 1.0),
            Some(ZmodMeasureCounterText::Break(4))
        );
        assert_eq!(
            zmod_measure_counter_text(36.0, 9.0, &segs, 1, false, 2, 1.0),
            Some(ZmodMeasureCounterText::Break(3))
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 2, true, 2, 1.0),
            Some(ZmodMeasureCounterText::Total(8))
        );
        assert_eq!(
            zmod_measure_counter_text(36.0, 9.0, &segs, 1, false, 0, 1.0),
            None
        );
    }

    #[test]
    fn zmod_measure_counter_text_handles_negative_song_time() {
        let stream_first = [StreamSegment {
            start: 0,
            end: 8,
            is_break: false,
        }];
        let break_first = [
            StreamSegment {
                start: 0,
                end: 2,
                is_break: true,
            },
            StreamSegment {
                start: 2,
                end: 8,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_measure_counter_text(-4.0, -1.0, &stream_first, 0, false, 1, 1.0),
            Some(ZmodMeasureCounterText::Break(2))
        );
        assert_eq!(
            zmod_measure_counter_text(-4.0, -1.0, &break_first, 0, false, 1, 1.0),
            Some(ZmodMeasureCounterText::Break(4))
        );
    }

    #[test]
    fn zmod_broken_run_counter_text_uses_merged_stream_length() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 10,
                is_break: true,
            },
            StreamSegment {
                start: 10,
                end: 14,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_broken_run_counter_text(3.0, &segs, 0, 14),
            Some(ZmodMeasureCounterText::Ratio {
                current: 4,
                total: 14
            })
        );
        assert_eq!(
            zmod_broken_run_counter_text(-1.0, &segs, 0, 14),
            Some(ZmodMeasureCounterText::Break(2))
        );
        assert_eq!(zmod_broken_run_counter_text(9.0, &segs, 1, 10), None);
    }

    #[test]
    fn timing_window_from_num_saturates_to_w5() {
        assert_eq!(timing_window_from_num(0), TimingWindow::W0);
        assert_eq!(timing_window_from_num(4), TimingWindow::W4);
        assert_eq!(timing_window_from_num(5), TimingWindow::W5);
        assert_eq!(timing_window_from_num(99), TimingWindow::W5);
    }

    #[test]
    fn zmod_percent_from_points_matches_two_decimal_floor() {
        assert_eq!(zmod_percent_from_points(-5, 100), 0.0);
        assert_eq!(zmod_percent_from_points(1, 3), 33.33);
        assert_eq!(zmod_percent_from_points(125, 100), 125.0);
        assert_eq!(zmod_percent_from_points(50, 0), 0.0);
    }

    #[test]
    fn error_bar_colors_follow_judgment_palette() {
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W0, true),
            [33.0 / 255.0, 204.0 / 255.0, 232.0 / 255.0, 1.0]
        );
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W1, true),
            [1.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W1, false),
            [33.0 / 255.0, 204.0 / 255.0, 232.0 / 255.0, 1.0]
        );
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W5, true),
            [201.0 / 255.0, 133.0 / 255.0, 94.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn zmod_subtractive_counter_uses_whites_for_ex_paths() {
        let itg = MiniIndicatorProgress {
            w2: 4,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&itg, MiniIndicatorScoreType::Itg),
            (4, false)
        );

        let ex = MiniIndicatorProgress {
            w2: 0,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&ex, MiniIndicatorScoreType::Ex),
            (7, false)
        );

        let hard_ex = MiniIndicatorProgress {
            w2: 1,
            white_10ms_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&hard_ex, MiniIndicatorScoreType::HardEx),
            (7, true)
        );
    }

    #[test]
    fn zmod_subtractive_points_supports_all_score_types() {
        let itg = MiniIndicatorProgress {
            current_possible_dp: 20,
            actual_dp: 16,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&itg, MiniIndicatorScoreType::Itg),
            4
        );

        let itg_mine = MiniIndicatorProgress {
            current_possible_dp: 0,
            actual_dp: -6,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&itg_mine, MiniIndicatorScoreType::Itg),
            6
        );

        let ex = MiniIndicatorProgress {
            white_count: 3,
            w2: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(zmod_subtractive_points(&ex, MiniIndicatorScoreType::Ex), 6);

        let ex_with_great = MiniIndicatorProgress {
            white_count: 3,
            w2: 1,
            w3: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&ex_with_great, MiniIndicatorScoreType::Ex),
            11
        );

        let hard_ex = MiniIndicatorProgress {
            white_10ms_count: 3,
            w2: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&hard_ex, MiniIndicatorScoreType::HardEx),
            8
        );
    }

    #[test]
    fn zmod_mini_indicator_zoom_matches_size_setting() {
        assert!(
            (zmod_mini_indicator_zoom(MiniIndicatorSize::Default) - 0.35).abs() <= f32::EPSILON
        );
        assert!((zmod_mini_indicator_zoom(MiniIndicatorSize::Large) - 0.5).abs() <= f32::EPSILON);
    }

    #[test]
    fn effective_mini_value_applies_fallback_big_and_clamp() {
        assert_eq!(effective_mini_value(80.0, 25.0, 0.0), 0.8);
        assert_eq!(effective_mini_value(f32::NAN, 25.0, 0.0), 0.25);
        assert_eq!(effective_mini_value(80.0, 25.0, 1.0), -0.2);
        assert_eq!(effective_mini_value(-200.0, 25.0, 0.0), -1.0);
        assert_eq!(effective_mini_value(200.0, 25.0, 0.0), 1.5);
    }

    #[test]
    fn zmod_pace_colors_match_expected_channels() {
        assert_eq!(zmod_rival_color(99.0, 98.0), [0.0, 1.0, 1.0, 1.0]);
        assert_eq!(zmod_rival_color(98.0, 99.0), [1.0, 0.0, 0.0, 1.0]);

        let ahead = zmod_pacemaker_color(101.0, 100.0);
        assert!((ahead[0] - 0.99).abs() <= 1e-6);
        assert!((ahead[1] - 0.51).abs() <= 1e-6);
        assert_eq!(ahead[2], 1.0);
        assert_eq!(ahead[3], 1.0);
    }

    #[test]
    fn zmod_combo_glow_color_interpolates_sine_phase() {
        fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
            for i in 0..4 {
                assert!(
                    (actual[i] - expected[i]).abs() <= 1e-6,
                    "channel {i}: {} != {}",
                    actual[i],
                    expected[i]
                );
            }
        }

        let color1 = [0.0, 0.2, 0.4, 1.0];
        let color2 = [1.0, 0.6, 0.0, 1.0];

        assert_rgba_close(
            zmod_combo_glow_color(color1, color2, 0.0),
            [0.5, 0.4, 0.2, 1.0],
        );
        assert_rgba_close(
            zmod_combo_glow_color(color1, color2, 0.2),
            [1.0, 0.6, 0.0, 1.0],
        );
        assert_rgba_close(
            zmod_combo_glow_color(color1, color2, 0.6),
            [0.0, 0.2, 0.4, 1.0],
        );
    }

    #[test]
    fn zmod_combo_grade_colors_match_palettes() {
        fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
            for i in 0..4 {
                assert!(
                    (actual[i] - expected[i]).abs() <= 1e-6,
                    "channel {i}: {} != {}",
                    actual[i],
                    expected[i]
                );
            }
        }

        let (fa1, fa2) = zmod_combo_glow_pair(JudgeGrade::Fantastic, false);
        assert_rgba_close(fa1, [200.0 / 255.0, 1.0, 1.0, 1.0]);
        assert_rgba_close(fa2, [107.0 / 255.0, 240.0 / 255.0, 1.0, 1.0]);
        assert_rgba_close(
            zmod_combo_solid_color(JudgeGrade::Excellent, false),
            [226.0 / 255.0, 156.0 / 255.0, 24.0 / 255.0, 1.0],
        );
        assert_eq!(
            zmod_combo_solid_color(JudgeGrade::Miss, false),
            [1.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn zmod_combo_quint_uses_fa_plus_palette() {
        fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
            for i in 0..4 {
                assert!(
                    (actual[i] - expected[i]).abs() <= 1e-6,
                    "channel {i}: {} != {}",
                    actual[i],
                    expected[i]
                );
            }
        }

        let (quint1, quint2) = zmod_combo_glow_pair(JudgeGrade::Fantastic, true);
        assert_rgba_close(quint1, [247.0 / 255.0, 192.0 / 255.0, 254.0 / 255.0, 1.0]);
        assert_rgba_close(quint2, [233.0 / 255.0, 40.0 / 255.0, 1.0, 1.0]);
        assert_rgba_close(
            zmod_combo_solid_color(JudgeGrade::Fantastic, true),
            [233.0 / 255.0, 40.0 / 255.0, 1.0, 1.0],
        );
    }

    #[test]
    fn zmod_combo_quint_active_requires_fa_plus_and_only_w0_hits() {
        let quint = timing::WindowCounts {
            w0: 3,
            ..timing::WindowCounts::default()
        };
        assert!(zmod_combo_quint_active(true, quint));
        assert!(!zmod_combo_quint_active(false, quint));

        let with_w1 = timing::WindowCounts { w1: 1, ..quint };
        assert!(!zmod_combo_quint_active(true, with_w1));

        let with_miss = timing::WindowCounts { miss: 1, ..quint };
        assert!(!zmod_combo_quint_active(true, with_miss));
    }

    #[test]
    fn zmod_resolved_mini_indicator_mode_uses_legacy_fallbacks() {
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::RivalScoring, true, true),
            MiniIndicatorMode::RivalScoring
        );
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::None, true, true),
            MiniIndicatorMode::SubtractiveScoring
        );
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::None, false, true),
            MiniIndicatorMode::Pacemaker
        );
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::None, false, false),
            MiniIndicatorMode::None
        );
    }

    #[test]
    fn zmod_resolved_combo_color_gates_full_combo_rainbow() {
        let params = ZmodComboColorParams {
            style: ZmodComboColorStyle::Rainbow,
            full_combo_mode: true,
            combo: 10,
            full_combo_grade: Some(JudgeGrade::Decent),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            quint_active: false,
            elapsed_s: 0.0,
        };
        assert_eq!(zmod_resolved_combo_color(params), [1.0, 1.0, 1.0, 1.0]);

        let active = ZmodComboColorParams {
            full_combo_grade: Some(JudgeGrade::Great),
            ..params
        };
        assert_eq!(
            zmod_resolved_combo_color(active),
            zmod_combo_rainbow_color(0.0, false, 10)
        );
    }

    #[test]
    fn zmod_resolved_combo_color_uses_current_or_full_grade() {
        let current = ZmodComboColorParams {
            style: ZmodComboColorStyle::Solid,
            full_combo_mode: false,
            combo: 0,
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Great),
            quint_active: false,
            elapsed_s: 0.0,
        };
        assert_eq!(
            zmod_static_combo_color(current),
            zmod_combo_solid_color(JudgeGrade::Great, false)
        );

        let full = ZmodComboColorParams {
            full_combo_mode: true,
            quint_active: true,
            ..current
        };
        assert_eq!(
            zmod_resolved_combo_color(full),
            zmod_combo_solid_color(JudgeGrade::Fantastic, true)
        );
    }

    #[test]
    fn zmod_indicator_default_color_uses_judgment_thresholds() {
        assert_eq!(
            zmod_indicator_default_color(96.0),
            [33.0 / 255.0, 204.0 / 255.0, 232.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(89.0),
            [226.0 / 255.0, 156.0 / 255.0, 24.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(80.0),
            [102.0 / 255.0, 201.0 / 255.0, 85.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(68.0),
            [180.0 / 255.0, 92.0 / 255.0, 1.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(67.99),
            [1.0, 48.0 / 255.0, 48.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn zmod_indicator_detailed_color_uses_expanded_thresholds() {
        assert_eq!(zmod_indicator_detailed_color(99.0), [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(
            zmod_indicator_detailed_color(98.0),
            [37.0 / 255.0, 110.0 / 255.0, 206.0 / 255.0, 1.0]
        );
        assert_eq!(zmod_indicator_detailed_color(96.0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(
            zmod_indicator_detailed_color(94.0),
            [253.0 / 255.0, 163.0 / 255.0, 7.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_detailed_color(90.0),
            [121.0 / 255.0, 169.0 / 255.0, 1.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_detailed_color(85.0),
            [185.0 / 255.0, 50.0 / 255.0, 226.0 / 255.0, 1.0]
        );
        assert_eq!(zmod_indicator_detailed_color(84.99), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn zmod_combo_rainbow_color_applies_scroll_combo_offset() {
        assert_eq!(
            zmod_combo_rainbow_color(0.0, false, 0),
            [1.0, 0.0, 0.0, 1.0]
        );

        let scrolled = zmod_combo_rainbow_color(0.0, true, 10);
        assert!((scrolled[0] - 1.0).abs() <= 1e-6);
        assert!((scrolled[1] - 0.78).abs() <= 1e-6);
        assert_eq!(scrolled[2], 0.0);
        assert_eq!(scrolled[3], 1.0);
    }

    #[test]
    fn zmod_mini_indicator_output_handles_subtractive_count_and_percent() {
        let params = ZmodMiniIndicatorParams {
            mode: MiniIndicatorMode::SubtractiveScoring,
            color_style: MiniIndicatorColorStyle::Default,
            subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
            score_type: MiniIndicatorScoreType::Itg,
            combo_color: [0.2, 0.3, 0.4, 1.0],
            is_failing: false,
            life: 1.0,
            rival_score_percent: 0.0,
            target_score_percent: 0.0,
            stream_completion: None,
        };
        let count = MiniIndicatorProgress {
            judged_any: true,
            kept_percent: 99.0,
            lost_percent: 1.0,
            w2: 4,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_mini_indicator_output(&count, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::NegativeInt(4),
                color: rgba8(0xff, 0x55, 0xcc),
            })
        );

        let forced_percent = MiniIndicatorProgress { w3: 1, ..count };
        assert_eq!(
            zmod_mini_indicator_output(&forced_percent, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 1.0,
                    negative: true,
                },
                color: zmod_indicator_default_color(99.0),
            })
        );
    }

    #[test]
    fn zmod_mini_indicator_output_handles_rival_pacemaker_and_stream() {
        let mut params = ZmodMiniIndicatorParams {
            mode: MiniIndicatorMode::RivalScoring,
            color_style: MiniIndicatorColorStyle::Default,
            subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
            score_type: MiniIndicatorScoreType::Itg,
            combo_color: [0.2, 0.3, 0.4, 1.0],
            is_failing: false,
            life: 1.0,
            rival_score_percent: 99.0,
            target_score_percent: 98.0,
            stream_completion: Some(0.95),
        };
        let progress = MiniIndicatorProgress {
            judged_any: true,
            current_score_percent: 98.0,
            current_possible_ratio: 0.5,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 48.5,
                    negative: false,
                },
                color: zmod_rival_color(98.0, 49.5),
            })
        );

        params.mode = MiniIndicatorMode::Pacemaker;
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 49.0,
                    negative: false,
                },
                color: zmod_pacemaker_color(9800.0, 4900.0),
            })
        );

        params.mode = MiniIndicatorMode::StreamProg;
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(95.0),
                color: [0.0, 1.0, 0.5, 1.0],
            })
        );
    }

    #[test]
    fn zmod_stream_prog_completion_counts_stream_beats_only() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 2,
                is_break: false,
            },
            StreamSegment {
                start: 2,
                end: 4,
                is_break: true,
            },
            StreamSegment {
                start: 4,
                end: 6,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, -1.0),
            Some(0.0)
        );
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, f32::NAN),
            Some(0.0)
        );
        assert_eq!(zmod_stream_prog_completion_for_beat(0.0, &segs, 1.0), None);
        assert_eq!(zmod_stream_prog_completion_for_beat(4.0, &[], 1.0), None);
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, 3.0),
            Some(0.25)
        );
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, 19.0),
            Some(0.75)
        );
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, 23.0),
            Some(1.0)
        );
    }

    #[test]
    fn error_bar_text_scalable_zoom_matches_sl_fork_curve_at_default_threshold() {
        fn assert_close(actual: f32, expected: f32) {
            assert!(
                (actual - expected).abs() <= 0.0001,
                "{actual} != {expected}"
            );
        }

        let w2_ms = timing::TimingProfile::default_itg_with_fa_plus().windows_s[0] * 1000.0;
        let smaller_white_ms = timing::FA_PLUS_W010_MS;
        let w1_ms = timing::FA_PLUS_W0_MS;
        let inner_mid_ms = (smaller_white_ms + w1_ms) * 0.5;
        let fantastic_mid_ms = (w1_ms + w2_ms) * 0.5;

        assert_close(
            error_bar_text_scalable_zoom(inner_mid_ms, 10.0, w2_ms),
            0.35,
        );
        assert_close(
            error_bar_text_scalable_zoom(fantastic_mid_ms, 10.0, w2_ms),
            0.4,
        );
        assert_close(error_bar_text_scalable_zoom(w2_ms + 1.0, 10.0, w2_ms), 0.45);
    }

    #[test]
    fn player_metric_y_applies_notefield_offset_after_reverse_mix() {
        const EPS: f32 = 1e-5;
        let center_y = 240.0;
        let offset_y = -22.0;

        let standard = player_metric_y(center_y, offset_y, 0.0, -90.0, 90.0);
        let reverse = player_metric_y(center_y, offset_y, 1.0, -90.0, 90.0);
        let split = player_metric_y(center_y, offset_y, 0.5, -90.0, 90.0);

        assert!((standard - 128.0).abs() <= EPS);
        assert!((reverse - 308.0).abs() <= EPS);
        assert!((split - 218.0).abs() <= EPS);
    }

    #[test]
    fn notefield_view_proj_rejects_invalid_screen_sizes() {
        assert!(notefield_view_proj(0.0, 480.0, 320.0, 240.0, 0.0, 0.0, false).is_none());
        assert!(notefield_view_proj(640.0, f32::NAN, 320.0, 240.0, 0.0, 0.0, false).is_none());
    }

    #[test]
    fn notefield_view_proj_returns_finite_matrix_for_flat_field() {
        let matrix = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.0, 0.0, false)
            .expect("valid notefield projection");

        assert!(matrix.to_cols_array().into_iter().all(f32::is_finite));
    }

    #[test]
    fn notefield_view_proj_changes_with_tilt_skew_and_reverse() {
        let flat = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.0, 0.0, false)
            .expect("flat projection");
        let tilted = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.5, 0.3, false)
            .expect("tilted projection");
        let reverse = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.5, 0.3, true)
            .expect("reverse projection");

        assert_ne!(flat.to_cols_array(), tilted.to_cols_array());
        assert_ne!(tilted.to_cols_array(), reverse.to_cols_array());
    }

    #[test]
    fn hud_y_only_uses_reverse_branch_for_full_reverse() {
        let normal_y = 100.0;
        let reverse_y = 200.0;
        let centered_y = 300.0;
        assert!((hud_y(normal_y, reverse_y, centered_y, false, 0.3) - 160.0).abs() <= 1e-6);
        assert!((hud_y(normal_y, reverse_y, centered_y, true, 0.3) - 230.0).abs() <= 1e-6);
    }

    fn default_zmod_layout_params() -> ZmodLayoutParams {
        ZmodLayoutParams {
            judgment_height: 40.0,
            has_error_bar: true,
            has_judgment_texture: true,
            error_bar_up: false,
            has_measure_counter: false,
            measure_counter_up: false,
            broken_run: false,
            mini_indicator_position: LayoutMiniIndicatorPosition::Default,
        }
    }

    #[test]
    fn hud_layout_offsets_apply_independently() {
        let params = HudLayoutParams {
            zmod: default_zmod_layout_params(),
            has_judgment_texture: true,
            error_bar_up: false,
            error_bar_offset: 25.0,
        };
        let base = hud_layout_ys(100.0, 160.0, false, HudLayoutOffsets::default(), params);
        let moved_judgment = hud_layout_ys(
            100.0,
            160.0,
            false,
            HudLayoutOffsets {
                judgment_extra_y: 25.0,
                ..HudLayoutOffsets::default()
            },
            params,
        );
        assert_eq!(moved_judgment.judgment_y, 125.0);
        assert_eq!(moved_judgment.zmod_layout.combo_y, base.zmod_layout.combo_y);
        assert_eq!(moved_judgment.error_bar_y, base.error_bar_y);

        let moved_combo = hud_layout_ys(
            100.0,
            160.0,
            false,
            HudLayoutOffsets {
                combo_extra_y: -30.0,
                ..HudLayoutOffsets::default()
            },
            params,
        );
        assert_eq!(moved_combo.judgment_y, base.judgment_y);
        assert_eq!(
            moved_combo.zmod_layout.combo_y,
            base.zmod_layout.combo_y - 30.0
        );
        assert_eq!(moved_combo.error_bar_y, base.error_bar_y);

        let moved_error_bar = hud_layout_ys(
            100.0,
            160.0,
            false,
            HudLayoutOffsets {
                error_bar_extra_y: 18.0,
                ..HudLayoutOffsets::default()
            },
            params,
        );
        assert_eq!(moved_error_bar.judgment_y, base.judgment_y);
        assert_eq!(
            moved_error_bar.zmod_layout.combo_y,
            base.zmod_layout.combo_y
        );
        assert_eq!(moved_error_bar.error_bar_y, base.error_bar_y + 18.0);
    }

    #[test]
    fn zmod_layout_places_measure_and_subtractive_rows() {
        let mut params = default_zmod_layout_params();
        params.has_measure_counter = true;
        params.measure_counter_up = true;
        params.broken_run = true;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);

        assert_eq!(layout.measure_counter_y, Some(56.0));
        assert_eq!(layout.subtractive_scoring_y, 143.0);
        assert_eq!(layout.subtractive_scoring_addx, 0.0);
        assert_eq!(layout.combo_y, 171.0);

        params.mini_indicator_position = LayoutMiniIndicatorPosition::UnderUpArrow;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);
        assert_eq!(layout.subtractive_scoring_y, 76.0);
        assert_eq!(layout.subtractive_scoring_addx, -60.0);
    }

    #[test]
    fn combo_actor_zoom_matches_itgmania_player_mini_formula() {
        assert!((combo_actor_zoom(0.0) - 1.0).abs() <= 1e-6);
        assert!((combo_actor_zoom(1.0) - 0.5).abs() <= 1e-6);
        assert!((combo_actor_zoom(0.5) - 0.5_f32.sqrt()).abs() <= 1e-6);
        assert!((combo_actor_zoom(-1.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_actor_zoom_matches_itgmania_player_mini_formula_without_judgment_back() {
        assert!((judgment_actor_zoom(0.0, false, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.0, false, 0.0, 0.0) - 0.5).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.5, false, 0.0, 0.0) - 0.5_f32.sqrt()).abs() <= 1e-6);
        assert!((judgment_actor_zoom(-1.0, false, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, -1.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, 1.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, -1.0, 1.0) - 1.0).abs() <= 1e-6);
        for &mini in &[-1.0_f32, 0.0, 0.25, 0.5, 1.0, 1.5] {
            assert!(
                (judgment_actor_zoom(mini, false, -1.0, 0.0) - combo_actor_zoom(mini)).abs()
                    <= 1e-6
            );
        }
    }

    #[test]
    fn judgment_actor_zoom_matches_arrow_cloud_judgment_back_formula() {
        assert!((judgment_actor_zoom(0.35, true, 0.0, 0.0) - 0.825).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.5, true, 0.0, 0.0) - 0.35).abs() <= 1e-6);
        assert!((judgment_actor_zoom(-1.0, true, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, true, -1.0, 0.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_tilt_thresholds_deadzone_and_cap() {
        let params = JudgmentTiltParams {
            enabled: true,
            grade: JudgeGrade::Fantastic,
            time_error_ms: 5.0,
            min_threshold_ms: 5.0,
            max_threshold_ms: 20.0,
            multiplier: 1.0,
        };
        assert_eq!(judgment_tilt_rotation_deg(params), 0.0);
        assert!(
            (judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 10.0,
                ..params
            }) + 1.5)
                .abs()
                <= 1e-6
        );
        assert!(
            (judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 40.0,
                ..params
            }) + 4.5)
                .abs()
                <= 1e-6
        );
    }

    #[test]
    fn judgment_tilt_keeps_early_late_direction() {
        let params = JudgmentTiltParams {
            enabled: true,
            grade: JudgeGrade::Fantastic,
            time_error_ms: -10.0,
            min_threshold_ms: 0.0,
            max_threshold_ms: 50.0,
            multiplier: 1.0,
        };
        assert!(judgment_tilt_rotation_deg(params) > 0.0);
        assert!(
            judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 10.0,
                ..params
            }) < 0.0
        );
    }

    fn tap_rows_params(time_error_ms: f32) -> TapJudgmentRowsParams {
        TapJudgmentRowsParams {
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W0),
            time_error_ms,
            frame_rows: 7,
            show_fa_plus_window: false,
            fa_plus_10ms_blue_window: false,
            split_15_10ms: false,
            custom_fantastic_window: false,
        }
    }

    #[test]
    fn tap_judgment_rows_overlay_white_for_split_15_10_hits() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                split_15_10ms: true,
                ..tap_rows_params(12.0)
            }),
            (0, Some(1))
        );
    }

    #[test]
    fn tap_judgment_rows_keep_plain_blue_when_split_is_off() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
    }

    #[test]
    fn tap_judgment_rows_use_10ms_blue_window() {
        let blue = TapJudgmentRowsParams {
            show_fa_plus_window: true,
            fa_plus_10ms_blue_window: true,
            time_error_ms: timing::FA_PLUS_W010_MS,
            ..tap_rows_params(0.0)
        };
        let white = TapJudgmentRowsParams {
            time_error_ms: 12.0,
            ..blue
        };

        assert_eq!(tap_judgment_rows(blue), (0, None));
        assert_eq!(tap_judgment_rows(white), (1, None));
    }

    #[test]
    fn tap_judgment_rows_split_keeps_blue_base_above_10ms() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: true,
                split_15_10ms: true,
                ..tap_rows_params(12.0)
            }),
            (0, Some(1))
        );
    }

    #[test]
    fn tap_judgment_rows_ignore_split_without_fa_plus_window() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                split_15_10ms: true,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
    }

    #[test]
    fn tap_judgment_rows_defer_to_custom_window_over_fixed_split() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                window: Some(TimingWindow::W1),
                time_error_ms: 14.0,
                show_fa_plus_window: true,
                split_15_10ms: true,
                custom_fantastic_window: true,
                ..tap_rows_params(0.0)
            }),
            (1, None)
        );
    }

    #[test]
    fn tap_judgment_rows_keep_six_row_assets_unsplit() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                split_15_10ms: true,
                frame_rows: 6,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                grade: JudgeGrade::Excellent,
                window: Some(TimingWindow::W1),
                time_error_ms: 18.0,
                show_fa_plus_window: true,
                split_15_10ms: true,
                frame_rows: 6,
                ..tap_rows_params(0.0)
            }),
            (1, None)
        );
    }

    #[test]
    fn average_error_bar_mini_scale_shrinks_with_mini() {
        assert!((average_error_bar_mini_scale(0.0) - 1.1).abs() <= 1e-6);
        assert!((average_error_bar_mini_scale(1.0) - 0.555).abs() <= 1e-6);
        assert_eq!(average_error_bar_mini_scale(4.0), 0.0);
    }

    #[test]
    fn held_miss_zoom_pops_then_fades() {
        assert_eq!(held_miss_zoom(0.0, 0.0), (0.8, 0.75));
        assert_eq!(held_miss_zoom(0.2, 0.0), (0.75, 0.75));
        let faded = held_miss_zoom(0.5, 0.0);
        assert!(faded.0.abs() <= 1e-6);
        assert!(faded.1.abs() <= 1e-6);
        assert_eq!(held_miss_zoom(0.2, 1.0), (0.375, 0.375));
    }

    #[test]
    fn hold_tail_cap_bounds_join_at_body_bottom_for_normal_scroll() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        let (top, bottom) = hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(96.0))
            .expect("cap should draw");
        assert_eq!((top, bottom), (96.0, 120.0));
    }

    #[test]
    fn hold_tail_cap_bounds_falls_back_when_body_is_below_tail_anchor() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(104.0), Some(160.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn hold_tail_cap_bounds_skip_when_body_does_not_reach_tail() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(70.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(140.0), Some(200.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, None, Some(95.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn collapsed_hold_body_uses_tail_cap_fallback_bounds() {
        let body_top = 120.0;
        let body_bottom = 120.0;
        let natural_top = 100.0;
        let natural_bottom = 100.0;
        assert_eq!(
            clipped_hold_body_bounds(body_top, body_bottom, natural_top, natural_bottom),
            None
        );
        assert_eq!(
            hold_tail_cap_bounds(natural_bottom, 24.0, None, None),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn hold_body_bottom_for_tail_cap_joins_tail_edge_with_overlap() {
        assert_eq!(hold_body_bottom_for_tail_cap(140.0, 100.0, 0.0), 140.0);
        assert_eq!(hold_body_bottom_for_tail_cap(140.0, 100.0, 24.0), 101.0);
        assert_eq!(hold_body_bottom_for_tail_cap(99.5, 100.0, 24.0), 101.0);
        assert_eq!(hold_body_bottom_for_tail_cap(80.0, 100.0, 24.0), 80.0);
    }

    #[test]
    fn collapsed_hold_draw_span_still_draws_caps() {
        assert_eq!(hold_draw_span(120.0, 120.0, 480.0), Some((120.0, 120.0)));
    }

    #[test]
    fn tiny_hold_body_repeat_uses_mesh_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 0.25);
        assert!(budget >= 3602);
        assert!(!allow_legacy);
    }

    #[test]
    fn normal_hold_body_repeat_keeps_legacy_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 64.0);
        assert_eq!(budget, 2048);
        assert!(allow_legacy);
    }

    #[test]
    fn hold_strip_row_3d_preserves_row_z() {
        let row = hold_strip_row_3d(
            [64.0, 128.0, 12.5],
            [0.0, 16.0],
            8.0,
            0.0,
            1.0,
            0.5,
            [1.0; 4],
        );
        assert!((row[0].pos[2] - 12.5).abs() <= 1e-6);
        assert!((row[1].pos[2] - 12.5).abs() <= 1e-6);
    }

    #[test]
    fn hold_strip_actor_carries_depth_test_flag() {
        let actor = hold_strip_actor(
            Arc::from("hold.png"),
            Arc::from([]),
            BlendMode::Alpha,
            true,
            42,
        );
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                depth_test: true,
                ..
            }
        ));
    }

    #[test]
    fn hold_strip_glow_actor_uses_texture_mask_pass() {
        let actor = hold_strip_glow_actor(Arc::from("hold.png"), Arc::from([]), true, 43);
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                tint: [1.0, 1.0, 1.0, 0.0],
                glow: [1.0, 1.0, 1.0, 1.0],
                depth_test: true,
                ..
            }
        ));
    }

    #[test]
    fn actor_with_world_z_updates_textured_mesh_depth() {
        let actor = hold_strip_actor(
            Arc::from("hold.png"),
            Arc::from([]),
            BlendMode::Alpha,
            false,
            42,
        );
        let actor = actor_with_world_z(actor, 12.5);
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                world_z,
                ..
            } if (world_z - 12.5).abs() <= 1e-6
        ));
    }

    #[test]
    fn share_actor_range_drains_into_shared_frame() {
        let mut actors = vec![
            hold_strip_actor(
                Arc::from("a.png"),
                Arc::from([]),
                BlendMode::Alpha,
                false,
                1,
            ),
            hold_strip_actor(
                Arc::from("b.png"),
                Arc::from([]),
                BlendMode::Alpha,
                false,
                2,
            ),
        ];
        let shared = share_actor_range(&mut actors, 1).expect("range should be shared");
        assert_eq!(actors.len(), 2);
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].len(), 1);
        match &actors[1] {
            Actor::SharedFrame { size, children, .. } => {
                assert!(matches!(size, [SizeSpec::Fill, SizeSpec::Fill]));
                assert!(Arc::ptr_eq(children, &shared[0]));
            }
            _ => panic!("expected shared frame"),
        }
    }

    #[test]
    fn built_notefield_empty_has_no_actor_outputs() {
        let built = BuiltNotefield::empty(320.0);
        assert_eq!(built.layout_center_x, 320.0);
        assert!(built.field_actors.is_empty());
        assert!(built.judgment_actors.is_none());
        assert!(built.combo_actors.is_none());
    }

    #[test]
    fn tap_note_types_choose_noteskin_animation_parts() {
        assert!(matches!(
            tap_part_for_note_type(NoteType::Tap),
            NoteAnimPart::Tap
        ));
        assert!(matches!(
            tap_part_for_note_type(NoteType::Fake),
            NoteAnimPart::Fake
        ));
        assert!(matches!(
            tap_part_for_note_type(NoteType::Lift),
            NoteAnimPart::Lift
        ));
        assert_eq!(mine_part(), NoteAnimPart::Mine);
    }

    #[test]
    fn hold_note_types_choose_noteskin_animation_parts() {
        let hold = hold_parts_for_note_type(NoteType::Hold);
        assert_eq!(hold.head, NoteAnimPart::HoldHead);
        assert_eq!(hold.body, NoteAnimPart::HoldBody);
        assert_eq!(hold.topcap, NoteAnimPart::HoldTopCap);
        assert_eq!(hold.bottomcap, NoteAnimPart::HoldBottomCap);

        let roll = hold_parts_for_note_type(NoteType::Roll);
        assert_eq!(roll.head, NoteAnimPart::RollHead);
        assert_eq!(roll.body, NoteAnimPart::RollBody);
        assert_eq!(roll.topcap, NoteAnimPart::RollTopCap);
        assert_eq!(roll.bottomcap, NoteAnimPart::RollBottomCap);

        assert_eq!(hold_head_part_for_roll(false), NoteAnimPart::HoldHead);
        assert_eq!(hold_head_part_for_roll(true), NoteAnimPart::RollHead);
    }

    #[test]
    fn same_row_tap_replacement_selects_enabled_head() {
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, false, true, false, true),
            Some(TapReplacementHead {
                is_roll: false,
                part: NoteAnimPart::HoldHead
            })
        );
        assert_eq!(
            tap_replacement_head(NoteType::Lift, false, true, false, true, true),
            Some(TapReplacementHead {
                is_roll: true,
                part: NoteAnimPart::RollHead
            })
        );
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, true, true, true, true),
            Some(TapReplacementHead {
                is_roll: false,
                part: NoteAnimPart::HoldHead
            })
        );
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, true, true, true, false),
            Some(TapReplacementHead {
                is_roll: true,
                part: NoteAnimPart::RollHead
            })
        );
    }

    #[test]
    fn same_row_tap_replacement_ignores_disabled_or_nontap_notes() {
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, false, false, true, true),
            None
        );
        assert_eq!(
            tap_replacement_head(NoteType::Hold, true, true, true, true, true),
            None
        );
        assert_eq!(
            tap_replacement_head(NoteType::Fake, true, true, true, true, true),
            None
        );
    }

    #[test]
    fn bottom_cap_uv_window_matches_itg_add_to_tex_coord_progression() {
        let (v0, v1) = bottom_cap_uv_window(0.0, 1.0, 12.0, 24.0, false)
            .expect("partial cap should produce UVs");
        assert!((v0 - 0.5).abs() <= 1e-6);
        assert!((v1 - 1.0).abs() <= 1e-6);

        let (full_v0, full_v1) =
            bottom_cap_uv_window(0.0, 1.0, 24.0, 24.0, false).expect("full cap should produce UVs");
        assert!((full_v0 - 0.0).abs() <= 1e-6);
        assert!((full_v1 - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_honors_top_anchor_when_reverse() {
        let (v0, v1) = bottom_cap_uv_window(0.2, 0.8, 12.0, 24.0, true)
            .expect("top-anchored reverse path should produce UVs");
        assert!((v0 - 0.2).abs() <= 1e-6);
        assert!((v1 - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_rejects_degenerate_inputs() {
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 0.0, 24.0, false), None);
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 24.0, 0.0, false), None);
    }

    #[test]
    fn hold_segment_pose_keeps_vertical_segments_unrotated() {
        let (center, length, rotation) = hold_segment_pose([32.0, 100.0], [32.0, 180.0]);
        assert_eq!(center, [32.0, 140.0]);
        assert!((length - 80.0).abs() <= 1e-6);
        assert!(rotation.abs() <= 1e-6);
    }

    #[test]
    fn hold_segment_pose_uses_diagonal_length_and_rotation() {
        let (center, length, rotation) = hold_segment_pose([0.0, 0.0], [30.0, 40.0]);
        assert_eq!(center, [15.0, 20.0]);
        assert!((length - 50.0).abs() <= 1e-6);
        assert!((rotation - 36.869_896).abs() <= 1e-5);
    }

    #[test]
    fn song_time_ns_helpers_convert_signed_deltas() {
        assert_eq!(song_time_ns_to_seconds(1_500_000_000), 1.5);
        assert_eq!(
            song_time_ns_delta_seconds(1_250_000_000, 2_000_000_000),
            -0.75
        );
    }

    #[test]
    fn mine_hides_after_any_final_resolution() {
        assert!(!mine_hides_after_resolution(None));
        assert!(mine_hides_after_resolution(Some(MineResult::Hit)));
        assert!(mine_hides_after_resolution(Some(MineResult::Avoided)));
    }

    #[test]
    fn visible_note_window_uses_itg_rows_not_dense_rows() {
        let notes = vec![
            test_note_at_dense_row(0.0, 0),
            test_note_at_dense_row(4.0, 1),
        ];
        let note_indices = vec![0usize, 1usize];
        let mut visited = Vec::new();

        for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(3.5), beat_to_note_row(4.5))),
            |note_index| visited.push(note_index),
        );

        assert_eq!(note_itg_row(&notes[1]), beat_to_note_row(4.0));
        assert_eq!(visited, vec![1]);
    }

    #[test]
    fn visible_hold_window_includes_holds_started_before_range() {
        let notes = vec![test_hold_at_beat(0.0, 8.0), test_hold_at_beat(12.0, 16.0)];
        let hold_indices = vec![0usize, 1usize];
        let visible_range = Some((beat_to_note_row(4.0), beat_to_note_row(5.0)));
        let mut visited = Vec::new();

        for_each_visible_hold_index(&hold_indices, &notes, visible_range, |note_index| {
            visited.push(note_index);
        });

        assert!(hold_overlaps_visible_window(0, &notes, visible_range));
        assert!(!hold_overlaps_visible_window(1, &notes, visible_range));
        assert_eq!(visited, vec![0]);
    }

    #[test]
    fn find_first_displayed_beat_uses_note_count_cutoff() {
        let stats = (0..80)
            .map(|i| NoteCountStat {
                beat: i as f32 * 0.25,
                notes_lower: i,
                notes_upper: i + 1,
            })
            .collect::<Vec<_>>();

        let first =
            find_first_displayed_beat(20.0, 120.0, &stats, |_| 0.0).expect("finite beat range");

        assert!((3.9..=4.1).contains(&first), "first beat was {first}");
    }

    #[test]
    fn find_first_displayed_beat_falls_back_without_count_cache() {
        let first = find_first_displayed_beat(8.0, 120.0, &[], |beat| (beat - 4.0) * 64.0)
            .expect("finite beat range");

        assert!((4.0..=4.001).contains(&first), "first beat was {first}");
    }

    #[test]
    fn find_first_displayed_beat_rejects_invalid_inputs() {
        assert_eq!(
            find_first_displayed_beat(f32::NAN, 120.0, &[], |_| 0.0),
            None
        );
        assert_eq!(
            find_first_displayed_beat(0.0, f32::INFINITY, &[], |_| 0.0),
            None
        );
    }

    #[test]
    fn find_last_displayed_beat_searches_until_draw_distance() {
        let last = find_last_displayed_beat(0.0, 120.0, 1.0, false, |beat| (beat * 64.0, true))
            .expect("finite beat range");

        assert!((last - 1.875).abs() <= 0.001, "last beat was {last}");
    }

    #[test]
    fn find_last_displayed_beat_caps_slow_scroll_lookahead() {
        let last = find_last_displayed_beat(4.0, 120.0, 0.5, false, |_| (0.0, true))
            .expect("finite beat range");

        assert_eq!(last, 20.0);
    }

    #[test]
    fn find_last_displayed_beat_handles_invalid_and_boomerang_inputs() {
        assert_eq!(
            find_last_displayed_beat(f32::NAN, 120.0, 1.0, false, |_| (0.0, true)),
            None
        );
        assert_eq!(
            find_last_displayed_beat(0.0, f32::INFINITY, 1.0, false, |_| (0.0, true)),
            None
        );

        let normal = find_last_displayed_beat(0.0, 120.0, 1.0, false, |_| (200.0, false)).unwrap();
        let boomerang =
            find_last_displayed_beat(0.0, 120.0, 1.0, true, |_| (200.0, false)).unwrap();

        assert!(boomerang > normal);
    }
}
