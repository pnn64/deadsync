use crate::act;
use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::assets::{self, AssetManager};
use crate::engine::audio;
use crate::engine::gfx::BlendMode;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::game::parsing::noteskin::{
    self, NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, Quantization, SpriteSlot,
};
use crate::game::song::SongData;
use crate::screens::components::shared::heart_bg;
use crate::screens::components::shared::noteskin_model::noteskin_model_actor;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ScreenAction};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ----------------------------- cursor tweening ----------------------------- */
// Simply Love metrics.ini uses 0.1 for both [ScreenOptions] TweenSeconds and CursorTweenSeconds.
// Player Options row/cursor motion should keep this exact parity timing.
const SL_OPTION_ROW_TWEEN_SECONDS: f32 = 0.1;
const CURSOR_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;
const ROW_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;
// Simply Love [ScreenOptions] uses RowOnCommand/RowOffCommand with linear,0.2.
const PANE_FADE_SECONDS: f32 = 0.2;
const TAP_EXPLOSION_PREVIEW_SPEED: f32 = 0.7;
// Spacing between inline items in OptionRows (pixels at current zoom)
const INLINE_SPACING: f32 = 15.75;
const TILT_INTENSITY_MIN: f32 = 0.05;
const TILT_INTENSITY_MAX: f32 = 10.00;
const TILT_INTENSITY_STEP: f32 = 0.05;
const HUD_OFFSET_MIN: i32 = crate::game::profile::HUD_OFFSET_MIN;
const HUD_OFFSET_MAX: i32 = crate::game::profile::HUD_OFFSET_MAX;
const HUD_OFFSET_ZERO_INDEX: usize = (-HUD_OFFSET_MIN) as usize;

// Match Simply Love / ScreenOptions defaults.
const VISIBLE_ROWS: usize = 10;
const ROW_START_OFFSET: f32 = -164.0;
const ROW_HEIGHT: f32 = 33.0;
const TITLE_BG_WIDTH: f32 = 127.0;

fn hud_offset_choices() -> Vec<String> {
    (HUD_OFFSET_MIN..=HUD_OFFSET_MAX)
        .map(|v| v.to_string())
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct RowWindow {
    first_start: i32,
    first_end: i32,
    second_start: i32,
    second_end: i32,
}

#[inline(always)]
fn compute_row_window(
    total_rows: usize,
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
) -> RowWindow {
    if total_rows == 0 {
        return RowWindow {
            first_start: 0,
            first_end: 0,
            second_start: 0,
            second_end: 0,
        };
    }

    let total_rows_i = total_rows as i32;
    if total_rows <= VISIBLE_ROWS {
        return RowWindow {
            first_start: 0,
            first_end: total_rows_i,
            second_start: total_rows_i,
            second_end: total_rows_i,
        };
    }

    let total = VISIBLE_ROWS as i32;
    let halfsize = total / 2;

    // Mirror ITGmania ScreenOptions::PositionRows() semantics (signed math matters).
    let p1_choice = if active[P1] {
        selected_row[P1] as i32
    } else {
        selected_row[P2] as i32
    };
    let p2_choice = if active[P2] {
        selected_row[P2] as i32
    } else {
        selected_row[P1] as i32
    };
    let p1_choice = p1_choice.clamp(0, total_rows_i - 1);
    let p2_choice = p2_choice.clamp(0, total_rows_i - 1);

    let (mut first_start, mut first_end, mut second_start, mut second_end) =
        if active[P1] && active[P2] {
            let earliest = p1_choice.min(p2_choice);
            let first_start = (earliest - halfsize / 2).max(0);
            let first_end = first_start + halfsize;

            let latest = p1_choice.max(p2_choice);
            let second_start = (latest - halfsize / 2).max(0).max(first_end);
            let second_end = second_start + halfsize;
            (first_start, first_end, second_start, second_end)
        } else {
            let first_start = (p1_choice - halfsize).max(0);
            let first_end = first_start + total;
            (first_start, first_end, first_end, first_end)
        };

    first_end = first_end.min(total_rows_i);
    second_end = second_end.min(total_rows_i);

    loop {
        let sum = (first_end - first_start) + (second_end - second_start);
        if sum >= total_rows_i || sum >= total {
            break;
        }
        if second_start > first_end {
            second_start -= 1;
        } else if first_start > 0 {
            first_start -= 1;
        } else if second_end < total_rows_i {
            second_end += 1;
        } else {
            break;
        }
    }

    RowWindow {
        first_start,
        first_end,
        second_start,
        second_end,
    }
}

#[inline(always)]
fn row_layout_params() -> (f32, f32) {
    // Must match the geometry in get_actors(): rows align to the help box.
    let frame_h = ROW_HEIGHT;
    let first_row_center_y = screen_center_y() + ROW_START_OFFSET;
    let help_box_h = 40.0_f32;
    let help_box_bottom_y = screen_height() - 36.0;
    let help_top_y = help_box_bottom_y - help_box_h;
    let n_rows_f = VISIBLE_ROWS as f32;
    let mut row_gap = if n_rows_f > 0.0 {
        (n_rows_f - 0.5).mul_add(-frame_h, help_top_y - first_row_center_y) / n_rows_f
    } else {
        0.0
    };
    if !row_gap.is_finite() || row_gap < 0.0 {
        row_gap = 0.0;
    }
    (first_row_center_y, frame_h + row_gap)
}

#[inline(always)]
fn init_row_tweens(
    rows: &[Row],
    selected_row: [usize; PLAYER_SLOTS],
    active: [bool; PLAYER_SLOTS],
    hide_active_mask: [u8; PLAYER_SLOTS],
    error_bar_active_mask: [u8; PLAYER_SLOTS],
    allow_per_player_global_offsets: bool,
) -> Vec<RowTween> {
    let total_rows = rows.len();
    if total_rows == 0 {
        return Vec::new();
    }

    let (first_row_center_y, row_step) = row_layout_params();
    let visibility = row_visibility(
        rows,
        active,
        hide_active_mask,
        error_bar_active_mask,
        allow_per_player_global_offsets,
    );
    let visible_rows = count_visible_rows(rows, visibility);
    if visible_rows == 0 {
        let y = first_row_center_y - row_step * 0.5;
        return (0..total_rows)
            .map(|_| RowTween {
                from_y: y,
                to_y: y,
                from_a: 0.0,
                to_a: 0.0,
                t: 1.0,
            })
            .collect();
    }

    let selected_visible = std::array::from_fn(|player_idx| {
        let idx = selected_row[player_idx].min(total_rows.saturating_sub(1));
        row_to_visible_index(rows, idx, visibility).unwrap_or(0)
    });
    let w = compute_row_window(visible_rows, selected_visible, active);
    let mid_pos = (VISIBLE_ROWS as f32) * 0.5 - 0.5;
    let bottom_pos = (VISIBLE_ROWS as f32) - 0.5;
    let measure_counter_anchor_visible_idx =
        parent_anchor_visible_index(rows, RowId::MeasureCounter, visibility);
    let judgment_font_anchor_visible_idx =
        parent_anchor_visible_index(rows, RowId::JudgmentFont, visibility);
    let judgment_tilt_anchor_visible_idx =
        parent_anchor_visible_index(rows, RowId::JudgmentTilt, visibility);
    let combo_font_anchor_visible_idx =
        parent_anchor_visible_index(rows, RowId::ComboFont, visibility);
    let error_bar_anchor_visible_idx = parent_anchor_visible_index(rows, RowId::ErrorBar, visibility);
    let hide_anchor_visible_idx = parent_anchor_visible_index(rows, RowId::Hide, visibility);

    let mut out: Vec<RowTween> = Vec::with_capacity(total_rows);
    let mut visible_idx = 0i32;
    for i in 0..total_rows {
        let visible = is_row_visible(rows, i, visibility);
        let (f_pos, hidden) = if visible {
            let ii = visible_idx;
            visible_idx += 1;
            f_pos_for_visible_idx(ii, w, mid_pos, bottom_pos)
        } else {
            let anchor =
                rows.get(i)
                    .and_then(|row| match conditional_row_parent(row.id) {
                        Some(RowId::MeasureCounter) => measure_counter_anchor_visible_idx,
                        Some(RowId::JudgmentFont) => judgment_font_anchor_visible_idx,
                        Some(RowId::JudgmentTilt) => judgment_tilt_anchor_visible_idx,
                        Some(RowId::ComboFont) => combo_font_anchor_visible_idx,
                        Some(RowId::ErrorBar) => error_bar_anchor_visible_idx,
                        Some(RowId::Hide) => hide_anchor_visible_idx,
                        _ => None,
                    });
            if let Some(anchor_idx) = anchor {
                let (anchor_f_pos, _) = f_pos_for_visible_idx(anchor_idx, w, mid_pos, bottom_pos);
                (anchor_f_pos, true)
            } else {
                (-0.5, true)
            }
        };

        let y = (row_step * f_pos) + first_row_center_y;
        let a = if hidden { 0.0 } else { 1.0 };
        out.push(RowTween {
            from_y: y,
            to_y: y,
            from_a: a,
            to_a: a,
            t: 1.0,
        });
    }

    out
}

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

const PLAYER_SLOTS: usize = 2;
const P1: usize = 0;
const P2: usize = 1;

const MATCH_NOTESKIN_LABEL: &str = "MatchNoteSkinLabel";
const NO_TAP_EXPLOSION_LABEL: &str = "NoTapExplosionLabel";

#[inline(always)]
fn active_player_indices(active: [bool; PLAYER_SLOTS]) -> impl Iterator<Item = usize> {
    [P1, P2]
        .into_iter()
        .filter(move |&player_idx| active[player_idx])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsPane {
    Main,
    Advanced,
    Uncommon,
}

#[derive(Clone, Copy, Debug)]
enum PaneTransition {
    None,
    FadingOut { target: OptionsPane, t: f32 },
    FadingIn { t: f32 },
}

impl PaneTransition {
    #[inline(always)]
    fn alpha(self) -> f32 {
        match self {
            Self::None => 1.0,
            Self::FadingOut { t, .. } => (1.0 - t).clamp(0.0, 1.0),
            Self::FadingIn { t } => t.clamp(0.0, 1.0),
        }
    }

    #[inline(always)]
    fn is_active(self) -> bool {
        !matches!(self, Self::None)
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowId {
    TypeOfSpeedMod,
    SpeedMod,
    Mini,
    Perspective,
    NoteSkin,
    MineSkin,
    ReceptorSkin,
    TapExplosionSkin,
    JudgmentFont,
    JudgmentOffsetX,
    JudgmentOffsetY,
    ComboFont,
    ComboOffsetX,
    ComboOffsetY,
    HoldJudgment,
    BackgroundFilter,
    NoteFieldOffsetX,
    NoteFieldOffsetY,
    VisualDelay,
    GlobalOffsetShift,
    MusicRate,
    Stepchart,
    WhatComesNext,
    Exit,
    // Advanced pane
    Turn,
    Scroll,
    Hide,
    LifeMeterType,
    LifeBarOptions,
    DataVisualizations,
    DensityGraphBackground,
    TargetScore,
    ActionOnMissedTarget,
    MiniIndicator,
    IndicatorScoreType,
    GameplayExtras,
    ComboColors,
    ComboColorMode,
    CarryCombo,
    JudgmentTilt,
    JudgmentTiltIntensity,
    JudgmentBehindArrows,
    OffsetIndicator,
    ErrorBar,
    ErrorBarTrim,
    ErrorBarOptions,
    ErrorBarOffsetX,
    ErrorBarOffsetY,
    MeasureCounter,
    MeasureCounterLookahead,
    MeasureCounterOptions,
    MeasureLines,
    RescoreEarlyHits,
    EarlyDecentWayOffOptions,
    ResultsExtras,
    TimingWindows,
    FAPlusOptions,
    CustomBlueFantasticWindow,
    CustomBlueFantasticWindowMs,
    // Uncommon pane
    Insert,
    Remove,
    Holds,
    Accel,
    Effect,
    Appearance,
    Attacks,
    HideLightType,
    GameplayExtrasMore,
}

pub struct Row {
    pub id: RowId,
    pub name: LookupKey,
    pub choices: Vec<String>,
    pub selected_choice_index: [usize; PLAYER_SLOTS],
    pub help: Vec<String>,
    pub choice_difficulty_indices: Option<Vec<usize>>,
}

#[derive(Clone, Debug)]
pub struct FixedStepchart {
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct SpeedMod {
    pub mod_type: String, // "X", "C", "M"
    pub value: f32,
}

#[inline(always)]
fn scroll_speed_for_mod(speed_mod: &SpeedMod) -> crate::game::scroll::ScrollSpeedSetting {
    match speed_mod.mod_type.as_str() {
        "C" => crate::game::scroll::ScrollSpeedSetting::CMod(speed_mod.value),
        "X" => crate::game::scroll::ScrollSpeedSetting::XMod(speed_mod.value),
        "M" => crate::game::scroll::ScrollSpeedSetting::MMod(speed_mod.value),
        _ => crate::game::scroll::ScrollSpeedSetting::default(),
    }
}

#[inline(always)]
fn sync_profile_scroll_speed(profile: &mut crate::game::profile::Profile, speed_mod: &SpeedMod) {
    profile.scroll_speed = scroll_speed_for_mod(speed_mod);
}

#[derive(Clone, Copy, Debug)]
struct RowTween {
    from_y: f32,
    to_y: f32,
    from_a: f32,
    to_a: f32,
    t: f32,
}

impl RowTween {
    #[inline(always)]
    fn y(&self) -> f32 {
        (self.to_y - self.from_y).mul_add(self.t, self.from_y)
    }

    #[inline(always)]
    fn a(&self) -> f32 {
        (self.to_a - self.from_a).mul_add(self.t, self.from_a)
    }
}

pub struct State {
    pub song: Arc<SongData>,
    pub return_screen: Screen,
    pub fixed_stepchart: Option<FixedStepchart>,
    pub chart_steps_index: [usize; PLAYER_SLOTS],
    pub chart_difficulty_index: [usize; PLAYER_SLOTS],
    pub rows: Vec<Row>,
    pub selected_row: [usize; PLAYER_SLOTS],
    pub prev_selected_row: [usize; PLAYER_SLOTS],
    // For Scroll row: bitmask of which options are enabled.
    // 0 => Normal scroll (no special modifier).
    pub scroll_active_mask: [u8; PLAYER_SLOTS],
    // For Hide row: bitmask of which options are enabled.
    // bit0 = Targets, bit1 = Background, bit2 = Combo, bit3 = Life,
    // bit4 = Score, bit5 = Danger, bit6 = Combo Explosions.
    pub hide_active_mask: [u8; PLAYER_SLOTS],
    // For FA+ Options row: bitmask of which options are enabled.
    // bit0 = Display FA+ Window, bit1 = Display EX Score, bit2 = Display H.EX Score,
    // bit3 = Display FA+ Pane, bit4 = 10ms Blue Window, bit5 = 15/10ms Split.
    pub fa_plus_active_mask: [u8; PLAYER_SLOTS],
    // For Early Decent/Way Off Options row: bitmask of which options are enabled.
    // bit0 = Hide Judgments, bit1 = Hide NoteField Flash.
    pub early_dw_active_mask: [u8; PLAYER_SLOTS],
    // For Gameplay Extras row: bitmask of which options are enabled.
    // bit0 = Flash Column for Miss, bit1 = Density Graph at Top,
    // bit2 = Column Cues, bit3 = Display Scorebox.
    pub gameplay_extras_active_mask: [u8; PLAYER_SLOTS],
    // For Gameplay Extras (More) row: bitmask of which options are enabled.
    // bit0 = Column Cues, bit1 = Display Scorebox.
    pub gameplay_extras_more_active_mask: [u8; PLAYER_SLOTS],
    // For Results Extras row: bitmask of which options are enabled.
    // bit0 = Track Early Judgments.
    pub results_extras_active_mask: [u8; PLAYER_SLOTS],
    // For Life Bar Options row: bitmask of which options are enabled.
    // bit0 = Rainbow Max, bit1 = Responsive Colors, bit2 = Show Life Percentage.
    pub life_bar_options_active_mask: [u8; PLAYER_SLOTS],
    // For Error Bar row: bitmask of which options are enabled.
    // bit0 = Colorful, bit1 = Monochrome, bit2 = Text, bit3 = Highlight, bit4 = Average.
    pub error_bar_active_mask: [u8; PLAYER_SLOTS],
    // For Error Bar Options row: bitmask of which options are enabled.
    // bit0 = Move Up, bit1 = Multi-Tick (Simply Love semantics).
    pub error_bar_options_active_mask: [u8; PLAYER_SLOTS],
    // For Measure Counter Options row: bitmask of which options are enabled.
    // bit0 = Move Left, bit1 = Move Up, bit2 = Vertical Lookahead,
    // bit3 = Broken Run Total, bit4 = Run Timer.
    pub measure_counter_options_active_mask: [u8; PLAYER_SLOTS],
    // For Insert row: bitmask of enabled chart insert transforms.
    // bit0 = Wide, bit1 = Big, bit2 = Quick, bit3 = BMRize,
    // bit4 = Skippy, bit5 = Echo, bit6 = Stomp.
    pub insert_active_mask: [u8; PLAYER_SLOTS],
    // For Remove row: bitmask of enabled chart removal transforms.
    // bit0 = Little, bit1 = No Mines, bit2 = No Holds, bit3 = No Jumps,
    // bit4 = No Hands, bit5 = No Quads, bit6 = No Lifts, bit7 = No Fakes.
    pub remove_active_mask: [u8; PLAYER_SLOTS],
    // For Holds row: bitmask of enabled hold transforms.
    // bit0 = Planted, bit1 = Floored, bit2 = Twister,
    // bit3 = No Rolls, bit4 = Holds To Rolls.
    pub holds_active_mask: [u8; PLAYER_SLOTS],
    // For Accel Effects row: bitmask of enabled acceleration transforms.
    // bit0 = Boost, bit1 = Brake, bit2 = Wave, bit3 = Expand, bit4 = Boomerang.
    pub accel_effects_active_mask: [u8; PLAYER_SLOTS],
    // For Visual Effects row: bitmask of enabled visual transforms.
    // bit0 = Drunk, bit1 = Dizzy, bit2 = Confusion, bit3 = Big,
    // bit4 = Flip, bit5 = Invert, bit6 = Tornado, bit7 = Tipsy,
    // bit8 = Bumpy, bit9 = Beat.
    pub visual_effects_active_mask: [u16; PLAYER_SLOTS],
    // For Appearance Effects row: bitmask of enabled appearance transforms.
    // bit0 = Hidden, bit1 = Sudden, bit2 = Stealth, bit3 = Blink, bit4 = R.Vanish.
    pub appearance_effects_active_mask: [u8; PLAYER_SLOTS],
    pub active_color_index: i32,
    pub speed_mod: [SpeedMod; PLAYER_SLOTS],
    pub music_rate: f32,
    pub current_pane: OptionsPane,
    pub scroll_focus_player: usize,
    bg: heart_bg::State,
    pub nav_key_held_direction: [Option<NavDirection>; PLAYER_SLOTS],
    pub nav_key_held_since: [Option<Instant>; PLAYER_SLOTS],
    pub nav_key_last_scrolled_at: [Option<Instant>; PLAYER_SLOTS],
    pub start_held_since: [Option<Instant>; PLAYER_SLOTS],
    pub start_last_triggered_at: [Option<Instant>; PLAYER_SLOTS],
    inline_choice_x: [f32; PLAYER_SLOTS],
    arcade_row_focus: [bool; PLAYER_SLOTS],
    allow_per_player_global_offsets: bool,
    pub player_profiles: [crate::game::profile::Profile; PLAYER_SLOTS],
    noteskin_names: Vec<String>,
    noteskin_cache: HashMap<String, Arc<Noteskin>>,
    noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    mine_noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    receptor_noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    tap_explosion_noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    preview_time: f32,
    preview_beat: f32,
    help_anim_time: [f32; PLAYER_SLOTS],
    // Combo preview state (for Combo Font row)
    combo_preview_count: u32,
    combo_preview_elapsed: f32,
    // Cursor ring tween (StopTweening/BeginTweening parity with ITGmania ScreenOptions::TweenCursor).
    cursor_initialized: [bool; PLAYER_SLOTS],
    cursor_from_x: [f32; PLAYER_SLOTS],
    cursor_from_y: [f32; PLAYER_SLOTS],
    cursor_from_w: [f32; PLAYER_SLOTS],
    cursor_from_h: [f32; PLAYER_SLOTS],
    cursor_to_x: [f32; PLAYER_SLOTS],
    cursor_to_y: [f32; PLAYER_SLOTS],
    cursor_to_w: [f32; PLAYER_SLOTS],
    cursor_to_h: [f32; PLAYER_SLOTS],
    cursor_t: [f32; PLAYER_SLOTS],
    row_tweens: Vec<RowTween>,
    pane_transition: PaneTransition,
    menu_lr_chord: screen_input::MenuLrChordTracker,
}

// Format music rate like Simply Love wants:
fn fmt_music_rate(rate: f32) -> String {
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
fn fmt_tilt_intensity(value: f32) -> String {
    format!("{value:.2}")
}

fn tilt_intensity_choices() -> Vec<String> {
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

fn custom_fantastic_window_choices() -> Vec<String> {
    let lo = crate::game::profile::CUSTOM_FANTASTIC_WINDOW_MIN_MS;
    let hi = crate::game::profile::CUSTOM_FANTASTIC_WINDOW_MAX_MS;
    let mut out = Vec::with_capacity((hi - lo + 1) as usize);
    for ms in lo..=hi {
        out.push(format!("{ms}ms"));
    }
    out
}

fn resolve_p1_chart<'a>(
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

// Prefer #DISPLAYBPM for reference BPM (use max of range or single value); fallback to song.max_bpm, then 120.
fn reference_bpm_for_song(song: &SongData, chart: Option<&ChartData>) -> f32 {
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
fn difficulty_display_name(index: usize) -> String {
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

fn music_rate_display_name(state: &State) -> String {
    let p1_chart = resolve_p1_chart(&state.song, &state.chart_steps_index);
    let is_random = p1_chart.is_some_and(|c| {
        matches!(c.display_bpm, Some(crate::game::chart::ChartDisplayBpm::Random))
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
    tr_fmt("PlayerOptions", "MusicRate", &[("bpm", &bpm_str)])
        .replace("\\n", "\n")
}

#[inline(always)]
fn display_bpm_pair_for_options(
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
fn speed_mod_bpm_pair(
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
fn format_speed_bpm_pair(lo: f32, hi: f32) -> String {
    let lo_i = lo.round() as i32;
    let hi_i = hi.round() as i32;
    if lo_i == hi_i {
        lo_i.to_string()
    } else {
        format!("{lo_i}-{hi_i}")
    }
}

#[inline(always)]
fn perspective_speed_mult(perspective: crate::game::profile::Perspective) -> f32 {
    match perspective {
        crate::game::profile::Perspective::Overhead => 1.0,
        crate::game::profile::Perspective::Hallway => 0.75,
        crate::game::profile::Perspective::Distant => 33.0 / 39.0,
        crate::game::profile::Perspective::Incoming => 33.0 / 43.0,
        crate::game::profile::Perspective::Space => 0.825,
    }
}

#[inline(always)]
fn speed_mod_helper_scroll_text(
    song: &SongData,
    chart: Option<&ChartData>,
    speed_mod: &SpeedMod,
    music_rate: f32,
) -> String {
    speed_mod_bpm_pair(song, chart, speed_mod, music_rate)
        .map_or_else(String::new, |(lo, hi)| format_speed_bpm_pair(lo, hi))
}

#[inline(always)]
fn speed_mod_helper_scaled_text(
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
fn measure_wendy_text_width(asset_manager: &AssetManager, text: &str) -> f32 {
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
fn round_to_step(x: f32, step: f32) -> f32 {
    if !x.is_finite() || !step.is_finite() || step <= 0.0 {
        return x;
    }
    (x / step).round() * step
}

#[inline(always)]
fn noteskin_cols_per_player(play_style: crate::game::profile::PlayStyle) -> usize {
    match play_style {
        crate::game::profile::PlayStyle::Double => 8,
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Versus => 4,
    }
}

fn load_noteskin_cached(skin: &str, cols_per_player: usize) -> Option<Arc<Noteskin>> {
    let style = noteskin::Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
    noteskin::load_itg_skin_cached(&style, skin).ok()
}

fn discover_noteskin_names() -> Vec<String> {
    noteskin::discover_itg_skins("dance")
}

fn build_noteskin_override_choices(noteskin_names: &[String]) -> Vec<String> {
    let mut choices = Vec::with_capacity(noteskin_names.len() + 1);
    choices.push(tr("PlayerOptions", "MatchNoteSkinLabel").to_string());
    if noteskin_names.is_empty() {
        choices.push(crate::game::profile::NoteSkin::DEFAULT_NAME.to_string());
    } else {
        choices.extend(noteskin_names.iter().cloned());
    }
    choices
}

fn build_tap_explosion_noteskin_choices(noteskin_names: &[String]) -> Vec<String> {
    let mut choices = Vec::with_capacity(noteskin_names.len() + 2);
    choices.push(tr("PlayerOptions", "MatchNoteSkinLabel").to_string());
    choices.push(tr("PlayerOptions", "NoTapExplosionLabel").to_string());
    if noteskin_names.is_empty() {
        choices.push(crate::game::profile::NoteSkin::DEFAULT_NAME.to_string());
    } else {
        choices.extend(noteskin_names.iter().cloned());
    }
    choices
}

fn build_noteskin_cache(
    cols_per_player: usize,
    initial_names: &[String],
) -> HashMap<String, Arc<Noteskin>> {
    let mut cache = HashMap::with_capacity(initial_names.len());
    for name in initial_names {
        if let Some(noteskin) = load_noteskin_cached(name, cols_per_player) {
            cache.insert(name.clone(), noteskin);
        }
    }
    cache
}

fn push_noteskin_name_once(names: &mut Vec<String>, skin: &crate::game::profile::NoteSkin) {
    if skin.is_none_choice() {
        return;
    }
    let skin_name = skin.as_str().to_string();
    if !names.iter().any(|name| name == &skin_name) {
        names.push(skin_name);
    }
}

fn cached_noteskin(
    cache: &HashMap<String, Arc<Noteskin>>,
    skin: &crate::game::profile::NoteSkin,
) -> Option<Arc<Noteskin>> {
    cache.get(skin.as_str()).cloned()
}

fn fallback_noteskin(cache: &HashMap<String, Arc<Noteskin>>) -> Option<Arc<Noteskin>> {
    cache
        .get(crate::game::profile::NoteSkin::DEFAULT_NAME)
        .cloned()
        .or_else(|| cache.values().next().cloned())
}

fn cached_or_load_noteskin(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    skin: &crate::game::profile::NoteSkin,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if let Some(ns) = cached_noteskin(cache, skin) {
        return Some(ns);
    }

    if let Some(loaded) = load_noteskin_cached(skin.as_str(), cols_per_player) {
        cache.insert(skin.as_str().to_string(), loaded.clone());
        return Some(loaded);
    }

    if let Some(ns) = fallback_noteskin(cache) {
        return Some(ns);
    }

    if !skin
        .as_str()
        .eq_ignore_ascii_case(crate::game::profile::NoteSkin::DEFAULT_NAME)
        && let Some(loaded) = load_noteskin_cached(
            crate::game::profile::NoteSkin::DEFAULT_NAME,
            cols_per_player,
        )
    {
        cache.insert(
            crate::game::profile::NoteSkin::DEFAULT_NAME.to_string(),
            loaded.clone(),
        );
        return Some(loaded);
    }

    fallback_noteskin(cache)
}

fn cached_or_load_noteskin_exact(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    skin: &crate::game::profile::NoteSkin,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if let Some(ns) = cached_noteskin(cache, skin) {
        return Some(ns);
    }

    let loaded = load_noteskin_cached(skin.as_str(), cols_per_player)?;
    cache.insert(skin.as_str().to_string(), loaded.clone());
    Some(loaded)
}

fn resolved_noteskin_override_preview(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    noteskin: &crate::game::profile::NoteSkin,
    override_noteskin: Option<&crate::game::profile::NoteSkin>,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if let Some(override_noteskin) = override_noteskin
        && let Some(ns) = cached_or_load_noteskin_exact(cache, override_noteskin, cols_per_player)
    {
        return Some(ns);
    }

    cached_or_load_noteskin(cache, noteskin, cols_per_player)
}

fn resolved_tap_explosion_preview(
    cache: &mut HashMap<String, Arc<Noteskin>>,
    noteskin: &crate::game::profile::NoteSkin,
    tap_explosion_noteskin: Option<&crate::game::profile::NoteSkin>,
    cols_per_player: usize,
) -> Option<Arc<Noteskin>> {
    if tap_explosion_noteskin.is_some_and(crate::game::profile::NoteSkin::is_none_choice) {
        return None;
    }

    resolved_noteskin_override_preview(cache, noteskin, tap_explosion_noteskin, cols_per_player)
}

fn sync_noteskin_previews_for_player(state: &mut State, player_idx: usize) {
    let cols_per_player = noteskin_cols_per_player(crate::game::profile::get_session_play_style());
    let noteskin_setting = state.player_profiles[player_idx].noteskin.clone();
    let mine_noteskin_setting = state.player_profiles[player_idx].mine_noteskin.clone();
    let receptor_noteskin_setting = state.player_profiles[player_idx].receptor_noteskin.clone();
    let tap_explosion_noteskin_setting = state.player_profiles[player_idx]
        .tap_explosion_noteskin
        .clone();
    state.noteskin[player_idx] = cached_or_load_noteskin(
        &mut state.noteskin_cache,
        &noteskin_setting,
        cols_per_player,
    );
    state.mine_noteskin[player_idx] = resolved_noteskin_override_preview(
        &mut state.noteskin_cache,
        &noteskin_setting,
        mine_noteskin_setting.as_ref(),
        cols_per_player,
    );
    state.receptor_noteskin[player_idx] = resolved_noteskin_override_preview(
        &mut state.noteskin_cache,
        &noteskin_setting,
        receptor_noteskin_setting.as_ref(),
        cols_per_player,
    );
    state.tap_explosion_noteskin[player_idx] = resolved_tap_explosion_preview(
        &mut state.noteskin_cache,
        &noteskin_setting,
        tap_explosion_noteskin_setting.as_ref(),
        cols_per_player,
    );
}

#[inline(always)]
fn choose_different_screen_label(return_screen: Screen) -> String {
    match return_screen {
        Screen::SelectCourse => tr("PlayerOptions", "ChooseDifferentCourse").to_string(),
        _ => tr("PlayerOptions", "ChooseDifferentSong").to_string(),
    }
}

fn what_comes_next_choices(pane: OptionsPane, return_screen: Screen) -> Vec<String> {
    let choose_different = choose_different_screen_label(return_screen);
    match pane {
        OptionsPane::Main => vec![
            tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
            choose_different,
            tr("PlayerOptions", "WhatComesNextAdvancedModifiers").to_string(),
            tr("PlayerOptions", "WhatComesNextUncommonModifiers").to_string(),
        ],
        OptionsPane::Advanced => vec![
            tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
            choose_different,
            tr("PlayerOptions", "WhatComesNextMainModifiers").to_string(),
            tr("PlayerOptions", "WhatComesNextUncommonModifiers").to_string(),
        ],
        OptionsPane::Uncommon => vec![
            tr("PlayerOptions", "WhatComesNextGameplay").to_string(),
            choose_different,
            tr("PlayerOptions", "WhatComesNextMainModifiers").to_string(),
            tr("PlayerOptions", "WhatComesNextAdvancedModifiers").to_string(),
        ],
    }
}

fn build_main_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    session_music_rate: f32,
    noteskin_names: &[String],
    return_screen: Screen,
    fixed_stepchart: Option<&FixedStepchart>,
) -> Vec<Row> {
    let speed_mod_value_str = match speed_mod.mod_type.as_str() {
        "X" => format!("{:.2}x", speed_mod.value),
        "C" => format!("C{}", speed_mod.value as i32),
        "M" => format!("M{}", speed_mod.value as i32),
        _ => String::new(),
    };
    let (stepchart_choices, stepchart_choice_indices, initial_stepchart_choice_index) =
        if let Some(fixed) = fixed_stepchart {
            let fixed_steps_idx = chart_steps_index[session_persisted_player_idx()];
            (
                vec![fixed.label.clone()],
                vec![fixed_steps_idx],
                [0; PLAYER_SLOTS],
            )
        } else {
            // Build Stepchart choices from the song's charts for the current play style, ordered
            // Beginner..Challenge, then Edit charts.
            let target_chart_type = crate::game::profile::get_session_play_style().chart_type();
            let mut stepchart_choices: Vec<String> = Vec::with_capacity(5);
            let mut stepchart_choice_indices: Vec<usize> = Vec::with_capacity(5);
            for (i, file_name) in crate::engine::present::color::FILE_DIFFICULTY_NAMES
                .iter()
                .enumerate()
            {
                if let Some(chart) = song.charts.iter().find(|c| {
                    c.chart_type.eq_ignore_ascii_case(target_chart_type)
                        && c.difficulty.eq_ignore_ascii_case(file_name)
                }) {
                    let display_name = difficulty_display_name(i);
                    stepchart_choices.push(format!("{} {}", display_name, chart.meter));
                    stepchart_choice_indices.push(i);
                }
            }
            for (i, chart) in
                crate::screens::select_music::edit_charts_sorted(song, target_chart_type)
                    .into_iter()
                    .enumerate()
            {
                let desc = chart.description.trim();
                if desc.is_empty() {
                    stepchart_choices.push(
                        tr_fmt("PlayerOptions", "EditChartMeter", &[("meter", &chart.meter.to_string())]).to_string()
                    );
                } else {
                    stepchart_choices.push(
                        tr_fmt("PlayerOptions", "EditChartDescMeter", &[("desc", desc), ("meter", &chart.meter.to_string())]).to_string()
                    );
                }
                stepchart_choice_indices
                    .push(crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() + i);
            }
            // Fallback if none found (defensive; SelectMusic filters songs by play style).
            if stepchart_choices.is_empty() {
                stepchart_choices.push(tr("PlayerOptions", "CurrentStepchartLabel").to_string());
                let base_pref = preferred_difficulty_index[session_persisted_player_idx()].min(
                    crate::engine::present::color::FILE_DIFFICULTY_NAMES
                        .len()
                        .saturating_sub(1),
                );
                stepchart_choice_indices.push(base_pref);
            }
            let initial_stepchart_choice_index: [usize; PLAYER_SLOTS] =
                std::array::from_fn(|player_idx| {
                    let steps_idx = chart_steps_index[player_idx];
                    let pref_idx = preferred_difficulty_index[player_idx].min(
                        crate::engine::present::color::FILE_DIFFICULTY_NAMES
                            .len()
                            .saturating_sub(1),
                    );
                    stepchart_choice_indices
                        .iter()
                        .position(|&idx| idx == steps_idx)
                        .or_else(|| {
                            stepchart_choice_indices
                                .iter()
                                .position(|&idx| idx == pref_idx)
                        })
                        .unwrap_or(0)
                });
            (
                stepchart_choices,
                stepchart_choice_indices,
                initial_stepchart_choice_index,
            )
        };
    vec![
        Row {
            id: RowId::TypeOfSpeedMod,
            name: lookup_key("PlayerOptions", "TypeOfSpeedMod"),
            choices: vec![
                tr("PlayerOptions", "SpeedModTypeX").to_string(),
                tr("PlayerOptions", "SpeedModTypeC").to_string(),
                tr("PlayerOptions", "SpeedModTypeM").to_string(),
            ],
            selected_choice_index: [match speed_mod.mod_type.as_str() {
                "X" => 0,
                "C" => 1,
                "M" => 2,
                _ => 1, // Default to C
            }; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "TypeOfSpeedModHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::SpeedMod,
            name: lookup_key("PlayerOptions", "SpeedMod"),
            choices: vec![speed_mod_value_str], // Display only the current value
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "SpeedModHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Mini,
            name: lookup_key("PlayerOptions", "Mini"),
            choices: (-100..=150).map(|v| format!("{v}%")).collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "MiniHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Perspective,
            name: lookup_key("PlayerOptions", "Perspective"),
            choices: vec![
                tr("PlayerOptions", "PerspectiveOverhead").to_string(),
                tr("PlayerOptions", "PerspectiveHallway").to_string(),
                tr("PlayerOptions", "PerspectiveDistant").to_string(),
                tr("PlayerOptions", "PerspectiveIncoming").to_string(),
                tr("PlayerOptions", "PerspectiveSpace").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "PerspectiveHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::NoteSkin,
            name: lookup_key("PlayerOptions", "NoteSkin"),
            choices: if noteskin_names.is_empty() {
                vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()]
            } else {
                noteskin_names.to_vec()
            },
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "NoteSkinHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MineSkin,
            name: lookup_key("PlayerOptions", "MineSkin"),
            choices: build_noteskin_override_choices(noteskin_names),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "MineSkinHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ReceptorSkin,
            name: lookup_key("PlayerOptions", "ReceptorSkin"),
            choices: build_noteskin_override_choices(noteskin_names),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "ReceptorSkinHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::TapExplosionSkin,
            name: lookup_key("PlayerOptions", "TapExplosionSkin"),
            choices: build_tap_explosion_noteskin_choices(noteskin_names),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "TapExplosionSkinHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::JudgmentFont,
            name: lookup_key("PlayerOptions", "JudgmentFont"),
            choices: assets::judgment_texture_choices()
                .iter()
                .map(|choice| choice.label.clone())
                .collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "JudgmentFontHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::JudgmentOffsetX,
            name: lookup_key("PlayerOptions", "JudgmentOffsetX"),
            choices: hud_offset_choices(),
            selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "JudgmentOffsetXHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::JudgmentOffsetY,
            name: lookup_key("PlayerOptions", "JudgmentOffsetY"),
            choices: hud_offset_choices(),
            selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "JudgmentOffsetYHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ComboFont,
            name: lookup_key("PlayerOptions", "ComboFont"),
            choices: vec![
                tr("PlayerOptions", "ComboFontWendy").to_string(),
                tr("PlayerOptions", "ComboFontArialRounded").to_string(),
                tr("PlayerOptions", "ComboFontAsap").to_string(),
                tr("PlayerOptions", "ComboFontBebasNeue").to_string(),
                tr("PlayerOptions", "ComboFontSourceCode").to_string(),
                tr("PlayerOptions", "ComboFontWork").to_string(),
                tr("PlayerOptions", "ComboFontWendyCursed").to_string(),
                tr("PlayerOptions", "ComboFontNone").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "ComboFontHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ComboOffsetX,
            name: lookup_key("PlayerOptions", "ComboOffsetX"),
            choices: hud_offset_choices(),
            selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "ComboOffsetXHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ComboOffsetY,
            name: lookup_key("PlayerOptions", "ComboOffsetY"),
            choices: hud_offset_choices(),
            selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "ComboOffsetYHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::HoldJudgment,
            name: lookup_key("PlayerOptions", "HoldJudgment"),
            choices: assets::hold_judgment_texture_choices()
                .iter()
                .map(|choice| choice.label.clone())
                .collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "HoldJudgmentHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::BackgroundFilter,
            name: lookup_key("PlayerOptions", "BackgroundFilter"),
            choices: vec![
                tr("PlayerOptions", "BackgroundFilterOff").to_string(),
                tr("PlayerOptions", "BackgroundFilterDark").to_string(),
                tr("PlayerOptions", "BackgroundFilterDarker").to_string(),
                tr("PlayerOptions", "BackgroundFilterDarkest").to_string(),
            ],
            selected_choice_index: [3; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "BackgroundFilterHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::NoteFieldOffsetX,
            name: lookup_key("PlayerOptions", "NoteFieldOffsetX"),
            choices: (0..=50).map(|v| v.to_string()).collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "NoteFieldOffsetXHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::NoteFieldOffsetY,
            name: lookup_key("PlayerOptions", "NoteFieldOffsetY"),
            choices: (-50..=50).map(|v| v.to_string()).collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "NoteFieldOffsetYHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::VisualDelay,
            name: lookup_key("PlayerOptions", "VisualDelay"),
            choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
            selected_choice_index: [100; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "VisualDelayHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::GlobalOffsetShift,
            name: lookup_key("PlayerOptions", "GlobalOffsetShift"),
            choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
            selected_choice_index: [100; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "GlobalOffsetShiftHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MusicRate,
            name: lookup_key("PlayerOptions", "MusicRate"),
            choices: vec![fmt_music_rate(session_music_rate.clamp(0.5, 3.0))],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "MusicRateHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Stepchart,
            name: lookup_key("PlayerOptions", "Stepchart"),
            choices: stepchart_choices,
            selected_choice_index: initial_stepchart_choice_index,
            help: tr("PlayerOptionsHelp", "StepchartHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: Some(stepchart_choice_indices),
        },
        Row {
            id: RowId::WhatComesNext,
            name: lookup_key("PlayerOptions", "WhatComesNext"),
            choices: what_comes_next_choices(OptionsPane::Main, return_screen),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: tr("PlayerOptionsHelp", "WhatComesNextHelp").split("\\n").map(|s| s.to_string()).collect(),
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Exit,
            name: lookup_key("Common", "Exit"),
            choices: vec![tr("Common", "Exit").to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![String::new()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_advanced_rows(return_screen: Screen) -> Vec<Row> {
    let mut gameplay_extras_choices = vec![
        tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss").to_string(),
        tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop").to_string(),
        tr("PlayerOptions", "GameplayExtrasColumnCues").to_string(),
    ];
    if crate::game::scores::is_gs_get_scores_service_allowed() {
        gameplay_extras_choices.push(tr("PlayerOptions", "GameplayExtrasDisplayScorebox").to_string());
    }

    vec![
        Row {
            id: RowId::Turn,
            name: lookup_key("PlayerOptions", "Turn"),
            choices: vec![
                tr("PlayerOptions", "TurnNone").to_string(),
                tr("PlayerOptions", "TurnMirror").to_string(),
                tr("PlayerOptions", "TurnLeft").to_string(),
                tr("PlayerOptions", "TurnRight").to_string(),
                tr("PlayerOptions", "TurnLRMirror").to_string(),
                tr("PlayerOptions", "TurnUDMirror").to_string(),
                tr("PlayerOptions", "TurnShuffle").to_string(),
                tr("PlayerOptions", "TurnBlender").to_string(),
                tr("PlayerOptions", "TurnRandom").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "TurnHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Scroll,
            name: lookup_key("PlayerOptions", "Scroll"),
            choices: vec![
                tr("PlayerOptions", "ScrollReverse").to_string(),
                tr("PlayerOptions", "ScrollSplit").to_string(),
                tr("PlayerOptions", "ScrollAlternate").to_string(),
                tr("PlayerOptions", "ScrollCross").to_string(),
                tr("PlayerOptions", "ScrollCentered").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ScrollHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Hide,
            name: lookup_key("PlayerOptions", "Hide"),
            choices: vec![
                tr("PlayerOptions", "HideTargets").to_string(),
                tr("PlayerOptions", "HideBackground").to_string(),
                tr("PlayerOptions", "HideCombo").to_string(),
                tr("PlayerOptions", "HideLife").to_string(),
                tr("PlayerOptions", "HideScore").to_string(),
                tr("PlayerOptions", "HideDanger").to_string(),
                tr("PlayerOptions", "HideComboExplosions").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "HideHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::LifeMeterType,
            name: lookup_key("PlayerOptions", "LifeMeterType"),
            choices: vec![
                tr("PlayerOptions", "LifeMeterTypeStandard").to_string(),
                tr("PlayerOptions", "LifeMeterTypeSurround").to_string(),
                tr("PlayerOptions", "LifeMeterTypeVertical").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "LifeMeterTypeHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::LifeBarOptions,
            name: lookup_key("PlayerOptions", "LifeBarOptions"),
            choices: vec![
                tr("PlayerOptions", "LifeBarOptionsRainbowMax").to_string(),
                tr("PlayerOptions", "LifeBarOptionsResponsiveColors").to_string(),
                tr("PlayerOptions", "LifeBarOptionsShowLifePercentage").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "LifeBarOptionsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::DataVisualizations,
            name: lookup_key("PlayerOptions", "DataVisualizations"),
            choices: vec![
                tr("PlayerOptions", "DataVisualizationsNone").to_string(),
                tr("PlayerOptions", "DataVisualizationsTargetScoreGraph").to_string(),
                tr("PlayerOptions", "DataVisualizationsStepStatistics").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "DataVisualizationsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::DensityGraphBackground,
            name: lookup_key("PlayerOptions", "DensityGraphBackground"),
            choices: vec![
                tr("PlayerOptions", "DensityGraphBackgroundSolid").to_string(),
                tr("PlayerOptions", "DensityGraphBackgroundTransparent").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "DensityGraphBackgroundHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::TargetScore,
            name: lookup_key("PlayerOptions", "TargetScore"),
            choices: vec![
                tr("PlayerOptions", "TargetScoreCMinus").to_string(),
                tr("PlayerOptions", "TargetScoreC").to_string(),
                tr("PlayerOptions", "TargetScoreCPlus").to_string(),
                tr("PlayerOptions", "TargetScoreBMinus").to_string(),
                tr("PlayerOptions", "TargetScoreB").to_string(),
                tr("PlayerOptions", "TargetScoreBPlus").to_string(),
                tr("PlayerOptions", "TargetScoreAMinus").to_string(),
                tr("PlayerOptions", "TargetScoreA").to_string(),
                tr("PlayerOptions", "TargetScoreAPlus").to_string(),
                tr("PlayerOptions", "TargetScoreSMinus").to_string(),
                tr("PlayerOptions", "TargetScoreS").to_string(),
                tr("PlayerOptions", "TargetScoreSPlus").to_string(),
                tr("PlayerOptions", "TargetScoreMachineBest").to_string(),
                tr("PlayerOptions", "TargetScorePersonalBest").to_string(),
            ],
            selected_choice_index: [10; PLAYER_SLOTS], // S by default
            help: vec![tr("PlayerOptionsHelp", "TargetScoreHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ActionOnMissedTarget,
            name: lookup_key("PlayerOptions", "TargetScoreMissPolicy"),
            choices: vec![
                tr("PlayerOptions", "TargetScoreMissPolicyNothing").to_string(),
                tr("PlayerOptions", "TargetScoreMissPolicyFail").to_string(),
                tr("PlayerOptions", "TargetScoreMissPolicyRestartSong").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "TargetScoreMissPolicyHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MiniIndicator,
            name: lookup_key("PlayerOptions", "MiniIndicator"),
            choices: vec![
                tr("PlayerOptions", "MiniIndicatorNone").to_string(),
                tr("PlayerOptions", "MiniIndicatorSubtractiveScoring").to_string(),
                tr("PlayerOptions", "MiniIndicatorPredictiveScoring").to_string(),
                tr("PlayerOptions", "MiniIndicatorPaceScoring").to_string(),
                tr("PlayerOptions", "MiniIndicatorRivalScoring").to_string(),
                tr("PlayerOptions", "MiniIndicatorPacemaker").to_string(),
                tr("PlayerOptions", "MiniIndicatorStreamProg").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "MiniIndicatorHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::IndicatorScoreType,
            name: lookup_key("PlayerOptions", "IndicatorScoreType"),
            choices: vec![
                tr("PlayerOptions", "IndicatorScoreTypeITG").to_string(),
                tr("PlayerOptions", "IndicatorScoreTypeEX").to_string(),
                tr("PlayerOptions", "IndicatorScoreTypeHEX").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "IndicatorScoreTypeHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::GameplayExtras,
            name: lookup_key("PlayerOptions", "GameplayExtras"),
            choices: gameplay_extras_choices,
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "GameplayExtrasHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ComboColors,
            name: lookup_key("PlayerOptions", "ComboColors"),
            choices: vec![
                tr("PlayerOptions", "ComboColorsGlow").to_string(),
                tr("PlayerOptions", "ComboColorsSolid").to_string(),
                tr("PlayerOptions", "ComboColorsRainbow").to_string(),
                tr("PlayerOptions", "ComboColorsRainbowScroll").to_string(),
                tr("PlayerOptions", "ComboColorsNone").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ComboColorsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ComboColorMode,
            name: lookup_key("PlayerOptions", "ComboColorMode"),
            choices: vec![
                tr("PlayerOptions", "ComboColorModeFullCombo").to_string(),
                tr("PlayerOptions", "ComboColorModeCurrentCombo").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ComboColorModeHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::CarryCombo,
            name: lookup_key("PlayerOptions", "CarryCombo"),
            choices: vec![
                tr("PlayerOptions", "CarryComboNo").to_string(),
                tr("PlayerOptions", "CarryComboYes").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "CarryComboHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::JudgmentTilt,
            name: lookup_key("PlayerOptions", "JudgmentTilt"),
            choices: vec![
                tr("PlayerOptions", "JudgmentTiltNo").to_string(),
                tr("PlayerOptions", "JudgmentTiltYes").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "JudgmentTiltHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::JudgmentTiltIntensity,
            name: lookup_key("PlayerOptions", "JudgmentTiltIntensity"),
            choices: tilt_intensity_choices(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "JudgmentTiltIntensityHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::JudgmentBehindArrows,
            name: lookup_key("PlayerOptions", "JudgmentBehindArrows"),
            choices: vec![
                tr("PlayerOptions", "JudgmentBehindArrowsOff").to_string(),
                tr("PlayerOptions", "JudgmentBehindArrowsOn").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "JudgmentBehindArrowsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::OffsetIndicator,
            name: lookup_key("PlayerOptions", "OffsetIndicator"),
            choices: vec![
                tr("PlayerOptions", "OffsetIndicatorOff").to_string(),
                tr("PlayerOptions", "OffsetIndicatorOn").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "OffsetIndicatorHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ErrorBar,
            name: lookup_key("PlayerOptions", "ErrorBar"),
            choices: vec![
                tr("PlayerOptions", "ErrorBarColorful").to_string(),
                tr("PlayerOptions", "ErrorBarMonochrome").to_string(),
                tr("PlayerOptions", "ErrorBarText").to_string(),
                tr("PlayerOptions", "ErrorBarHighlight").to_string(),
                tr("PlayerOptions", "ErrorBarAverage").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ErrorBarHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ErrorBarTrim,
            name: lookup_key("PlayerOptions", "ErrorBarTrim"),
            choices: vec![
                tr("PlayerOptions", "ErrorBarTrimOff").to_string(),
                tr("PlayerOptions", "ErrorBarTrimFantastic").to_string(),
                tr("PlayerOptions", "ErrorBarTrimExcellent").to_string(),
                tr("PlayerOptions", "ErrorBarTrimGreat").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ErrorBarTrimHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ErrorBarOptions,
            name: lookup_key("PlayerOptions", "ErrorBarOptions"),
            choices: vec![
                tr("PlayerOptions", "ErrorBarOptionsMoveUp").to_string(),
                tr("PlayerOptions", "ErrorBarOptionsMultiTick").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ErrorBarOptionsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ErrorBarOffsetX,
            name: lookup_key("PlayerOptions", "ErrorBarOffsetX"),
            choices: hud_offset_choices(),
            selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ErrorBarOffsetXHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ErrorBarOffsetY,
            name: lookup_key("PlayerOptions", "ErrorBarOffsetY"),
            choices: hud_offset_choices(),
            selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ErrorBarOffsetYHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MeasureCounter,
            name: lookup_key("PlayerOptions", "MeasureCounter"),
            choices: vec![
                tr("PlayerOptions", "MeasureCounterNone").to_string(),
                tr("PlayerOptions", "MeasureCounter8th").to_string(),
                tr("PlayerOptions", "MeasureCounter12th").to_string(),
                tr("PlayerOptions", "MeasureCounter16th").to_string(),
                tr("PlayerOptions", "MeasureCounter24th").to_string(),
                tr("PlayerOptions", "MeasureCounter32nd").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "MeasureCounterHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MeasureCounterLookahead,
            name: lookup_key("PlayerOptions", "MeasureCounterLookahead"),
            choices: vec![
                tr("PlayerOptions", "MeasureCounterLookahead0").to_string(),
                tr("PlayerOptions", "MeasureCounterLookahead1").to_string(),
                tr("PlayerOptions", "MeasureCounterLookahead2").to_string(),
                tr("PlayerOptions", "MeasureCounterLookahead3").to_string(),
                tr("PlayerOptions", "MeasureCounterLookahead4").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "MeasureCounterLookaheadHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MeasureCounterOptions,
            name: lookup_key("PlayerOptions", "MeasureCounterOptions"),
            choices: vec![
                tr("PlayerOptions", "MeasureCounterOptionsMoveLeft").to_string(),
                tr("PlayerOptions", "MeasureCounterOptionsMoveUp").to_string(),
                tr("PlayerOptions", "MeasureCounterOptionsVerticalLookahead").to_string(),
                tr("PlayerOptions", "MeasureCounterOptionsBrokenRunTotal").to_string(),
                tr("PlayerOptions", "MeasureCounterOptionsRunTimer").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "MeasureCounterOptionsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::MeasureLines,
            name: lookup_key("PlayerOptions", "MeasureLines"),
            choices: vec![
                tr("PlayerOptions", "MeasureLinesOff").to_string(),
                tr("PlayerOptions", "MeasureLinesMeasure").to_string(),
                tr("PlayerOptions", "MeasureLinesQuarter").to_string(),
                tr("PlayerOptions", "MeasureLinesEighth").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "MeasureLinesHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::RescoreEarlyHits,
            name: lookup_key("PlayerOptions", "RescoreEarlyHits"),
            choices: vec![
                tr("PlayerOptions", "RescoreEarlyHitsNo").to_string(),
                tr("PlayerOptions", "RescoreEarlyHitsYes").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "RescoreEarlyHitsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::EarlyDecentWayOffOptions,
            name: lookup_key("PlayerOptions", "EarlyDecentWayOffOptions"),
            choices: vec![
                tr("PlayerOptions", "EarlyDecentWayOffOptionsHideJudgments").to_string(),
                tr("PlayerOptions", "EarlyDecentWayOffOptionsHideNoteFieldFlash").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "EarlyDecentWayOffOptionsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::ResultsExtras,
            name: lookup_key("PlayerOptions", "ResultsExtras"),
            choices: vec![tr("PlayerOptions", "ResultsExtrasTrackEarlyJudgments").to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "ResultsExtrasHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::TimingWindows,
            name: lookup_key("PlayerOptions", "TimingWindows"),
            choices: vec![
                tr("PlayerOptions", "TimingWindowsNone").to_string(),
                tr("PlayerOptions", "TimingWindowsWayOffs").to_string(),
                tr("PlayerOptions", "TimingWindowsDecentsAndWayOffs").to_string(),
                tr("PlayerOptions", "TimingWindowsFantasticsAndExcellents").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "TimingWindowsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::FAPlusOptions,
            name: lookup_key("PlayerOptions", "FAPlusOptions"),
            choices: vec![
                tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusWindow").to_string(),
                tr("PlayerOptions", "FAPlusOptionsDisplayEXScore").to_string(),
                tr("PlayerOptions", "FAPlusOptionsDisplayHEXScore").to_string(),
                tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusPane").to_string(),
                tr("PlayerOptions", "FAPlusOptions10msBlueWindow").to_string(),
                tr("PlayerOptions", "FAPlusOptions1510msSplit").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "FAPlusOptionsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::CustomBlueFantasticWindow,
            name: lookup_key("PlayerOptions", "CustomBlueFantasticWindow"),
            choices: vec![
                tr("PlayerOptions", "CustomBlueFantasticWindowNo").to_string(),
                tr("PlayerOptions", "CustomBlueFantasticWindowYes").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "CustomBlueFantasticWindowHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::CustomBlueFantasticWindowMs,
            name: lookup_key("PlayerOptions", "CustomBlueFantasticWindowMs"),
            choices: custom_fantastic_window_choices(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "CustomBlueFantasticWindowMsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::WhatComesNext,
            name: lookup_key("PlayerOptions", "WhatComesNext"),
            choices: what_comes_next_choices(OptionsPane::Advanced, return_screen),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "WhatComesNextAdvancedHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Exit,
            name: lookup_key("Common", "Exit"),
            choices: vec![tr("Common", "Exit").to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![String::new()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_uncommon_rows(return_screen: Screen) -> Vec<Row> {
    let rows = vec![
        Row {
            id: RowId::Insert,
            name: lookup_key("PlayerOptions", "Insert"),
            choices: vec![
                tr("PlayerOptions", "InsertWide").to_string(),
                tr("PlayerOptions", "InsertBig").to_string(),
                tr("PlayerOptions", "InsertQuick").to_string(),
                tr("PlayerOptions", "InsertBMRize").to_string(),
                tr("PlayerOptions", "InsertSkippy").to_string(),
                tr("PlayerOptions", "InsertEcho").to_string(),
                tr("PlayerOptions", "InsertStomp").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "InsertHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Remove,
            name: lookup_key("PlayerOptions", "Remove"),
            choices: vec![
                tr("PlayerOptions", "RemoveLittle").to_string(),
                tr("PlayerOptions", "RemoveNoMines").to_string(),
                tr("PlayerOptions", "RemoveNoHolds").to_string(),
                tr("PlayerOptions", "RemoveNoJumps").to_string(),
                tr("PlayerOptions", "RemoveNoHands").to_string(),
                tr("PlayerOptions", "RemoveNoQuads").to_string(),
                tr("PlayerOptions", "RemoveNoLifts").to_string(),
                tr("PlayerOptions", "RemoveNoFakes").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "RemoveHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Holds,
            name: lookup_key("PlayerOptions", "Holds"),
            choices: vec![
                tr("PlayerOptions", "HoldsPlanted").to_string(),
                tr("PlayerOptions", "HoldsFloored").to_string(),
                tr("PlayerOptions", "HoldsTwister").to_string(),
                tr("PlayerOptions", "HoldsNoRolls").to_string(),
                tr("PlayerOptions", "HoldsToRolls").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "HoldsHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Accel,
            name: lookup_key("PlayerOptions", "Accel"),
            choices: vec![
                tr("PlayerOptions", "AccelBoost").to_string(),
                tr("PlayerOptions", "AccelBrake").to_string(),
                tr("PlayerOptions", "AccelWave").to_string(),
                tr("PlayerOptions", "AccelExpand").to_string(),
                tr("PlayerOptions", "AccelBoomerang").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "AccelHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Effect,
            name: lookup_key("PlayerOptions", "Effect"),
            choices: vec![
                tr("PlayerOptions", "EffectDrunk").to_string(),
                tr("PlayerOptions", "EffectDizzy").to_string(),
                tr("PlayerOptions", "EffectConfusion").to_string(),
                tr("PlayerOptions", "EffectBig").to_string(),
                tr("PlayerOptions", "EffectFlip").to_string(),
                tr("PlayerOptions", "EffectInvert").to_string(),
                tr("PlayerOptions", "EffectTornado").to_string(),
                tr("PlayerOptions", "EffectTipsy").to_string(),
                tr("PlayerOptions", "EffectBumpy").to_string(),
                tr("PlayerOptions", "EffectBeat").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "EffectHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Appearance,
            name: lookup_key("PlayerOptions", "Appearance"),
            choices: vec![
                tr("PlayerOptions", "AppearanceHidden").to_string(),
                tr("PlayerOptions", "AppearanceSudden").to_string(),
                tr("PlayerOptions", "AppearanceStealth").to_string(),
                tr("PlayerOptions", "AppearanceBlink").to_string(),
                tr("PlayerOptions", "AppearanceRVanish").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "AppearanceHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Attacks,
            name: lookup_key("PlayerOptions", "Attacks"),
            choices: vec![
                tr("PlayerOptions", "AttacksOn").to_string(),
                tr("PlayerOptions", "AttacksRandomAttacks").to_string(),
                tr("PlayerOptions", "AttacksOff").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "AttacksHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::HideLightType,
            name: lookup_key("PlayerOptions", "HideLightType"),
            choices: vec![
                tr("PlayerOptions", "HideLightTypeNoHideLights").to_string(),
                tr("PlayerOptions", "HideLightTypeHideAllLights").to_string(),
                tr("PlayerOptions", "HideLightTypeHideMarqueeLights").to_string(),
                tr("PlayerOptions", "HideLightTypeHideBassLights").to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![tr("PlayerOptionsHelp", "HideLightTypeHelp").to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::WhatComesNext,
            name: lookup_key("PlayerOptions", "WhatComesNext"),
            choices: what_comes_next_choices(OptionsPane::Uncommon, return_screen),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                tr("PlayerOptionsHelp", "WhatComesNextHelp1").to_string(),
                tr("PlayerOptionsHelp", "WhatComesNextHelp2").to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            id: RowId::Exit,
            name: lookup_key("Common", "Exit"),
            choices: vec![tr("Common", "Exit").to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![String::new()],
            choice_difficulty_indices: None,
        },
    ];
    rows
}

fn build_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    session_music_rate: f32,
    pane: OptionsPane,
    noteskin_names: &[String],
    return_screen: Screen,
    fixed_stepchart: Option<&FixedStepchart>,
) -> Vec<Row> {
    match pane {
        OptionsPane::Main => build_main_rows(
            song,
            speed_mod,
            chart_steps_index,
            preferred_difficulty_index,
            session_music_rate,
            noteskin_names,
            return_screen,
            fixed_stepchart,
        ),
        OptionsPane::Advanced => build_advanced_rows(return_screen),
        OptionsPane::Uncommon => build_uncommon_rows(return_screen),
    }
}

fn apply_profile_defaults(
    rows: &mut [Row],
    profile: &crate::game::profile::Profile,
    player_idx: usize,
) -> (
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u16,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
    u8,
) {
    let mut scroll_active_mask: u8 = 0;
    let mut hide_active_mask: u8 = 0;
    let mut insert_active_mask: u8 = 0;
    let mut remove_active_mask: u8 = 0;
    let mut holds_active_mask: u8 = 0;
    let mut accel_effects_active_mask: u8 = 0;
    let mut visual_effects_active_mask: u16 = 0;
    let mut appearance_effects_active_mask: u8 = 0;
    let mut fa_plus_active_mask: u8 = 0;
    let mut early_dw_active_mask: u8 = 0;
    let mut gameplay_extras_active_mask: u8 = 0;
    let mut gameplay_extras_more_active_mask: u8 = 0;
    let mut results_extras_active_mask: u8 = 0;
    let mut life_bar_options_active_mask: u8 = 0;
    let mut error_bar_active_mask: u8 =
        crate::game::profile::normalize_error_bar_mask(profile.error_bar_active_mask);
    if error_bar_active_mask == 0 {
        error_bar_active_mask = crate::game::profile::error_bar_mask_from_style(
            profile.error_bar,
            profile.error_bar_text,
        );
    }
    let mut error_bar_options_active_mask: u8 = 0;
    let mut measure_counter_options_active_mask: u8 = 0;
    let match_ns_label = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
    let no_tap_label = tr("PlayerOptions", NO_TAP_EXPLOSION_LABEL);
    // Initialize Background Filter row from profile setting (Off, Dark, Darker, Darkest)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::BackgroundFilter) {
        row.selected_choice_index[player_idx] = match profile.background_filter {
            crate::game::profile::BackgroundFilter::Off => 0,
            crate::game::profile::BackgroundFilter::Dark => 1,
            crate::game::profile::BackgroundFilter::Darker => 2,
            crate::game::profile::BackgroundFilter::Darkest => 3,
        };
    }
    // Initialize Judgment Font row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::JudgmentFont) {
        row.selected_choice_index[player_idx] = assets::judgment_texture_choices()
            .iter()
            .position(|choice| {
                choice
                    .key
                    .eq_ignore_ascii_case(profile.judgment_graphic.as_str())
            })
            .unwrap_or(0);
    }
    // Initialize NoteSkin row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::NoteSkin) {
        row.selected_choice_index[player_idx] = row
            .choices
            .iter()
            .position(|c| c.eq_ignore_ascii_case(profile.noteskin.as_str()))
            .or_else(|| {
                row.choices.iter().position(|c| {
                    c.eq_ignore_ascii_case(crate::game::profile::NoteSkin::DEFAULT_NAME)
                })
            })
            .unwrap_or(0);
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::MineSkin) {
        row.selected_choice_index[player_idx] = profile.mine_noteskin.as_ref().map_or_else(
            || {
                row.choices
                    .iter()
                    .position(|c| c.as_str() == match_ns_label.as_ref())
                    .unwrap_or(0)
            },
            |mine_noteskin| {
                row.choices
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(mine_noteskin.as_str()))
                    .or_else(|| row.choices.iter().position(|c| c.as_str() == match_ns_label.as_ref()))
                    .unwrap_or(0)
            },
        );
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ReceptorSkin) {
        row.selected_choice_index[player_idx] = profile.receptor_noteskin.as_ref().map_or_else(
            || {
                row.choices
                    .iter()
                    .position(|c| c.as_str() == match_ns_label.as_ref())
                    .unwrap_or(0)
            },
            |receptor_noteskin| {
                row.choices
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(receptor_noteskin.as_str()))
                    .or_else(|| row.choices.iter().position(|c| c.as_str() == match_ns_label.as_ref()))
                    .unwrap_or(0)
            },
        );
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::TapExplosionSkin) {
        row.selected_choice_index[player_idx] =
            profile.tap_explosion_noteskin.as_ref().map_or_else(
                || {
                    row.choices
                        .iter()
                        .position(|c| c.as_str() == match_ns_label.as_ref())
                        .unwrap_or(0)
                },
                |tap_explosion_noteskin| {
                    if tap_explosion_noteskin.is_none_choice() {
                        row.choices
                            .iter()
                            .position(|c| c.as_str() == no_tap_label.as_ref())
                            .unwrap_or(0)
                    } else {
                        row.choices
                            .iter()
                            .position(|c| c.eq_ignore_ascii_case(tap_explosion_noteskin.as_str()))
                            .or_else(|| row.choices.iter().position(|c| c.as_str() == match_ns_label.as_ref()))
                            .unwrap_or(0)
                    }
                },
            );
    }
    // Initialize Combo Font row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ComboFont) {
        row.selected_choice_index[player_idx] = match profile.combo_font {
            crate::game::profile::ComboFont::Wendy => 0,
            crate::game::profile::ComboFont::ArialRounded => 1,
            crate::game::profile::ComboFont::Asap => 2,
            crate::game::profile::ComboFont::BebasNeue => 3,
            crate::game::profile::ComboFont::SourceCode => 4,
            crate::game::profile::ComboFont::Work => 5,
            crate::game::profile::ComboFont::WendyCursed => 6,
            crate::game::profile::ComboFont::None => 7,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ComboColors) {
        row.selected_choice_index[player_idx] = match profile.combo_colors {
            crate::game::profile::ComboColors::Glow => 0,
            crate::game::profile::ComboColors::Solid => 1,
            crate::game::profile::ComboColors::Rainbow => 2,
            crate::game::profile::ComboColors::RainbowScroll => 3,
            crate::game::profile::ComboColors::None => 4,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ComboColorMode) {
        row.selected_choice_index[player_idx] = match profile.combo_mode {
            crate::game::profile::ComboMode::FullCombo => 0,
            crate::game::profile::ComboMode::CurrentCombo => 1,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::CarryCombo) {
        row.selected_choice_index[player_idx] = if profile.carry_combo_between_songs {
            1
        } else {
            0
        };
    }
    // Initialize Hold Judgment row from profile setting (Love, mute, ITG2, None)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::HoldJudgment) {
        row.selected_choice_index[player_idx] = assets::hold_judgment_texture_choices()
            .iter()
            .position(|choice| {
                choice
                    .key
                    .eq_ignore_ascii_case(profile.hold_judgment_graphic.as_str())
            })
            .unwrap_or(0);
    }
    // Initialize Mini row from profile (range -100..150, stored as percent).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Mini) {
        let val = profile.mini_percent.clamp(-100, 150);
        let needle = format!("{val}%");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Perspective row from profile setting (Overhead, Hallway, Distant, Incoming, Space).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Perspective) {
        row.selected_choice_index[player_idx] = match profile.perspective {
            crate::game::profile::Perspective::Overhead => 0,
            crate::game::profile::Perspective::Hallway => 1,
            crate::game::profile::Perspective::Distant => 2,
            crate::game::profile::Perspective::Incoming => 3,
            crate::game::profile::Perspective::Space => 4,
        };
    }
    // Initialize NoteField Offset X from profile (0..50, non-negative; P1 uses negative sign at render time)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::NoteFieldOffsetX) {
        let val = profile.note_field_offset_x.clamp(0, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize NoteField Offset Y from profile (-50..50)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::NoteFieldOffsetY) {
        let val = profile.note_field_offset_y.clamp(-50, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Offset X from profile (HUD offset range)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::JudgmentOffsetX) {
        let val = profile
            .judgment_offset_x
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Offset Y from profile (HUD offset range)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::JudgmentOffsetY) {
        let val = profile
            .judgment_offset_y
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Combo Offset X from profile (HUD offset range)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ComboOffsetX) {
        let val = profile.combo_offset_x.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Combo Offset Y from profile (HUD offset range)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ComboOffsetY) {
        let val = profile.combo_offset_y.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Error Bar Offset X from profile (HUD offset range)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ErrorBarOffsetX) {
        let val = profile
            .error_bar_offset_x
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Error Bar Offset Y from profile (HUD offset range)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ErrorBarOffsetY) {
        let val = profile
            .error_bar_offset_y
            .clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Visual Delay from profile (-100..100ms)
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::VisualDelay) {
        let val = profile.visual_delay_ms.clamp(-100, 100);
        let needle = format!("{val}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::GlobalOffsetShift) {
        let val = profile.global_offset_shift_ms.clamp(-100, 100);
        let needle = format!("{val}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Tilt rows from profile (Simply Love semantics).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::JudgmentTilt) {
        row.selected_choice_index[player_idx] = if profile.judgment_tilt { 1 } else { 0 };
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::JudgmentTiltIntensity)
    {
        let stepped = round_to_step(
            profile
                .tilt_multiplier
                .clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX),
            TILT_INTENSITY_STEP,
        )
        .clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX);
        let needle = fmt_tilt_intensity(stepped);
        row.selected_choice_index[player_idx] = row
            .choices
            .iter()
            .position(|c| c == &needle)
            .unwrap_or(0)
            .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::JudgmentBehindArrows) {
        row.selected_choice_index[player_idx] = if profile.judgment_back { 1 } else { 0 };
    }
    // Initialize Error Bar rows from profile (Simply Love semantics).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::OffsetIndicator) {
        row.selected_choice_index[player_idx] = if profile.error_ms_display { 1 } else { 0 };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ErrorBar) {
        if error_bar_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (error_bar_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::DataVisualizations) {
        row.selected_choice_index[player_idx] = match profile.data_visualizations {
            crate::game::profile::DataVisualizations::None => 0,
            crate::game::profile::DataVisualizations::TargetScoreGraph => 1,
            crate::game::profile::DataVisualizations::StepStatistics => 2,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::TargetScore) {
        row.selected_choice_index[player_idx] = match profile.target_score {
            crate::game::profile::TargetScoreSetting::CMinus => 0,
            crate::game::profile::TargetScoreSetting::C => 1,
            crate::game::profile::TargetScoreSetting::CPlus => 2,
            crate::game::profile::TargetScoreSetting::BMinus => 3,
            crate::game::profile::TargetScoreSetting::B => 4,
            crate::game::profile::TargetScoreSetting::BPlus => 5,
            crate::game::profile::TargetScoreSetting::AMinus => 6,
            crate::game::profile::TargetScoreSetting::A => 7,
            crate::game::profile::TargetScoreSetting::APlus => 8,
            crate::game::profile::TargetScoreSetting::SMinus => 9,
            crate::game::profile::TargetScoreSetting::S => 10,
            crate::game::profile::TargetScoreSetting::SPlus => 11,
            crate::game::profile::TargetScoreSetting::MachineBest => 12,
            crate::game::profile::TargetScoreSetting::PersonalBest => 13,
        }
        .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::LifeMeterType) {
        row.selected_choice_index[player_idx] = match profile.lifemeter_type {
            crate::game::profile::LifeMeterType::Standard => 0,
            crate::game::profile::LifeMeterType::Surround => 1,
            crate::game::profile::LifeMeterType::Vertical => 2,
        };
    }
    if profile.rainbow_max {
        life_bar_options_active_mask |= 1u8 << 0;
    }
    if profile.responsive_colors {
        life_bar_options_active_mask |= 1u8 << 1;
    }
    if profile.show_life_percent {
        life_bar_options_active_mask |= 1u8 << 2;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::LifeBarOptions) {
        if life_bar_options_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (life_bar_options_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ErrorBarTrim) {
        row.selected_choice_index[player_idx] = match profile.error_bar_trim {
            crate::game::profile::ErrorBarTrim::Off => 0,
            crate::game::profile::ErrorBarTrim::Fantastic => 1,
            crate::game::profile::ErrorBarTrim::Excellent => 2,
            crate::game::profile::ErrorBarTrim::Great => 3,
        };
    }
    if profile.error_bar_up {
        error_bar_options_active_mask |= 1u8 << 0;
    }
    if profile.error_bar_multi_tick {
        error_bar_options_active_mask |= 1u8 << 1;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ErrorBarOptions) {
        if error_bar_options_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (error_bar_options_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    // Initialize Measure Counter rows (zmod semantics).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::MeasureCounter) {
        row.selected_choice_index[player_idx] = match profile.measure_counter {
            crate::game::profile::MeasureCounter::None => 0,
            crate::game::profile::MeasureCounter::Eighth => 1,
            crate::game::profile::MeasureCounter::Twelfth => 2,
            crate::game::profile::MeasureCounter::Sixteenth => 3,
            crate::game::profile::MeasureCounter::TwentyFourth => 4,
            crate::game::profile::MeasureCounter::ThirtySecond => 5,
        };
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::MeasureCounterLookahead)
    {
        row.selected_choice_index[player_idx] = (profile.measure_counter_lookahead.min(4) as usize)
            .min(row.choices.len().saturating_sub(1));
    }
    if profile.measure_counter_left {
        measure_counter_options_active_mask |= 1u8 << 0;
    }
    if profile.measure_counter_up {
        measure_counter_options_active_mask |= 1u8 << 1;
    }
    if profile.measure_counter_vert {
        measure_counter_options_active_mask |= 1u8 << 2;
    }
    if profile.broken_run {
        measure_counter_options_active_mask |= 1u8 << 3;
    }
    if profile.run_timer {
        measure_counter_options_active_mask |= 1u8 << 4;
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::MeasureCounterOptions)
    {
        if measure_counter_options_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (measure_counter_options_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::MeasureLines) {
        row.selected_choice_index[player_idx] = match profile.measure_lines {
            crate::game::profile::MeasureLines::Off => 0,
            crate::game::profile::MeasureLines::Measure => 1,
            crate::game::profile::MeasureLines::Quarter => 2,
            crate::game::profile::MeasureLines::Eighth => 3,
        };
    }
    // Initialize Turn row from profile setting.
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Turn) {
        row.selected_choice_index[player_idx] = match profile.turn_option {
            crate::game::profile::TurnOption::None => 0,
            crate::game::profile::TurnOption::Mirror => 1,
            crate::game::profile::TurnOption::Left => 2,
            crate::game::profile::TurnOption::Right => 3,
            crate::game::profile::TurnOption::LRMirror => 4,
            crate::game::profile::TurnOption::UDMirror => 5,
            crate::game::profile::TurnOption::Shuffle => 6,
            crate::game::profile::TurnOption::Blender => 7,
            crate::game::profile::TurnOption::Random => 8,
        }
        .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::RescoreEarlyHits) {
        row.selected_choice_index[player_idx] = if profile.rescore_early_hits { 1 } else { 0 };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::TimingWindows) {
        row.selected_choice_index[player_idx] = match profile.timing_windows {
            crate::game::profile::TimingWindowsOption::None => 0,
            crate::game::profile::TimingWindowsOption::WayOffs => 1,
            crate::game::profile::TimingWindowsOption::DecentsAndWayOffs => 2,
            crate::game::profile::TimingWindowsOption::FantasticsAndExcellents => 3,
        }
        .min(row.choices.len().saturating_sub(1));
    }
    if profile.track_early_judgments {
        results_extras_active_mask |= 1u8 << 0;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::ResultsExtras) {
        if results_extras_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (results_extras_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::MiniIndicator) {
        row.selected_choice_index[player_idx] = match profile.mini_indicator {
            crate::game::profile::MiniIndicator::None => 0,
            crate::game::profile::MiniIndicator::SubtractiveScoring => 1,
            crate::game::profile::MiniIndicator::PredictiveScoring => 2,
            crate::game::profile::MiniIndicator::PaceScoring => 3,
            crate::game::profile::MiniIndicator::RivalScoring => 4,
            crate::game::profile::MiniIndicator::Pacemaker => 5,
            crate::game::profile::MiniIndicator::StreamProg => 6,
        }
        .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::IndicatorScoreType) {
        row.selected_choice_index[player_idx] = match profile.mini_indicator_score_type {
            crate::game::profile::MiniIndicatorScoreType::Itg => 0,
            crate::game::profile::MiniIndicatorScoreType::Ex => 1,
            crate::game::profile::MiniIndicatorScoreType::HardEx => 2,
        }
        .min(row.choices.len().saturating_sub(1));
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::EarlyDecentWayOffOptions)
    {
        if profile.hide_early_dw_judgments {
            early_dw_active_mask |= 1u8 << 0;
        }
        if profile.hide_early_dw_flash {
            early_dw_active_mask |= 1u8 << 1;
        }

        if early_dw_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (early_dw_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    // Initialize FA+ Options row from profile (independent toggles).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::FAPlusOptions) {
        // Cursor always starts on the first option; toggled state is reflected visually.
        row.selected_choice_index[player_idx] = 0;
    }
    if profile.show_fa_plus_window {
        fa_plus_active_mask |= 1u8 << 0;
    }
    if profile.show_ex_score {
        fa_plus_active_mask |= 1u8 << 1;
    }
    if profile.show_hard_ex_score {
        fa_plus_active_mask |= 1u8 << 2;
    }
    if profile.show_fa_plus_pane {
        fa_plus_active_mask |= 1u8 << 3;
    }
    if profile.fa_plus_10ms_blue_window {
        fa_plus_active_mask |= 1u8 << 4;
    }
    if profile.split_15_10ms {
        fa_plus_active_mask |= 1u8 << 5;
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::CustomBlueFantasticWindow)
    {
        row.selected_choice_index[player_idx] = if profile.custom_fantastic_window {
            1
        } else {
            0
        };
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::CustomBlueFantasticWindowMs)
    {
        let ms = crate::game::profile::clamp_custom_fantastic_window_ms(
            profile.custom_fantastic_window_ms,
        );
        let target = format!("{ms}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &target) {
            row.selected_choice_index[player_idx] = idx;
        }
    }

    // Initialize Gameplay Extras row from profile (multi-choice toggle group).
    if profile.column_flash_on_miss {
        gameplay_extras_active_mask |= 1u8 << 0;
    }
    if profile.nps_graph_at_top {
        gameplay_extras_active_mask |= 1u8 << 1;
    }
    if profile.column_cues {
        gameplay_extras_active_mask |= 1u8 << 2;
        gameplay_extras_more_active_mask |= 1u8 << 0;
    }
    if profile.display_scorebox {
        gameplay_extras_active_mask |= 1u8 << 3;
        gameplay_extras_more_active_mask |= 1u8 << 1;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::GameplayExtras) {
        if gameplay_extras_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (gameplay_extras_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.id == RowId::DensityGraphBackground)
    {
        row.selected_choice_index[player_idx] = if profile.transparent_density_graph_bg {
            1
        } else {
            0
        };
    }

    // Initialize Gameplay Extras (More) row from profile (multi-choice toggle group).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::GameplayExtrasMore) {
        if gameplay_extras_more_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (gameplay_extras_more_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }

    // Initialize Hide row from profile (multi-choice toggle group).
    if profile.hide_targets {
        hide_active_mask |= 1u8 << 0;
    }
    if profile.hide_song_bg {
        hide_active_mask |= 1u8 << 1;
    }
    if profile.hide_combo {
        hide_active_mask |= 1u8 << 2;
    }
    if profile.hide_lifebar {
        hide_active_mask |= 1u8 << 3;
    }
    if profile.hide_score {
        hide_active_mask |= 1u8 << 4;
    }
    if profile.hide_danger {
        hide_active_mask |= 1u8 << 5;
    }
    if profile.hide_combo_explosions {
        hide_active_mask |= 1u8 << 6;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Hide) {
        if hide_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (hide_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }

    // Initialize Scroll row from profile setting (multi-choice toggle group).
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Scroll) {
        use crate::game::profile::ScrollOption;
        // Choice indices are fixed by construction order in build_advanced_rows:
        // 0=Reverse, 1=Split, 2=Alternate, 3=Cross, 4=Centered
        const REVERSE: usize = 0;
        const SPLIT: usize = 1;
        const ALTERNATE: usize = 2;
        const CROSS: usize = 3;
        const CENTERED: usize = 4;
        let flags: &[(ScrollOption, usize)] = &[
            (ScrollOption::Reverse, REVERSE),
            (ScrollOption::Split, SPLIT),
            (ScrollOption::Alternate, ALTERNATE),
            (ScrollOption::Cross, CROSS),
            (ScrollOption::Centered, CENTERED),
        ];
        for &(flag, idx) in flags {
            if profile.scroll_option.contains(flag) && idx < row.choices.len() && idx < 8 {
                scroll_active_mask |= 1u8 << (idx as u8);
            }
        }

        // Cursor starts at the first active choice if any, otherwise at the first option.
        if scroll_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (scroll_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Insert) {
        insert_active_mask =
            crate::game::profile::normalize_insert_mask(profile.insert_active_mask);
        if insert_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (insert_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Remove) {
        remove_active_mask =
            crate::game::profile::normalize_remove_mask(profile.remove_active_mask);
        if remove_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (remove_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Holds) {
        holds_active_mask = crate::game::profile::normalize_holds_mask(profile.holds_active_mask);
        if holds_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (holds_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Accel) {
        accel_effects_active_mask =
            crate::game::profile::normalize_accel_effects_mask(profile.accel_effects_active_mask);
        if accel_effects_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (accel_effects_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Effect) {
        visual_effects_active_mask =
            crate::game::profile::normalize_visual_effects_mask(profile.visual_effects_active_mask);
        if visual_effects_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u16 << (*i as u16);
                    (visual_effects_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Appearance) {
        appearance_effects_active_mask = crate::game::profile::normalize_appearance_effects_mask(
            profile.appearance_effects_active_mask,
        );
        if appearance_effects_active_mask != 0 {
            let first_idx = (0..row.choices.len())
                .find(|i| {
                    let bit = 1u8 << (*i as u8);
                    (appearance_effects_active_mask & bit) != 0
                })
                .unwrap_or(0);
            row.selected_choice_index[player_idx] = first_idx;
        } else {
            row.selected_choice_index[player_idx] = 0;
        }
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::Attacks) {
        row.selected_choice_index[player_idx] = match profile.attack_mode {
            crate::game::profile::AttackMode::On => 0,
            crate::game::profile::AttackMode::Random => 1,
            crate::game::profile::AttackMode::Off => 2,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.id == RowId::HideLightType) {
        row.selected_choice_index[player_idx] = match profile.hide_light_type {
            crate::game::profile::HideLightType::NoHideLights => 0,
            crate::game::profile::HideLightType::HideAllLights => 1,
            crate::game::profile::HideLightType::HideMarqueeLights => 2,
            crate::game::profile::HideLightType::HideBassLights => 3,
        };
    }
    (
        scroll_active_mask,
        hide_active_mask,
        insert_active_mask,
        remove_active_mask,
        holds_active_mask,
        accel_effects_active_mask,
        visual_effects_active_mask,
        appearance_effects_active_mask,
        fa_plus_active_mask,
        early_dw_active_mask,
        gameplay_extras_active_mask,
        gameplay_extras_more_active_mask,
        results_extras_active_mask,
        life_bar_options_active_mask,
        error_bar_active_mask,
        error_bar_options_active_mask,
        measure_counter_options_active_mask,
    )
}

pub fn init(
    song: Arc<SongData>,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    active_color_index: i32,
    return_screen: Screen,
    fixed_stepchart: Option<FixedStepchart>,
) -> State {
    let session_music_rate = crate::game::profile::get_session_music_rate();
    let allow_per_player_global_offsets =
        crate::config::get().machine_allow_per_player_global_offsets;
    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);

    let speed_mod_p1 = match p1_profile.scroll_speed {
        crate::game::scroll::ScrollSpeedSetting::CMod(bpm) => SpeedMod {
            mod_type: "C".to_string(),
            value: bpm,
        },
        crate::game::scroll::ScrollSpeedSetting::XMod(mult) => SpeedMod {
            mod_type: "X".to_string(),
            value: mult,
        },
        crate::game::scroll::ScrollSpeedSetting::MMod(bpm) => SpeedMod {
            mod_type: "M".to_string(),
            value: bpm,
        },
    };
    let speed_mod_p2 = match p2_profile.scroll_speed {
        crate::game::scroll::ScrollSpeedSetting::CMod(bpm) => SpeedMod {
            mod_type: "C".to_string(),
            value: bpm,
        },
        crate::game::scroll::ScrollSpeedSetting::XMod(mult) => SpeedMod {
            mod_type: "X".to_string(),
            value: mult,
        },
        crate::game::scroll::ScrollSpeedSetting::MMod(bpm) => SpeedMod {
            mod_type: "M".to_string(),
            value: bpm,
        },
    };
    let chart_difficulty_index: [usize; PLAYER_SLOTS] = std::array::from_fn(|player_idx| {
        let steps_idx = chart_steps_index[player_idx];
        let mut diff_idx = preferred_difficulty_index[player_idx].min(
            crate::engine::present::color::FILE_DIFFICULTY_NAMES
                .len()
                .saturating_sub(1),
        );
        if steps_idx < crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() {
            diff_idx = steps_idx;
        }
        diff_idx
    });

    let noteskin_names = discover_noteskin_names();
    let mut rows = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Main,
        &noteskin_names,
        return_screen,
        fixed_stepchart.as_ref(),
    );
    let player_profiles = [p1_profile.clone(), p2_profile.clone()];
    let (
        scroll_active_mask_p1,
        hide_active_mask_p1,
        insert_active_mask_p1,
        remove_active_mask_p1,
        holds_active_mask_p1,
        accel_effects_active_mask_p1,
        visual_effects_active_mask_p1,
        appearance_effects_active_mask_p1,
        fa_plus_active_mask_p1,
        early_dw_active_mask_p1,
        gameplay_extras_active_mask_p1,
        gameplay_extras_more_active_mask_p1,
        results_extras_active_mask_p1,
        life_bar_options_active_mask_p1,
        error_bar_active_mask_p1,
        error_bar_options_active_mask_p1,
        measure_counter_options_active_mask_p1,
    ) = apply_profile_defaults(&mut rows, &player_profiles[P1], P1);
    let (
        scroll_active_mask_p2,
        hide_active_mask_p2,
        insert_active_mask_p2,
        remove_active_mask_p2,
        holds_active_mask_p2,
        accel_effects_active_mask_p2,
        visual_effects_active_mask_p2,
        appearance_effects_active_mask_p2,
        fa_plus_active_mask_p2,
        early_dw_active_mask_p2,
        gameplay_extras_active_mask_p2,
        gameplay_extras_more_active_mask_p2,
        results_extras_active_mask_p2,
        life_bar_options_active_mask_p2,
        error_bar_active_mask_p2,
        error_bar_options_active_mask_p2,
        measure_counter_options_active_mask_p2,
    ) = apply_profile_defaults(&mut rows, &player_profiles[P2], P2);

    let cols_per_player = noteskin_cols_per_player(crate::game::profile::get_session_play_style());
    let mut initial_noteskin_names = vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()];
    for profile in &player_profiles {
        push_noteskin_name_once(&mut initial_noteskin_names, &profile.noteskin);
        if let Some(skin) = profile.mine_noteskin.as_ref() {
            push_noteskin_name_once(&mut initial_noteskin_names, skin);
        }
        if let Some(skin) = profile.receptor_noteskin.as_ref() {
            push_noteskin_name_once(&mut initial_noteskin_names, skin);
        }
        if let Some(skin) = profile.tap_explosion_noteskin.as_ref() {
            push_noteskin_name_once(&mut initial_noteskin_names, skin);
        }
    }
    let mut noteskin_cache = build_noteskin_cache(cols_per_player, &initial_noteskin_names);
    let noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] = std::array::from_fn(|i| {
        cached_or_load_noteskin(
            &mut noteskin_cache,
            &player_profiles[i].noteskin,
            cols_per_player,
        )
    });
    let mine_noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] = std::array::from_fn(|i| {
        resolved_noteskin_override_preview(
            &mut noteskin_cache,
            &player_profiles[i].noteskin,
            player_profiles[i].mine_noteskin.as_ref(),
            cols_per_player,
        )
    });
    let receptor_noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] =
        std::array::from_fn(|i| {
            resolved_noteskin_override_preview(
                &mut noteskin_cache,
                &player_profiles[i].noteskin,
                player_profiles[i].receptor_noteskin.as_ref(),
                cols_per_player,
            )
        });
    let tap_explosion_noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] =
        std::array::from_fn(|i| {
            resolved_tap_explosion_preview(
                &mut noteskin_cache,
                &player_profiles[i].noteskin,
                player_profiles[i].tap_explosion_noteskin.as_ref(),
                cols_per_player,
            )
        });
    let active = session_active_players();
    let row_tweens = init_row_tweens(
        &rows,
        [0; PLAYER_SLOTS],
        active,
        [hide_active_mask_p1, hide_active_mask_p2],
        [error_bar_active_mask_p1, error_bar_active_mask_p2],
        allow_per_player_global_offsets,
    );
    State {
        song,
        return_screen,
        fixed_stepchart,
        chart_steps_index,
        chart_difficulty_index,
        rows,
        selected_row: [0; PLAYER_SLOTS],
        prev_selected_row: [0; PLAYER_SLOTS],
        scroll_active_mask: [scroll_active_mask_p1, scroll_active_mask_p2],
        hide_active_mask: [hide_active_mask_p1, hide_active_mask_p2],
        insert_active_mask: [insert_active_mask_p1, insert_active_mask_p2],
        remove_active_mask: [remove_active_mask_p1, remove_active_mask_p2],
        holds_active_mask: [holds_active_mask_p1, holds_active_mask_p2],
        accel_effects_active_mask: [accel_effects_active_mask_p1, accel_effects_active_mask_p2],
        visual_effects_active_mask: [visual_effects_active_mask_p1, visual_effects_active_mask_p2],
        appearance_effects_active_mask: [
            appearance_effects_active_mask_p1,
            appearance_effects_active_mask_p2,
        ],
        fa_plus_active_mask: [fa_plus_active_mask_p1, fa_plus_active_mask_p2],
        early_dw_active_mask: [early_dw_active_mask_p1, early_dw_active_mask_p2],
        gameplay_extras_active_mask: [
            gameplay_extras_active_mask_p1,
            gameplay_extras_active_mask_p2,
        ],
        gameplay_extras_more_active_mask: [
            gameplay_extras_more_active_mask_p1,
            gameplay_extras_more_active_mask_p2,
        ],
        results_extras_active_mask: [results_extras_active_mask_p1, results_extras_active_mask_p2],
        life_bar_options_active_mask: [
            life_bar_options_active_mask_p1,
            life_bar_options_active_mask_p2,
        ],
        error_bar_active_mask: [error_bar_active_mask_p1, error_bar_active_mask_p2],
        error_bar_options_active_mask: [
            error_bar_options_active_mask_p1,
            error_bar_options_active_mask_p2,
        ],
        measure_counter_options_active_mask: [
            measure_counter_options_active_mask_p1,
            measure_counter_options_active_mask_p2,
        ],
        active_color_index,
        speed_mod: [speed_mod_p1, speed_mod_p2],
        music_rate: session_music_rate,
        current_pane: OptionsPane::Main,
        scroll_focus_player: P1,
        bg: heart_bg::State::new(),
        nav_key_held_direction: [None; PLAYER_SLOTS],
        nav_key_held_since: [None; PLAYER_SLOTS],
        nav_key_last_scrolled_at: [None; PLAYER_SLOTS],
        start_held_since: [None; PLAYER_SLOTS],
        start_last_triggered_at: [None; PLAYER_SLOTS],
        inline_choice_x: [f32::NAN; PLAYER_SLOTS],
        arcade_row_focus: [true; PLAYER_SLOTS],
        allow_per_player_global_offsets,
        player_profiles,
        noteskin_names,
        noteskin_cache,
        noteskin: noteskin_previews,
        mine_noteskin: mine_noteskin_previews,
        receptor_noteskin: receptor_noteskin_previews,
        tap_explosion_noteskin: tap_explosion_noteskin_previews,
        preview_time: 0.0,
        preview_beat: 0.0,
        help_anim_time: [0.0; PLAYER_SLOTS],
        combo_preview_count: 0,
        combo_preview_elapsed: 0.0,
        cursor_initialized: [false; PLAYER_SLOTS],
        cursor_from_x: [0.0; PLAYER_SLOTS],
        cursor_from_y: [0.0; PLAYER_SLOTS],
        cursor_from_w: [0.0; PLAYER_SLOTS],
        cursor_from_h: [0.0; PLAYER_SLOTS],
        cursor_to_x: [0.0; PLAYER_SLOTS],
        cursor_to_y: [0.0; PLAYER_SLOTS],
        cursor_to_w: [0.0; PLAYER_SLOTS],
        cursor_to_h: [0.0; PLAYER_SLOTS],
        cursor_t: [1.0; PLAYER_SLOTS],
        row_tweens,
        pane_transition: PaneTransition::None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

#[inline(always)]
fn session_active_players() -> [bool; PLAYER_SLOTS] {
    let play_style = crate::game::profile::get_session_play_style();
    let side = crate::game::profile::get_session_player_side();
    let joined = [
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1),
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2),
    ];
    let joined_count = usize::from(joined[P1]) + usize::from(joined[P2]);
    match play_style {
        crate::game::profile::PlayStyle::Versus => {
            if joined_count > 0 {
                joined
            } else {
                [true, true]
            }
        }
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Double => {
            if joined_count == 1 {
                joined
            } else {
                match side {
                    crate::game::profile::PlayerSide::P1 => [true, false],
                    crate::game::profile::PlayerSide::P2 => [false, true],
                }
            }
        }
    }
}

#[inline(always)]
fn arcade_options_navigation_active() -> bool {
    crate::config::get().arcade_options_navigation
}

#[inline(always)]
const fn pane_uses_arcade_next_row(pane: OptionsPane) -> bool {
    !matches!(pane, OptionsPane::Main)
}

#[inline(always)]
fn session_persisted_player_idx() -> usize {
    let play_style = crate::game::profile::get_session_play_style();
    let side = crate::game::profile::get_session_player_side();
    match play_style {
        crate::game::profile::PlayStyle::Versus => P1,
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Double => {
            match side {
                crate::game::profile::PlayerSide::P1 => P1,
                crate::game::profile::PlayerSide::P2 => P2,
            }
        }
    }
}

const ARCADE_NEXT_ROW_TEXT: &str = "▼";

#[derive(Clone, Copy, Debug)]
struct RowVisibility {
    show_measure_counter_children: bool,
    show_judgment_offsets: bool,
    show_judgment_tilt_intensity: bool,
    show_combo_offsets: bool,
    show_error_bar_children: bool,
    show_custom_fantastic_window_ms: bool,
    show_density_graph_background: bool,
    show_combo_rows: bool,
    show_lifebar_rows: bool,
    show_indicator_score_type: bool,
    show_global_offset_shift: bool,
}

#[inline(always)]
fn row_visible_with_flags(id: RowId, visibility: RowVisibility) -> bool {
    if id == RowId::MeasureCounterLookahead || id == RowId::MeasureCounterOptions {
        return visibility.show_measure_counter_children;
    }
    if id == RowId::JudgmentOffsetX || id == RowId::JudgmentOffsetY {
        return visibility.show_judgment_offsets;
    }
    if id == RowId::JudgmentTiltIntensity {
        return visibility.show_judgment_tilt_intensity;
    }
    if id == RowId::ComboOffsetX || id == RowId::ComboOffsetY {
        return visibility.show_combo_offsets;
    }
    if id == RowId::ErrorBarTrim
        || id == RowId::ErrorBarOptions
        || id == RowId::ErrorBarOffsetX
        || id == RowId::ErrorBarOffsetY
    {
        return visibility.show_error_bar_children;
    }
    if id == RowId::CustomBlueFantasticWindowMs {
        return visibility.show_custom_fantastic_window_ms;
    }
    if id == RowId::DensityGraphBackground {
        return visibility.show_density_graph_background;
    }
    if id == RowId::ComboColors
        || id == RowId::ComboColorMode
        || id == RowId::CarryCombo
    {
        return visibility.show_combo_rows;
    }
    if id == RowId::LifeMeterType || id == RowId::LifeBarOptions {
        return visibility.show_lifebar_rows;
    }
    if id == RowId::IndicatorScoreType {
        return visibility.show_indicator_score_type;
    }
    if id == RowId::GlobalOffsetShift {
        return visibility.show_global_offset_shift;
    }
    true
}

#[inline(always)]
fn conditional_row_parent(id: RowId) -> Option<RowId> {
    if id == RowId::MeasureCounterLookahead || id == RowId::MeasureCounterOptions {
        return Some(RowId::MeasureCounter);
    }
    if id == RowId::JudgmentOffsetX || id == RowId::JudgmentOffsetY {
        return Some(RowId::JudgmentFont);
    }
    if id == RowId::JudgmentTiltIntensity {
        return Some(RowId::JudgmentTilt);
    }
    if id == RowId::ComboOffsetX || id == RowId::ComboOffsetY {
        return Some(RowId::ComboFont);
    }
    if id == RowId::ErrorBarTrim
        || id == RowId::ErrorBarOptions
        || id == RowId::ErrorBarOffsetX
        || id == RowId::ErrorBarOffsetY
    {
        return Some(RowId::ErrorBar);
    }
    if id == RowId::CustomBlueFantasticWindowMs {
        return Some(RowId::CustomBlueFantasticWindow);
    }
    if id == RowId::DensityGraphBackground {
        return Some(RowId::DataVisualizations);
    }
    if id == RowId::ComboColors
        || id == RowId::ComboColorMode
        || id == RowId::CarryCombo
        || id == RowId::LifeMeterType
        || id == RowId::LifeBarOptions
    {
        return Some(RowId::Hide);
    }
    if id == RowId::IndicatorScoreType {
        return Some(RowId::MiniIndicator);
    }
    None
}

fn measure_counter_children_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::MeasureCounter) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

fn judgment_offsets_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::JudgmentFont) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        // "None" is always the last choice for font/texture rows.
        if choice_idx != max_choice {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
fn judgment_tilt_intensity_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::JudgmentTilt) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

fn combo_offsets_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::ComboFont) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        // "None" is always the last choice for font/texture rows.
        if choice_idx != max_choice {
            return true;
        }
    }
    !any_active
}

fn error_bar_children_visible(
    active: [bool; PLAYER_SLOTS],
    error_bar_active_mask: [u8; PLAYER_SLOTS],
) -> bool {
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        if crate::game::profile::normalize_error_bar_mask(error_bar_active_mask[player_idx]) != 0 {
            return true;
        }
    }
    !any_active
}

fn custom_fantastic_window_ms_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::CustomBlueFantasticWindow) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

fn density_graph_background_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::DataVisualizations) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx == 2 {
            return true;
        }
    }
    !any_active
}

fn combo_rows_visible(active: [bool; PLAYER_SLOTS], hide_active_mask: [u8; PLAYER_SLOTS]) -> bool {
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let hide_combo = (hide_active_mask[player_idx] & (1u8 << 2)) != 0;
        if !hide_combo {
            return true;
        }
    }
    !any_active
}

fn lifebar_rows_visible(
    active: [bool; PLAYER_SLOTS],
    hide_active_mask: [u8; PLAYER_SLOTS],
) -> bool {
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let hide_lifebar = (hide_active_mask[player_idx] & (1u8 << 3)) != 0;
        if !hide_lifebar {
            return true;
        }
    }
    !any_active
}

fn indicator_score_type_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.id == RowId::MiniIndicator) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in active_player_indices(active) {
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        // Visible for Subtractive(1), Predictive(2), Pace(3)
        if (1..=3).contains(&choice_idx) {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
fn row_visibility(
    rows: &[Row],
    active: [bool; PLAYER_SLOTS],
    hide_active_mask: [u8; PLAYER_SLOTS],
    error_bar_active_mask: [u8; PLAYER_SLOTS],
    allow_per_player_global_offsets: bool,
) -> RowVisibility {
    RowVisibility {
        show_measure_counter_children: measure_counter_children_visible(rows, active),
        show_judgment_offsets: judgment_offsets_visible(rows, active),
        show_judgment_tilt_intensity: judgment_tilt_intensity_visible(rows, active),
        show_combo_offsets: combo_offsets_visible(rows, active),
        show_error_bar_children: error_bar_children_visible(active, error_bar_active_mask),
        show_custom_fantastic_window_ms: custom_fantastic_window_ms_visible(rows, active),
        show_density_graph_background: density_graph_background_visible(rows, active),
        show_combo_rows: combo_rows_visible(active, hide_active_mask),
        show_lifebar_rows: lifebar_rows_visible(active, hide_active_mask),
        show_indicator_score_type: indicator_score_type_visible(rows, active),
        show_global_offset_shift: allow_per_player_global_offsets,
    }
}

#[inline(always)]
fn is_row_visible(rows: &[Row], row_idx: usize, visibility: RowVisibility) -> bool {
    rows.get(row_idx)
        .is_some_and(|row| row_visible_with_flags(row.id, visibility))
}

fn count_visible_rows(rows: &[Row], visibility: RowVisibility) -> usize {
    rows.iter()
        .filter(|row| row_visible_with_flags(row.id, visibility))
        .count()
}

fn row_to_visible_index(rows: &[Row], row_idx: usize, visibility: RowVisibility) -> Option<usize> {
    if row_idx >= rows.len() {
        return None;
    }
    if !is_row_visible(rows, row_idx, visibility) {
        return None;
    }
    let mut pos = 0usize;
    for i in 0..row_idx {
        if is_row_visible(rows, i, visibility) {
            pos += 1;
        }
    }
    Some(pos)
}

fn fallback_visible_row(rows: &[Row], row_idx: usize, visibility: RowVisibility) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    let start = row_idx.min(rows.len().saturating_sub(1));
    for i in start..rows.len() {
        if is_row_visible(rows, i, visibility) {
            return Some(i);
        }
    }
    (0..start)
        .rev()
        .find(|&i| is_row_visible(rows, i, visibility))
}

fn next_visible_row(
    rows: &[Row],
    current_row: usize,
    dir: NavDirection,
    visibility: RowVisibility,
) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    let len = rows.len();
    let mut idx = current_row.min(len.saturating_sub(1));
    if !is_row_visible(rows, idx, visibility) {
        idx = fallback_visible_row(rows, idx, visibility)?;
    }
    for _ in 0..len {
        idx = match dir {
            NavDirection::Up => (idx + len - 1) % len,
            NavDirection::Down => (idx + 1) % len,
            NavDirection::Left | NavDirection::Right => return Some(idx),
        };
        if is_row_visible(rows, idx, visibility) {
            return Some(idx);
        }
    }
    None
}

fn parent_anchor_visible_index(
    rows: &[Row],
    parent_id: RowId,
    visibility: RowVisibility,
) -> Option<i32> {
    rows.iter()
        .position(|row| row.id == parent_id)
        .and_then(|idx| row_to_visible_index(rows, idx, visibility))
        .map(|idx| idx as i32)
}

#[inline(always)]
fn f_pos_for_visible_idx(
    visible_idx: i32,
    window: RowWindow,
    mid_pos: f32,
    bottom_pos: f32,
) -> (f32, bool) {
    let hidden_above = visible_idx < window.first_start;
    let hidden_mid = visible_idx >= window.first_end && visible_idx < window.second_start;
    let hidden_below = visible_idx >= window.second_end;
    if hidden_above {
        return (-0.5, true);
    }
    if hidden_mid {
        return (mid_pos, true);
    }
    if hidden_below {
        return (bottom_pos, true);
    }

    let shown_pos = if visible_idx < window.first_end {
        visible_idx - window.first_start
    } else {
        (window.first_end - window.first_start) + (visible_idx - window.second_start)
    };
    (shown_pos as f32, false)
}

fn sync_selected_rows_with_visibility(state: &mut State, active: [bool; PLAYER_SLOTS]) {
    if state.rows.is_empty() {
        state.selected_row = [0; PLAYER_SLOTS];
        state.prev_selected_row = [0; PLAYER_SLOTS];
        return;
    }
    let visibility = row_visibility(
        &state.rows,
        active,
        state.hide_active_mask,
        state.error_bar_active_mask,
        state.allow_per_player_global_offsets,
    );
    for player_idx in [P1, P2] {
        let idx = state.selected_row[player_idx].min(state.rows.len().saturating_sub(1));
        if is_row_visible(&state.rows, idx, visibility) {
            state.selected_row[player_idx] = idx;
            continue;
        }
        if let Some(fallback) = fallback_visible_row(&state.rows, idx, visibility) {
            state.selected_row[player_idx] = fallback;
            if active[player_idx] {
                state.prev_selected_row[player_idx] = fallback;
            }
        }
    }
}

#[inline(always)]
fn row_is_shared(id: RowId) -> bool {
    id == RowId::Exit || id == RowId::WhatComesNext || id == RowId::MusicRate
}

#[inline(always)]
fn row_shows_all_choices_inline(id: RowId) -> bool {
    id == RowId::Perspective
        || id == RowId::BackgroundFilter
        || id == RowId::Stepchart
        || id == RowId::WhatComesNext
        || id == RowId::ActionOnMissedTarget
        || id == RowId::ErrorBar
        || id == RowId::ErrorBarTrim
        || id == RowId::ErrorBarOptions
        || id == RowId::OffsetIndicator
        || id == RowId::JudgmentBehindArrows
        || id == RowId::MeasureCounter
        || id == RowId::MeasureCounterLookahead
        || id == RowId::MeasureCounterOptions
        || id == RowId::MeasureLines
        || id == RowId::TimingWindows
        || id == RowId::JudgmentTilt
        || id == RowId::MiniIndicator
        || id == RowId::IndicatorScoreType
        || id == RowId::Turn
        || id == RowId::Scroll
        || id == RowId::Hide
        || id == RowId::LifeMeterType
        || id == RowId::LifeBarOptions
        || id == RowId::DataVisualizations
        || id == RowId::DensityGraphBackground
        || id == RowId::ComboColors
        || id == RowId::ComboColorMode
        || id == RowId::CarryCombo
        || id == RowId::GameplayExtras
        || id == RowId::GameplayExtrasMore
        || id == RowId::ResultsExtras
        || id == RowId::RescoreEarlyHits
        || id == RowId::CustomBlueFantasticWindow
        || id == RowId::EarlyDecentWayOffOptions
        || id == RowId::FAPlusOptions
        || id == RowId::Insert
        || id == RowId::Remove
        || id == RowId::Holds
        || id == RowId::Accel
        || id == RowId::Effect
        || id == RowId::Appearance
        || id == RowId::Attacks
        || id == RowId::HideLightType
}

#[inline(always)]
fn row_supports_inline_nav(row: &Row) -> bool {
    !row.choices.is_empty() && row_shows_all_choices_inline(row.id)
}

#[inline(always)]
fn row_toggles_with_start(id: RowId) -> bool {
    id == RowId::Scroll
        || id == RowId::Hide
        || id == RowId::Insert
        || id == RowId::Remove
        || id == RowId::Holds
        || id == RowId::Accel
        || id == RowId::Effect
        || id == RowId::Appearance
        || id == RowId::LifeBarOptions
        || id == RowId::GameplayExtras
        || id == RowId::GameplayExtrasMore
        || id == RowId::ResultsExtras
        || id == RowId::ErrorBar
        || id == RowId::ErrorBarOptions
        || id == RowId::MeasureCounterOptions
        || id == RowId::FAPlusOptions
        || id == RowId::EarlyDecentWayOffOptions
}

#[inline(always)]
fn row_selects_on_focus_move(id: RowId) -> bool {
    id == RowId::Stepchart
}

#[inline(always)]
fn row_allows_arcade_next_row(state: &State, row_idx: usize) -> bool {
    arcade_options_navigation_active()
        && pane_uses_arcade_next_row(state.current_pane)
        && state
            .rows
            .get(row_idx)
            .is_some_and(|row| row.id != RowId::Exit && row_supports_inline_nav(row))
}

#[inline(always)]
fn arcade_row_uses_choice_focus(state: &State, player_idx: usize) -> bool {
    if !arcade_options_navigation_active() || !pane_uses_arcade_next_row(state.current_pane) {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.selected_row[idx].min(state.rows.len().saturating_sub(1));
    state.rows.get(row_idx).is_some_and(row_supports_inline_nav)
}

fn inline_choice_centers(
    choices: &[String],
    asset_manager: &AssetManager,
    left_x: f32,
) -> Vec<f32> {
    if choices.is_empty() {
        return Vec::new();
    }
    let mut centers: Vec<f32> = Vec::with_capacity(choices.len());
    let mut x = left_x;
    let zoom = 0.835_f32;
    for text in choices {
        let (draw_w, _) = measure_option_text(asset_manager, text, zoom);
        centers.push(draw_w.mul_add(0.5, x));
        x += draw_w + INLINE_SPACING;
    }
    centers
}

fn focused_inline_choice_index(
    state: &State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) -> Option<usize> {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row = state.rows.get(row_idx)?;
    if !row_supports_inline_nav(row) {
        return None;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return None;
    }
    let mut focus_idx = row.selected_choice_index[idx].min(centers.len().saturating_sub(1));
    let anchor_x = state.inline_choice_x[idx];
    if anchor_x.is_finite() {
        let mut best_dist = f32::INFINITY;
        for (i, &center_x) in centers.iter().enumerate() {
            let dist = (center_x - anchor_x).abs();
            if dist < best_dist {
                best_dist = dist;
                focus_idx = i;
            }
        }
    }
    Some(focus_idx)
}

fn move_inline_focus(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) -> bool {
    if state.rows.is_empty() || delta == 0 {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.selected_row[idx].min(state.rows.len().saturating_sub(1));
    let Some(row) = state.rows.get(row_idx) else {
        return false;
    };
    if !row_supports_inline_nav(row) {
        return false;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return false;
    }
    if row_allows_arcade_next_row(state, row_idx) {
        if state.arcade_row_focus[idx] {
            if delta <= 0 {
                return false;
            }
            state.arcade_row_focus[idx] = false;
            state.inline_choice_x[idx] = centers[0];
            return true;
        }
        let Some(current_idx) = focused_inline_choice_index(state, asset_manager, idx, row_idx)
        else {
            return false;
        };
        if delta < 0 {
            if current_idx == 0 {
                state.arcade_row_focus[idx] = true;
                state.inline_choice_x[idx] = f32::NAN;
                return true;
            }
            state.inline_choice_x[idx] = centers[current_idx - 1];
            return true;
        }
        if current_idx + 1 >= centers.len() {
            return false;
        }
        state.inline_choice_x[idx] = centers[current_idx + 1];
        return true;
    }
    let Some(current_idx) = focused_inline_choice_index(state, asset_manager, idx, row_idx) else {
        return false;
    };
    let n = centers.len() as isize;
    let next_idx = ((current_idx as isize + delta).rem_euclid(n)) as usize;
    state.inline_choice_x[idx] = centers[next_idx];
    true
}

fn commit_inline_focus_selection(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) -> bool {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let Some(row) = state.rows.get(row_idx) else {
        return false;
    };
    if !row_supports_inline_nav(row) {
        return false;
    }
    let Some(focus_idx) = focused_inline_choice_index(state, asset_manager, idx, row_idx) else {
        return false;
    };
    let is_shared = row_is_shared(row.id);
    if let Some(row) = state.rows.get_mut(row_idx) {
        if is_shared {
            let changed = row.selected_choice_index.iter().any(|&v| v != focus_idx);
            row.selected_choice_index = [focus_idx; PLAYER_SLOTS];
            return changed;
        }
        let changed = row.selected_choice_index[idx] != focus_idx;
        row.selected_choice_index[idx] = focus_idx;
        return changed;
    }
    false
}

fn sync_inline_intent_from_row(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if row_allows_arcade_next_row(state, row_idx) && state.arcade_row_focus[idx] {
        state.inline_choice_x[idx] = f32::NAN;
        return;
    }
    let Some(row) = state.rows.get(row_idx) else {
        return;
    };
    if !row_supports_inline_nav(row) {
        return;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return;
    }
    let sel = row.selected_choice_index[idx].min(centers.len().saturating_sub(1));
    state.inline_choice_x[idx] = centers[sel];
}

fn apply_inline_intent_to_row(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    row_idx: usize,
) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if row_allows_arcade_next_row(state, row_idx) && state.arcade_row_focus[idx] {
        state.inline_choice_x[idx] = f32::NAN;
        return;
    }
    let Some(row) = state.rows.get(row_idx) else {
        return;
    };
    if !row_supports_inline_nav(row) {
        return;
    }
    let centers = inline_choice_centers(
        &row.choices,
        asset_manager,
        inline_choice_left_x_for_row(state, row_idx),
    );
    if centers.is_empty() {
        return;
    }
    let sel = row.selected_choice_index[idx].min(centers.len().saturating_sub(1));
    if state.current_pane == OptionsPane::Main {
        state.inline_choice_x[idx] = centers[sel];
        return;
    }
    if !state.inline_choice_x[idx].is_finite() {
        state.inline_choice_x[idx] = centers[sel];
    }
}

fn move_selection_vertical(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    dir: NavDirection,
) {
    if !matches!(dir, NavDirection::Up | NavDirection::Down) || state.rows.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    sync_selected_rows_with_visibility(state, active);
    let visibility = row_visibility(
        &state.rows,
        active,
        state.hide_active_mask,
        state.error_bar_active_mask,
        state.allow_per_player_global_offsets,
    );
    let current_row = state.selected_row[idx].min(state.rows.len().saturating_sub(1));
    if !state.inline_choice_x[idx].is_finite() {
        if let Some((anchor_x, _, _, _)) = cursor_dest_for_player(state, asset_manager, idx) {
            state.inline_choice_x[idx] = anchor_x;
        } else {
            sync_inline_intent_from_row(state, asset_manager, idx, current_row);
        }
    }
    if let Some(next_row) = next_visible_row(&state.rows, current_row, dir, visibility) {
        state.selected_row[idx] = next_row;
        state.arcade_row_focus[idx] = row_allows_arcade_next_row(state, next_row);
        apply_inline_intent_to_row(state, asset_manager, idx, next_row);
    }
}

#[inline(always)]
fn measure_option_text(asset_manager: &AssetManager, text: &str, zoom: f32) -> (f32, f32) {
    let mut out_w = 40.0_f32;
    let mut out_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            out_h = (metrics_font.height as f32).max(1.0) * zoom;
            let mut w = crate::engine::present::font::measure_line_width_logical(
                metrics_font,
                text,
                all_fonts,
            ) as f32;
            if !w.is_finite() || w <= 0.0 {
                w = 1.0;
            }
            out_w = w * zoom;
        });
    });
    (out_w, out_h)
}

#[inline(always)]
fn inline_choice_left_x() -> f32 {
    widescale(162.0, 176.0)
}

#[inline(always)]
fn arcade_inline_choice_shift_x() -> f32 {
    widescale(6.0, 8.0)
}

#[inline(always)]
fn arcade_next_row_gap_x() -> f32 {
    widescale(5.0, 6.0)
}

#[inline(always)]
fn inline_choice_left_x_for_row(state: &State, row_idx: usize) -> f32 {
    inline_choice_left_x()
        + if row_allows_arcade_next_row(state, row_idx) {
            arcade_inline_choice_shift_x()
        } else {
            0.0
        }
}

#[inline(always)]
fn arcade_next_row_visible(state: &State, row_idx: usize) -> bool {
    row_allows_arcade_next_row(state, row_idx)
}

#[inline(always)]
fn arcade_row_focuses_next_row(state: &State, player_idx: usize, row_idx: usize) -> bool {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    row_allows_arcade_next_row(state, row_idx)
        && state.arcade_row_focus[idx]
        && state.selected_row[idx] == row_idx
}

fn arcade_next_row_layout(
    state: &State,
    row_idx: usize,
    asset_manager: &AssetManager,
    zoom: f32,
) -> (f32, f32, f32) {
    let (draw_w, draw_h) = measure_option_text(asset_manager, ARCADE_NEXT_ROW_TEXT, zoom);
    let left_x = inline_choice_left_x_for_row(state, row_idx) - draw_w - arcade_next_row_gap_x();
    (left_x, draw_w, draw_h)
}

fn cursor_dest_for_player(
    state: &State,
    asset_manager: &AssetManager,
    player_idx: usize,
) -> Option<(f32, f32, f32, f32)> {
    if state.rows.is_empty() {
        return None;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let visibility = row_visibility(
        &state.rows,
        session_active_players(),
        state.hide_active_mask,
        state.error_bar_active_mask,
        state.allow_per_player_global_offsets,
    );
    let mut row_idx = state.selected_row[player_idx].min(state.rows.len().saturating_sub(1));
    if !is_row_visible(&state.rows, row_idx, visibility) {
        row_idx = fallback_visible_row(&state.rows, row_idx, visibility)?;
    }
    let row = state.rows.get(row_idx)?;

    let y = state
        .row_tweens
        .get(row_idx)
        .map(|tw| tw.to_y)
        .unwrap_or_else(|| {
            // Fallback (no windowing) if row tweens aren't initialized yet.
            let (y0, step) = row_layout_params();
            (row_idx as f32).mul_add(step, y0)
        });

    let value_zoom = 0.835_f32;
    let border_w = widescale(2.0, 2.5);
    let pad_y = widescale(6.0, 8.0);
    let min_pad_x = widescale(2.0, 3.0);
    let max_pad_x = widescale(22.0, 28.0);
    let width_ref = widescale(180.0, 220.0);

    let speed_mod_x = screen_center_x() + widescale(-77.0, -100.0);

    // Shared geometry for Music Rate centering (must match get_actors()).
    let help_box_w = widescale(614.0, 792.0);
    let help_box_x = widescale(13.0, 30.666);
    let row_left = help_box_x;
    let row_width = help_box_w;
    let item_col_left = row_left + TITLE_BG_WIDTH;
    let item_col_w = row_width - TITLE_BG_WIDTH;
    let music_rate_center_x = item_col_left + item_col_w * 0.5;

    if row.id == RowId::Exit {
        // Exit row is shared (OptionRowExit); its cursor is centered on Speed Mod helper X.
        let choice_text = row
            .choices
            .get(row.selected_choice_index[P1])
            .or_else(|| row.choices.first())?;
        let (draw_w, draw_h) = measure_option_text(asset_manager, choice_text, value_zoom);
        let mut size_t = draw_w / width_ref;
        if !size_t.is_finite() {
            size_t = 0.0;
        }
        size_t = size_t.clamp(0.0, 1.0);
        let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
        if pad_x > max_pad_by_spacing {
            pad_x = max_pad_by_spacing;
        }
        let ring_w = draw_w + pad_x * 2.0;
        let ring_h = draw_h + pad_y * 2.0;
        return Some((speed_mod_x, y, ring_w, ring_h));
    }

    if row_shows_all_choices_inline(row.id) {
        if row.choices.is_empty() {
            return None;
        }
        let spacing = INLINE_SPACING;
        let choice_inner_left = inline_choice_left_x_for_row(state, row_idx);
        let mut widths: Vec<f32> = Vec::with_capacity(row.choices.len());
        let mut text_h: f32 = 16.0;
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |metrics_font| {
                text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                for text in &row.choices {
                    let mut w = crate::engine::present::font::measure_line_width_logical(
                        metrics_font,
                        text,
                        all_fonts,
                    ) as f32;
                    if !w.is_finite() || w <= 0.0 {
                        w = 1.0;
                    }
                    widths.push(w * value_zoom);
                }
            });
        });
        if widths.is_empty() {
            return None;
        }
        if arcade_row_focuses_next_row(state, player_idx, row_idx) {
            let (left_x, draw_w, draw_h) =
                arcade_next_row_layout(state, row_idx, asset_manager, value_zoom);
            let mut size_t = draw_w / width_ref;
            if !size_t.is_finite() {
                size_t = 0.0;
            }
            size_t = size_t.clamp(0.0, 1.0);
            let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
            let max_pad_by_spacing = (spacing - border_w).max(min_pad_x);
            if pad_x > max_pad_by_spacing {
                pad_x = max_pad_by_spacing;
            }
            let ring_w = draw_w + pad_x * 2.0;
            let ring_h = draw_h + pad_y * 2.0;
            return Some((draw_w.mul_add(0.5, left_x), y, ring_w, ring_h));
        }

        let focus_idx = focused_inline_choice_index(state, asset_manager, player_idx, row_idx)
            .unwrap_or_else(|| row.selected_choice_index[player_idx])
            .min(widths.len().saturating_sub(1));
        let mut left_x = choice_inner_left;
        for w in widths.iter().take(focus_idx) {
            left_x += *w + spacing;
        }
        let draw_w = widths[focus_idx];
        let center_x = draw_w.mul_add(0.5, left_x);

        let mut size_t = draw_w / width_ref;
        if !size_t.is_finite() {
            size_t = 0.0;
        }
        size_t = size_t.clamp(0.0, 1.0);
        let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
        let max_pad_by_spacing = (spacing - border_w).max(min_pad_x);
        if pad_x > max_pad_by_spacing {
            pad_x = max_pad_by_spacing;
        }
        let ring_w = draw_w + pad_x * 2.0;
        let ring_h = text_h + pad_y * 2.0;
        return Some((center_x, y, ring_w, ring_h));
    }

    // Single value rows (ShowOneInRow).
    let mut center_x = speed_mod_x;
    if row.id == RowId::MusicRate {
        center_x = music_rate_center_x;
    } else if player_idx == P2 {
        center_x = screen_center_x().mul_add(2.0, -center_x);
    }

    let display_text = if arcade_row_focuses_next_row(state, player_idx, row_idx) {
        ARCADE_NEXT_ROW_TEXT.to_string()
    } else if row.id == RowId::SpeedMod {
        match state.speed_mod[player_idx].mod_type.as_str() {
            "X" => format!("{:.2}x", state.speed_mod[player_idx].value),
            "C" => format!("C{}", state.speed_mod[player_idx].value as i32),
            "M" => format!("M{}", state.speed_mod[player_idx].value as i32),
            _ => String::new(),
        }
    } else if row.id == RowId::TypeOfSpeedMod {
        let idx = match state.speed_mod[player_idx].mod_type.as_str() {
            "X" => 0,
            "C" => 1,
            "M" => 2,
            _ => 1,
        };
        row.choices.get(idx).cloned().unwrap_or_default()
    } else {
        let idx = row.selected_choice_index[player_idx].min(row.choices.len().saturating_sub(1));
        row.choices.get(idx).cloned().unwrap_or_default()
    };

    let (draw_w, draw_h) = measure_option_text(asset_manager, &display_text, value_zoom);
    let mut size_t = draw_w / width_ref;
    if !size_t.is_finite() {
        size_t = 0.0;
    }
    size_t = size_t.clamp(0.0, 1.0);
    let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
    let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
    if pad_x > max_pad_by_spacing {
        pad_x = max_pad_by_spacing;
    }
    let ring_w = draw_w + pad_x * 2.0;
    let ring_h = draw_h + pad_y * 2.0;
    Some((center_x, y, ring_w, ring_h))
}

fn change_choice_for_player(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) {
    if state.rows.is_empty() {
        return;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[player_idx].min(state.rows.len().saturating_sub(1));
    let id = state.rows[row_index].id;
    if id == RowId::Exit {
        return;
    }
    let is_shared = row_is_shared(id);

    // Shared row: Music Rate
    if id == RowId::MusicRate {
        let row = &mut state.rows[row_index];
        let increment = 0.01f32;
        let min_rate = 0.05f32;
        let max_rate = 3.00f32;
        state.music_rate += delta as f32 * increment;
        state.music_rate = (state.music_rate / increment).round() * increment;
        state.music_rate = state.music_rate.clamp(min_rate, max_rate);
        row.choices[0] = fmt_music_rate(state.music_rate);

        audio::play_sfx("assets/sounds/change_value.ogg");
        crate::game::profile::set_session_music_rate(state.music_rate);
        audio::set_music_rate(state.music_rate);
        return;
    }

    // Per-player row: Speed Mod numeric
    if id == RowId::SpeedMod {
        let speed_mod = {
            let speed_mod = &mut state.speed_mod[player_idx];
            let (upper, increment) = match speed_mod.mod_type.as_str() {
                "X" => (20.0, 0.05),
                "C" | "M" => (2000.0, 5.0),
                _ => (1.0, 0.1),
            };
            speed_mod.value += delta as f32 * increment;
            speed_mod.value = (speed_mod.value / increment).round() * increment;
            speed_mod.value = speed_mod.value.clamp(increment, upper);
            speed_mod.clone()
        };
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
        audio::play_sfx("assets/sounds/change_value.ogg");
        return;
    }

    let play_style = crate::game::profile::get_session_play_style();
    let persisted_idx = session_persisted_player_idx();
    let should_persist =
        play_style == crate::game::profile::PlayStyle::Versus || player_idx == persisted_idx;
    let persist_side = if player_idx == P1 {
        crate::game::profile::PlayerSide::P1
    } else {
        crate::game::profile::PlayerSide::P2
    };

    let row = &mut state.rows[row_index];
    let num_choices = row.choices.len();
    if num_choices == 0 {
        return;
    }
    let mut visibility_changed = false;

    let current_idx = row.selected_choice_index[player_idx] as isize;
    let new_index = ((current_idx + delta + num_choices as isize) % num_choices as isize) as usize;

    if is_shared {
        row.selected_choice_index = [new_index; PLAYER_SLOTS];
    } else {
        row.selected_choice_index[player_idx] = new_index;
    }

    if id == RowId::TypeOfSpeedMod {
        let new_type = match row.selected_choice_index[player_idx] {
            0 => "X",
            1 => "C",
            2 => "M",
            _ => "C",
        };

        let speed_mod = &mut state.speed_mod[player_idx];
        let old_type = speed_mod.mod_type.clone();
        let old_value = speed_mod.value;
        let reference_bpm = reference_bpm_for_song(
            &state.song,
            resolve_p1_chart(&state.song, &state.chart_steps_index),
        );
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let target_bpm: f32 = match old_type.as_str() {
            "C" | "M" => old_value,
            "X" => (reference_bpm * rate * old_value).round(),
            _ => 600.0,
        };
        let new_value = match new_type {
            "X" => {
                let denom = reference_bpm * rate;
                let raw = if denom.is_finite() && denom > 0.0 {
                    target_bpm / denom
                } else {
                    1.0
                };
                let stepped = round_to_step(raw, 0.05);
                stepped.clamp(0.05, 20.0)
            }
            "C" | "M" => {
                let stepped = round_to_step(target_bpm, 5.0);
                stepped.clamp(5.0, 2000.0)
            }
            _ => 600.0,
        };
        speed_mod.mod_type = new_type.to_string();
        speed_mod.value = new_value;
        let speed_mod = speed_mod.clone();
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
    } else if id == RowId::Turn {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::TurnOption::None,
            1 => crate::game::profile::TurnOption::Mirror,
            2 => crate::game::profile::TurnOption::Left,
            3 => crate::game::profile::TurnOption::Right,
            4 => crate::game::profile::TurnOption::LRMirror,
            5 => crate::game::profile::TurnOption::UDMirror,
            6 => crate::game::profile::TurnOption::Shuffle,
            7 => crate::game::profile::TurnOption::Blender,
            8 => crate::game::profile::TurnOption::Random,
            _ => crate::game::profile::TurnOption::None,
        };
        state.player_profiles[player_idx].turn_option = setting;
        if should_persist {
            crate::game::profile::update_turn_option_for_side(persist_side, setting);
        }
    } else if id == RowId::Accel || id == RowId::Effect || id == RowId::Appearance {
        // Multi-select rows toggled with Start; Left/Right only moves cursor.
    } else if id == RowId::Attacks {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::AttackMode::On,
            1 => crate::game::profile::AttackMode::Random,
            2 => crate::game::profile::AttackMode::Off,
            _ => crate::game::profile::AttackMode::On,
        };
        state.player_profiles[player_idx].attack_mode = setting;
        if should_persist {
            crate::game::profile::update_attack_mode_for_side(persist_side, setting);
        }
    } else if id == RowId::HideLightType {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::HideLightType::NoHideLights,
            1 => crate::game::profile::HideLightType::HideAllLights,
            2 => crate::game::profile::HideLightType::HideMarqueeLights,
            3 => crate::game::profile::HideLightType::HideBassLights,
            _ => crate::game::profile::HideLightType::NoHideLights,
        };
        state.player_profiles[player_idx].hide_light_type = setting;
        if should_persist {
            crate::game::profile::update_hide_light_type_for_side(persist_side, setting);
        }
    } else if id == RowId::RescoreEarlyHits {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].rescore_early_hits = enabled;
        if should_persist {
            crate::game::profile::update_rescore_early_hits_for_side(persist_side, enabled);
        }
    } else if id == RowId::TimingWindows {
        let setting = match row.selected_choice_index[player_idx] {
            1 => crate::game::profile::TimingWindowsOption::WayOffs,
            2 => crate::game::profile::TimingWindowsOption::DecentsAndWayOffs,
            3 => crate::game::profile::TimingWindowsOption::FantasticsAndExcellents,
            _ => crate::game::profile::TimingWindowsOption::None,
        };
        state.player_profiles[player_idx].timing_windows = setting;
        if should_persist {
            crate::game::profile::update_timing_windows_for_side(persist_side, setting);
        }
    } else if id == RowId::CustomBlueFantasticWindow {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].custom_fantastic_window = enabled;
        if should_persist {
            crate::game::profile::update_custom_fantastic_window_for_side(persist_side, enabled);
        }
        visibility_changed = true;
    } else if id == RowId::CustomBlueFantasticWindowMs {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<u8>()
        {
            let ms = crate::game::profile::clamp_custom_fantastic_window_ms(raw);
            state.player_profiles[player_idx].custom_fantastic_window_ms = ms;
            if should_persist {
                crate::game::profile::update_custom_fantastic_window_ms_for_side(persist_side, ms);
            }
        }
    } else if id == RowId::MiniIndicator {
        // Choice indices are fixed by construction order in build_advanced_rows:
        // 0=None, 1=Subtractive, 2=Predictive, 3=Pace, 4=Rival, 5=Pacemaker, 6=StreamProg
        let choice_idx = row.selected_choice_index[player_idx]
            .min(row.choices.len().saturating_sub(1));
        let mini_indicator = match choice_idx {
            1 => crate::game::profile::MiniIndicator::SubtractiveScoring,
            2 => crate::game::profile::MiniIndicator::PredictiveScoring,
            3 => crate::game::profile::MiniIndicator::PaceScoring,
            4 => crate::game::profile::MiniIndicator::RivalScoring,
            5 => crate::game::profile::MiniIndicator::Pacemaker,
            6 => crate::game::profile::MiniIndicator::StreamProg,
            _ => crate::game::profile::MiniIndicator::None,
        };
        let subtractive_scoring =
            mini_indicator == crate::game::profile::MiniIndicator::SubtractiveScoring;
        let pacemaker = mini_indicator == crate::game::profile::MiniIndicator::Pacemaker;
        state.player_profiles[player_idx].mini_indicator = mini_indicator;
        state.player_profiles[player_idx].subtractive_scoring = subtractive_scoring;
        state.player_profiles[player_idx].pacemaker = pacemaker;

        if should_persist {
            let profile_ref = &state.player_profiles[player_idx];
            crate::game::profile::update_mini_indicator_for_side(persist_side, mini_indicator);
            crate::game::profile::update_gameplay_extras_for_side(
                persist_side,
                profile_ref.column_flash_on_miss,
                subtractive_scoring,
                pacemaker,
                profile_ref.nps_graph_at_top,
            );
        }
        visibility_changed = true;
    } else if id == RowId::IndicatorScoreType {
        let choice = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or("ITG");
        let score_type = match choice {
            "EX" => crate::game::profile::MiniIndicatorScoreType::Ex,
            "H.EX" => crate::game::profile::MiniIndicatorScoreType::HardEx,
            _ => crate::game::profile::MiniIndicatorScoreType::Itg,
        };
        state.player_profiles[player_idx].mini_indicator_score_type = score_type;
        if should_persist {
            crate::game::profile::update_mini_indicator_score_type_for_side(
                persist_side,
                score_type,
            );
        }
    } else if id == RowId::DensityGraphBackground {
        let transparent = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].transparent_density_graph_bg = transparent;
        if should_persist {
            crate::game::profile::update_transparent_density_graph_bg_for_side(
                persist_side,
                transparent,
            );
        }
    } else if id == RowId::BackgroundFilter {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::BackgroundFilter::Off,
            1 => crate::game::profile::BackgroundFilter::Dark,
            2 => crate::game::profile::BackgroundFilter::Darker,
            3 => crate::game::profile::BackgroundFilter::Darkest,
            _ => crate::game::profile::BackgroundFilter::Darkest,
        };
        state.player_profiles[player_idx].background_filter = setting;
        if should_persist {
            crate::game::profile::update_background_filter_for_side(persist_side, setting);
        }
    } else if id == RowId::Mini {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx]) {
            let trimmed = choice.trim_end_matches('%');
            if let Ok(val) = trimmed.parse::<i32>() {
                state.player_profiles[player_idx].mini_percent = val;
                if should_persist {
                    crate::game::profile::update_mini_percent_for_side(persist_side, val);
                }
            }
        }
    } else if id == RowId::Perspective {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::Perspective::Overhead,
            1 => crate::game::profile::Perspective::Hallway,
            2 => crate::game::profile::Perspective::Distant,
            3 => crate::game::profile::Perspective::Incoming,
            4 => crate::game::profile::Perspective::Space,
            _ => crate::game::profile::Perspective::Overhead,
        };
        state.player_profiles[player_idx].perspective = setting;
        if should_persist {
            crate::game::profile::update_perspective_for_side(persist_side, setting);
        }
    } else if id == RowId::NoteFieldOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].note_field_offset_x = raw;
            if should_persist {
                crate::game::profile::update_notefield_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::NoteFieldOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].note_field_offset_y = raw;
            if should_persist {
                crate::game::profile::update_notefield_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::JudgmentOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].judgment_offset_x = raw;
            if should_persist {
                crate::game::profile::update_judgment_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::JudgmentOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].judgment_offset_y = raw;
            if should_persist {
                crate::game::profile::update_judgment_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ComboOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].combo_offset_x = raw;
            if should_persist {
                crate::game::profile::update_combo_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ComboOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].combo_offset_y = raw;
            if should_persist {
                crate::game::profile::update_combo_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ErrorBarOffsetX {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].error_bar_offset_x = raw;
            if should_persist {
                crate::game::profile::update_error_bar_offset_x_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::ErrorBarOffsetY {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].error_bar_offset_y = raw;
            if should_persist {
                crate::game::profile::update_error_bar_offset_y_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::VisualDelay {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<i32>()
        {
            state.player_profiles[player_idx].visual_delay_ms = raw;
            if should_persist {
                crate::game::profile::update_visual_delay_ms_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::GlobalOffsetShift {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<i32>()
        {
            state.player_profiles[player_idx].global_offset_shift_ms = raw;
            if should_persist {
                crate::game::profile::update_global_offset_shift_ms_for_side(persist_side, raw);
            }
        }
    } else if id == RowId::JudgmentTilt {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].judgment_tilt = enabled;
        if should_persist {
            crate::game::profile::update_judgment_tilt_for_side(persist_side, enabled);
        }
        visibility_changed = true;
    } else if id == RowId::JudgmentTiltIntensity {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(mult) = choice.parse::<f32>()
        {
            let mult = round_to_step(mult, TILT_INTENSITY_STEP)
                .clamp(TILT_INTENSITY_MIN, TILT_INTENSITY_MAX);
            state.player_profiles[player_idx].tilt_multiplier = mult;
            if should_persist {
                crate::game::profile::update_tilt_multiplier_for_side(persist_side, mult);
            }
        }
    } else if id == RowId::JudgmentBehindArrows {
        let enabled = row.selected_choice_index[player_idx] != 0;
        state.player_profiles[player_idx].judgment_back = enabled;
        if should_persist {
            crate::game::profile::update_judgment_back_for_side(persist_side, enabled);
        }
    } else if id == RowId::LifeMeterType {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::LifeMeterType::Standard,
            1 => crate::game::profile::LifeMeterType::Surround,
            2 => crate::game::profile::LifeMeterType::Vertical,
            _ => crate::game::profile::LifeMeterType::Standard,
        };
        state.player_profiles[player_idx].lifemeter_type = setting;
        if should_persist {
            crate::game::profile::update_lifemeter_type_for_side(persist_side, setting);
        }
    } else if id == RowId::LifeBarOptions {
        // Multi-select row toggled with Start; Left/Right only moves cursor.
    } else if id == RowId::DataVisualizations {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::DataVisualizations::None,
            1 => crate::game::profile::DataVisualizations::TargetScoreGraph,
            2 => crate::game::profile::DataVisualizations::StepStatistics,
            _ => crate::game::profile::DataVisualizations::None,
        };
        state.player_profiles[player_idx].data_visualizations = setting;
        if should_persist {
            crate::game::profile::update_data_visualizations_for_side(persist_side, setting);
        }
        visibility_changed = true;
    } else if id == RowId::TargetScore {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::TargetScoreSetting::CMinus,
            1 => crate::game::profile::TargetScoreSetting::C,
            2 => crate::game::profile::TargetScoreSetting::CPlus,
            3 => crate::game::profile::TargetScoreSetting::BMinus,
            4 => crate::game::profile::TargetScoreSetting::B,
            5 => crate::game::profile::TargetScoreSetting::BPlus,
            6 => crate::game::profile::TargetScoreSetting::AMinus,
            7 => crate::game::profile::TargetScoreSetting::A,
            8 => crate::game::profile::TargetScoreSetting::APlus,
            9 => crate::game::profile::TargetScoreSetting::SMinus,
            10 => crate::game::profile::TargetScoreSetting::S,
            11 => crate::game::profile::TargetScoreSetting::SPlus,
            12 => crate::game::profile::TargetScoreSetting::MachineBest,
            13 => crate::game::profile::TargetScoreSetting::PersonalBest,
            _ => crate::game::profile::TargetScoreSetting::S,
        };
        state.player_profiles[player_idx].target_score = setting;
        if should_persist {
            crate::game::profile::update_target_score_for_side(persist_side, setting);
        }
    } else if id == RowId::OffsetIndicator {
        let enabled = row.selected_choice_index[player_idx] != 0;
        state.player_profiles[player_idx].error_ms_display = enabled;
        if should_persist {
            crate::game::profile::update_error_ms_display_for_side(persist_side, enabled);
        }
    } else if id == RowId::ErrorBar {
        // Multi-select row toggled with Start; Left/Right only moves cursor.
    } else if id == RowId::ErrorBarTrim {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::ErrorBarTrim::Off,
            1 => crate::game::profile::ErrorBarTrim::Fantastic,
            2 => crate::game::profile::ErrorBarTrim::Excellent,
            3 => crate::game::profile::ErrorBarTrim::Great,
            _ => crate::game::profile::ErrorBarTrim::Off,
        };
        state.player_profiles[player_idx].error_bar_trim = setting;
        if should_persist {
            crate::game::profile::update_error_bar_trim_for_side(persist_side, setting);
        }
    } else if id == RowId::MeasureCounter {
        visibility_changed = true;
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::MeasureCounter::None,
            1 => crate::game::profile::MeasureCounter::Eighth,
            2 => crate::game::profile::MeasureCounter::Twelfth,
            3 => crate::game::profile::MeasureCounter::Sixteenth,
            4 => crate::game::profile::MeasureCounter::TwentyFourth,
            5 => crate::game::profile::MeasureCounter::ThirtySecond,
            _ => crate::game::profile::MeasureCounter::None,
        };
        state.player_profiles[player_idx].measure_counter = setting;
        if should_persist {
            crate::game::profile::update_measure_counter_for_side(persist_side, setting);
        }
    } else if id == RowId::MeasureCounterLookahead {
        let lookahead = (row.selected_choice_index[player_idx] as u8).min(4);
        state.player_profiles[player_idx].measure_counter_lookahead = lookahead;
        if should_persist {
            crate::game::profile::update_measure_counter_lookahead_for_side(
                persist_side,
                lookahead,
            );
        }
    } else if id == RowId::MeasureLines {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::MeasureLines::Off,
            1 => crate::game::profile::MeasureLines::Measure,
            2 => crate::game::profile::MeasureLines::Quarter,
            3 => crate::game::profile::MeasureLines::Eighth,
            _ => crate::game::profile::MeasureLines::Off,
        };
        state.player_profiles[player_idx].measure_lines = setting;
        if should_persist {
            crate::game::profile::update_measure_lines_for_side(persist_side, setting);
        }
    } else if id == RowId::JudgmentFont {
        let setting = assets::judgment_texture_choices()
            .get(row.selected_choice_index[player_idx])
            .map(|choice| crate::game::profile::JudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].judgment_graphic = setting;
        if should_persist {
            crate::game::profile::update_judgment_graphic_for_side(
                persist_side,
                state.player_profiles[player_idx].judgment_graphic.clone(),
            );
        }
        visibility_changed = true;
    } else if id == RowId::ComboFont {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::ComboFont::Wendy,
            1 => crate::game::profile::ComboFont::ArialRounded,
            2 => crate::game::profile::ComboFont::Asap,
            3 => crate::game::profile::ComboFont::BebasNeue,
            4 => crate::game::profile::ComboFont::SourceCode,
            5 => crate::game::profile::ComboFont::Work,
            6 => crate::game::profile::ComboFont::WendyCursed,
            7 => crate::game::profile::ComboFont::None,
            _ => crate::game::profile::ComboFont::Wendy,
        };
        state.player_profiles[player_idx].combo_font = setting;
        if should_persist {
            crate::game::profile::update_combo_font_for_side(persist_side, setting);
        }
        visibility_changed = true;
    } else if id == RowId::ComboColors {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::ComboColors::Glow,
            1 => crate::game::profile::ComboColors::Solid,
            2 => crate::game::profile::ComboColors::Rainbow,
            3 => crate::game::profile::ComboColors::RainbowScroll,
            4 => crate::game::profile::ComboColors::None,
            _ => crate::game::profile::ComboColors::Glow,
        };
        state.player_profiles[player_idx].combo_colors = setting;
        if should_persist {
            crate::game::profile::update_combo_colors_for_side(persist_side, setting);
        }
    } else if id == RowId::ComboColorMode {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::ComboMode::FullCombo,
            1 => crate::game::profile::ComboMode::CurrentCombo,
            _ => crate::game::profile::ComboMode::FullCombo,
        };
        state.player_profiles[player_idx].combo_mode = setting;
        if should_persist {
            crate::game::profile::update_combo_mode_for_side(persist_side, setting);
        }
    } else if id == RowId::CarryCombo {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].carry_combo_between_songs = enabled;
        if should_persist {
            crate::game::profile::update_carry_combo_between_songs_for_side(persist_side, enabled);
        }
    } else if id == RowId::HoldJudgment {
        let setting = assets::hold_judgment_texture_choices()
            .get(row.selected_choice_index[player_idx])
            .map(|choice| crate::game::profile::HoldJudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].hold_judgment_graphic = setting;
        if should_persist {
            crate::game::profile::update_hold_judgment_graphic_for_side(
                persist_side,
                state.player_profiles[player_idx]
                    .hold_judgment_graphic
                    .clone(),
            );
        }
    } else if id == RowId::NoteSkin {
        let setting_name = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .cloned()
            .unwrap_or_else(|| crate::game::profile::NoteSkin::DEFAULT_NAME.to_string());
        let setting = crate::game::profile::NoteSkin::new(&setting_name);
        state.player_profiles[player_idx].noteskin = setting.clone();
        if should_persist {
            crate::game::profile::update_noteskin_for_side(persist_side, setting.clone());
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::MineSkin {
        let match_noteskin = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
        let selected = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or(match_noteskin.as_ref());
        let setting = if selected == match_noteskin.as_ref() {
            None
        } else {
            Some(crate::game::profile::NoteSkin::new(selected))
        };
        state.player_profiles[player_idx]
            .mine_noteskin
            .clone_from(&setting);
        if should_persist {
            crate::game::profile::update_mine_noteskin_for_side(persist_side, setting);
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::ReceptorSkin {
        let match_noteskin = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
        let selected = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or(match_noteskin.as_ref());
        let setting = if selected == match_noteskin.as_ref() {
            None
        } else {
            Some(crate::game::profile::NoteSkin::new(selected))
        };
        state.player_profiles[player_idx]
            .receptor_noteskin
            .clone_from(&setting);
        if should_persist {
            crate::game::profile::update_receptor_noteskin_for_side(persist_side, setting);
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::TapExplosionSkin {
        let match_noteskin = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
        let no_tap_explosion = tr("PlayerOptions", NO_TAP_EXPLOSION_LABEL);
        let selected = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(String::as_str)
            .unwrap_or(match_noteskin.as_ref());
        let setting = if selected == match_noteskin.as_ref() {
            None
        } else if selected == no_tap_explosion.as_ref() {
            Some(crate::game::profile::NoteSkin::none_choice())
        } else {
            Some(crate::game::profile::NoteSkin::new(selected))
        };
        state.player_profiles[player_idx]
            .tap_explosion_noteskin
            .clone_from(&setting);
        if should_persist {
            crate::game::profile::update_tap_explosion_noteskin_for_side(persist_side, setting);
        }
        sync_noteskin_previews_for_player(state, player_idx);
    } else if id == RowId::Stepchart
        && let Some(diff_indices) = &row.choice_difficulty_indices
        && let Some(&difficulty_idx) = diff_indices.get(row.selected_choice_index[player_idx])
    {
        state.chart_steps_index[player_idx] = difficulty_idx;
        if difficulty_idx < crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() {
            state.chart_difficulty_index[player_idx] = difficulty_idx;
        }
    }

    if visibility_changed {
        sync_selected_rows_with_visibility(state, session_active_players());
    }
    sync_inline_intent_from_row(state, asset_manager, player_idx, row_index);
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub fn apply_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) {
    if state.rows.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.selected_row[idx].min(state.rows.len().saturating_sub(1));
    if let Some(row) = state.rows.get(row_idx)
        && row_supports_inline_nav(row)
    {
        if state.current_pane == OptionsPane::Main || row_selects_on_focus_move(row.id) {
            change_choice_for_player(state, asset_manager, idx, delta);
            return;
        }
        if move_inline_focus(state, asset_manager, idx, delta) {
            audio::play_sfx("assets/sounds/change_value.ogg");
        }
        return;
    }
    change_choice_for_player(state, asset_manager, player_idx, delta);
}

// Keyboard input is handled centrally via the virtual dispatcher in app
pub fn update(state: &mut State, dt: f32, asset_manager: &AssetManager) -> Option<ScreenAction> {
    // Keep options-screen noteskin previews on a stable clock.
    // ITG/SL preview actors are not driven by selected chart BPM, so tying this to song BPM
    // makes beat-based skins (e.g. cel) appear too fast/slow depending on the selected chart.
    const PREVIEW_BPM: f32 = 120.0;
    state.preview_time += dt;
    state.preview_beat += dt * (PREVIEW_BPM / 60.0);
    let active = session_active_players();
    let now = Instant::now();
    let arcade_style = crate::config::get().arcade_options_navigation;
    let mut pending_action: Option<ScreenAction> = None;
    sync_selected_rows_with_visibility(state, active);

    // Hold-to-scroll per player.
    for player_idx in active_player_indices(active) {
        let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
            state.nav_key_held_direction[player_idx],
            state.nav_key_held_since[player_idx],
            state.nav_key_last_scrolled_at[player_idx],
        ) else {
            continue;
        };
        if now.duration_since(held_since) <= NAV_INITIAL_HOLD_DELAY
            || now.duration_since(last_scrolled_at) < NAV_REPEAT_SCROLL_INTERVAL
        {
            continue;
        }

        if state.rows.is_empty() {
            continue;
        }
        match direction {
            NavDirection::Up => {
                move_selection_vertical(state, asset_manager, active, player_idx, NavDirection::Up);
            }
            NavDirection::Down => {
                move_selection_vertical(
                    state,
                    asset_manager,
                    active,
                    player_idx,
                    NavDirection::Down,
                );
            }
            NavDirection::Left => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, -1) {
                    apply_choice_delta(state, asset_manager, player_idx, -1);
                }
            }
            NavDirection::Right => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, 1) {
                    apply_choice_delta(state, asset_manager, player_idx, 1);
                }
            }
        }
        state.nav_key_last_scrolled_at[player_idx] = Some(now);
    }

    if arcade_style {
        for player_idx in active_player_indices(active) {
            let action = repeat_held_arcade_start(state, asset_manager, active, player_idx, now);
            if pending_action.is_none() {
                pending_action = action;
            }
        }
    }

    match state.pane_transition {
        PaneTransition::None => {}
        PaneTransition::FadingOut { target, t } => {
            if PANE_FADE_SECONDS <= 0.0 {
                apply_pane(state, target);
                state.pane_transition = PaneTransition::None;
            } else {
                let next_t = (t + dt / PANE_FADE_SECONDS).min(1.0);
                if next_t >= 1.0 {
                    apply_pane(state, target);
                    state.pane_transition = PaneTransition::FadingIn { t: 0.0 };
                } else {
                    state.pane_transition = PaneTransition::FadingOut { target, t: next_t };
                }
            }
        }
        PaneTransition::FadingIn { t } => {
            if PANE_FADE_SECONDS <= 0.0 {
                state.pane_transition = PaneTransition::None;
            } else {
                let next_t = (t + dt / PANE_FADE_SECONDS).min(1.0);
                if next_t >= 1.0 {
                    state.pane_transition = PaneTransition::None;
                } else {
                    state.pane_transition = PaneTransition::FadingIn { t: next_t };
                }
            }
        }
    }

    // Advance help reveal timers.
    for player_idx in active_player_indices(active) {
        state.help_anim_time[player_idx] += dt;
    }

    // If either player is on the Combo Font row, tick the preview combo once per second.
    let mut combo_row_active = false;
    for player_idx in active_player_indices(active) {
        if let Some(row) = state.rows.get(state.selected_row[player_idx])
            && row.id == RowId::ComboFont
        {
            combo_row_active = true;
            break;
        }
    }
    if combo_row_active {
        state.combo_preview_elapsed += dt;
        if state.combo_preview_elapsed >= 1.0 {
            state.combo_preview_elapsed -= 1.0;
            state.combo_preview_count = state.combo_preview_count.saturating_add(1);
        }
    } else {
        state.combo_preview_elapsed = 0.0;
    }

    // Row frame tweening: mimic ScreenOptions::PositionRows() + OptionRow::SetDestination()
    // so rows slide smoothly as the visible window scrolls.
    let total_rows = state.rows.len();
    let (first_row_center_y, row_step) = row_layout_params();
    if total_rows == 0 {
        state.row_tweens.clear();
    } else if state.row_tweens.len() != total_rows {
        state.row_tweens = init_row_tweens(
            &state.rows,
            state.selected_row,
            active,
            state.hide_active_mask,
            state.error_bar_active_mask,
            state.allow_per_player_global_offsets,
        );
    } else {
        let visibility = row_visibility(
            &state.rows,
            active,
            state.hide_active_mask,
            state.error_bar_active_mask,
            state.allow_per_player_global_offsets,
        );
        let visible_rows = count_visible_rows(&state.rows, visibility);
        if visible_rows == 0 {
            let y = first_row_center_y - row_step * 0.5;
            for tw in &mut state.row_tweens {
                let cur_y = tw.y();
                let cur_a = tw.a();
                if (y - tw.to_y).abs() > 0.01 || tw.to_a != 0.0 {
                    tw.from_y = cur_y;
                    tw.from_a = cur_a;
                    tw.to_y = y;
                    tw.to_a = 0.0;
                    tw.t = 0.0;
                }
                if tw.t < 1.0 {
                    if ROW_TWEEN_SECONDS > 0.0 {
                        tw.t = (tw.t + dt / ROW_TWEEN_SECONDS).min(1.0);
                    } else {
                        tw.t = 1.0;
                    }
                }
            }
        } else {
            let selected_visible = std::array::from_fn(|player_idx| {
                let row_idx = state.selected_row[player_idx].min(total_rows.saturating_sub(1));
                row_to_visible_index(&state.rows, row_idx, visibility).unwrap_or(0)
            });
            let w = compute_row_window(visible_rows, selected_visible, active);
            let mid_pos = (VISIBLE_ROWS as f32) * 0.5 - 0.5;
            let bottom_pos = (VISIBLE_ROWS as f32) - 0.5;
            let measure_counter_anchor_visible_idx =
                parent_anchor_visible_index(&state.rows, RowId::MeasureCounter, visibility);
            let judgment_tilt_anchor_visible_idx =
                parent_anchor_visible_index(&state.rows, RowId::JudgmentTilt, visibility);
            let error_bar_anchor_visible_idx =
                parent_anchor_visible_index(&state.rows, RowId::ErrorBar, visibility);
            let hide_anchor_visible_idx =
                parent_anchor_visible_index(&state.rows, RowId::Hide, visibility);
            let mut visible_idx = 0i32;
            for i in 0..total_rows {
                let visible = is_row_visible(&state.rows, i, visibility);
                let (f_pos, hidden) = if visible {
                    let ii = visible_idx;
                    visible_idx += 1;
                    f_pos_for_visible_idx(ii, w, mid_pos, bottom_pos)
                } else {
                    let anchor = state.rows.get(i).and_then(|row| {
                        match conditional_row_parent(row.id) {
                            Some(RowId::MeasureCounter) => measure_counter_anchor_visible_idx,
                            Some(RowId::JudgmentTilt) => judgment_tilt_anchor_visible_idx,
                            Some(RowId::ErrorBar) => error_bar_anchor_visible_idx,
                            Some(RowId::Hide) => hide_anchor_visible_idx,
                            _ => None,
                        }
                    });
                    if let Some(anchor_idx) = anchor {
                        let (anchor_f_pos, _) =
                            f_pos_for_visible_idx(anchor_idx, w, mid_pos, bottom_pos);
                        (anchor_f_pos, true)
                    } else {
                        (-0.5, true)
                    }
                };

                let dest_y = first_row_center_y + row_step * f_pos;
                let dest_a = if hidden { 0.0 } else { 1.0 };

                let tw = &mut state.row_tweens[i];
                let cur_y = tw.y();
                let cur_a = tw.a();
                if (dest_y - tw.to_y).abs() > 0.01 || dest_a != tw.to_a {
                    tw.from_y = cur_y;
                    tw.from_a = cur_a;
                    tw.to_y = dest_y;
                    tw.to_a = dest_a;
                    tw.t = 0.0;
                }
                if tw.t < 1.0 {
                    if ROW_TWEEN_SECONDS > 0.0 {
                        tw.t = (tw.t + dt / ROW_TWEEN_SECONDS).min(1.0);
                    } else {
                        tw.t = 1.0;
                    }
                }
            }
        }
    }

    // Reset help reveal and play SFX when a player changes rows.
    for player_idx in active_player_indices(active) {
        if state.selected_row[player_idx] == state.prev_selected_row[player_idx] {
            continue;
        }
        match state.nav_key_held_direction[player_idx] {
            Some(NavDirection::Up) => audio::play_sfx("assets/sounds/prev_row.ogg"),
            Some(NavDirection::Down) => audio::play_sfx("assets/sounds/next_row.ogg"),
            _ => audio::play_sfx("assets/sounds/next_row.ogg"),
        }

        state.help_anim_time[player_idx] = 0.0;
        state.prev_selected_row[player_idx] = state.selected_row[player_idx];
    }

    // Retarget cursor tween destinations to match current selection and row destinations.
    for player_idx in active_player_indices(active) {
        let Some((to_x, to_y, to_w, to_h)) =
            cursor_dest_for_player(state, asset_manager, player_idx)
        else {
            continue;
        };

        let needs_cursor_init = !state.cursor_initialized[player_idx];
        if needs_cursor_init {
            state.cursor_initialized[player_idx] = true;
            state.cursor_from_x[player_idx] = to_x;
            state.cursor_from_y[player_idx] = to_y;
            state.cursor_from_w[player_idx] = to_w;
            state.cursor_from_h[player_idx] = to_h;
            state.cursor_to_x[player_idx] = to_x;
            state.cursor_to_y[player_idx] = to_y;
            state.cursor_to_w[player_idx] = to_w;
            state.cursor_to_h[player_idx] = to_h;
            state.cursor_t[player_idx] = 1.0;
        } else {
            let dx = (to_x - state.cursor_to_x[player_idx]).abs();
            let dy = (to_y - state.cursor_to_y[player_idx]).abs();
            let dw = (to_w - state.cursor_to_w[player_idx]).abs();
            let dh = (to_h - state.cursor_to_h[player_idx]).abs();
            if dx > 0.01 || dy > 0.01 || dw > 0.01 || dh > 0.01 {
                let t = state.cursor_t[player_idx].clamp(0.0, 1.0);
                let cur_x = (state.cursor_to_x[player_idx] - state.cursor_from_x[player_idx])
                    .mul_add(t, state.cursor_from_x[player_idx]);
                let cur_y = (state.cursor_to_y[player_idx] - state.cursor_from_y[player_idx])
                    .mul_add(t, state.cursor_from_y[player_idx]);
                let cur_w = (state.cursor_to_w[player_idx] - state.cursor_from_w[player_idx])
                    .mul_add(t, state.cursor_from_w[player_idx]);
                let cur_h = (state.cursor_to_h[player_idx] - state.cursor_from_h[player_idx])
                    .mul_add(t, state.cursor_from_h[player_idx]);

                state.cursor_from_x[player_idx] = cur_x;
                state.cursor_from_y[player_idx] = cur_y;
                state.cursor_from_w[player_idx] = cur_w;
                state.cursor_from_h[player_idx] = cur_h;
                state.cursor_to_x[player_idx] = to_x;
                state.cursor_to_y[player_idx] = to_y;
                state.cursor_to_w[player_idx] = to_w;
                state.cursor_to_h[player_idx] = to_h;
                state.cursor_t[player_idx] = 0.0;
            }
        }
    }

    // Advance cursor tween.
    for player_idx in [P1, P2] {
        if state.cursor_t[player_idx] < 1.0 {
            if CURSOR_TWEEN_SECONDS > 0.0 {
                state.cursor_t[player_idx] =
                    (state.cursor_t[player_idx] + dt / CURSOR_TWEEN_SECONDS).min(1.0);
            } else {
                state.cursor_t[player_idx] = 1.0;
            }
        }
    }

    pending_action
}

// Helpers for hold-to-scroll controlled by the app dispatcher
pub fn on_nav_press(state: &mut State, player_idx: usize, dir: NavDirection) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.scroll_focus_player = idx;
    state.nav_key_held_direction[idx] = Some(dir);
    state.nav_key_held_since[idx] = Some(Instant::now());
    state.nav_key_last_scrolled_at[idx] = Some(Instant::now());
}

pub fn on_nav_release(state: &mut State, player_idx: usize, dir: NavDirection) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    if state.nav_key_held_direction[idx] == Some(dir) {
        state.nav_key_held_direction[idx] = None;
        state.nav_key_held_since[idx] = None;
        state.nav_key_last_scrolled_at[idx] = None;
    }
}

#[inline(always)]
fn on_start_press(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let now = Instant::now();
    state.start_held_since[idx] = Some(now);
    state.start_last_triggered_at[idx] = Some(now);
}

#[inline(always)]
fn clear_start_hold(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.start_held_since[idx] = None;
    state.start_last_triggered_at[idx] = None;
}

fn toggle_scroll_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Scroll {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.scroll_active_mask[idx] & bit) != 0 {
        state.scroll_active_mask[idx] &= !bit;
    } else {
        state.scroll_active_mask[idx] |= bit;
    }

    // Rebuild the ScrollOption bitmask from the active choices.
    use crate::game::profile::ScrollOption;
    let mut setting = ScrollOption::Normal;
    if state.scroll_active_mask[idx] != 0 {
        if (state.scroll_active_mask[idx] & (1u8 << 0)) != 0 {
            setting = setting.union(ScrollOption::Reverse);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 1)) != 0 {
            setting = setting.union(ScrollOption::Split);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 2)) != 0 {
            setting = setting.union(ScrollOption::Alternate);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 3)) != 0 {
            setting = setting.union(ScrollOption::Cross);
        }
        if (state.scroll_active_mask[idx] & (1u8 << 4)) != 0 {
            setting = setting.union(ScrollOption::Centered);
        }
    }
    state.player_profiles[idx].scroll_option = setting;
    state.player_profiles[idx].reverse_scroll = setting.contains(ScrollOption::Reverse);
    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_scroll_option_for_side(side, setting);
    }
    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_hide_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Hide {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.hide_active_mask[idx] & bit) != 0 {
        state.hide_active_mask[idx] &= !bit;
    } else {
        state.hide_active_mask[idx] |= bit;
    }

    let hide_targets = (state.hide_active_mask[idx] & (1u8 << 0)) != 0;
    let hide_song_bg = (state.hide_active_mask[idx] & (1u8 << 1)) != 0;
    let hide_combo = (state.hide_active_mask[idx] & (1u8 << 2)) != 0;
    let hide_lifebar = (state.hide_active_mask[idx] & (1u8 << 3)) != 0;
    let hide_score = (state.hide_active_mask[idx] & (1u8 << 4)) != 0;
    let hide_danger = (state.hide_active_mask[idx] & (1u8 << 5)) != 0;
    let hide_combo_explosions = (state.hide_active_mask[idx] & (1u8 << 6)) != 0;

    state.player_profiles[idx].hide_targets = hide_targets;
    state.player_profiles[idx].hide_song_bg = hide_song_bg;
    state.player_profiles[idx].hide_combo = hide_combo;
    state.player_profiles[idx].hide_lifebar = hide_lifebar;
    state.player_profiles[idx].hide_score = hide_score;
    state.player_profiles[idx].hide_danger = hide_danger;
    state.player_profiles[idx].hide_combo_explosions = hide_combo_explosions;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_hide_options_for_side(
            side,
            hide_targets,
            hide_song_bg,
            hide_combo,
            hide_lifebar,
            hide_score,
            hide_danger,
            hide_combo_explosions,
        );
    }

    sync_selected_rows_with_visibility(state, session_active_players());
    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_insert_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Insert {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 7 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.insert_active_mask[idx] & bit) != 0 {
        state.insert_active_mask[idx] &= !bit;
    } else {
        state.insert_active_mask[idx] |= bit;
    }
    state.insert_active_mask[idx] =
        crate::game::profile::normalize_insert_mask(state.insert_active_mask[idx]);
    let mask = state.insert_active_mask[idx];
    state.player_profiles[idx].insert_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_insert_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_remove_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Remove {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.remove_active_mask[idx] & bit) != 0 {
        state.remove_active_mask[idx] &= !bit;
    } else {
        state.remove_active_mask[idx] |= bit;
    }
    state.remove_active_mask[idx] =
        crate::game::profile::normalize_remove_mask(state.remove_active_mask[idx]);
    let mask = state.remove_active_mask[idx];
    state.player_profiles[idx].remove_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_remove_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_holds_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Holds {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < state.rows[row_index].choices.len().min(u8::BITS as usize) {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.holds_active_mask[idx] & bit) != 0 {
        state.holds_active_mask[idx] &= !bit;
    } else {
        state.holds_active_mask[idx] |= bit;
    }
    state.holds_active_mask[idx] =
        crate::game::profile::normalize_holds_mask(state.holds_active_mask[idx]);
    let mask = state.holds_active_mask[idx];
    state.player_profiles[idx].holds_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_holds_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_accel_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Accel {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < state.rows[row_index].choices.len().min(u8::BITS as usize) {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.accel_effects_active_mask[idx] & bit) != 0 {
        state.accel_effects_active_mask[idx] &= !bit;
    } else {
        state.accel_effects_active_mask[idx] |= bit;
    }
    state.accel_effects_active_mask[idx] =
        crate::game::profile::normalize_accel_effects_mask(state.accel_effects_active_mask[idx]);
    let mask = state.accel_effects_active_mask[idx];
    state.player_profiles[idx].accel_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_accel_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_visual_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Effect {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 10 {
        1u16 << (choice_index as u16)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.visual_effects_active_mask[idx] & bit) != 0 {
        state.visual_effects_active_mask[idx] &= !bit;
    } else {
        state.visual_effects_active_mask[idx] |= bit;
    }
    state.visual_effects_active_mask[idx] =
        crate::game::profile::normalize_visual_effects_mask(state.visual_effects_active_mask[idx]);
    let mask = state.visual_effects_active_mask[idx];
    state.player_profiles[idx].visual_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_visual_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_appearance_effects_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::Appearance {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < state.rows[row_index].choices.len().min(u8::BITS as usize) {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.appearance_effects_active_mask[idx] & bit) != 0 {
        state.appearance_effects_active_mask[idx] &= !bit;
    } else {
        state.appearance_effects_active_mask[idx] |= bit;
    }
    state.appearance_effects_active_mask[idx] =
        crate::game::profile::normalize_appearance_effects_mask(
            state.appearance_effects_active_mask[idx],
        );
    let mask = state.appearance_effects_active_mask[idx];
    state.player_profiles[idx].appearance_effects_active_mask = mask;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_appearance_effects_mask_for_side(side, mask);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_life_bar_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::LifeBarOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 3 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.life_bar_options_active_mask[idx] & bit) != 0 {
        state.life_bar_options_active_mask[idx] &= !bit;
    } else {
        state.life_bar_options_active_mask[idx] |= bit;
    }

    let rainbow_max = (state.life_bar_options_active_mask[idx] & (1u8 << 0)) != 0;
    let responsive_colors = (state.life_bar_options_active_mask[idx] & (1u8 << 1)) != 0;
    let show_life_percent = (state.life_bar_options_active_mask[idx] & (1u8 << 2)) != 0;
    state.player_profiles[idx].rainbow_max = rainbow_max;
    state.player_profiles[idx].responsive_colors = responsive_colors;
    state.player_profiles[idx].show_life_percent = show_life_percent;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_rainbow_max_for_side(side, rainbow_max);
        crate::game::profile::update_responsive_colors_for_side(side, responsive_colors);
        crate::game::profile::update_show_life_percent_for_side(side, show_life_percent);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_fa_plus_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::FAPlusOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < state.rows[row_index].choices.len().min(u8::BITS as usize) {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.fa_plus_active_mask[idx] & bit) != 0 {
        state.fa_plus_active_mask[idx] &= !bit;
    } else {
        state.fa_plus_active_mask[idx] |= bit;
    }

    let window_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 0)) != 0;
    let ex_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 1)) != 0;
    let hard_ex_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 2)) != 0;
    let pane_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 3)) != 0;
    let ten_ms_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 4)) != 0;
    let split_15_10ms_enabled = (state.fa_plus_active_mask[idx] & (1u8 << 5)) != 0;
    state.player_profiles[idx].show_fa_plus_window = window_enabled;
    state.player_profiles[idx].show_ex_score = ex_enabled;
    state.player_profiles[idx].show_hard_ex_score = hard_ex_enabled;
    state.player_profiles[idx].show_fa_plus_pane = pane_enabled;
    state.player_profiles[idx].fa_plus_10ms_blue_window = ten_ms_enabled;
    state.player_profiles[idx].split_15_10ms = split_15_10ms_enabled;
    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_show_fa_plus_window_for_side(side, window_enabled);
        crate::game::profile::update_show_ex_score_for_side(side, ex_enabled);
        crate::game::profile::update_show_hard_ex_score_for_side(side, hard_ex_enabled);
        crate::game::profile::update_show_fa_plus_pane_for_side(side, pane_enabled);
        crate::game::profile::update_fa_plus_10ms_blue_window_for_side(side, ten_ms_enabled);
        crate::game::profile::update_split_15_10ms_for_side(side, split_15_10ms_enabled);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_results_extras_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::ResultsExtras {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 1 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.results_extras_active_mask[idx] & bit) != 0 {
        state.results_extras_active_mask[idx] &= !bit;
    } else {
        state.results_extras_active_mask[idx] |= bit;
    }

    let track_early_judgments = (state.results_extras_active_mask[idx] & (1u8 << 0)) != 0;
    state.player_profiles[idx].track_early_judgments = track_early_judgments;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_track_early_judgments_for_side(side, track_early_judgments);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_error_bar_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::ErrorBar {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 5 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.error_bar_active_mask[idx] & bit) != 0 {
        state.error_bar_active_mask[idx] &= !bit;
    } else {
        state.error_bar_active_mask[idx] |= bit;
    }
    state.error_bar_active_mask[idx] =
        crate::game::profile::normalize_error_bar_mask(state.error_bar_active_mask[idx]);
    let mask = state.error_bar_active_mask[idx];
    state.player_profiles[idx].error_bar_active_mask = mask;
    state.player_profiles[idx].error_bar = crate::game::profile::error_bar_style_from_mask(mask);
    state.player_profiles[idx].error_bar_text =
        crate::game::profile::error_bar_text_from_mask(mask);

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_error_bar_mask_for_side(side, mask);
    }

    sync_selected_rows_with_visibility(state, session_active_players());
    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_error_bar_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::ErrorBarOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 2 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.error_bar_options_active_mask[idx] & bit) != 0 {
        state.error_bar_options_active_mask[idx] &= !bit;
    } else {
        state.error_bar_options_active_mask[idx] |= bit;
    }

    let up = (state.error_bar_options_active_mask[idx] & (1u8 << 0)) != 0;
    let multi_tick = (state.error_bar_options_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].error_bar_up = up;
    state.player_profiles[idx].error_bar_multi_tick = multi_tick;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_error_bar_options_for_side(side, up, multi_tick);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_measure_counter_options_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::MeasureCounterOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 5 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.measure_counter_options_active_mask[idx] & bit) != 0 {
        state.measure_counter_options_active_mask[idx] &= !bit;
    } else {
        state.measure_counter_options_active_mask[idx] |= bit;
    }

    let left = (state.measure_counter_options_active_mask[idx] & (1u8 << 0)) != 0;
    let up = (state.measure_counter_options_active_mask[idx] & (1u8 << 1)) != 0;
    let vert = (state.measure_counter_options_active_mask[idx] & (1u8 << 2)) != 0;
    let broken_run = (state.measure_counter_options_active_mask[idx] & (1u8 << 3)) != 0;
    let run_timer = (state.measure_counter_options_active_mask[idx] & (1u8 << 4)) != 0;

    state.player_profiles[idx].measure_counter_left = left;
    state.player_profiles[idx].measure_counter_up = up;
    state.player_profiles[idx].measure_counter_vert = vert;
    state.player_profiles[idx].broken_run = broken_run;
    state.player_profiles[idx].run_timer = run_timer;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_measure_counter_options_for_side(
            side, left, up, vert, broken_run, run_timer,
        );
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_early_dw_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::EarlyDecentWayOffOptions {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = if choice_index < 2 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    if (state.early_dw_active_mask[idx] & bit) != 0 {
        state.early_dw_active_mask[idx] &= !bit;
    } else {
        state.early_dw_active_mask[idx] |= bit;
    }

    let hide_judgments = (state.early_dw_active_mask[idx] & (1u8 << 0)) != 0;
    let hide_flash = (state.early_dw_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].hide_early_dw_judgments = hide_judgments;
    state.player_profiles[idx].hide_early_dw_flash = hide_flash;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_early_dw_options_for_side(side, hide_judgments, hide_flash);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_gameplay_extras_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::GameplayExtras {
            return;
        }
    } else {
        return;
    }

    let row = &state.rows[row_index];
    let choice_index = row.selected_choice_index[idx];
    let ge_flash = tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss");
    let ge_density = tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop");
    let ge_column_cues = tr("PlayerOptions", "GameplayExtrasColumnCues");
    let ge_scorebox = tr("PlayerOptions", "GameplayExtrasDisplayScorebox");
    let bit = row
        .choices
        .get(choice_index)
        .map(|choice| {
            let choice_str = choice.as_str();
            if choice_str == ge_flash.as_ref() {
                1u8 << 0
            } else if choice_str == ge_density.as_ref() {
                1u8 << 1
            } else if choice_str == ge_column_cues.as_ref() {
                1u8 << 2
            } else if choice_str == ge_scorebox.as_ref() {
                1u8 << 3
            } else {
                0
            }
        })
        .unwrap_or(0);
    if bit == 0 {
        return;
    }

    if (state.gameplay_extras_active_mask[idx] & bit) != 0 {
        state.gameplay_extras_active_mask[idx] &= !bit;
    } else {
        state.gameplay_extras_active_mask[idx] |= bit;
    }

    let column_flash_on_miss = (state.gameplay_extras_active_mask[idx] & (1u8 << 0)) != 0;
    let nps_graph_at_top = (state.gameplay_extras_active_mask[idx] & (1u8 << 1)) != 0;
    let column_cues = (state.gameplay_extras_active_mask[idx] & (1u8 << 2)) != 0;
    let display_scorebox = (state.gameplay_extras_active_mask[idx] & (1u8 << 3)) != 0;
    let subtractive_scoring = state.player_profiles[idx].subtractive_scoring;
    let pacemaker = state.player_profiles[idx].pacemaker;

    state.player_profiles[idx].column_flash_on_miss = column_flash_on_miss;
    state.player_profiles[idx].nps_graph_at_top = nps_graph_at_top;
    state.player_profiles[idx].column_cues = column_cues;
    state.player_profiles[idx].display_scorebox = display_scorebox;
    state.gameplay_extras_more_active_mask[idx] =
        (column_cues as u8) | ((display_scorebox as u8) << 1);

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_gameplay_extras_for_side(
            side,
            column_flash_on_miss,
            subtractive_scoring,
            pacemaker,
            nps_graph_at_top,
        );
        crate::game::profile::update_column_cues_for_side(side, column_cues);
        crate::game::profile::update_display_scorebox_for_side(side, display_scorebox);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_gameplay_extras_more_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.id != RowId::GameplayExtrasMore {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index[idx];
    let bit = match choice_index {
        0 => 1u8 << 0, // Column Cues
        1 => 1u8 << 1, // Display Scorebox
        _ => return,
    };

    if (state.gameplay_extras_more_active_mask[idx] & bit) != 0 {
        state.gameplay_extras_more_active_mask[idx] &= !bit;
    } else {
        state.gameplay_extras_more_active_mask[idx] |= bit;
    }

    let column_cues = (state.gameplay_extras_more_active_mask[idx] & (1u8 << 0)) != 0;
    let display_scorebox = (state.gameplay_extras_more_active_mask[idx] & (1u8 << 1)) != 0;
    state.player_profiles[idx].column_cues = column_cues;
    state.player_profiles[idx].display_scorebox = display_scorebox;

    let play_style = crate::game::profile::get_session_play_style();
    let should_persist = play_style == crate::game::profile::PlayStyle::Versus
        || idx == session_persisted_player_idx();
    if should_persist {
        let side = if idx == P1 {
            crate::game::profile::PlayerSide::P1
        } else {
            crate::game::profile::PlayerSide::P2
        };
        crate::game::profile::update_column_cues_for_side(side, column_cues);
        crate::game::profile::update_display_scorebox_for_side(side, display_scorebox);
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn apply_pane(state: &mut State, pane: OptionsPane) {
    let speed_mod = &state.speed_mod[session_persisted_player_idx()];
    let mut rows = build_rows(
        &state.song,
        speed_mod,
        state.chart_steps_index,
        state.chart_difficulty_index,
        state.music_rate,
        pane,
        &state.noteskin_names,
        state.return_screen,
        state.fixed_stepchart.as_ref(),
    );
    let (
        scroll_active_mask_p1,
        hide_active_mask_p1,
        insert_active_mask_p1,
        remove_active_mask_p1,
        holds_active_mask_p1,
        accel_effects_active_mask_p1,
        visual_effects_active_mask_p1,
        appearance_effects_active_mask_p1,
        fa_plus_active_mask_p1,
        early_dw_active_mask_p1,
        gameplay_extras_active_mask_p1,
        gameplay_extras_more_active_mask_p1,
        results_extras_active_mask_p1,
        life_bar_options_active_mask_p1,
        error_bar_active_mask_p1,
        error_bar_options_active_mask_p1,
        measure_counter_options_active_mask_p1,
    ) = apply_profile_defaults(&mut rows, &state.player_profiles[P1], P1);
    let (
        scroll_active_mask_p2,
        hide_active_mask_p2,
        insert_active_mask_p2,
        remove_active_mask_p2,
        holds_active_mask_p2,
        accel_effects_active_mask_p2,
        visual_effects_active_mask_p2,
        appearance_effects_active_mask_p2,
        fa_plus_active_mask_p2,
        early_dw_active_mask_p2,
        gameplay_extras_active_mask_p2,
        gameplay_extras_more_active_mask_p2,
        results_extras_active_mask_p2,
        life_bar_options_active_mask_p2,
        error_bar_active_mask_p2,
        error_bar_options_active_mask_p2,
        measure_counter_options_active_mask_p2,
    ) = apply_profile_defaults(&mut rows, &state.player_profiles[P2], P2);
    state.rows = rows;
    state.scroll_active_mask = [scroll_active_mask_p1, scroll_active_mask_p2];
    state.hide_active_mask = [hide_active_mask_p1, hide_active_mask_p2];
    state.insert_active_mask = [insert_active_mask_p1, insert_active_mask_p2];
    state.remove_active_mask = [remove_active_mask_p1, remove_active_mask_p2];
    state.holds_active_mask = [holds_active_mask_p1, holds_active_mask_p2];
    state.accel_effects_active_mask = [accel_effects_active_mask_p1, accel_effects_active_mask_p2];
    state.visual_effects_active_mask =
        [visual_effects_active_mask_p1, visual_effects_active_mask_p2];
    state.appearance_effects_active_mask = [
        appearance_effects_active_mask_p1,
        appearance_effects_active_mask_p2,
    ];
    state.fa_plus_active_mask = [fa_plus_active_mask_p1, fa_plus_active_mask_p2];
    state.early_dw_active_mask = [early_dw_active_mask_p1, early_dw_active_mask_p2];
    state.gameplay_extras_active_mask = [
        gameplay_extras_active_mask_p1,
        gameplay_extras_active_mask_p2,
    ];
    state.gameplay_extras_more_active_mask = [
        gameplay_extras_more_active_mask_p1,
        gameplay_extras_more_active_mask_p2,
    ];
    state.results_extras_active_mask =
        [results_extras_active_mask_p1, results_extras_active_mask_p2];
    state.life_bar_options_active_mask = [
        life_bar_options_active_mask_p1,
        life_bar_options_active_mask_p2,
    ];
    state.error_bar_active_mask = [error_bar_active_mask_p1, error_bar_active_mask_p2];
    state.error_bar_options_active_mask = [
        error_bar_options_active_mask_p1,
        error_bar_options_active_mask_p2,
    ];
    state.measure_counter_options_active_mask = [
        measure_counter_options_active_mask_p1,
        measure_counter_options_active_mask_p2,
    ];
    state.current_pane = pane;
    state.selected_row = [0; PLAYER_SLOTS];
    state.prev_selected_row = [0; PLAYER_SLOTS];
    state.inline_choice_x = [f32::NAN; PLAYER_SLOTS];
    state.arcade_row_focus = [false; PLAYER_SLOTS];
    state.start_held_since = [None; PLAYER_SLOTS];
    state.start_last_triggered_at = [None; PLAYER_SLOTS];
    state.cursor_initialized = [false; PLAYER_SLOTS];
    state.cursor_from_x = [0.0; PLAYER_SLOTS];
    state.cursor_from_y = [0.0; PLAYER_SLOTS];
    state.cursor_from_w = [0.0; PLAYER_SLOTS];
    state.cursor_from_h = [0.0; PLAYER_SLOTS];
    state.cursor_to_x = [0.0; PLAYER_SLOTS];
    state.cursor_to_y = [0.0; PLAYER_SLOTS];
    state.cursor_to_w = [0.0; PLAYER_SLOTS];
    state.cursor_to_h = [0.0; PLAYER_SLOTS];
    state.cursor_t = [1.0; PLAYER_SLOTS];
    state.help_anim_time = [0.0; PLAYER_SLOTS];
    let active = session_active_players();
    state.row_tweens = init_row_tweens(
        &state.rows,
        state.selected_row,
        active,
        state.hide_active_mask,
        state.error_bar_active_mask,
        state.allow_per_player_global_offsets,
    );
    state.arcade_row_focus = std::array::from_fn(|player_idx| {
        row_allows_arcade_next_row(state, state.selected_row[player_idx])
    });
}

fn switch_to_pane(state: &mut State, pane: OptionsPane) {
    if state.current_pane == pane {
        return;
    }
    audio::play_sfx("assets/sounds/start.ogg");

    state.nav_key_held_direction = [None; PLAYER_SLOTS];
    state.nav_key_held_since = [None; PLAYER_SLOTS];
    state.nav_key_last_scrolled_at = [None; PLAYER_SLOTS];
    state.start_held_since = [None; PLAYER_SLOTS];
    state.start_last_triggered_at = [None; PLAYER_SLOTS];

    state.pane_transition = match state.pane_transition {
        PaneTransition::FadingOut { t, .. } => PaneTransition::FadingOut { target: pane, t },
        _ => PaneTransition::FadingOut {
            target: pane,
            t: 0.0,
        },
    };
}

fn focus_exit_row(state: &mut State, active: [bool; PLAYER_SLOTS], player_idx: usize) {
    if state.rows.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.selected_row[idx] = state.rows.len().saturating_sub(1);
    state.arcade_row_focus[idx] = row_allows_arcade_next_row(state, state.selected_row[idx]);
    sync_selected_rows_with_visibility(state, active);
}

#[inline(always)]
fn finish_start_without_action(
    state: &mut State,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    should_focus_exit: bool,
) -> Option<ScreenAction> {
    if should_focus_exit {
        focus_exit_row(state, active, player_idx);
    }
    None
}

fn handle_nav_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    dir: NavDirection,
    pressed: bool,
) {
    if !active[player_idx] || state.rows.is_empty() {
        return;
    }
    if pressed {
        sync_selected_rows_with_visibility(state, active);
        match dir {
            NavDirection::Up => {
                move_selection_vertical(state, asset_manager, active, player_idx, NavDirection::Up)
            }
            NavDirection::Down => move_selection_vertical(
                state,
                asset_manager,
                active,
                player_idx,
                NavDirection::Down,
            ),
            NavDirection::Left => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, -1) {
                    apply_choice_delta(state, asset_manager, player_idx, -1);
                    if arcade_row_uses_choice_focus(state, player_idx) {
                        state.arcade_row_focus[player_idx.min(PLAYER_SLOTS - 1)] = false;
                    }
                }
            }
            NavDirection::Right => {
                if !move_arcade_horizontal_focus(state, asset_manager, player_idx, 1) {
                    apply_choice_delta(state, asset_manager, player_idx, 1);
                    if arcade_row_uses_choice_focus(state, player_idx) {
                        state.arcade_row_focus[player_idx.min(PLAYER_SLOTS - 1)] = false;
                    }
                }
            }
        }
        on_nav_press(state, player_idx, dir);
    } else {
        on_nav_release(state, player_idx, dir);
    }
}

#[inline(always)]
fn clear_nav_hold(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    state.nav_key_held_direction[idx] = None;
    state.nav_key_held_since[idx] = None;
    state.nav_key_last_scrolled_at[idx] = None;
}

#[inline(always)]
fn player_side_for_idx(player_idx: usize) -> crate::game::profile::PlayerSide {
    if player_idx == P2 {
        crate::game::profile::PlayerSide::P2
    } else {
        crate::game::profile::PlayerSide::P1
    }
}

fn handle_arcade_start_press(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    repeated: bool,
) -> Option<ScreenAction> {
    if screen_input::menu_lr_both_held(&state.menu_lr_chord, player_side_for_idx(player_idx)) {
        handle_arcade_prev_event(state, asset_manager, active, player_idx);
        return None;
    }
    if repeated && !state.rows.is_empty() {
        let idx = player_idx.min(PLAYER_SLOTS - 1);
        let row_idx = state.selected_row[idx].min(state.rows.len().saturating_sub(1));
        if row_idx + 1 == state.rows.len() {
            return None;
        }
    }
    handle_arcade_start_event(state, asset_manager, active, player_idx)
}

fn repeat_held_arcade_start(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
    now: Instant,
) -> Option<ScreenAction> {
    if !active[player_idx] {
        clear_start_hold(state, player_idx);
        return None;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let (Some(held_since), Some(last_triggered_at)) = (
        state.start_held_since[idx],
        state.start_last_triggered_at[idx],
    ) else {
        return None;
    };
    if now.duration_since(held_since) <= NAV_INITIAL_HOLD_DELAY
        || now.duration_since(last_triggered_at) < NAV_REPEAT_SCROLL_INTERVAL
    {
        return None;
    }
    state.start_last_triggered_at[idx] = Some(now);
    handle_arcade_start_press(state, asset_manager, active, player_idx, true)
}

fn move_arcade_horizontal_focus(
    state: &mut State,
    asset_manager: &AssetManager,
    player_idx: usize,
    delta: isize,
) -> bool {
    if delta == 0 || state.rows.is_empty() {
        return false;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_idx = state.selected_row[idx].min(state.rows.len().saturating_sub(1));
    let Some(row) = state.rows.get(row_idx) else {
        return false;
    };
    let row_supports_inline = row_supports_inline_nav(row);
    let num_choices = row.choices.len();
    let current_choice = row
        .selected_choice_index
        .get(idx)
        .copied()
        .unwrap_or(0)
        .min(num_choices.saturating_sub(1));
    if !row_allows_arcade_next_row(state, row_idx) {
        return false;
    }
    if row_supports_inline {
        apply_choice_delta(state, asset_manager, idx, delta);
        return true;
    }
    if num_choices <= 1 {
        return false;
    }
    if state.arcade_row_focus[idx] {
        if delta < 0 {
            return false;
        }
        state.arcade_row_focus[idx] = false;
        if current_choice == 0 {
            audio::play_sfx("assets/sounds/change_value.ogg");
        } else {
            change_choice_for_player(state, asset_manager, idx, -(current_choice as isize));
        }
        return true;
    }
    if delta < 0 {
        if current_choice == 0 {
            state.arcade_row_focus[idx] = true;
            audio::play_sfx("assets/sounds/change_value.ogg");
            return true;
        }
        change_choice_for_player(state, asset_manager, idx, -1);
        return true;
    }
    if current_choice + 1 >= num_choices {
        return false;
    }
    change_choice_for_player(state, asset_manager, idx, 1);
    true
}

fn handle_arcade_prev_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
) {
    if !active[player_idx] || state.rows.is_empty() {
        return;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let prev_row = state.selected_row[idx];
    clear_nav_hold(state, player_idx);
    move_selection_vertical(state, asset_manager, active, player_idx, NavDirection::Up);
    if state.selected_row[idx] != prev_row {
        audio::play_sfx("assets/sounds/prev_row.ogg");
        state.help_anim_time[idx] = 0.0;
        state.prev_selected_row[idx] = state.selected_row[idx];
    }
}

fn handle_arcade_start_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
) -> Option<ScreenAction> {
    if !active[player_idx] {
        return None;
    }
    sync_selected_rows_with_visibility(state, active);
    let num_rows = state.rows.len();
    if num_rows == 0 {
        return None;
    }
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx].min(num_rows.saturating_sub(1));
    if row_index + 1 == num_rows {
        state.arcade_row_focus[idx] = row_allows_arcade_next_row(state, row_index);
        return handle_start_event(state, asset_manager, active, idx);
    }
    if arcade_row_uses_choice_focus(state, idx) && !state.arcade_row_focus[idx] {
        let action = handle_start_event(state, asset_manager, active, idx);
        state.arcade_row_focus[idx] = row_allows_arcade_next_row(state, row_index);
        return action;
    }
    move_selection_vertical(state, asset_manager, active, idx, NavDirection::Down);
    state.arcade_row_focus[idx] = row_allows_arcade_next_row(state, state.selected_row[idx]);
    None
}

fn handle_start_event(
    state: &mut State,
    asset_manager: &AssetManager,
    active: [bool; PLAYER_SLOTS],
    player_idx: usize,
) -> Option<ScreenAction> {
    if !active[player_idx] {
        return None;
    }
    sync_selected_rows_with_visibility(state, active);
    let num_rows = state.rows.len();
    if num_rows == 0 {
        return None;
    }
    let row_index = state.selected_row[player_idx].min(num_rows.saturating_sub(1));
    let should_focus_exit = state.current_pane == OptionsPane::Main && row_index + 1 < num_rows;
    let row = state.rows.get(row_index)?;
    let id = row.id;
    let row_supports_inline = row_supports_inline_nav(row);
    if row_supports_inline {
        let changed = commit_inline_focus_selection(state, asset_manager, player_idx, row_index);
        if changed && !row_toggles_with_start(id) {
            change_choice_for_player(state, asset_manager, player_idx, 0);
            return finish_start_without_action(state, active, player_idx, should_focus_exit);
        }
    }
    if id == RowId::Scroll {
        toggle_scroll_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Hide {
        toggle_hide_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Insert {
        toggle_insert_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Remove {
        toggle_remove_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Holds {
        toggle_holds_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Accel {
        toggle_accel_effects_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Effect {
        toggle_visual_effects_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::Appearance {
        toggle_appearance_effects_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::LifeBarOptions {
        toggle_life_bar_options_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::GameplayExtras {
        toggle_gameplay_extras_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::GameplayExtrasMore {
        toggle_gameplay_extras_more_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::ResultsExtras {
        toggle_results_extras_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::ErrorBar {
        toggle_error_bar_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::ErrorBarOptions {
        toggle_error_bar_options_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::MeasureCounterOptions {
        toggle_measure_counter_options_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::FAPlusOptions {
        toggle_fa_plus_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if id == RowId::EarlyDecentWayOffOptions {
        toggle_early_dw_row(state, player_idx);
        return finish_start_without_action(state, active, player_idx, should_focus_exit);
    }
    if row_index == num_rows.saturating_sub(1)
        && let Some(what_comes_next_row) = state.rows.get(num_rows.saturating_sub(2))
        && what_comes_next_row.id == RowId::WhatComesNext
    {
        let choice_idx = what_comes_next_row.selected_choice_index[player_idx];
        if let Some(choice) = what_comes_next_row.choices.get(choice_idx) {
            let gameplay = tr("PlayerOptions", "WhatComesNextGameplay");
            let advanced = tr("PlayerOptions", "WhatComesNextAdvancedModifiers");
            let uncommon = tr("PlayerOptions", "WhatComesNextUncommonModifiers");
            let main_mods = tr("PlayerOptions", "WhatComesNextMainModifiers");
            let choose_different = choose_different_screen_label(state.return_screen);
            let choice_str = choice.as_str();
            if choice_str == gameplay.as_ref() {
                audio::play_sfx("assets/sounds/start.ogg");
                return Some(ScreenAction::Navigate(Screen::Gameplay));
            } else if choice_str == choose_different {
                audio::play_sfx("assets/sounds/start.ogg");
                return Some(ScreenAction::Navigate(state.return_screen));
            } else if choice_str == advanced.as_ref() {
                switch_to_pane(state, OptionsPane::Advanced);
            } else if choice_str == uncommon.as_ref() {
                switch_to_pane(state, OptionsPane::Uncommon);
            } else if choice_str == main_mods.as_ref() {
                switch_to_pane(state, OptionsPane::Main);
            }
        }
    }
    finish_start_without_action(state, active, player_idx, should_focus_exit)
}

pub fn handle_input(
    state: &mut State,
    asset_manager: &AssetManager,
    ev: &InputEvent,
) -> ScreenAction {
    let active = session_active_players();
    let dedicated_three_key = screen_input::dedicated_three_key_nav_enabled();
    let arcade_style = crate::config::get().arcade_options_navigation;
    if arcade_options_navigation_active() || dedicated_three_key {
        screen_input::track_menu_lr_chord(&mut state.menu_lr_chord, ev);
    }
    let three_key_action = (!dedicated_three_key)
        .then(|| screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev))
        .flatten();
    if state.pane_transition.is_active() {
        if let Some((side, screen_input::ThreeKeyMenuAction::Cancel)) = three_key_action {
            let player_idx = screen_input::player_side_ix(side);
            if active[player_idx] {
                return ScreenAction::Navigate(state.return_screen);
            }
        }
        return match ev.action {
            VirtualAction::p1_back if ev.pressed && active[P1] => {
                ScreenAction::Navigate(state.return_screen)
            }
            VirtualAction::p2_back if ev.pressed && active[P2] => {
                ScreenAction::Navigate(state.return_screen)
            }
            _ => ScreenAction::None,
        };
    }
    if let Some((side, nav)) = three_key_action {
        let player_idx = screen_input::player_side_ix(side);
        if !active[player_idx] {
            return ScreenAction::None;
        }
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                handle_nav_event(
                    state,
                    asset_manager,
                    active,
                    player_idx,
                    NavDirection::Up,
                    true,
                );
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
                handle_nav_event(
                    state,
                    asset_manager,
                    active,
                    player_idx,
                    NavDirection::Down,
                    true,
                );
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Confirm => {
                clear_nav_hold(state, player_idx);
                if let Some(action) = handle_start_event(state, asset_manager, active, player_idx) {
                    return action;
                }
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Cancel => {
                clear_nav_hold(state, player_idx);
                ScreenAction::Navigate(state.return_screen)
            }
        };
    }
    match ev.action {
        VirtualAction::p1_back if ev.pressed && active[P1] => {
            return ScreenAction::Navigate(state.return_screen);
        }
        VirtualAction::p2_back if ev.pressed && active[P2] => {
            return ScreenAction::Navigate(state.return_screen);
        }
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Up,
                ev.pressed,
            );
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Down,
                ev.pressed,
            );
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Left,
                ev.pressed,
            );
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P1,
                NavDirection::Right,
                ev.pressed,
            );
        }
        VirtualAction::p1_start => {
            if !ev.pressed {
                clear_start_hold(state, P1);
                return ScreenAction::None;
            }
            if arcade_style {
                on_start_press(state, P1);
                if let Some(action) =
                    handle_arcade_start_press(state, asset_manager, active, P1, false)
                {
                    return action;
                }
                return ScreenAction::None;
            }
            if let Some(action) = handle_start_event(state, asset_manager, active, P1) {
                return action;
            }
        }
        VirtualAction::p1_select if ev.pressed && arcade_style => {
            handle_arcade_prev_event(state, asset_manager, active, P1);
            return ScreenAction::None;
        }
        VirtualAction::p2_up | VirtualAction::p2_menu_up => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Up,
                ev.pressed,
            );
        }
        VirtualAction::p2_down | VirtualAction::p2_menu_down => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Down,
                ev.pressed,
            );
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Left,
                ev.pressed,
            );
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            handle_nav_event(
                state,
                asset_manager,
                active,
                P2,
                NavDirection::Right,
                ev.pressed,
            );
        }
        VirtualAction::p2_start => {
            if !ev.pressed {
                clear_start_hold(state, P2);
                return ScreenAction::None;
            }
            if arcade_style {
                on_start_press(state, P2);
                if let Some(action) =
                    handle_arcade_start_press(state, asset_manager, active, P2, false)
                {
                    return action;
                }
                return ScreenAction::None;
            }
            if let Some(action) = handle_start_event(state, asset_manager, active, P2) {
                return action;
            }
        }
        VirtualAction::p2_select if ev.pressed && arcade_style => {
            handle_arcade_prev_event(state, asset_manager, active, P2);
            return ScreenAction::None;
        }
        _ => {}
    }
    ScreenAction::None
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);
    let active = session_active_players();
    let show_p2 = active[P1] && active[P2];
    let pane_alpha = state.pane_transition.alpha();
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    let select_modifiers = tr("ScreenTitles", "SelectModifiers");
    actors.push(screen_bar::build(ScreenBarParams {
        title: &select_modifiers,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1);
    let p2_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2);
    let p1_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P1);
    let p2_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P2);

    let insert_card = tr("Common", "InsertCard");
    let press_start = tr("Common", "PressStart");

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                insert_card.as_ref()
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                insert_card.as_ref()
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let event_mode = tr("Common", "EventMode");
    actors.push(screen_bar::build(ScreenBarParams {
        title: &event_mode,
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));
    // zmod ScreenPlayerOptions overlay/default.lua speed helper parity.
    let speed_mod_y = 48.0;
    let speed_mod_zoom = 0.5_f32;
    let speed_mod_scaled_y = 52.0_f32;
    let speed_mod_scaled_zoom = 0.3_f32;
    let speed_mod_x_p1 = screen_center_x() + widescale(-77.0, -100.0);
    let speed_mod_x_p2 = screen_center_x() + widescale(140.0, 154.0);
    let speed_mod_x = speed_mod_x_p1;
    // All previews (judgment, hold, noteskin, combo) share this center line.
    // Tweak these to dial in parity with Simply Love.
    const PREVIEW_CENTER_OFFSET_NORMAL: f32 = 80.75; // 4:3
    const PREVIEW_CENTER_OFFSET_WIDE: f32 = 98.75; // 16:9
    let preview_center_x =
        speed_mod_x_p1 + widescale(PREVIEW_CENTER_OFFSET_NORMAL, PREVIEW_CENTER_OFFSET_WIDE);

    let player_color_index = |player_idx: usize| {
        if player_idx == P2 {
            state.active_color_index - 2
        } else {
            state.active_color_index
        }
    };
    let speed_x_for = |player_idx: usize| {
        if player_idx == P2 {
            speed_mod_x_p2
        } else {
            speed_mod_x_p1
        }
    };
    let preview_dx = preview_center_x - speed_mod_x_p1;
    let preview_x_for = |player_idx: usize| speed_x_for(player_idx) + preview_dx;

    if state.current_pane == OptionsPane::Main {
        for player_idx in active_player_indices(active) {
            let speed_mod = &state.speed_mod[player_idx];
            let speed_color = color::simply_love_rgba(player_color_index(player_idx));
            let p_chart = resolve_p1_chart(&state.song, &state.chart_steps_index);
            let main_scroll =
                speed_mod_helper_scroll_text(&state.song, p_chart, speed_mod, state.music_rate);
            let speed_prefix = speed_mod.mod_type.as_str();
            let speed_text = format!("{speed_prefix}{main_scroll}");
            // zmod uses GetWidth() from the main helper actor (unzoomed width), then +w*0.4.
            let main_draw_w = measure_wendy_text_width(asset_manager, &speed_text);
            let speed_x = speed_x_for(player_idx);

            actors.push(act!(text: font("wendy"): settext(speed_text):
                align(0.5, 0.5): xy(speed_x, speed_mod_y): zoom(speed_mod_zoom):
                diffuse(speed_color[0], speed_color[1], speed_color[2], pane_alpha):
                z(121)
            ));

            let scaled_scroll = speed_mod_helper_scaled_text(
                &state.song,
                p_chart,
                speed_mod,
                state.music_rate,
                &state.player_profiles[player_idx],
            );
            if scaled_scroll != main_scroll {
                let scaled_text = format!("{speed_prefix}{scaled_scroll}");
                let scaled_x = speed_x + main_draw_w * 0.4;
                actors.push(act!(text: font("wendy"): settext(scaled_text):
                    align(0.5, 0.5): xy(scaled_x, speed_mod_scaled_y): zoom(speed_mod_scaled_zoom):
                    diffuse(speed_color[0], speed_color[1], speed_color[2], 0.8 * pane_alpha):
                    z(121)
                ));
            }
        }
    }
    /* ---------- SHARED GEOMETRY (rows aligned to help box) ---------- */
    // Help Text Box (from underlay.lua) — define this first so rows can match its width/left.
    let help_box_h = 40.0;
    let help_box_w = widescale(614.0, 792.0);
    let help_box_x = widescale(13.0, 30.666);
    let help_box_bottom_y = screen_height() - 36.0;
    let total_rows = state.rows.len();
    let frame_h = ROW_HEIGHT;
    let (fallback_y0, fallback_row_step) = row_layout_params();
    let row_alpha_cutoff: f32 = 0.001;
    // Make row frame LEFT and WIDTH exactly match the help box.
    let row_left = help_box_x;
    let row_width = help_box_w;
    //let row_center_x = row_left + (row_width * 0.5);
    let title_zoom = 0.88;
    // Title text x: slightly less padding so text sits further left.
    let title_left_pad = widescale(7.0, 13.0);
    let title_x = row_left + title_left_pad;
    // Keep header labels bounded to the title column so they never overlap option values.
    let title_max_w = (TITLE_BG_WIDTH - title_left_pad - 5.0).max(0.0);
    let cursor_now = |player_idx: usize| -> Option<(f32, f32, f32, f32)> {
        if player_idx >= PLAYER_SLOTS || !state.cursor_initialized[player_idx] {
            return None;
        }
        let t = state.cursor_t[player_idx].clamp(0.0, 1.0);
        let x = (state.cursor_to_x[player_idx] - state.cursor_from_x[player_idx])
            .mul_add(t, state.cursor_from_x[player_idx]);
        let y = (state.cursor_to_y[player_idx] - state.cursor_from_y[player_idx])
            .mul_add(t, state.cursor_from_y[player_idx]);
        let w = (state.cursor_to_w[player_idx] - state.cursor_from_w[player_idx])
            .mul_add(t, state.cursor_from_w[player_idx]);
        let h = (state.cursor_to_h[player_idx] - state.cursor_from_h[player_idx])
            .mul_add(t, state.cursor_from_h[player_idx]);
        Some((x, y, w, h))
    };

    for item_idx in 0..total_rows {
        let (current_row_y, row_alpha) = state
            .row_tweens
            .get(item_idx)
            .map(|tw| (tw.y(), tw.a()))
            .unwrap_or_else(|| {
                (
                    (item_idx as f32).mul_add(fallback_row_step, fallback_y0),
                    1.0,
                )
            });
        let row_alpha = (row_alpha * pane_alpha).clamp(0.0, 1.0);
        if row_alpha <= row_alpha_cutoff {
            continue;
        }
        let a = row_alpha;

        let is_active = (active[P1] && item_idx == state.selected_row[P1])
            || (active[P2] && item_idx == state.selected_row[P2]);
        let row = &state.rows[item_idx];
        let active_bg = color::rgba_hex("#333333");
        let inactive_bg_base = color::rgba_hex("#071016");
        let bg_color = if is_active {
            active_bg
        } else {
            [
                inactive_bg_base[0],
                inactive_bg_base[1],
                inactive_bg_base[2],
                0.8,
            ]
        };
        // Row background — matches help box width & left
        actors.push(act!(quad:
            align(0.0, 0.5): xy(row_left, current_row_y):
            zoomto(row_width, frame_h):
            diffuse(bg_color[0], bg_color[1], bg_color[2], bg_color[3] * a):
            z(100)
        ));
        if row.id != RowId::Exit {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(row_left, current_row_y):
                zoomto(TITLE_BG_WIDTH, frame_h):
                diffuse(0.0, 0.0, 0.0, 0.25 * a):
                z(101)
            ));
        }
        // Left column (row titles)
        let mut title_color = if is_active {
            let mut c = color::simply_love_rgba(state.active_color_index);
            c[3] = 1.0;
            c
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        title_color[3] *= a;
        // Handle multi-line row titles (e.g., "Music Rate\nbpm: 120")
        if row.id == RowId::MusicRate {
            let display = music_rate_display_name(state);
            let lines: Vec<&str> = display.split('\n').collect();
            if lines.len() == 2 {
                actors.push(act!(text: font("miso"): settext(lines[0].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y - 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
                actors.push(act!(text: font("miso"): settext(lines[1].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y + 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
            } else {
                actors.push(act!(text: font("miso"): settext(display):
                    align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
            }
        } else {
            actors.push(act!(text: font("miso"): settext(row.name.get().to_string()):
                align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                horizalign(left): maxwidth(title_max_w):
                z(101)
            ));
        }
        // Inactive option text color should be #808080 (alpha 1.0)
        let mut sl_gray = color::rgba_hex("#808080");
        sl_gray[3] *= a;
        // Some rows should display all choices inline
        let show_all_choices_inline = row_shows_all_choices_inline(row.id);
        let show_arcade_next_row = arcade_next_row_visible(state, item_idx);
        // Choice area: For single-choice rows (ShowOneInRow), use ItemsLongRowP1X positioning
        // For multi-choice rows (ShowAllInRow), use ItemsStartX positioning
        // ItemsLongRowP1X = WideScale(_screen.cx-100, _screen.cx-130) from Simply Love metrics
        // ItemsStartX = WideScale(146, 160) from Simply Love metrics
        let choice_inner_left = if show_all_choices_inline {
            inline_choice_left_x_for_row(state, item_idx)
        } else {
            screen_center_x() + widescale(-100.0, -130.0) // ItemsLongRowP1X for single-choice rows
        };
        if row.id == RowId::Exit {
            // Special case for the last "Exit" row
            let choice_text = &row.choices[row.selected_choice_index[P1]];
            let choice_color = if is_active {
                [1.0, 1.0, 1.0, a]
            } else {
                sl_gray
            };
            // Align Exit horizontally with other single-value options (Speed Mod line)
            let choice_center_x = speed_mod_x;
            actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(0.835):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                z(101)
            ));
            // Draw the selection cursor for the centered "Exit" text when active
            if is_active {
                let border_w = widescale(2.0, 2.5);
                for player_idx in active_player_indices(active) {
                    if state.selected_row[player_idx] != item_idx {
                        continue;
                    }
                    let Some((center_x, center_y, ring_w, ring_h)) = cursor_now(player_idx) else {
                        continue;
                    };

                    let left = center_x - ring_w * 0.5;
                    let right = center_x + ring_w * 0.5;
                    let top = center_y - ring_h * 0.5;
                    let bottom = center_y + ring_h * 0.5;
                    let mut ring_color = color::decorative_rgba(player_color_index(player_idx));
                    ring_color[3] *= a;

                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, top + border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, bottom - border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(left + border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(right - border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                }
            }
        } else if show_all_choices_inline {
            // Render every option horizontally; when active, all options should be white.
            // The active option gets an underline (quad) drawn just below the text.
            let value_zoom = 0.835;
            let spacing = 15.75;
            let next_row_item = show_arcade_next_row
                .then(|| arcade_next_row_layout(state, item_idx, asset_manager, value_zoom));
            let mut widths: Vec<f32> = Vec::with_capacity(row.choices.len());
            let mut text_h: f32 = 16.0;
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                    for text in &row.choices {
                        let mut w = crate::engine::present::font::measure_line_width_logical(
                            metrics_font,
                            text,
                            all_fonts,
                        ) as f32;
                        if !w.is_finite() || w <= 0.0 {
                            w = 1.0;
                        }
                        widths.push(w * value_zoom);
                    }
                });
            });
            let mut x_positions: Vec<f32> = Vec::with_capacity(widths.len());
            {
                let mut x = choice_inner_left;
                for w in &widths {
                    x_positions.push(x);
                    x += *w + spacing;
                }
            }
            // Draw underline under active options:
            // - For normal rows: underline the currently selected choice.
            // - For Scroll row: underline each enabled scroll mode (multi-select).
            // - For FA+ Options row: underline each enabled FA+ toggle (multi-select).
            if row.id == RowId::Scroll {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.scroll_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Hide {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.hide_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Insert {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.insert_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Remove {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.remove_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Holds {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.holds_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Accel {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.accel_effects_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Effect {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.visual_effects_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u16 << (idx as u16);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::Appearance {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.appearance_effects_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::LifeBarOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.life_bar_options_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::FAPlusOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.fa_plus_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::GameplayExtras {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.gameplay_extras_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::GameplayExtrasMore {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.gameplay_extras_more_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::ResultsExtras {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.results_extras_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::MeasureCounterOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.measure_counter_options_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::ErrorBar {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.error_bar_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::ErrorBarOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.error_bar_options_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else if row.id == RowId::EarlyDecentWayOffOptions {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let mask = state.early_dw_active_mask[player_idx];
                    if mask == 0 {
                        continue;
                    }
                    let underline_y = underline_y_for(player_idx);
                    let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                    line_color[3] *= a;
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            let underline_w = draw_w.ceil();
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        }
                    }
                }
            } else {
                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                let offset = widescale(3.0, 4.0);
                let underline_base_y = current_row_y + text_h * 0.5 + offset;
                let underline_y_for = |player_idx: usize| {
                    if active[P1] && active[P2] {
                        (player_idx as f32).mul_add(line_thickness + 1.0, underline_base_y)
                    } else {
                        underline_base_y
                    }
                };
                for player_idx in active_player_indices(active) {
                    let idx =
                        row.selected_choice_index[player_idx].min(widths.len().saturating_sub(1));
                    if let Some(sel_x) = x_positions.get(idx).copied() {
                        let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                        let underline_w = draw_w.ceil();
                        let underline_y = underline_y_for(player_idx);
                        let mut line_color = color::decorative_rgba(player_color_index(player_idx));
                        line_color[3] *= a;
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(sel_x, underline_y):
                            zoomto(underline_w, line_thickness):
                            diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                            z(101)
                        ));
                    }
                }
            }
            // Draw the 4-sided cursor ring around the selected option when this row is active.
            if !widths.is_empty() {
                let border_w = widescale(2.0, 2.5);
                for player_idx in active_player_indices(active) {
                    if state.selected_row[player_idx] != item_idx {
                        continue;
                    }
                    let Some((center_x, center_y, ring_w, ring_h)) = cursor_now(player_idx) else {
                        continue;
                    };

                    let left = center_x - ring_w * 0.5;
                    let right = center_x + ring_w * 0.5;
                    let top = center_y - ring_h * 0.5;
                    let bottom = center_y + ring_h * 0.5;
                    let mut ring_color = color::decorative_rgba(player_color_index(player_idx));
                    ring_color[3] *= a;
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, top + border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy((left + right) * 0.5, bottom - border_w * 0.5):
                        zoomto(ring_w, border_w):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(left + border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(right - border_w * 0.5, (top + bottom) * 0.5):
                        zoomto(border_w, ring_h):
                        diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                        z(101)
                    ));
                }
            }
            // Draw each option's text (active row: all white; inactive: #808080)
            if let Some((next_row_x, _, _)) = next_row_item {
                let next_row_color = if is_active {
                    [1.0, 1.0, 1.0, a]
                } else {
                    sl_gray
                };
                actors.push(act!(text: font("miso"): settext(ARCADE_NEXT_ROW_TEXT):
                    align(0.0, 0.5): xy(next_row_x, current_row_y): zoom(value_zoom):
                    diffuse(
                        next_row_color[0],
                        next_row_color[1],
                        next_row_color[2],
                        next_row_color[3]
                    ):
                    z(101)
                ));
            }
            for (idx, text) in row.choices.iter().enumerate() {
                let x = x_positions.get(idx).copied().unwrap_or(choice_inner_left);
                let color_rgba = if is_active {
                    [1.0, 1.0, 1.0, a]
                } else {
                    sl_gray
                };
                actors.push(act!(text: font("miso"): settext(text.clone()):
                    align(0.0, 0.5): xy(x, current_row_y): zoom(value_zoom):
                    diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]):
                    z(101)
                ));
            }
        } else {
            // Single value display (default behavior)
            // By default, align single-value choices to the same line as Speed Mod.
            // For Music Rate, center within the item column (to match SL parity).
            let primary_player_idx = if active[P1] { P1 } else { P2 };
            let mut choice_center_x = speed_mod_x;
            if row.id == RowId::MusicRate {
                let item_col_left = row_left + TITLE_BG_WIDTH;
                let item_col_w = row_width - TITLE_BG_WIDTH;
                choice_center_x = item_col_left + item_col_w * 0.5;
            } else if primary_player_idx == P2 {
                choice_center_x = screen_center_x().mul_add(2.0, -choice_center_x);
            }
            let choice_text_idx = row.selected_choice_index[primary_player_idx]
                .min(row.choices.len().saturating_sub(1));
            let choice_text = row
                .choices
                .get(choice_text_idx)
                .unwrap_or_else(|| row.choices.first().expect("OptionRow must have choices"));
            let choice_color = if is_active {
                [1.0, 1.0, 1.0, a]
            } else {
                sl_gray
            };
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    let choice_display_text =
                        if arcade_row_focuses_next_row(state, primary_player_idx, item_idx) {
                            ARCADE_NEXT_ROW_TEXT.to_string()
                        } else if row.id == RowId::SpeedMod {
                            match state.speed_mod[primary_player_idx].mod_type.as_str() {
                                "X" => format!("{:.2}x", state.speed_mod[primary_player_idx].value),
                                "C" => format!(
                                    "C{}",
                                    state.speed_mod[primary_player_idx].value as i32
                                ),
                                "M" => format!(
                                    "M{}",
                                    state.speed_mod[primary_player_idx].value as i32
                                ),
                                _ => String::new(),
                            }
                        } else {
                            choice_text.clone()
                        };
                    let mut text_w = crate::engine::present::font::measure_line_width_logical(
                        metrics_font,
                        &choice_display_text,
                        all_fonts,
                    ) as f32;
                    if !text_w.is_finite() || text_w <= 0.0 {
                        text_w = 1.0;
                    }
                    let text_h = (metrics_font.height as f32).max(1.0);
                    let value_zoom = 0.835;
                    let draw_w = text_w * value_zoom;
                    let draw_h = text_h * value_zoom;
                    actors.push(act!(text: font("miso"): settext(choice_display_text):
                        align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(value_zoom):
                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                        z(101)
                    ));
                    // Underline (always visible) — fixed pixel thickness for consistency
                    let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                    let underline_w = draw_w.ceil(); // pixel-align for crispness
                    let offset = widescale(3.0, 4.0); // place just under the baseline
                    let underline_y = current_row_y + draw_h * 0.5 + offset;
                    let underline_left_x = choice_center_x - draw_w * 0.5;
                    let mut line_color = color::decorative_rgba(player_color_index(primary_player_idx));
                    line_color[3] *= a;
                    actors.push(act!(quad:
                        align(0.0, 0.5): // start at text's left edge
                        xy(underline_left_x, underline_y):
                        zoomto(underline_w, line_thickness):
                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                        z(101)
                    ));
                    // Encircling cursor around the active option value (programmatic border)
                    if active[primary_player_idx] && state.selected_row[primary_player_idx] == item_idx {
                        let border_w = widescale(2.0, 2.5);
                        if let Some((center_x, center_y, ring_w, ring_h)) =
                            cursor_now(primary_player_idx)
                        {
                            let left = center_x - ring_w * 0.5;
                            let right = center_x + ring_w * 0.5;
                            let top = center_y - ring_h * 0.5;
                            let bottom = center_y + ring_h * 0.5;
                            let mut ring_color =
                                color::decorative_rgba(player_color_index(primary_player_idx));
                            ring_color[3] *= a;
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(center_x, top + border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(center_x, bottom - border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(left + border_w * 0.5, center_y):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(right - border_w * 0.5, center_y):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                        }
                    }
                    let p2_text = if show_p2 && row.id != RowId::MusicRate {
                        if arcade_row_focuses_next_row(state, P2, item_idx) {
                            ARCADE_NEXT_ROW_TEXT.to_string()
                        } else if row.id == RowId::SpeedMod {
                            match state.speed_mod[P2].mod_type.as_str() {
                                "X" => format!("{:.2}x", state.speed_mod[P2].value),
                                "C" => format!("C{}", state.speed_mod[P2].value as i32),
                                "M" => format!("M{}", state.speed_mod[P2].value as i32),
                                _ => String::new(),
                            }
                        } else if row.id == RowId::TypeOfSpeedMod {
                            let idx = match state.speed_mod[P2].mod_type.as_str() {
                                "X" => 0,
                                "C" => 1,
                                "M" => 2,
                                _ => 1,
                            };
                            row.choices.get(idx).cloned().unwrap_or_default()
                        } else {
                            let idx = row
                                .selected_choice_index[P2]
                                .min(row.choices.len().saturating_sub(1));
                            row.choices.get(idx).cloned().unwrap_or_default()
                        }
                    } else {
                        String::new()
                    };
                    if show_p2 && row.id != RowId::MusicRate {
                        let p2_choice_center_x = screen_center_x().mul_add(2.0, -choice_center_x);
                        let mut p2_w = crate::engine::present::font::measure_line_width_logical(
                            metrics_font,
                            &p2_text,
                            all_fonts,
                        ) as f32;
                        if !p2_w.is_finite() || p2_w <= 0.0 {
                            p2_w = 1.0;
                        }
                        let p2_draw_w = p2_w * value_zoom;
                        actors.push(act!(text: font("miso"): settext(p2_text.clone()):
                            align(0.5, 0.5): xy(p2_choice_center_x, current_row_y): zoom(value_zoom):
                            diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                            z(101)
                        ));
                        let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                        let underline_w = p2_draw_w.ceil();
                        let offset = widescale(3.0, 4.0);
                        let underline_y = current_row_y + draw_h * 0.5 + offset;
                        let underline_left_x = p2_choice_center_x - p2_draw_w * 0.5;
                        let mut line_color = color::decorative_rgba(player_color_index(P2));
                        line_color[3] *= a;
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(underline_left_x, underline_y):
                            zoomto(underline_w, line_thickness):
                            diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                            z(101)
                        ));
                        if active[P2] && state.selected_row[P2] == item_idx {
                            let border_w = widescale(2.0, 2.5);
                            if let Some((center_x, center_y, ring_w, ring_h)) = cursor_now(P2) {
                                let left = center_x - ring_w * 0.5;
                                let right = center_x + ring_w * 0.5;
                                let top = center_y - ring_h * 0.5;
                                let bottom = center_y + ring_h * 0.5;
                                let mut ring_color = color::decorative_rgba(player_color_index(P2));
                                ring_color[3] *= a;
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(center_x, top + border_w * 0.5):
                                    zoomto(ring_w, border_w):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(center_x, bottom - border_w * 0.5):
                                    zoomto(ring_w, border_w):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(left + border_w * 0.5, center_y):
                                    zoomto(border_w, ring_h):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(right - border_w * 0.5, center_y):
                                    zoomto(border_w, ring_h):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                            }
                        }
                    }
                    // Add previews for the selected value on each side.
                    if row.id == RowId::JudgmentFont {
                        let texture_for = |player_idx: usize| -> Option<&str> {
                            assets::judgment_texture_choices()
                                .get(row.selected_choice_index[player_idx])
                                .and_then(|choice| {
                                    if choice.key.eq_ignore_ascii_case("None") {
                                        None
                                    } else {
                                        assets::resolve_texture_choice(
                                            Some(choice.key.as_str()),
                                            assets::judgment_texture_choices(),
                                        )
                                    }
                                })
                        };
                        if let Some(texture) = texture_for(primary_player_idx) {
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_x_for(primary_player_idx), current_row_y):
                                setstate(0):
                                zoom(0.225):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                        if show_p2
                            && primary_player_idx != P2
                            && let Some(texture) = texture_for(P2)
                        {
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_x_for(P2), current_row_y):
                                setstate(0):
                                zoom(0.225):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                    }
                    // Add hold judgment preview for "Hold Judgment" row showing both frames (Held and Let Go)
                    if row.id == RowId::HoldJudgment {
                        let texture_for = |player_idx: usize| -> Option<&str> {
                            assets::hold_judgment_texture_choices()
                                .get(row.selected_choice_index[player_idx])
                                .and_then(|choice| {
                                    if choice.key.eq_ignore_ascii_case("None") {
                                        None
                                    } else {
                                        assets::resolve_texture_choice(
                                            Some(choice.key.as_str()),
                                            assets::hold_judgment_texture_choices(),
                                        )
                                    }
                                })
                        };
                        let draw_hold_preview = |texture: &str, center_x: f32, actors: &mut Vec<Actor>| {
                            let zoom = 0.225;
                            let tex_w = crate::assets::texture_dims(texture)
                                .map_or(128.0, |meta| meta.w.max(1) as f32);
                            let center_offset = tex_w * zoom * 0.4;

                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(center_x - center_offset, current_row_y):
                                setstate(0):
                                zoom(zoom):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(center_x + center_offset, current_row_y):
                                setstate(1):
                                zoom(zoom):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        };
                        if let Some(texture) = texture_for(primary_player_idx) {
                            draw_hold_preview(texture, preview_x_for(primary_player_idx), &mut actors);
                        }
                        if show_p2
                            && primary_player_idx != P2
                            && let Some(texture) = texture_for(P2)
                        {
                            draw_hold_preview(texture, preview_x_for(P2), &mut actors);
                        }
                    }
                    // Match ITGmania themes that show four directional noteskin preview arrows
                    // with explicit quant offsets: Left/Down/Up/Right and 0/1/3/2 quant indices.
                    if row.id == RowId::NoteSkin
                        || row.id == RowId::MineSkin
                        || row.id == RowId::ReceptorSkin
                        || row.id == RowId::TapExplosionSkin
                    {
                        const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;
                        const PREVIEW_SCALE: f32 = 0.45;
                        const PREVIEW_ARROWS: [(usize, f32, f32); 4] = [
                            (0, 0.0, -1.5),
                            (1, 1.0, -0.5),
                            (2, 3.0, 0.5),
                            (3, 2.0, 1.5),
                        ];
                        let draw_noteskin_note =
                            |ns: &Noteskin,
                             note_idx: usize,
                             quant_idx: f32,
                             center_x: f32,
                             actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                let elapsed = state.preview_time;
                                let beat = state.preview_beat;
                                let note_uv_phase = ns.tap_note_uv_phase(elapsed, beat, 0.0);
                                let tap_spacing = ns.note_display_metrics.part_texture_translate
                                    [NoteAnimPart::Tap as usize]
                                    .note_color_spacing;
                                let uv_translate =
                                    [tap_spacing[0] * quant_idx, tap_spacing[1] * quant_idx];
                                if let Some(note_slots) = ns.note_layers.get(note_idx) {
                                    let primary_h = note_slots
                                        .first()
                                        .map(|slot| slot.logical_size()[1].max(1.0))
                                        .unwrap_or(1.0);
                                    let note_scale = if primary_h > f32::EPSILON {
                                        target_height / primary_h
                                    } else {
                                        PREVIEW_SCALE
                                    };
                                    for (layer_idx, note_slot) in note_slots.iter().enumerate() {
                                        let draw = note_slot.model_draw_at(elapsed, beat);
                                        if !draw.visible {
                                            continue;
                                        }
                                        let frame = note_slot.frame_index(elapsed, beat);
                                        let uv_elapsed = if note_slot.model.is_some() {
                                            note_uv_phase
                                        } else {
                                            elapsed
                                        };
                                        let uv = note_slot.uv_for_frame_at(frame, uv_elapsed);
                                        let uv = [
                                            uv[0] + uv_translate[0],
                                            uv[1] + uv_translate[1],
                                            uv[2] + uv_translate[0],
                                            uv[3] + uv_translate[1],
                                        ];
                                        let slot_size = note_slot.logical_size();
                                        let base_size = [slot_size[0] * note_scale, slot_size[1] * note_scale];
                                        let rot_rad = (-note_slot.def.rotation_deg as f32).to_radians();
                                        let (sin_r, cos_r) = rot_rad.sin_cos();
                                        let ox = draw.pos[0] * note_scale;
                                        let oy = draw.pos[1] * note_scale;
                                        let center = [
                                            center_x + ox * cos_r - oy * sin_r,
                                            current_row_y + ox * sin_r + oy * cos_r,
                                        ];
                                        let size = [
                                            base_size[0] * draw.zoom[0].max(0.0),
                                            base_size[1] * draw.zoom[1].max(0.0),
                                        ];
                                        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                                            continue;
                                        }
                                        let color = [draw.tint[0], draw.tint[1], draw.tint[2], draw.tint[3] * a];
                                        let blend = if draw.blend_add {
                                            BlendMode::Add
                                        } else {
                                            BlendMode::Alpha
                                        };
                                        let z = 102 + layer_idx as i32;
                                        if let Some(model_actor) = noteskin_model_actor(
                                            note_slot,
                                            center,
                                            size,
                                            uv,
                                            -note_slot.def.rotation_deg as f32,
                                            elapsed,
                                            beat,
                                            color,
                                            blend,
                                            z as i16,
                                        ) {
                                            actors.push(model_actor);
                                        } else if draw.blend_add {
                                            actors.push(act!(sprite(note_slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(center[0], center[1]):
                                                setsize(size[0], size[1]):
                                                rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(color[0], color[1], color[2], color[3]):
                                                blend(add):
                                                z(z)
                                            ));
                                        } else {
                                            actors.push(act!(sprite(note_slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(center[0], center[1]):
                                                setsize(size[0], size[1]):
                                                rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(color[0], color[1], color[2], color[3]):
                                                blend(normal):
                                                z(z)
                                            ));
                                        }
                                    }
                                    return;
                                }
                                let Some(note_slot) = ns.notes.get(note_idx) else {
                                    return;
                                };
                                let frame = note_slot.frame_index(elapsed, beat);
                                let uv_elapsed = if note_slot.model.is_some() {
                                    note_uv_phase
                                } else {
                                    elapsed
                                };
                                let uv = note_slot.uv_for_frame_at(frame, uv_elapsed);
                                let uv = [
                                    uv[0] + uv_translate[0],
                                    uv[1] + uv_translate[1],
                                    uv[2] + uv_translate[0],
                                    uv[3] + uv_translate[1],
                                ];
                                let size_raw = note_slot.logical_size();
                                let width = size_raw[0].max(1.0);
                                let height = size_raw[1].max(1.0);
                                let scale = if height > 0.0 {
                                    target_height / height
                                } else {
                                    PREVIEW_SCALE
                                };
                                let size = [width * scale, target_height];
                                let center = [center_x, current_row_y];
                                if let Some(model_actor) = noteskin_model_actor(
                                    note_slot,
                                    center,
                                    size,
                                    uv,
                                    -note_slot.def.rotation_deg as f32,
                                    elapsed,
                                    beat,
                                    [1.0, 1.0, 1.0, a],
                                    BlendMode::Alpha,
                                    102,
                                ) {
                                    actors.push(model_actor);
                                } else {
                                    actors.push(act!(sprite(note_slot.texture_key_shared()):
                                        align(0.5, 0.5):
                                        xy(center[0], center[1]):
                                        setsize(size[0], size[1]):
                                        rotationz(-note_slot.def.rotation_deg as f32):
                                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                        diffuse(1.0, 1.0, 1.0, a):
                                        z(102)
                                    ));
                                }
                            };
                        let draw_noteskin_preview =
                            |ns: &Noteskin, center_x: f32, actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                for (col, quant_idx, x_mult) in PREVIEW_ARROWS {
                                    let x = center_x + x_mult * target_height;
                                    let note_idx =
                                        col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
                                    draw_noteskin_note(ns, note_idx, quant_idx, x, actors);
                                }
                            };
                        let draw_mine_preview =
                            |mine_ns: &Noteskin, center_x: f32, actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                let mine_col = if mine_ns.mines.len() > 1 || mine_ns.mine_frames.len() > 1 {
                                    1
                                } else {
                                    0
                                };
                                let fill_slot =
                                    mine_ns.mines.get(mine_col).and_then(|slot| slot.as_ref());
                                let frame_slot = mine_ns
                                    .mine_frames
                                    .get(mine_col)
                                    .and_then(|slot| slot.as_ref());
                                let Some(primary_slot) = frame_slot.or(fill_slot) else {
                                    return;
                                };
                                let mine_phase =
                                    mine_ns.tap_mine_uv_phase(state.preview_time, state.preview_beat, 0.0);
                                let mine_translation =
                                    mine_ns.part_uv_translation(NoteAnimPart::Mine, 0.0, false);
                                let mine_center = [center_x, current_row_y];
                                let scale_mine_slot = |slot: &SpriteSlot| {
                                    let size = slot
                                        .model
                                        .as_ref()
                                        .map(|model| model.size())
                                        .unwrap_or_else(|| {
                                            let logical = slot.logical_size();
                                            [logical[0], logical[1]]
                                        });
                                    let width = size[0].max(1.0);
                                    let height = size[1].max(1.0);
                                    let scale = target_height / height;
                                    [width * scale, target_height]
                                };
                                let draw_mine_slot =
                                    |slot: &SpriteSlot, alpha: f32, z: i32, actors: &mut Vec<Actor>| {
                                        let draw = slot.model_draw_at(state.preview_time, state.preview_beat);
                                        if !draw.visible {
                                            return;
                                        }
                                        let frame = slot.frame_index_from_phase(mine_phase);
                                        let uv_elapsed = if slot.model.is_some() {
                                            mine_phase
                                        } else {
                                            state.preview_time
                                        };
                                        let uv = slot.uv_for_frame_at(frame, uv_elapsed);
                                        let uv = [
                                            uv[0] + mine_translation[0],
                                            uv[1] + mine_translation[1],
                                            uv[2] + mine_translation[0],
                                            uv[3] + mine_translation[1],
                                        ];
                                        let size = scale_mine_slot(slot);
                                        if let Some(model_actor) = noteskin_model_actor(
                                            slot,
                                            mine_center,
                                            size,
                                            uv,
                                            -slot.def.rotation_deg as f32,
                                            state.preview_time,
                                            state.preview_beat,
                                            [1.0, 1.0, 1.0, alpha],
                                            BlendMode::Alpha,
                                            z as i16,
                                        ) {
                                            actors.push(model_actor);
                                        } else {
                                            actors.push(act!(sprite(slot.texture_key_shared()):
                                                align(0.5, 0.5):
                                                xy(mine_center[0], mine_center[1]):
                                                setsize(size[0], size[1]):
                                                rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                                diffuse(1.0, 1.0, 1.0, alpha):
                                                z(z)
                                            ));
                                        }
                                    };
                                if let Some(slot) = fill_slot {
                                    draw_mine_slot(slot, 0.85 * a, 106, actors);
                                }
                                if let Some(slot) = frame_slot {
                                    draw_mine_slot(slot, a, 107, actors);
                                } else if fill_slot.is_none() {
                                    draw_mine_slot(primary_slot, a, 107, actors);
                                }
                            };
                        let draw_receptor_preview =
                            |receptor_ns: &Noteskin, center_x: f32, actors: &mut Vec<Actor>| {
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                let receptor_color =
                                    receptor_ns.receptor_pulse.color_for_beat(state.preview_beat);
                                let color = [
                                    receptor_color[0],
                                    receptor_color[1],
                                    receptor_color[2],
                                    receptor_color[3] * a,
                                ];
                                for (col, _, x_mult) in PREVIEW_ARROWS {
                                    let Some(receptor_slot) = receptor_ns.receptor_off.get(col) else {
                                        continue;
                                    };
                                    let frame = receptor_slot
                                        .frame_index(state.preview_time, state.preview_beat);
                                    let uv = receptor_slot
                                        .uv_for_frame_at(frame, state.preview_time);
                                    let logical = receptor_slot.logical_size();
                                    let width = logical[0].max(1.0);
                                    let height = logical[1].max(1.0);
                                    let scale = if height > f32::EPSILON {
                                        target_height / height
                                    } else {
                                        PREVIEW_SCALE
                                    };
                                    let size = [width * scale, target_height];
                                    let center = [center_x + x_mult * target_height, current_row_y];
                                    if let Some(model_actor) = noteskin_model_actor(
                                        receptor_slot,
                                        center,
                                        size,
                                        uv,
                                        -receptor_slot.def.rotation_deg as f32,
                                        state.preview_time,
                                        state.preview_beat,
                                        color,
                                        BlendMode::Alpha,
                                        106,
                                    ) {
                                        actors.push(model_actor);
                                    } else {
                                        actors.push(act!(sprite(receptor_slot.texture_key_shared()):
                                            align(0.5, 0.5):
                                            xy(center[0], center[1]):
                                            setsize(size[0], size[1]):
                                            rotationz(-receptor_slot.def.rotation_deg as f32):
                                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                            diffuse(color[0], color[1], color[2], color[3]):
                                            z(106)
                                        ));
                                    }
                                }
                            };
                        let draw_tap_explosion_preview = |explosion_ns: &Noteskin,
                                                          receptor_ns: &Noteskin,
                                                          center_x: f32,
                                                          actors: &mut Vec<Actor>| {
                            let preview_time = state.preview_time * TAP_EXPLOSION_PREVIEW_SPEED;
                            let preview_beat = state.preview_beat * TAP_EXPLOSION_PREVIEW_SPEED;
                            let Some(explosion) = explosion_ns
                                .tap_explosions
                                .get("W1")
                                .or_else(|| explosion_ns.tap_explosions.values().next())
                            else {
                                return;
                            };
                            let duration = explosion.animation.duration();
                            let anim_time = if duration > f32::EPSILON {
                                preview_time.rem_euclid(duration)
                            } else {
                                0.0
                            };
                            let explosion_visual = explosion.animation.state_at(anim_time);
                            if !explosion_visual.visible {
                                return;
                            }
                            let slot = &explosion.slot;
                            let beat_for_anim = if slot.source.is_beat_based() {
                                anim_time.max(0.0)
                            } else {
                                preview_beat
                            };
                            let frame = slot.frame_index(anim_time, beat_for_anim);
                            let uv_elapsed = if slot.model.is_some() {
                                anim_time
                            } else {
                                preview_time
                            };
                            let uv = slot.uv_for_frame_at(frame, uv_elapsed);
                            let logical = slot.logical_size();
                            let width = logical[0].max(1.0);
                            let height = logical[1].max(1.0);
                            let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                            let scale = if height > f32::EPSILON {
                                target_height / height
                            } else {
                                PREVIEW_SCALE
                            };
                            let size = [width * scale, target_height];
                            let rotation_deg = receptor_ns
                                .receptor_off
                                .first()
                                .map(|slot| slot.def.rotation_deg as f32)
                                .unwrap_or(0.0);
                            let color = [
                                explosion_visual.diffuse[0],
                                explosion_visual.diffuse[1],
                                explosion_visual.diffuse[2],
                                explosion_visual.diffuse[3] * a,
                            ];
                            let blend = if explosion.animation.blend_add {
                                BlendMode::Add
                            } else {
                                BlendMode::Alpha
                            };
                            if let Some(model_actor) = noteskin_model_actor(
                                slot,
                                [center_x, current_row_y],
                                [
                                    size[0] * explosion_visual.zoom.max(0.0),
                                    size[1] * explosion_visual.zoom.max(0.0),
                                ],
                                uv,
                                -rotation_deg,
                                anim_time,
                                beat_for_anim,
                                color,
                                blend,
                                107,
                            ) {
                                actors.push(model_actor);
                            } else if matches!(blend, BlendMode::Add) {
                                actors.push(act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center_x, current_row_y):
                                    setsize(size[0], size[1]):
                                    zoom(explosion_visual.zoom):
                                    rotationz(-rotation_deg):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(107)
                                ));
                            } else {
                                actors.push(act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center_x, current_row_y):
                                    setsize(size[0], size[1]):
                                    zoom(explosion_visual.zoom):
                                    rotationz(-rotation_deg):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(107)
                                ));
                            }
                        };
                        if row.id == RowId::NoteSkin {
                            if let Some(ns) = state.noteskin[primary_player_idx].as_ref() {
                                draw_noteskin_preview(
                                    ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2 && primary_player_idx != P2
                                && let Some(ns) = state.noteskin[P2].as_ref()
                            {
                                draw_noteskin_preview(ns, preview_x_for(P2), &mut actors);
                            }
                        } else if row.id == RowId::MineSkin {
                            if let Some(mine_ns) = state.mine_noteskin[primary_player_idx]
                                .as_deref()
                                .or_else(|| state.noteskin[primary_player_idx].as_deref())
                            {
                                draw_mine_preview(
                                    mine_ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2 && primary_player_idx != P2
                                && let Some(mine_ns) = state.mine_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                            {
                                draw_mine_preview(mine_ns, preview_x_for(P2), &mut actors);
                            }
                        } else if row.id == RowId::ReceptorSkin {
                            if let Some(receptor_ns) = state.receptor_noteskin[primary_player_idx]
                                .as_deref()
                                .or_else(|| state.noteskin[primary_player_idx].as_deref())
                            {
                                draw_receptor_preview(
                                    receptor_ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2
                                && primary_player_idx != P2
                                && let Some(receptor_ns) = state.receptor_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                            {
                                draw_receptor_preview(receptor_ns, preview_x_for(P2), &mut actors);
                            }
                        } else if row.id == RowId::TapExplosionSkin {
                            if !state.player_profiles[primary_player_idx]
                                .tap_explosion_noteskin_hidden()
                                && let Some(explosion_ns) = state.tap_explosion_noteskin
                                    [primary_player_idx]
                                    .as_deref()
                                    .or_else(|| state.noteskin[primary_player_idx].as_deref())
                            {
                                let receptor_ns = state.receptor_noteskin[primary_player_idx]
                                    .as_deref()
                                    .or_else(|| state.noteskin[primary_player_idx].as_deref())
                                    .unwrap_or(explosion_ns);
                                draw_tap_explosion_preview(
                                    explosion_ns,
                                    receptor_ns,
                                    preview_x_for(primary_player_idx),
                                    &mut actors,
                                );
                            }
                            if show_p2
                                && primary_player_idx != P2
                                && !state.player_profiles[P2].tap_explosion_noteskin_hidden()
                                && let Some(explosion_ns) = state.tap_explosion_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                            {
                                let receptor_ns = state.receptor_noteskin[P2]
                                    .as_deref()
                                    .or_else(|| state.noteskin[P2].as_deref())
                                    .unwrap_or(explosion_ns);
                                draw_tap_explosion_preview(
                                    explosion_ns,
                                    receptor_ns,
                                    preview_x_for(P2),
                                    &mut actors,
                                );
                            }
                        }
                    }
                    // Add combo preview for "Combo Font" row showing ticking numbers
                    if row.id == RowId::ComboFont {
                        let combo_text = state.combo_preview_count.to_string();
                        let combo_zoom = 0.45;
                        // Choice indices are fixed by construction order:
                        // 0=Wendy, 1=ArialRounded, 2=Asap, 3=BebasNeue, 4=SourceCode,
                        // 5=Work, 6=WendyCursed, 7=None
                        let combo_font_for = |idx: usize| -> Option<&'static str> {
                            match idx {
                            0 => Some("wendy_combo"),
                            1 => Some("combo_arial_rounded"),
                            2 => Some("combo_asap"),
                            3 => Some("combo_bebas_neue"),
                            4 => Some("combo_source_code"),
                            5 => Some("combo_work"),
                            6 => Some("combo_wendy_cursed"),
                            _ => None,
                            }
                        };
                        let p1_choice_idx = row.selected_choice_index[primary_player_idx]
                            .min(row.choices.len().saturating_sub(1));
                        if let Some(font_name) = combo_font_for(p1_choice_idx) {
                            actors.push(act!(text:
                                font(font_name): settext(combo_text.clone()):
                                align(0.5, 0.5):
                                xy(preview_x_for(primary_player_idx), current_row_y):
                                zoom(combo_zoom): horizalign(center):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                        if show_p2 && primary_player_idx != P2 {
                            let p2_choice_idx = row.selected_choice_index[P2]
                                .min(row.choices.len().saturating_sub(1));
                            if let Some(font_name) = combo_font_for(p2_choice_idx) {
                            actors.push(act!(text:
                                font(font_name): settext(combo_text):
                                align(0.5, 0.5):
                                xy(preview_x_for(P2), current_row_y):
                                zoom(combo_zoom): horizalign(center):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                            }
                        }
                    }
                });
            });
        }
    }
    // ------------------- Description content (selected) -------------------
    actors.push(act!(quad:
        align(0.0, 1.0): xy(help_box_x, help_box_bottom_y):
        zoomto(help_box_w, help_box_h):
        diffuse(0.0, 0.0, 0.0, 0.8 * pane_alpha)
    ));
    const REVEAL_DURATION: f32 = 0.5;
    let split_help = active[P1] && active[P2];
    for player_idx in active_player_indices(active) {
        let row_idx = state.selected_row[player_idx].min(state.rows.len().saturating_sub(1));
        let Some(row) = state.rows.get(row_idx) else {
            continue;
        };
        let help_text_color = color::simply_love_rgba(player_color_index(player_idx));
        let wrap_width = if split_help || player_idx == P2 {
            (help_box_w * 0.5) - 30.0
        } else {
            help_box_w - 30.0
        };
        let help_x = if split_help {
            (player_idx as f32).mul_add(help_box_w * 0.5, help_box_x + 12.0)
        } else if player_idx == P2 {
            help_box_x + help_box_w * 0.5 + 12.0
        } else {
            help_box_x + 12.0
        };

        let num_help_lines = row.help.len().max(1);
        let time_per_line = REVEAL_DURATION / num_help_lines as f32;

        if row.help.len() > 1 {
            let line_spacing = 12.0;
            let total_height = (row.help.len() as f32 - 1.0) * line_spacing;
            let start_y = help_box_bottom_y - (help_box_h * 0.5) - (total_height * 0.5);

            for (i, help_line) in row.help.iter().enumerate() {
                let start_time = i as f32 * time_per_line;
                let end_time = start_time + time_per_line;
                let anim_time = state.help_anim_time[player_idx];
                let visible_chars = if anim_time < start_time {
                    0
                } else if anim_time >= end_time {
                    help_line.chars().count()
                } else {
                    let line_fraction = (anim_time - start_time) / time_per_line;
                    let char_count = help_line.chars().count();
                    ((char_count as f32 * line_fraction).round() as usize).min(char_count)
                };
                let visible_text: String = help_line.chars().take(visible_chars).collect();

                let line_y = (i as f32).mul_add(line_spacing, start_y);
                actors.push(act!(text:
                    font("miso"): settext(visible_text):
                    align(0.0, 0.5):
                    xy(help_x, line_y):
                    zoom(0.825):
                    diffuse(help_text_color[0], help_text_color[1], help_text_color[2], pane_alpha):
                    maxwidth(wrap_width): horizalign(left):
                    z(101)
                ));
            }
        } else {
            let help_text = row.help.join(" | ");
            let char_count = help_text.chars().count();
            let fraction = (state.help_anim_time[player_idx] / REVEAL_DURATION).clamp(0.0, 1.0);
            let visible_chars = ((char_count as f32 * fraction).round() as usize).min(char_count);
            let visible_text: String = help_text.chars().take(visible_chars).collect();

            actors.push(act!(text:
                font("miso"): settext(visible_text):
                align(0.0, 0.5):
                xy(help_x, help_box_bottom_y - (help_box_h * 0.5)):
                zoom(0.825):
                diffuse(help_text_color[0], help_text_color[1], help_text_color[2], pane_alpha):
                maxwidth(wrap_width): horizalign(left):
                z(101)
            ));
        }
    }
    actors
}

#[cfg(test)]
mod tests {
    use super::{
        HUD_OFFSET_MAX, HUD_OFFSET_MIN, HUD_OFFSET_ZERO_INDEX, NAV_INITIAL_HOLD_DELAY,
        NAV_REPEAT_SCROLL_INTERVAL, P1, Row, RowId, SpeedMod,
        handle_arcade_start_event, hud_offset_choices, is_row_visible,
        repeat_held_arcade_start, row_visibility, session_active_players,
        sync_profile_scroll_speed,
    };
    use crate::assets::i18n::{LookupKey, lookup_key};
    use crate::assets::AssetManager;
    use crate::game::profile::{self, PlayStyle, PlayerSide, Profile};
    use crate::game::scroll::ScrollSpeedSetting;
    use crate::screens::Screen;
    use crate::test_support::{compose_scenarios, notefield_bench};
    use std::time::{Duration, Instant};

    fn ensure_i18n() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            crate::assets::i18n::init("en");
        });
    }

    fn test_row(id: RowId, name: LookupKey, choices: &[&str], selected_choice_index: [usize; 2]) -> Row {
        Row {
            id,
            name,
            choices: choices.iter().map(ToString::to_string).collect(),
            selected_choice_index,
            help: Vec::new(),
            choice_difficulty_indices: None,
        }
    }

    #[test]
    fn sync_profile_scroll_speed_matches_speed_mod() {
        let mut profile = Profile::default();

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: "X".to_string(),
                value: 1.5,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::XMod(1.5));

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: "M".to_string(),
                value: 750.0,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::MMod(750.0));

        sync_profile_scroll_speed(
            &mut profile,
            &SpeedMod {
                mod_type: "C".to_string(),
                value: 600.0,
            },
        );
        assert_eq!(profile.scroll_speed, ScrollSpeedSetting::CMod(600.0));
    }

    #[test]
    fn error_bar_offsets_hide_with_empty_error_bar_mask() {
        ensure_i18n();
        let rows = vec![
            test_row(RowId::ErrorBar, lookup_key("PlayerOptions", "ErrorBar"), &["Colorful"], [0, 0]),
            test_row(RowId::ErrorBarOffsetX, lookup_key("PlayerOptions", "ErrorBarOffsetX"), &["0"], [0, 0]),
        ];
        let visibility = row_visibility(&rows, [true, false], [0, 0], [0, 0], false);
        assert!(!is_row_visible(&rows, 1, visibility));

        let visibility = row_visibility(&rows, [true, false], [0, 0], [1, 0], false);
        assert!(is_row_visible(&rows, 1, visibility));
    }

    #[test]
    fn judgment_offsets_hide_when_judgment_font_is_none() {
        ensure_i18n();
        let rows = vec![
            test_row(RowId::JudgmentFont, lookup_key("PlayerOptions", "JudgmentFont"), &["Love", "None"], [1, 0]),
            test_row(RowId::JudgmentOffsetX, lookup_key("PlayerOptions", "JudgmentOffsetX"), &["0"], [0, 0]),
        ];
        let visibility = row_visibility(&rows, [true, false], [0, 0], [0, 0], false);
        assert!(!is_row_visible(&rows, 1, visibility));

        let rows = vec![
            test_row(RowId::JudgmentFont, lookup_key("PlayerOptions", "JudgmentFont"), &["Love", "None"], [0, 0]),
            test_row(RowId::JudgmentOffsetX, lookup_key("PlayerOptions", "JudgmentOffsetX"), &["0"], [0, 0]),
        ];
        let visibility = row_visibility(&rows, [true, false], [0, 0], [0, 0], false);
        assert!(is_row_visible(&rows, 1, visibility));
    }

    #[test]
    fn combo_offsets_hide_when_all_active_players_use_none_font() {
        ensure_i18n();
        let rows = vec![
            test_row(RowId::ComboFont, lookup_key("PlayerOptions", "ComboFont"), &["Wendy", "None"], [1, 1]),
            test_row(RowId::ComboOffsetX, lookup_key("PlayerOptions", "ComboOffsetX"), &["0"], [0, 0]),
        ];
        let visibility = row_visibility(&rows, [true, true], [0, 0], [0, 0], false);
        assert!(!is_row_visible(&rows, 1, visibility));

        let rows = vec![
            test_row(RowId::ComboFont, lookup_key("PlayerOptions", "ComboFont"), &["Wendy", "None"], [1, 0]),
            test_row(RowId::ComboOffsetX, lookup_key("PlayerOptions", "ComboOffsetX"), &["0"], [0, 0]),
        ];
        let visibility = row_visibility(&rows, [true, true], [0, 0], [0, 0], false);
        assert!(is_row_visible(&rows, 1, visibility));
    }

    #[test]
    fn hud_offset_choices_cover_full_range() {
        let choices = hud_offset_choices();
        assert_eq!(choices.first().map(String::as_str), Some("-250"));
        assert_eq!(
            choices.get(HUD_OFFSET_ZERO_INDEX).map(String::as_str),
            Some("0")
        );
        assert_eq!(choices.last().map(String::as_str), Some("250"));
        assert_eq!(choices.len() as i32, HUD_OFFSET_MAX - HUD_OFFSET_MIN + 1);
    }

    #[test]
    fn held_arcade_start_keeps_advancing_rows() {
        ensure_i18n();
        let base = notefield_bench::fixture();
        let song = base.state().song.clone();

        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);

        let mut asset_manager = AssetManager::new();
        for (name, font) in compose_scenarios::bench_fonts() {
            asset_manager.register_font(name, font);
        }

        let mut state = super::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);
        let active = session_active_players();
        let first_row = state.selected_row[P1];
        assert!(handle_arcade_start_event(&mut state, &asset_manager, active, P1).is_none());
        let second_row = state.selected_row[P1];
        assert!(second_row > first_row);

        let now = Instant::now();
        state.start_held_since[P1] = Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
        state.start_last_triggered_at[P1] =
            Some(now - NAV_REPEAT_SCROLL_INTERVAL - Duration::from_millis(1));

        assert!(repeat_held_arcade_start(&mut state, &asset_manager, active, P1, now).is_none());
        assert!(state.selected_row[P1] > second_row);
    }

    #[test]
    fn held_arcade_start_stops_at_exit_row() {
        ensure_i18n();
        let base = notefield_bench::fixture();
        let song = base.state().song.clone();

        profile::set_session_play_style(PlayStyle::Single);
        profile::set_session_player_side(PlayerSide::P1);
        profile::set_session_joined(true, false);

        let mut asset_manager = AssetManager::new();
        for (name, font) in compose_scenarios::bench_fonts() {
            asset_manager.register_font(name, font);
        }

        let mut state = super::init(song, [0; 2], [0; 2], 1, Screen::SelectMusic, None);
        let active = session_active_players();
        let last_row = state.rows.len().saturating_sub(1);
        state.selected_row[P1] = last_row;
        state.prev_selected_row[P1] = last_row;

        let now = Instant::now();
        state.start_held_since[P1] = Some(now - NAV_INITIAL_HOLD_DELAY - Duration::from_millis(1));
        state.start_last_triggered_at[P1] =
            Some(now - NAV_REPEAT_SCROLL_INTERVAL - Duration::from_millis(1));

        assert!(repeat_held_arcade_start(&mut state, &asset_manager, active, P1, now).is_none());
        assert_eq!(state.selected_row[P1], last_row);
    }
}
