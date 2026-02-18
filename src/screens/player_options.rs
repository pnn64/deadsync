use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::gfx::BlendMode;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::parsing::noteskin::{
    self, NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, Quantization,
};
use crate::game::song::SongData;
use crate::screens::components::heart_bg;
use crate::screens::components::notefield::noteskin_model_actor;
use crate::screens::components::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
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
// Spacing between inline items in OptionRows (pixels at current zoom)
const INLINE_SPACING: f32 = 15.75;
const TILT_INTENSITY_MIN: f32 = 0.05;
const TILT_INTENSITY_MAX: f32 = 3.00;
const TILT_INTENSITY_STEP: f32 = 0.05;

// Match Simply Love / ScreenOptions defaults.
const VISIBLE_ROWS: usize = 10;
const ROW_START_OFFSET: f32 = -164.0;
const ROW_HEIGHT: f32 = 33.0;
const TITLE_BG_WIDTH: f32 = 127.0;

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
        if !(active[P1] && active[P2]) {
            let first_start = (p1_choice - halfsize).max(0);
            let first_end = first_start + total;
            (first_start, first_end, first_end, first_end)
        } else {
            let earliest = p1_choice.min(p2_choice);
            let first_start = (earliest - halfsize / 2).max(0);
            let first_end = first_start + halfsize;

            let latest = p1_choice.max(p2_choice);
            let second_start = (latest - halfsize / 2).max(0).max(first_end);
            let second_end = second_start + halfsize;
            (first_start, first_end, second_start, second_end)
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
    error_bar_active_mask: [u8; PLAYER_SLOTS],
) -> Vec<RowTween> {
    let total_rows = rows.len();
    if total_rows == 0 {
        return Vec::new();
    }

    let (first_row_center_y, row_step) = row_layout_params();
    let visibility = row_visibility(rows, active, error_bar_active_mask);
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
        parent_anchor_visible_index(rows, ROW_MEASURE_COUNTER, visibility);
    let judgment_tilt_anchor_visible_idx =
        parent_anchor_visible_index(rows, ROW_JUDGMENT_TILT, visibility);
    let error_bar_anchor_visible_idx = parent_anchor_visible_index(rows, ROW_ERROR_BAR, visibility);

    let mut out: Vec<RowTween> = Vec::with_capacity(total_rows);
    let mut visible_idx = 0i32;
    for i in 0..total_rows {
        let visible = is_row_visible(rows, i, visibility);
        let (f_pos, hidden) = if visible {
            let ii = visible_idx;
            visible_idx += 1;
            f_pos_for_visible_idx(ii, w, mid_pos, bottom_pos)
        } else {
            let anchor = rows.get(i).and_then(|row| match conditional_row_parent(row.name.as_str()) {
                Some(ROW_MEASURE_COUNTER) => measure_counter_anchor_visible_idx,
                Some(ROW_JUDGMENT_TILT) => judgment_tilt_anchor_visible_idx,
                Some(ROW_ERROR_BAR) => error_bar_anchor_visible_idx,
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

pub struct Row {
    pub name: String,
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
    // bit3 = Display FA+ Pane, bit4 = 10ms Blue Window.
    pub fa_plus_active_mask: [u8; PLAYER_SLOTS],
    // For Early Decent/Way Off Options row: bitmask of which options are enabled.
    // bit0 = Hide Judgments, bit1 = Hide NoteField Flash.
    pub early_dw_active_mask: [u8; PLAYER_SLOTS],
    // For Gameplay Extras row: bitmask of which options are enabled.
    // bit0 = Flash Column for Miss, bit1 = Density Graph at Top.
    pub gameplay_extras_active_mask: [u8; PLAYER_SLOTS],
    // For Gameplay Extras (More) row: bitmask of which options are enabled.
    // bit0 = Column Cues, bit1 = Display Scorebox.
    pub gameplay_extras_more_active_mask: [u8; PLAYER_SLOTS],
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
    pub active_color_index: i32,
    pub speed_mod: [SpeedMod; PLAYER_SLOTS],
    pub music_rate: f32,
    pub current_pane: OptionsPane,
    pub scroll_focus_player: usize,
    bg: heart_bg::State,
    pub nav_key_held_direction: [Option<NavDirection>; PLAYER_SLOTS],
    pub nav_key_held_since: [Option<Instant>; PLAYER_SLOTS],
    pub nav_key_last_scrolled_at: [Option<Instant>; PLAYER_SLOTS],
    pub player_profiles: [crate::game::profile::Profile; PLAYER_SLOTS],
    noteskin_names: Vec<String>,
    noteskin_cache: HashMap<String, Arc<Noteskin>>,
    noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
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
    let count = (TILT_INTENSITY_MAX / TILT_INTENSITY_STEP).round() as usize;
    let mut out = Vec::with_capacity(count);
    for i in 1..=count {
        out.push(fmt_tilt_intensity(i as f32 * TILT_INTENSITY_STEP));
    }
    out
}

// Prefer #DISPLAYBPM for reference BPM (use max of range or single value); fallback to song.max_bpm, then 120.
fn reference_bpm_for_song(song: &SongData) -> f32 {
    let bpm = song
        .display_bpm_range()
        .map(|(_, hi)| hi as f32)
        .unwrap_or(song.max_bpm as f32);
    if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        120.0
    }
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

#[inline(always)]
fn choose_different_screen_label(return_screen: Screen) -> &'static str {
    match return_screen {
        Screen::SelectCourse => "Choose a Different Course",
        _ => "Choose a Different Song",
    }
}

fn what_comes_next_choices(pane: OptionsPane, return_screen: Screen) -> Vec<String> {
    let choose_different = choose_different_screen_label(return_screen).to_string();
    match pane {
        OptionsPane::Main => vec![
            "Gameplay".to_string(),
            choose_different,
            "Advanced Modifiers".to_string(),
            "Uncommon Modifiers".to_string(),
        ],
        OptionsPane::Advanced => vec![
            "Gameplay".to_string(),
            choose_different,
            "Main Modifiers".to_string(),
            "Uncommon Modifiers".to_string(),
        ],
        OptionsPane::Uncommon => vec![
            "Gameplay".to_string(),
            choose_different,
            "Main Modifiers".to_string(),
            "Advanced Modifiers".to_string(),
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
            for (i, file_name) in crate::ui::color::FILE_DIFFICULTY_NAMES.iter().enumerate() {
                if let Some(chart) = song.charts.iter().find(|c| {
                    c.chart_type.eq_ignore_ascii_case(target_chart_type)
                        && c.difficulty.eq_ignore_ascii_case(file_name)
                        && !c.notes.is_empty()
                }) {
                    let display_name = crate::ui::color::DISPLAY_DIFFICULTY_NAMES[i];
                    stepchart_choices.push(format!("{} {}", display_name, chart.meter));
                    stepchart_choice_indices.push(i);
                }
            }
            for (i, chart) in crate::screens::select_music::edit_charts_sorted(song, target_chart_type)
                .into_iter()
                .enumerate()
            {
                let desc = chart.description.trim();
                if desc.is_empty() {
                    stepchart_choices.push(format!("Edit {}", chart.meter));
                } else {
                    stepchart_choices.push(format!("Edit {} {}", desc, chart.meter));
                }
                stepchart_choice_indices.push(crate::ui::color::FILE_DIFFICULTY_NAMES.len() + i);
            }
            // Fallback if none found (defensive; SelectMusic filters songs by play style).
            if stepchart_choices.is_empty() {
                stepchart_choices.push("(Current)".to_string());
                let base_pref = preferred_difficulty_index[session_persisted_player_idx()].min(
                    crate::ui::color::FILE_DIFFICULTY_NAMES
                        .len()
                        .saturating_sub(1),
                );
                stepchart_choice_indices.push(base_pref);
            }
            let initial_stepchart_choice_index: [usize; PLAYER_SLOTS] =
                std::array::from_fn(|player_idx| {
                    let steps_idx = chart_steps_index[player_idx];
                    let pref_idx = preferred_difficulty_index[player_idx].min(
                        crate::ui::color::FILE_DIFFICULTY_NAMES
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
            name: "Type of Speed Mod".to_string(),
            choices: vec![
                "X (multiplier)".to_string(),
                "C (constant)".to_string(),
                "M (maximum)".to_string(),
            ],
            selected_choice_index: [match speed_mod.mod_type.as_str() {
                "X" => 0,
                "C" => 1,
                "M" => 2,
                _ => 1, // Default to C
            }; PLAYER_SLOTS],
            help: vec!["Change the way arrows react to changing BPMs.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Speed Mod".to_string(),
            choices: vec![speed_mod_value_str], // Display only the current value
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Adjust the speed at which arrows travel toward the targets.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Mini".to_string(),
            choices: (-100..=150).map(|v| format!("{v}%")).collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change the size of your arrows.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Perspective".to_string(),
            choices: vec![
                "Overhead".to_string(),
                "Hallway".to_string(),
                "Distant".to_string(),
                "Incoming".to_string(),
                "Space".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change the viewing angle of the arrow stream.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteSkin".to_string(),
            choices: if noteskin_names.is_empty() {
                vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()]
            } else {
                noteskin_names.to_vec()
            },
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change the appearance of the arrows.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Judgment Font".to_string(),
            choices: vec![
                "Love".to_string(),
                "Love Chroma".to_string(),
                "Rainbowmatic".to_string(),
                "GrooveNights".to_string(),
                "Emoticon".to_string(),
                "Censored".to_string(),
                "Chromatic".to_string(),
                "ITG2".to_string(),
                "Bebas".to_string(),
                "Code".to_string(),
                "Comic Sans".to_string(),
                "Focus".to_string(),
                "Grammar".to_string(),
                "Miso".to_string(),
                "Papyrus".to_string(),
                "Roboto".to_string(),
                "Shift".to_string(),
                "Tactics".to_string(),
                "Wendy".to_string(),
                "Wendy Chroma".to_string(),
                "None".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Pick your judgment font.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Combo Font".to_string(),
            choices: vec![
                "Wendy".to_string(),
                "Arial Rounded".to_string(),
                "Asap".to_string(),
                "Bebas Neue".to_string(),
                "Source Code".to_string(),
                "Work".to_string(),
                "Wendy (Cursed)".to_string(),
                "None".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Choose the font to count your combo. This font will also be used".to_string(),
                "for the Measure Counter if that is enabled.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Hold Judgment".to_string(),
            choices: vec![
                "Love".to_string(),
                "mute".to_string(),
                "ITG2".to_string(),
                "None".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change the judgment graphics displayed for hold notes.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Background Filter".to_string(),
            choices: vec![
                "Off".to_string(),
                "Dark".to_string(),
                "Darker".to_string(),
                "Darkest".to_string(),
            ],
            selected_choice_index: [3; PLAYER_SLOTS],
            help: vec![
                "Darken the underside of the playing field.".to_string(),
                "This will partially obscure background art.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteField Offset X".to_string(),
            choices: (0..=50).map(|v| v.to_string()).collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Adjust the horizontal position of the notefield (relative to the".to_string(),
                "center).".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteField Offset Y".to_string(),
            choices: (-50..=50).map(|v| v.to_string()).collect(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Adjust the vertical position of the notefield.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Visual Delay".to_string(),
            choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
            selected_choice_index: [100; PLAYER_SLOTS],
            help: vec![
                "Player specific visual delay. Negative values shifts the arrows".to_string(),
                "upwards, while positive values move them down.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: {
                let reference_bpm = reference_bpm_for_song(song);
                let effective_bpm = f64::from(reference_bpm) * f64::from(session_music_rate);
                let bpm_str = if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
                    format!("{}", effective_bpm.round() as i32)
                } else {
                    format!("{effective_bpm:.1}")
                };
                format!("Music Rate\nbpm: {bpm_str}")
            },
            choices: vec![fmt_music_rate(session_music_rate.clamp(0.5, 3.0))],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change the native speed of the music itself.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Stepchart".to_string(),
            choices: stepchart_choices,
            selected_choice_index: initial_stepchart_choice_index,
            help: vec!["Choose the stepchart you wish to play.".to_string()],
            choice_difficulty_indices: Some(stepchart_choice_indices),
        },
        Row {
            name: "What comes next?".to_string(),
            choices: what_comes_next_choices(OptionsPane::Main, return_screen),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Go back and choose a different chart or change additional".to_string(),
                "modifiers.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: String::new(),
            choices: vec!["Exit".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![String::new()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_advanced_rows(return_screen: Screen) -> Vec<Row> {
    let mut gameplay_extras_more_choices = vec!["Column Cues".to_string()];
    if crate::game::scores::is_gs_get_scores_service_allowed() {
        gameplay_extras_more_choices.push("Display Scorebox".to_string());
    }

    vec![
        Row {
            name: "Turn".to_string(),
            choices: vec![
                "None".to_string(),
                "Mirror".to_string(),
                "Left".to_string(),
                "Right".to_string(),
                "LRMirror".to_string(),
                "UDMirror".to_string(),
                "Shuffle".to_string(),
                "Blender".to_string(),
                "Random".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Apply simple transforms to the arrow directions.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Scroll".to_string(),
            choices: vec![
                "Reverse".to_string(),
                "Split".to_string(),
                "Alternate".to_string(),
                "Cross".to_string(),
                "Centered".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change how notes scroll relative to the receptors.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Hide".to_string(),
            choices: vec![
                "Targets".to_string(),
                "Background".to_string(),
                "Combo".to_string(),
                "Life".to_string(),
                "Score".to_string(),
                "Danger".to_string(),
                "Combo Explosions".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Hide parts of the gameplay UI.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "LifeMeter Type".to_string(),
            choices: vec![
                "Standard".to_string(),
                "Surround".to_string(),
                "Vertical".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Change the style of the lifebar.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Data Visualizations".to_string(),
            choices: vec![
                "None".to_string(),
                "Target Score Graph".to_string(),
                "Step Statistics".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Show additional graphs during gameplay and evaluation.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Target Score".to_string(),
            choices: vec![
                "C-".to_string(),
                "C".to_string(),
                "C+".to_string(),
                "B-".to_string(),
                "B".to_string(),
                "B+".to_string(),
                "A-".to_string(),
                "A".to_string(),
                "A+".to_string(),
                "S-".to_string(),
                "S".to_string(),
                "S+".to_string(),
                "Machine Best".to_string(),
                "Personal Best".to_string(),
            ],
            selected_choice_index: [10; PLAYER_SLOTS], // S by default
            help: vec!["Choose a grade or score to chase.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Action On Missed Target".to_string(),
            choices: vec![
                "Nothing".to_string(),
                "Fail".to_string(),
                "Restart Song".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Decide what happens if you fall behind your target score.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Mini Indicator".to_string(),
            choices: vec![
                "None".to_string(),
                "Subtractive Scoring".to_string(),
                "Predictive Scoring".to_string(),
                "Pace Scoring".to_string(),
                "Rival Scoring".to_string(),
                "Pacemaker".to_string(),
                "Stream Progress".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Display subtractive, predictive, paced, rival, or stream-progress".to_string(),
                "mini indicator on-screen.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Gameplay Extras".to_string(),
            choices: vec![
                "Flash Column for Miss".to_string(),
                "Density Graph at Top".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Extra feedback helpers shown during gameplay.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Gameplay Extras (More)".to_string(),
            choices: gameplay_extras_more_choices,
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Additional visual effects, cues, and score display options.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Judgment Tilt".to_string(),
            choices: vec!["No".to_string(), "Yes".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Toggle left/right tilt for judgment sprites.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Judgment Tilt Intensity".to_string(),
            choices: tilt_intensity_choices(),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["How strongly to tilt judgments left/right.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Offset Indicator".to_string(),
            choices: vec!["Off".to_string(), "On".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Show zmod-style per-tap offset text (e.g. -4.52ms).".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Error Bar".to_string(),
            choices: vec![
                "Colorful".to_string(),
                "Monochrome".to_string(),
                "Text".to_string(),
                "Highlight".to_string(),
                "Average".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Toggle Colorful/Monochrome bars and Text early/late indicator.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Error Bar Trim".to_string(),
            choices: vec![
                "Off".to_string(),
                "Fantastic".to_string(),
                "Excellent".to_string(),
                "Great".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Set the worst timing window that the error bar will show.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Error Bar Options".to_string(),
            choices: vec!["Move Up".to_string(), "Multi-Tick".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Adjust where the error bar appears and whether it shows multiple tick marks."
                    .to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Measure Counter".to_string(),
            choices: vec![
                "None".to_string(),
                "8th".to_string(),
                "12th".to_string(),
                "16th".to_string(),
                "24th".to_string(),
                "32nd".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Display a count of how long you have been streaming a specific type of note."
                    .to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Measure Counter Lookahead".to_string(),
            choices: vec![
                "0".to_string(),
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Set how many upcoming stream/break segments are displayed by the measure counter."
                    .to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Measure Counter Options".to_string(),
            choices: vec![
                "Move Left".to_string(),
                "Move Up".to_string(),
                "Vertical Lookahead".to_string(),
                "Broken Run Total".to_string(),
                "Run Timer".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Change how the Measure Counter is positioned and which extra displays are enabled."
                    .to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Measure Lines".to_string(),
            choices: vec![
                "Off".to_string(),
                "Measure".to_string(),
                "Quarter".to_string(),
                "Eighth".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Display horizontal lines on the notefield to indicate quantization.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Rescore Early Hits".to_string(),
            choices: vec!["Yes".to_string(), "No".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Allow early hits of Decents and Way Offs to be rescored to better judgments."
                    .to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Early Decent/Way Off Options".to_string(),
            choices: vec![
                "Hide Judgments".to_string(),
                "Hide NoteField Flash".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Set how early Decent and Way Off judgments are visually represented.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Timing Windows".to_string(),
            choices: vec![
                "None".to_string(),
                "Way Offs".to_string(),
                "Decents + Way Offs".to_string(),
                "Fantastics + Excellents".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Disable or simplify specific timing windows used for judgments.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "FA+ Options".to_string(),
            choices: vec![
                "Display FA+ Window".to_string(),
                "Display EX Score".to_string(),
                "Display H.EX Score".to_string(),
                "Display FA+ Pane".to_string(),
                "10ms Blue Window".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Toggle FA+ style timing window display and EX/H.EX scoring visuals.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "What comes next?".to_string(),
            choices: what_comes_next_choices(OptionsPane::Advanced, return_screen),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Jump to gameplay, another modifier pane,".to_string(),
                "or back to the select screen.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: String::new(),
            choices: vec!["Exit".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![String::new()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_uncommon_rows(return_screen: Screen) -> Vec<Row> {
    vec![
        Row {
            name: "Insert".to_string(),
            choices: vec![
                "Wide".to_string(),
                "Big".to_string(),
                "Quick".to_string(),
                "BMRize".to_string(),
                "Skippy".to_string(),
                "Echo".to_string(),
                "Stomp".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Add extra notes into the chart in unusual patterns.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Remove".to_string(),
            choices: vec![
                "Little".to_string(),
                "No Mines".to_string(),
                "No Holds".to_string(),
                "No Jumps".to_string(),
                "No Hands".to_string(),
                "No Quads".to_string(),
                "No Lifts".to_string(),
                "No Fakes".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Strip specific note types out of the chart.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Holds".to_string(),
            choices: vec![
                "Planted".to_string(),
                "Floored".to_string(),
                "Twister".to_string(),
                "No Rolls".to_string(),
                "Holds To Rolls".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Twist and reshape hold notes in strange ways.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Accel Effects".to_string(),
            choices: vec![
                "Boost".to_string(),
                "Brake".to_string(),
                "Wave".to_string(),
                "Expand".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Time-based acceleration and deceleration effects.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Visual Effects".to_string(),
            choices: vec![
                "Drunk".to_string(),
                "Dizzy".to_string(),
                "Confusion".to_string(),
                "Flip".to_string(),
                "Invert".to_string(),
                "Tornado".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Wild motion applied to the note field.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Appearance Effects".to_string(),
            choices: vec![
                "Hidden".to_string(),
                "Sudden".to_string(),
                "Stealth".to_string(),
                "Blink".to_string(),
                "R.Vanish".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Fade or hide incoming arrows in unusual ways.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Attacks".to_string(),
            choices: vec!["Off".to_string(), "On".to_string(), "Random".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Toggle charts that include attack modifiers.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Characters".to_string(),
            choices: vec![
                "None".to_string(),
                "Random".to_string(),
                "Select Per Song".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Dancing characters and how they are chosen.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Hide Light Type".to_string(),
            choices: vec![
                "No Hide Lights".to_string(),
                "Hide All Lights".to_string(),
                "Hide Marquee Lights".to_string(),
                "Hide Bass Lights".to_string(),
            ],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec!["Control how cabinet lights react during gameplay.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "What comes next?".to_string(),
            choices: what_comes_next_choices(OptionsPane::Uncommon, return_screen),
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![
                "Jump to gameplay, another modifier pane,".to_string(),
                "or back to the select screen.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: String::new(),
            choices: vec!["Exit".to_string()],
            selected_choice_index: [0; PLAYER_SLOTS],
            help: vec![String::new()],
            choice_difficulty_indices: None,
        },
    ]
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
) -> (u8, u8, u8, u8, u8, u8, u8, u8, u8) {
    let mut scroll_active_mask: u8 = 0;
    let mut hide_active_mask: u8 = 0;
    let mut fa_plus_active_mask: u8 = 0;
    let mut early_dw_active_mask: u8 = 0;
    let mut gameplay_extras_active_mask: u8 = 0;
    let mut gameplay_extras_more_active_mask: u8 = 0;
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
    // Initialize Background Filter row from profile setting (Off, Dark, Darker, Darkest)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Background Filter") {
        row.selected_choice_index[player_idx] = match profile.background_filter {
            crate::game::profile::BackgroundFilter::Off => 0,
            crate::game::profile::BackgroundFilter::Dark => 1,
            crate::game::profile::BackgroundFilter::Darker => 2,
            crate::game::profile::BackgroundFilter::Darkest => 3,
        };
    }
    // Initialize Judgment Font row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Judgment Font") {
        row.selected_choice_index[player_idx] = match profile.judgment_graphic {
            crate::game::profile::JudgmentGraphic::Love => 0,
            crate::game::profile::JudgmentGraphic::LoveChroma => 1,
            crate::game::profile::JudgmentGraphic::Rainbowmatic => 2,
            crate::game::profile::JudgmentGraphic::GrooveNights => 3,
            crate::game::profile::JudgmentGraphic::Emoticon => 4,
            crate::game::profile::JudgmentGraphic::Censored => 5,
            crate::game::profile::JudgmentGraphic::Chromatic => 6,
            crate::game::profile::JudgmentGraphic::ITG2 => 7,
            crate::game::profile::JudgmentGraphic::Bebas => 8,
            crate::game::profile::JudgmentGraphic::Code => 9,
            crate::game::profile::JudgmentGraphic::ComicSans => 10,
            crate::game::profile::JudgmentGraphic::Focus => 11,
            crate::game::profile::JudgmentGraphic::Grammar => 12,
            crate::game::profile::JudgmentGraphic::Miso => 13,
            crate::game::profile::JudgmentGraphic::Papyrus => 14,
            crate::game::profile::JudgmentGraphic::Roboto => 15,
            crate::game::profile::JudgmentGraphic::Shift => 16,
            crate::game::profile::JudgmentGraphic::Tactics => 17,
            crate::game::profile::JudgmentGraphic::Wendy => 18,
            crate::game::profile::JudgmentGraphic::WendyChroma => 19,
            crate::game::profile::JudgmentGraphic::None => 20,
        };
    }
    // Initialize NoteSkin row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.name == "NoteSkin") {
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
    // Initialize Combo Font row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Combo Font") {
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
    // Initialize Hold Judgment row from profile setting (Love, mute, ITG2, None)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Hold Judgment") {
        row.selected_choice_index[player_idx] = match profile.hold_judgment_graphic {
            crate::game::profile::HoldJudgmentGraphic::Love => 0,
            crate::game::profile::HoldJudgmentGraphic::Mute => 1,
            crate::game::profile::HoldJudgmentGraphic::ITG2 => 2,
            crate::game::profile::HoldJudgmentGraphic::None => 3,
        };
    }
    // Initialize Mini row from profile (range -100..150, stored as percent).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Mini") {
        let val = profile.mini_percent.clamp(-100, 150);
        let needle = format!("{val}%");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Perspective row from profile setting (Overhead, Hallway, Distant, Incoming, Space).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Perspective") {
        row.selected_choice_index[player_idx] = match profile.perspective {
            crate::game::profile::Perspective::Overhead => 0,
            crate::game::profile::Perspective::Hallway => 1,
            crate::game::profile::Perspective::Distant => 2,
            crate::game::profile::Perspective::Incoming => 3,
            crate::game::profile::Perspective::Space => 4,
        };
    }
    // Initialize NoteField Offset X from profile (0..50, non-negative; P1 uses negative sign at render time)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "NoteField Offset X") {
        let val = profile.note_field_offset_x.clamp(0, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize NoteField Offset Y from profile (-50..50)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "NoteField Offset Y") {
        let val = profile.note_field_offset_y.clamp(-50, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Visual Delay from profile (-100..100ms)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Visual Delay") {
        let val = profile.visual_delay_ms.clamp(-100, 100);
        let needle = format!("{val}ms");
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index[player_idx] = idx;
        }
    }
    // Initialize Judgment Tilt rows from profile (Simply Love semantics).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Judgment Tilt") {
        row.selected_choice_index[player_idx] = if profile.judgment_tilt { 1 } else { 0 };
    }
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.name == "Judgment Tilt Intensity")
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
    // Initialize Error Bar rows from profile (Simply Love semantics).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Offset Indicator") {
        row.selected_choice_index[player_idx] = if profile.error_ms_display { 1 } else { 0 };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Error Bar") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Data Visualizations") {
        row.selected_choice_index[player_idx] = match profile.data_visualizations {
            crate::game::profile::DataVisualizations::None => 0,
            crate::game::profile::DataVisualizations::TargetScoreGraph => 1,
            crate::game::profile::DataVisualizations::StepStatistics => 2,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Target Score") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "LifeMeter Type") {
        row.selected_choice_index[player_idx] = match profile.lifemeter_type {
            crate::game::profile::LifeMeterType::Standard => 0,
            crate::game::profile::LifeMeterType::Surround => 1,
            crate::game::profile::LifeMeterType::Vertical => 2,
        };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Error Bar Trim") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Error Bar Options") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Measure Counter") {
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
        .find(|r| r.name == "Measure Counter Lookahead")
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
        .find(|r| r.name == "Measure Counter Options")
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Measure Lines") {
        row.selected_choice_index[player_idx] = match profile.measure_lines {
            crate::game::profile::MeasureLines::Off => 0,
            crate::game::profile::MeasureLines::Measure => 1,
            crate::game::profile::MeasureLines::Quarter => 2,
            crate::game::profile::MeasureLines::Eighth => 3,
        };
    }
    // Initialize Turn row from profile setting.
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Turn") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Rescore Early Hits") {
        row.selected_choice_index[player_idx] = if profile.rescore_early_hits { 0 } else { 1 };
    }
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Mini Indicator") {
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
    if let Some(row) = rows
        .iter_mut()
        .find(|r| r.name == "Early Decent/Way Off Options")
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "FA+ Options") {
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

    // Initialize Gameplay Extras row from profile (multi-choice toggle group).
    if profile.column_flash_on_miss {
        gameplay_extras_active_mask |= 1u8 << 0;
    }
    if profile.nps_graph_at_top {
        gameplay_extras_active_mask |= 1u8 << 1;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Gameplay Extras") {
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

    // Initialize Gameplay Extras (More) row from profile (multi-choice toggle group).
    if profile.column_cues {
        gameplay_extras_more_active_mask |= 1u8 << 0;
    }
    if profile.display_scorebox {
        gameplay_extras_more_active_mask |= 1u8 << 1;
    }
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Gameplay Extras (More)") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Hide") {
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
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Scroll") {
        use crate::game::profile::ScrollOption;
        // Map profile flags onto row choice indices.
        if profile.scroll_option.contains(ScrollOption::Reverse)
            && let Some(idx) = row.choices.iter().position(|c| c == "Reverse")
            && idx < 8
        {
            scroll_active_mask |= 1u8 << (idx as u8);
        }
        if profile.scroll_option.contains(ScrollOption::Split)
            && let Some(idx) = row.choices.iter().position(|c| c == "Split")
            && idx < 8
        {
            scroll_active_mask |= 1u8 << (idx as u8);
        }
        if profile.scroll_option.contains(ScrollOption::Alternate)
            && let Some(idx) = row.choices.iter().position(|c| c == "Alternate")
            && idx < 8
        {
            scroll_active_mask |= 1u8 << (idx as u8);
        }
        if profile.scroll_option.contains(ScrollOption::Cross)
            && let Some(idx) = row.choices.iter().position(|c| c == "Cross")
            && idx < 8
        {
            scroll_active_mask |= 1u8 << (idx as u8);
        }
        if profile.scroll_option.contains(ScrollOption::Centered)
            && let Some(idx) = row.choices.iter().position(|c| c == "Centered")
            && idx < 8
        {
            scroll_active_mask |= 1u8 << (idx as u8);
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
    (
        scroll_active_mask,
        hide_active_mask,
        fa_plus_active_mask,
        early_dw_active_mask,
        gameplay_extras_active_mask,
        gameplay_extras_more_active_mask,
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
            crate::ui::color::FILE_DIFFICULTY_NAMES
                .len()
                .saturating_sub(1),
        );
        if steps_idx < crate::ui::color::FILE_DIFFICULTY_NAMES.len() {
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
        fa_plus_active_mask_p1,
        early_dw_active_mask_p1,
        gameplay_extras_active_mask_p1,
        gameplay_extras_more_active_mask_p1,
        error_bar_active_mask_p1,
        error_bar_options_active_mask_p1,
        measure_counter_options_active_mask_p1,
    ) = apply_profile_defaults(&mut rows, &player_profiles[P1], P1);
    let (
        scroll_active_mask_p2,
        hide_active_mask_p2,
        fa_plus_active_mask_p2,
        early_dw_active_mask_p2,
        gameplay_extras_active_mask_p2,
        gameplay_extras_more_active_mask_p2,
        error_bar_active_mask_p2,
        error_bar_options_active_mask_p2,
        measure_counter_options_active_mask_p2,
    ) = apply_profile_defaults(&mut rows, &player_profiles[P2], P2);

    let cols_per_player = noteskin_cols_per_player(crate::game::profile::get_session_play_style());
    let mut initial_noteskin_names = vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()];
    for profile in &player_profiles {
        let name = profile.noteskin.as_str().to_string();
        if !initial_noteskin_names.iter().any(|n| n == &name) {
            initial_noteskin_names.push(name);
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
    let active = session_active_players();
    let row_tweens = init_row_tweens(
        &rows,
        [0; PLAYER_SLOTS],
        active,
        [error_bar_active_mask_p1, error_bar_active_mask_p2],
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
        player_profiles,
        noteskin_names,
        noteskin_cache,
        noteskin: noteskin_previews,
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
    match play_style {
        crate::game::profile::PlayStyle::Versus => [true, true],
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Double => {
            match side {
                crate::game::profile::PlayerSide::P1 => [true, false],
                crate::game::profile::PlayerSide::P2 => [false, true],
            }
        }
    }
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

const ROW_MEASURE_COUNTER: &str = "Measure Counter";
const ROW_MEASURE_COUNTER_LOOKAHEAD: &str = "Measure Counter Lookahead";
const ROW_MEASURE_COUNTER_OPTIONS: &str = "Measure Counter Options";
const ROW_JUDGMENT_TILT: &str = "Judgment Tilt";
const ROW_JUDGMENT_TILT_INTENSITY: &str = "Judgment Tilt Intensity";
const ROW_ERROR_BAR: &str = "Error Bar";
const ROW_ERROR_BAR_TRIM: &str = "Error Bar Trim";
const ROW_ERROR_BAR_OPTIONS: &str = "Error Bar Options";

#[derive(Clone, Copy, Debug)]
struct RowVisibility {
    show_measure_counter_children: bool,
    show_judgment_tilt_intensity: bool,
    show_error_bar_children: bool,
}

#[inline(always)]
fn row_visible_with_flags(row_name: &str, visibility: RowVisibility) -> bool {
    if row_name == ROW_MEASURE_COUNTER_LOOKAHEAD || row_name == ROW_MEASURE_COUNTER_OPTIONS {
        return visibility.show_measure_counter_children;
    }
    if row_name == ROW_JUDGMENT_TILT_INTENSITY {
        return visibility.show_judgment_tilt_intensity;
    }
    if row_name == ROW_ERROR_BAR_TRIM || row_name == ROW_ERROR_BAR_OPTIONS {
        return visibility.show_error_bar_children;
    }
    true
}

#[inline(always)]
fn conditional_row_parent(row_name: &str) -> Option<&'static str> {
    if row_name == ROW_MEASURE_COUNTER_LOOKAHEAD || row_name == ROW_MEASURE_COUNTER_OPTIONS {
        return Some(ROW_MEASURE_COUNTER);
    }
    if row_name == ROW_JUDGMENT_TILT_INTENSITY {
        return Some(ROW_JUDGMENT_TILT);
    }
    if row_name == ROW_ERROR_BAR_TRIM || row_name == ROW_ERROR_BAR_OPTIONS {
        return Some(ROW_ERROR_BAR);
    }
    None
}

fn measure_counter_children_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.name == ROW_MEASURE_COUNTER) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
fn judgment_tilt_intensity_visible(rows: &[Row], active: [bool; PLAYER_SLOTS]) -> bool {
    let Some(row) = rows.iter().find(|r| r.name == ROW_JUDGMENT_TILT) else {
        return true;
    };
    let max_choice = row.choices.len().saturating_sub(1);
    let mut any_active = false;
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
        any_active = true;
        let choice_idx = row.selected_choice_index[player_idx].min(max_choice);
        if choice_idx != 0 {
            return true;
        }
    }
    !any_active
}

fn error_bar_children_visible(active: [bool; PLAYER_SLOTS], error_bar_active_mask: [u8; PLAYER_SLOTS]) -> bool {
    let mut any_active = false;
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
        any_active = true;
        if crate::game::profile::normalize_error_bar_mask(error_bar_active_mask[player_idx]) != 0 {
            return true;
        }
    }
    !any_active
}

#[inline(always)]
fn row_visibility(
    rows: &[Row],
    active: [bool; PLAYER_SLOTS],
    error_bar_active_mask: [u8; PLAYER_SLOTS],
) -> RowVisibility {
    RowVisibility {
        show_measure_counter_children: measure_counter_children_visible(rows, active),
        show_judgment_tilt_intensity: judgment_tilt_intensity_visible(rows, active),
        show_error_bar_children: error_bar_children_visible(active, error_bar_active_mask),
    }
}

#[inline(always)]
fn is_row_visible(rows: &[Row], row_idx: usize, visibility: RowVisibility) -> bool {
    rows.get(row_idx)
        .is_some_and(|row| row_visible_with_flags(row.name.as_str(), visibility))
}

fn count_visible_rows(rows: &[Row], visibility: RowVisibility) -> usize {
    rows.iter()
        .filter(|row| row_visible_with_flags(row.name.as_str(), visibility))
        .count()
}

fn row_to_visible_index(
    rows: &[Row],
    row_idx: usize,
    visibility: RowVisibility,
) -> Option<usize> {
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

fn fallback_visible_row(
    rows: &[Row],
    row_idx: usize,
    visibility: RowVisibility,
) -> Option<usize> {
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
    parent_name: &str,
    visibility: RowVisibility,
) -> Option<i32> {
    rows.iter()
        .position(|row| row.name == parent_name)
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
    let visibility = row_visibility(&state.rows, active, state.error_bar_active_mask);
    for player_idx in 0..PLAYER_SLOTS {
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
fn row_is_shared(row_name: &str) -> bool {
    row_name.is_empty() || row_name == "What comes next?" || row_name.starts_with("Music Rate")
}

#[inline(always)]
fn row_shows_all_choices_inline(row_name: &str) -> bool {
    row_name == "Perspective"
        || row_name == "Background Filter"
        || row_name == "Stepchart"
        || row_name == "What comes next?"
        || row_name == "Action On Missed Target"
        || row_name == "Error Bar"
        || row_name == "Error Bar Trim"
        || row_name == "Error Bar Options"
        || row_name == "Offset Indicator"
        || row_name == "Measure Counter"
        || row_name == "Measure Counter Lookahead"
        || row_name == "Measure Counter Options"
        || row_name == "Measure Lines"
        || row_name == "Timing Windows"
        || row_name == ROW_JUDGMENT_TILT
        || row_name == "Mini Indicator"
        || row_name == "Turn"
        || row_name == "Scroll"
        || row_name == "Hide"
        || row_name == "LifeMeter Type"
        || row_name == "Data Visualizations"
        || row_name.starts_with("Gameplay Extras")
        || row_name == "Rescore Early Hits"
        || row_name == "Early Decent/Way Off Options"
        || row_name == "FA+ Options"
        || row_name == "Insert"
        || row_name == "Remove"
        || row_name == "Holds"
        || row_name == "Accel Effects"
        || row_name == "Visual Effects"
        || row_name == "Appearance Effects"
        || row_name == "Attacks"
        || row_name == "Characters"
        || row_name == "Hide Light Type"
}

#[inline(always)]
fn measure_option_text(asset_manager: &AssetManager, text: &str, zoom: f32) -> (f32, f32) {
    let mut out_w = 40.0_f32;
    let mut out_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            out_h = (metrics_font.height as f32).max(1.0) * zoom;
            let mut w =
                crate::ui::font::measure_line_width_logical(metrics_font, text, all_fonts) as f32;
            if !w.is_finite() || w <= 0.0 {
                w = 1.0;
            }
            out_w = w * zoom;
        });
    });
    (out_w, out_h)
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
        state.error_bar_active_mask,
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

    if row.name.is_empty() {
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

    if row_shows_all_choices_inline(&row.name) {
        if row.choices.is_empty() {
            return None;
        }
        let spacing = INLINE_SPACING;
        let choice_inner_left = widescale(162.0, 176.0);
        let mut widths: Vec<f32> = Vec::with_capacity(row.choices.len());
        let mut text_h: f32 = 16.0;
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |metrics_font| {
                text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                for text in &row.choices {
                    let mut w =
                        crate::ui::font::measure_line_width_logical(metrics_font, text, all_fonts)
                            as f32;
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

        let sel_idx = row.selected_choice_index[player_idx].min(widths.len().saturating_sub(1));
        let mut left_x = choice_inner_left;
        for w in widths.iter().take(sel_idx) {
            left_x += *w + spacing;
        }
        let draw_w = widths[sel_idx];
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
    if row.name.starts_with("Music Rate") {
        center_x = music_rate_center_x;
    } else if player_idx == P2 {
        center_x = screen_center_x().mul_add(2.0, -center_x);
    }

    let display_text = if row.name == "Speed Mod" {
        match state.speed_mod[player_idx].mod_type.as_str() {
            "X" => format!("{:.2}x", state.speed_mod[player_idx].value),
            "C" => format!("C{}", state.speed_mod[player_idx].value as i32),
            "M" => format!("M{}", state.speed_mod[player_idx].value as i32),
            _ => String::new(),
        }
    } else if row.name == "Type of Speed Mod" {
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

fn change_choice_for_player(state: &mut State, player_idx: usize, delta: isize) {
    if state.rows.is_empty() {
        return;
    }
    let player_idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[player_idx].min(state.rows.len().saturating_sub(1));
    let row_name = state.rows[row_index].name.clone();
    if row_name.is_empty() {
        return;
    }
    let is_shared = row_is_shared(&row_name);

    // Shared row: Music Rate
    if row_name.starts_with("Music Rate") {
        let row = &mut state.rows[row_index];
        let increment = 0.01f32;
        let min_rate = 0.05f32;
        let max_rate = 3.00f32;
        state.music_rate += delta as f32 * increment;
        state.music_rate = (state.music_rate / increment).round() * increment;
        state.music_rate = state.music_rate.clamp(min_rate, max_rate);
        row.choices[0] = fmt_music_rate(state.music_rate);

        let reference_bpm = reference_bpm_for_song(&state.song);
        let effective_bpm = f64::from(reference_bpm) * f64::from(state.music_rate);
        let bpm_str = if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
            format!("{}", effective_bpm.round() as i32)
        } else {
            format!("{effective_bpm:.1}")
        };
        row.name = format!("Music Rate\nbpm: {bpm_str}");

        audio::play_sfx("assets/sounds/change_value.ogg");
        crate::game::profile::set_session_music_rate(state.music_rate);
        audio::set_music_rate(state.music_rate);
        return;
    }

    // Per-player row: Speed Mod numeric
    if row_name == "Speed Mod" {
        let speed_mod = &mut state.speed_mod[player_idx];
        let (upper, increment) = match speed_mod.mod_type.as_str() {
            "X" => (20.0, 0.05),
            "C" | "M" => (2000.0, 5.0),
            _ => (1.0, 0.1),
        };
        speed_mod.value += delta as f32 * increment;
        speed_mod.value = (speed_mod.value / increment).round() * increment;
        speed_mod.value = speed_mod.value.clamp(increment, upper);
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

    if row_name == "Type of Speed Mod" {
        let new_type = match row.selected_choice_index[player_idx] {
            0 => "X",
            1 => "C",
            2 => "M",
            _ => "C",
        };

        let speed_mod = &mut state.speed_mod[player_idx];
        let old_type = speed_mod.mod_type.clone();
        let old_value = speed_mod.value;
        let reference_bpm = reference_bpm_for_song(&state.song);
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
    } else if row_name == "Turn" {
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
    } else if row_name == "Rescore Early Hits" {
        let enabled = row.selected_choice_index[player_idx] == 0;
        state.player_profiles[player_idx].rescore_early_hits = enabled;
        if should_persist {
            crate::game::profile::update_rescore_early_hits_for_side(persist_side, enabled);
        }
    } else if row_name == "Mini Indicator" {
        let choice = row
            .choices
            .get(row.selected_choice_index[player_idx])
            .map(|s| s.as_str())
            .unwrap_or("None");
        let mini_indicator = match choice {
            "Subtractive Scoring" => crate::game::profile::MiniIndicator::SubtractiveScoring,
            "Predictive Scoring" => crate::game::profile::MiniIndicator::PredictiveScoring,
            "Pace Scoring" => crate::game::profile::MiniIndicator::PaceScoring,
            "Rival Scoring" => crate::game::profile::MiniIndicator::RivalScoring,
            "Pacemaker" => crate::game::profile::MiniIndicator::Pacemaker,
            "Stream Progress" => crate::game::profile::MiniIndicator::StreamProg,
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
    } else if row_name == "Background Filter" {
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
    } else if row_name == "Mini" {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx]) {
            let trimmed = choice.trim_end_matches('%');
            if let Ok(val) = trimmed.parse::<i32>() {
                state.player_profiles[player_idx].mini_percent = val;
                if should_persist {
                    crate::game::profile::update_mini_percent_for_side(persist_side, val);
                }
            }
        }
    } else if row_name == "Perspective" {
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
    } else if row_name == "NoteField Offset X" {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].note_field_offset_x = raw;
            if should_persist {
                crate::game::profile::update_notefield_offset_x_for_side(persist_side, raw);
            }
        }
    } else if row_name == "NoteField Offset Y" {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.parse::<i32>()
        {
            state.player_profiles[player_idx].note_field_offset_y = raw;
            if should_persist {
                crate::game::profile::update_notefield_offset_y_for_side(persist_side, raw);
            }
        }
    } else if row_name == "Visual Delay" {
        if let Some(choice) = row.choices.get(row.selected_choice_index[player_idx])
            && let Ok(raw) = choice.trim_end_matches("ms").parse::<i32>()
        {
            state.player_profiles[player_idx].visual_delay_ms = raw;
            if should_persist {
                crate::game::profile::update_visual_delay_ms_for_side(persist_side, raw);
            }
        }
    } else if row_name == ROW_JUDGMENT_TILT {
        let enabled = row.selected_choice_index[player_idx] == 1;
        state.player_profiles[player_idx].judgment_tilt = enabled;
        if should_persist {
            crate::game::profile::update_judgment_tilt_for_side(persist_side, enabled);
        }
        visibility_changed = true;
    } else if row_name == "Judgment Tilt Intensity" {
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
    } else if row_name == "LifeMeter Type" {
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
    } else if row_name == "Data Visualizations" {
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
    } else if row_name == "Target Score" {
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
    } else if row_name == "Offset Indicator" {
        let enabled = row.selected_choice_index[player_idx] != 0;
        state.player_profiles[player_idx].error_ms_display = enabled;
        if should_persist {
            crate::game::profile::update_error_ms_display_for_side(persist_side, enabled);
        }
    } else if row_name == "Error Bar" {
        // Multi-select row toggled with Start; Left/Right only moves cursor.
    } else if row_name == "Error Bar Trim" {
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
    } else if row_name == "Measure Counter" {
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
    } else if row_name == "Measure Counter Lookahead" {
        let lookahead = (row.selected_choice_index[player_idx] as u8).min(4);
        state.player_profiles[player_idx].measure_counter_lookahead = lookahead;
        if should_persist {
            crate::game::profile::update_measure_counter_lookahead_for_side(
                persist_side,
                lookahead,
            );
        }
    } else if row_name == "Measure Lines" {
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
    } else if row_name == "Judgment Font" {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::JudgmentGraphic::Love,
            1 => crate::game::profile::JudgmentGraphic::LoveChroma,
            2 => crate::game::profile::JudgmentGraphic::Rainbowmatic,
            3 => crate::game::profile::JudgmentGraphic::GrooveNights,
            4 => crate::game::profile::JudgmentGraphic::Emoticon,
            5 => crate::game::profile::JudgmentGraphic::Censored,
            6 => crate::game::profile::JudgmentGraphic::Chromatic,
            7 => crate::game::profile::JudgmentGraphic::ITG2,
            8 => crate::game::profile::JudgmentGraphic::Bebas,
            9 => crate::game::profile::JudgmentGraphic::Code,
            10 => crate::game::profile::JudgmentGraphic::ComicSans,
            11 => crate::game::profile::JudgmentGraphic::Focus,
            12 => crate::game::profile::JudgmentGraphic::Grammar,
            13 => crate::game::profile::JudgmentGraphic::Miso,
            14 => crate::game::profile::JudgmentGraphic::Papyrus,
            15 => crate::game::profile::JudgmentGraphic::Roboto,
            16 => crate::game::profile::JudgmentGraphic::Shift,
            17 => crate::game::profile::JudgmentGraphic::Tactics,
            18 => crate::game::profile::JudgmentGraphic::Wendy,
            19 => crate::game::profile::JudgmentGraphic::WendyChroma,
            20 => crate::game::profile::JudgmentGraphic::None,
            _ => crate::game::profile::JudgmentGraphic::Love,
        };
        state.player_profiles[player_idx].judgment_graphic = setting;
        if should_persist {
            crate::game::profile::update_judgment_graphic_for_side(persist_side, setting);
        }
    } else if row_name == "Combo Font" {
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
    } else if row_name == "Hold Judgment" {
        let setting = match row.selected_choice_index[player_idx] {
            0 => crate::game::profile::HoldJudgmentGraphic::Love,
            1 => crate::game::profile::HoldJudgmentGraphic::Mute,
            2 => crate::game::profile::HoldJudgmentGraphic::ITG2,
            3 => crate::game::profile::HoldJudgmentGraphic::None,
            _ => crate::game::profile::HoldJudgmentGraphic::Love,
        };
        state.player_profiles[player_idx].hold_judgment_graphic = setting;
        if should_persist {
            crate::game::profile::update_hold_judgment_graphic_for_side(persist_side, setting);
        }
    } else if row_name == "NoteSkin" {
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
        let cols_per_player =
            noteskin_cols_per_player(crate::game::profile::get_session_play_style());
        state.noteskin[player_idx] =
            cached_or_load_noteskin(&mut state.noteskin_cache, &setting, cols_per_player);
    } else if row_name == "Stepchart" {
        if let Some(diff_indices) = &row.choice_difficulty_indices
            && let Some(&difficulty_idx) = diff_indices.get(row.selected_choice_index[player_idx])
        {
            state.chart_steps_index[player_idx] = difficulty_idx;
            if difficulty_idx < crate::ui::color::FILE_DIFFICULTY_NAMES.len() {
                state.chart_difficulty_index[player_idx] = difficulty_idx;
            }
        }
    }

    if visibility_changed {
        sync_selected_rows_with_visibility(state, session_active_players());
    }
    audio::play_sfx("assets/sounds/change_value.ogg");
}

pub fn apply_choice_delta(state: &mut State, player_idx: usize, delta: isize) {
    change_choice_for_player(state, player_idx, delta);
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs
pub fn update(state: &mut State, dt: f32, asset_manager: &AssetManager) {
    // Update preview animation time and beat based on song BPM
    state.preview_time += dt;

    // Calculate beat increment based on the song's BPM
    // Use the song's min_bpm (or max_bpm if they're the same)
    let bpm = if (state.song.min_bpm - state.song.max_bpm).abs() < 1e-6 {
        state.song.min_bpm as f32
    } else {
        // For variable BPM songs, use min_bpm as a reasonable default
        state.song.min_bpm as f32
    };
    let bpm = if bpm > 0.0 { bpm } else { 120.0 }; // Fallback to 120 BPM

    let beats_per_second = bpm / 60.0;
    state.preview_beat += dt * beats_per_second;
    let active = session_active_players();
    let now = Instant::now();
    sync_selected_rows_with_visibility(state, active);

    // Hold-to-scroll per player.
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
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
        let visibility = row_visibility(&state.rows, active, state.error_bar_active_mask);
        match direction {
            NavDirection::Up => {
                if let Some(next_row) = next_visible_row(
                    &state.rows,
                    state.selected_row[player_idx],
                    NavDirection::Up,
                    visibility,
                ) {
                    state.selected_row[player_idx] = next_row;
                }
            }
            NavDirection::Down => {
                if let Some(next_row) = next_visible_row(
                    &state.rows,
                    state.selected_row[player_idx],
                    NavDirection::Down,
                    visibility,
                ) {
                    state.selected_row[player_idx] = next_row;
                }
            }
            NavDirection::Left => {
                change_choice_for_player(state, player_idx, -1);
            }
            NavDirection::Right => {
                change_choice_for_player(state, player_idx, 1);
            }
        }
        state.nav_key_last_scrolled_at[player_idx] = Some(now);
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
    for player_idx in 0..PLAYER_SLOTS {
        if active[player_idx] {
            state.help_anim_time[player_idx] += dt;
        }
    }

    // If either player is on the Combo Font row, tick the preview combo once per second.
    let mut combo_row_active = false;
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
        if let Some(row) = state.rows.get(state.selected_row[player_idx])
            && row.name == "Combo Font"
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
            state.error_bar_active_mask,
        );
    } else {
        let visibility = row_visibility(&state.rows, active, state.error_bar_active_mask);
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
                parent_anchor_visible_index(&state.rows, ROW_MEASURE_COUNTER, visibility);
            let judgment_tilt_anchor_visible_idx =
                parent_anchor_visible_index(&state.rows, ROW_JUDGMENT_TILT, visibility);
            let error_bar_anchor_visible_idx =
                parent_anchor_visible_index(&state.rows, ROW_ERROR_BAR, visibility);
            let mut visible_idx = 0i32;
            for i in 0..total_rows {
                let visible = is_row_visible(&state.rows, i, visibility);
                let (f_pos, hidden) = if visible {
                    let ii = visible_idx;
                    visible_idx += 1;
                    f_pos_for_visible_idx(ii, w, mid_pos, bottom_pos)
                } else {
                    let anchor =
                        state.rows
                            .get(i)
                            .and_then(|row| match conditional_row_parent(row.name.as_str()) {
                                Some(ROW_MEASURE_COUNTER) => measure_counter_anchor_visible_idx,
                                Some(ROW_JUDGMENT_TILT) => judgment_tilt_anchor_visible_idx,
                                Some(ROW_ERROR_BAR) => error_bar_anchor_visible_idx,
                                _ => None,
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
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
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
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
        let Some((to_x, to_y, to_w, to_h)) =
            cursor_dest_for_player(state, asset_manager, player_idx)
        else {
            continue;
        };

        if !state.cursor_initialized[player_idx] {
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
    for player_idx in 0..PLAYER_SLOTS {
        if state.cursor_t[player_idx] < 1.0 {
            if CURSOR_TWEEN_SECONDS > 0.0 {
                state.cursor_t[player_idx] =
                    (state.cursor_t[player_idx] + dt / CURSOR_TWEEN_SECONDS).min(1.0);
            } else {
                state.cursor_t[player_idx] = 1.0;
            }
        }
    }
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

fn toggle_scroll_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.name != "Scroll" {
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
        if row.name != "Hide" {
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

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_fa_plus_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.name != "FA+ Options" {
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
    state.player_profiles[idx].show_fa_plus_window = window_enabled;
    state.player_profiles[idx].show_ex_score = ex_enabled;
    state.player_profiles[idx].show_hard_ex_score = hard_ex_enabled;
    state.player_profiles[idx].show_fa_plus_pane = pane_enabled;
    state.player_profiles[idx].fa_plus_10ms_blue_window = ten_ms_enabled;
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
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_error_bar_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.name != "Error Bar" {
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
        if row.name != "Error Bar Options" {
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
        if row.name != "Measure Counter Options" {
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
        if row.name != "Early Decent/Way Off Options" {
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
        if row.name != "Gameplay Extras" {
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

    if (state.gameplay_extras_active_mask[idx] & bit) != 0 {
        state.gameplay_extras_active_mask[idx] &= !bit;
    } else {
        state.gameplay_extras_active_mask[idx] |= bit;
    }

    let column_flash_on_miss = (state.gameplay_extras_active_mask[idx] & (1u8 << 0)) != 0;
    let nps_graph_at_top = (state.gameplay_extras_active_mask[idx] & (1u8 << 1)) != 0;
    let subtractive_scoring = state.player_profiles[idx].subtractive_scoring;
    let pacemaker = state.player_profiles[idx].pacemaker;

    state.player_profiles[idx].column_flash_on_miss = column_flash_on_miss;
    state.player_profiles[idx].nps_graph_at_top = nps_graph_at_top;

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
    }

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_gameplay_extras_more_row(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let row_index = state.selected_row[idx];
    if let Some(row) = state.rows.get(row_index) {
        if row.name != "Gameplay Extras (More)" {
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
        fa_plus_active_mask_p1,
        early_dw_active_mask_p1,
        gameplay_extras_active_mask_p1,
        gameplay_extras_more_active_mask_p1,
        error_bar_active_mask_p1,
        error_bar_options_active_mask_p1,
        measure_counter_options_active_mask_p1,
    ) = apply_profile_defaults(&mut rows, &state.player_profiles[P1], P1);
    let (
        scroll_active_mask_p2,
        hide_active_mask_p2,
        fa_plus_active_mask_p2,
        early_dw_active_mask_p2,
        gameplay_extras_active_mask_p2,
        gameplay_extras_more_active_mask_p2,
        error_bar_active_mask_p2,
        error_bar_options_active_mask_p2,
        measure_counter_options_active_mask_p2,
    ) = apply_profile_defaults(&mut rows, &state.player_profiles[P2], P2);
    state.rows = rows;
    state.scroll_active_mask = [scroll_active_mask_p1, scroll_active_mask_p2];
    state.hide_active_mask = [hide_active_mask_p1, hide_active_mask_p2];
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
        state.error_bar_active_mask,
    );
}

fn switch_to_pane(state: &mut State, pane: OptionsPane) {
    if state.current_pane == pane {
        return;
    }
    audio::play_sfx("assets/sounds/start.ogg");

    state.nav_key_held_direction = [None; PLAYER_SLOTS];
    state.nav_key_held_since = [None; PLAYER_SLOTS];
    state.nav_key_last_scrolled_at = [None; PLAYER_SLOTS];

    state.pane_transition = match state.pane_transition {
        PaneTransition::FadingOut { t, .. } => PaneTransition::FadingOut { target: pane, t },
        _ => PaneTransition::FadingOut {
            target: pane,
            t: 0.0,
        },
    };
}

fn handle_nav_event(
    state: &mut State,
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
        let visibility = row_visibility(&state.rows, active, state.error_bar_active_mask);
        match dir {
            NavDirection::Up => {
                if let Some(next_row) = next_visible_row(
                    &state.rows,
                    state.selected_row[player_idx],
                    NavDirection::Up,
                    visibility,
                ) {
                    state.selected_row[player_idx] = next_row;
                }
            }
            NavDirection::Down => {
                if let Some(next_row) = next_visible_row(
                    &state.rows,
                    state.selected_row[player_idx],
                    NavDirection::Down,
                    visibility,
                ) {
                    state.selected_row[player_idx] = next_row;
                }
            }
            NavDirection::Left => apply_choice_delta(state, player_idx, -1),
            NavDirection::Right => apply_choice_delta(state, player_idx, 1),
        }
        on_nav_press(state, player_idx, dir);
    } else {
        on_nav_release(state, player_idx, dir);
    }
}

fn handle_start_event(
    state: &mut State,
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
    let Some(row) = state.rows.get(row_index) else {
        return None;
    };
    if row.name == "Scroll" {
        toggle_scroll_row(state, player_idx);
        return None;
    }
    if row.name == "Hide" {
        toggle_hide_row(state, player_idx);
        return None;
    }
    if row.name == "Gameplay Extras" {
        toggle_gameplay_extras_row(state, player_idx);
        return None;
    }
    if row.name == "Gameplay Extras (More)" {
        toggle_gameplay_extras_more_row(state, player_idx);
        return None;
    }
    if row.name == "Error Bar" {
        toggle_error_bar_row(state, player_idx);
        return None;
    }
    if row.name == "Error Bar Options" {
        toggle_error_bar_options_row(state, player_idx);
        return None;
    }
    if row.name == "Measure Counter Options" {
        toggle_measure_counter_options_row(state, player_idx);
        return None;
    }
    if row.name == "FA+ Options" {
        toggle_fa_plus_row(state, player_idx);
        return None;
    }
    if row.name == "Early Decent/Way Off Options" {
        toggle_early_dw_row(state, player_idx);
        return None;
    }
    if row_index == num_rows.saturating_sub(1)
        && let Some(what_comes_next_row) = state.rows.get(num_rows.saturating_sub(2))
        && what_comes_next_row.name == "What comes next?"
    {
        let choice_idx = what_comes_next_row.selected_choice_index[player_idx];
        if let Some(choice) = what_comes_next_row.choices.get(choice_idx) {
            match choice.as_str() {
                "Gameplay" => return Some(ScreenAction::Navigate(Screen::Gameplay)),
                c if c == choose_different_screen_label(state.return_screen) => {
                    return Some(ScreenAction::Navigate(state.return_screen));
                }
                "Advanced Modifiers" => switch_to_pane(state, OptionsPane::Advanced),
                "Uncommon Modifiers" => switch_to_pane(state, OptionsPane::Uncommon),
                "Main Modifiers" => switch_to_pane(state, OptionsPane::Main),
                _ => {}
            }
        }
    }
    None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    let active = session_active_players();
    if state.pane_transition.is_active() {
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
    match ev.action {
        VirtualAction::p1_back if ev.pressed && active[P1] => {
            return ScreenAction::Navigate(state.return_screen);
        }
        VirtualAction::p2_back if ev.pressed && active[P2] => {
            return ScreenAction::Navigate(state.return_screen);
        }
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            handle_nav_event(state, active, P1, NavDirection::Up, ev.pressed);
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            handle_nav_event(state, active, P1, NavDirection::Down, ev.pressed);
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            handle_nav_event(state, active, P1, NavDirection::Left, ev.pressed);
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            handle_nav_event(state, active, P1, NavDirection::Right, ev.pressed);
        }
        VirtualAction::p1_start if ev.pressed => {
            if let Some(action) = handle_start_event(state, active, P1) {
                return action;
            }
        }
        VirtualAction::p2_up | VirtualAction::p2_menu_up => {
            handle_nav_event(state, active, P2, NavDirection::Up, ev.pressed);
        }
        VirtualAction::p2_down | VirtualAction::p2_menu_down => {
            handle_nav_event(state, active, P2, NavDirection::Down, ev.pressed);
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            handle_nav_event(state, active, P2, NavDirection::Left, ev.pressed);
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            handle_nav_event(state, active, P2, NavDirection::Right, ev.pressed);
        }
        VirtualAction::p2_start if ev.pressed => {
            if let Some(action) = handle_start_event(state, active, P2) {
                return action;
            }
        }
        _ => {}
    }
    ScreenAction::None
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);
    let play_style = crate::game::profile::get_session_play_style();
    let show_p2 = play_style == crate::game::profile::PlayStyle::Versus;
    let active = session_active_players();
    let pane_alpha = state.pane_transition.alpha();
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));
    actors.push(screen_bar::build(ScreenBarParams {
        title: "SELECT MODIFIERS",
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

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
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
    // Speed Mod Helper Display (from overlay.lua)
    // Shows the effective scroll speed (e.g., "X390" for 3.25x on 120 BPM)
    let speed_mod_y = 48.0;
    let speed_mod_x = screen_center_x() + widescale(-77.0, -100.0);
    // All previews (judgment, hold, noteskin, combo) share this center line.
    // Tweak these to dial in parity with Simply Love.
    const PREVIEW_CENTER_OFFSET_NORMAL: f32 = 80.75; // 4:3
    const PREVIEW_CENTER_OFFSET_WIDE: f32 = 98.75; // 16:9
    let preview_center_x =
        speed_mod_x + widescale(PREVIEW_CENTER_OFFSET_NORMAL, PREVIEW_CENTER_OFFSET_WIDE);

    // Calculate effective BPM for display. For X-mod parity with gameplay, use reference BPM.
    let reference_bpm = reference_bpm_for_song(&state.song);
    let effective_song_bpm = f64::from(reference_bpm) * f64::from(state.music_rate);

    let player_color_index = |player_idx: usize| {
        if player_idx == P2 {
            state.active_color_index - 2
        } else {
            state.active_color_index
        }
    };
    let speed_x_for = |player_idx: usize| {
        if player_idx == P2 {
            screen_center_x().mul_add(2.0, -speed_mod_x)
        } else {
            speed_mod_x
        }
    };
    let preview_dx = preview_center_x - speed_mod_x;
    let preview_x_for = |player_idx: usize| speed_x_for(player_idx) + preview_dx;

    if state.current_pane == OptionsPane::Main {
        for player_idx in 0..PLAYER_SLOTS {
            if !active[player_idx] {
                continue;
            }
            let speed_mod = &state.speed_mod[player_idx];
            let speed_color = color::simply_love_rgba(player_color_index(player_idx));
            let speed_text = match speed_mod.mod_type.as_str() {
                "X" => {
                    // For X-mod, show the effective BPM accounting for music rate
                    // (e.g., "X390" for 3.25x on 120 BPM at 1.0x rate)
                    let effective_bpm =
                        (speed_mod.value * effective_song_bpm as f32).round() as i32;
                    format!("X{effective_bpm}")
                }
                "C" => format!("C{}", speed_mod.value as i32),
                "M" => format!("M{}", speed_mod.value as i32),
                _ => format!("{:.2}x", speed_mod.value),
            };

            actors.push(act!(text: font("wendy"): settext(speed_text):
                align(0.5, 0.5): xy(speed_x_for(player_idx), speed_mod_y): zoom(0.5):
                diffuse(speed_color[0], speed_color[1], speed_color[2], pane_alpha):
                z(121)
            ));
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
        if !row.name.is_empty() {
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
        if row.name.contains('\n') {
            let lines: Vec<&str> = row.name.split('\n').collect();
            if lines.len() == 2 {
                // First line (e.g., "Music Rate")
                actors.push(act!(text: font("miso"): settext(lines[0].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y - 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
                // Second line (e.g., "bpm: 120") - smaller and slightly below
                actors.push(act!(text: font("miso"): settext(lines[1].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y + 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
            } else {
                // Fallback for unexpected multi-line format
                actors.push(act!(text: font("miso"): settext(row.name.clone()):
                    align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(title_max_w):
                    z(101)
                ));
            }
        } else {
            // Single-line title (normal case)
            actors.push(act!(text: font("miso"): settext(row.name.clone()):
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
        let show_all_choices_inline = row_shows_all_choices_inline(&row.name);
        // Choice area: For single-choice rows (ShowOneInRow), use ItemsLongRowP1X positioning
        // For multi-choice rows (ShowAllInRow), use ItemsStartX positioning
        // ItemsLongRowP1X = WideScale(_screen.cx-100, _screen.cx-130) from Simply Love metrics
        // ItemsStartX = WideScale(146, 160) from Simply Love metrics
        let choice_inner_left = if show_all_choices_inline {
            widescale(162.0, 176.0)
        } else {
            screen_center_x() + widescale(-100.0, -130.0) // ItemsLongRowP1X for single-choice rows
        };
        if row.name.is_empty() {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] || state.selected_row[player_idx] != item_idx {
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
            // First pass: measure widths to lay out options inline
            let mut widths: Vec<f32> = Vec::with_capacity(row.choices.len());
            let mut text_h: f32 = 16.0;
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                    for text in &row.choices {
                        let mut w = crate::ui::font::measure_line_width_logical(
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
            // Build x positions for each option
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
            if row.name == "Scroll" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Hide" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "FA+ Options" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Gameplay Extras" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Gameplay Extras (More)" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Measure Counter Options" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Error Bar" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Error Bar Options" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
            } else if row.name == "Early Decent/Way Off Options" {
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] {
                        continue;
                    }
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
                for player_idx in 0..PLAYER_SLOTS {
                    if !active[player_idx] || state.selected_row[player_idx] != item_idx {
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
            if row.name.starts_with("Music Rate") {
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
                    let choice_display_text = if row.name == "Speed Mod" {
                        match state.speed_mod[primary_player_idx].mod_type.as_str() {
                            "X" => format!("{:.2}x", state.speed_mod[primary_player_idx].value),
                            "C" => format!("C{}", state.speed_mod[primary_player_idx].value as i32),
                            "M" => format!("M{}", state.speed_mod[primary_player_idx].value as i32),
                            _ => String::new(),
                        }
                    } else {
                        choice_text.clone()
                    };
                    let mut text_w = crate::ui::font::measure_line_width_logical(
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
                    let p2_text = if show_p2 && !row.name.starts_with("Music Rate") {
                        if row.name == "Speed Mod" {
                            match state.speed_mod[P2].mod_type.as_str() {
                                "X" => format!("{:.2}x", state.speed_mod[P2].value),
                                "C" => format!("C{}", state.speed_mod[P2].value as i32),
                                "M" => format!("M{}", state.speed_mod[P2].value as i32),
                                _ => String::new(),
                            }
                        } else if row.name == "Type of Speed Mod" {
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
                    if show_p2 && !row.name.starts_with("Music Rate") {
                        let p2_choice_center_x = screen_center_x().mul_add(2.0, -choice_center_x);
                        let mut p2_w = crate::ui::font::measure_line_width_logical(
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
                    if row.name == "Judgment Font" {
                        let texture_for = |text: &str| -> Option<&'static str> {
                            match text {
                            "Love" => Some("judgements/Love 2x7 (doubleres).png"),
                            "Love Chroma" => Some("judgements/Love Chroma 2x7 (doubleres).png"),
                            "Rainbowmatic" => Some("judgements/Rainbowmatic 2x7 (doubleres).png"),
                            "GrooveNights" => Some("judgements/GrooveNights 2x7 (doubleres).png"),
                            "Emoticon" => Some("judgements/Emoticon 2x7 (doubleres).png"),
                            "Censored" => Some("judgements/Censored 1x7 (doubleres).png"),
                            "Chromatic" => Some("judgements/Chromatic 2x7 (doubleres).png"),
                            "ITG2" => Some("judgements/ITG2 2x7 (doubleres).png"),
                            "Bebas" => Some("judgements/Bebas 2x7 (doubleres).png"),
                            "Code" => Some("judgements/Code 2x7 (doubleres).png"),
                            "Comic Sans" => Some("judgements/Comic Sans 2x7 (doubleres).png"),
                            "Focus" => Some("judgements/Focus 2x7 (doubleres).png"),
                            "Grammar" => Some("judgements/Grammar 2x7 (doubleres).png"),
                            "Miso" => Some("judgements/Miso 2x7 (doubleres).png"),
                            "Papyrus" => Some("judgements/Papyrus 2x7 (doubleres).png"),
                            "Roboto" => Some("judgements/Roboto 2x7 (doubleres).png"),
                            "Shift" => Some("judgements/Shift 2x7 (doubleres).png"),
                            "Tactics" => Some("judgements/Tactics 2x7 (doubleres).png"),
                            "Wendy" => Some("judgements/Wendy 2x7 (doubleres).png"),
                            "Wendy Chroma" => Some("judgements/Wendy Chroma 2x7 (doubleres).png"),
                            "None" => None,
                            _ => None,
                            }
                        };
                        if let Some(texture) = texture_for(choice_text.as_str()) {
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_x_for(primary_player_idx), current_row_y):
                                setstate(0):
                                zoom(0.225):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                        if show_p2 && primary_player_idx != P2
                            && let Some(texture) = texture_for(p2_text.as_str())
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
                    if row.name == "Hold Judgment" {
                        let texture_for = |text: &str| -> Option<&'static str> {
                            match text {
                            "Love" => Some("hold_judgements/Love 1x2 (doubleres).png"),
                            "mute" => Some("hold_judgements/mute 1x2 (doubleres).png"),
                            "ITG2" => Some("hold_judgements/ITG2 1x2 (doubleres).png"),
                            "None" => None,
                            _ => None,
                            }
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
                        if let Some(texture) = texture_for(choice_text.as_str()) {
                            draw_hold_preview(texture, preview_x_for(primary_player_idx), &mut actors);
                        }
                        if show_p2 && primary_player_idx != P2
                            && let Some(texture) = texture_for(p2_text.as_str())
                        {
                            draw_hold_preview(texture, preview_x_for(P2), &mut actors);
                        }
                    }
                    // Match ITGmania themes that show four directional noteskin preview arrows
                    // with explicit quant offsets: Left/Down/Up/Right and 0/1/3/2 quant indices.
                    if row.name == "NoteSkin" {
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
                                            actors.push(act!(sprite(note_slot.texture_key().to_string()):
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
                                            actors.push(act!(sprite(note_slot.texture_key().to_string()):
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
                                    actors.push(act!(sprite(note_slot.texture_key().to_string()):
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
                                    let note_idx = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
                                    draw_noteskin_note(ns, note_idx, quant_idx, x, actors);
                                }
                            };
                        if let Some(ns) = state.noteskin[primary_player_idx].as_ref() {
                            draw_noteskin_preview(ns, preview_x_for(primary_player_idx), &mut actors);
                        }
                        if show_p2 && primary_player_idx != P2
                            && let Some(ns) = state.noteskin[P2].as_ref()
                        {
                            draw_noteskin_preview(ns, preview_x_for(P2), &mut actors);
                        }
                    }
                    // Add combo preview for "Combo Font" row showing ticking numbers
                    if row.name == "Combo Font" {
                        let combo_text = state.combo_preview_count.to_string();
                        let combo_zoom = 0.45;
                        let combo_font_for = |text: &str| -> Option<&'static str> {
                            match text {
                            "Wendy" => Some("wendy_combo"),
                            "Arial Rounded" => Some("combo_arial_rounded"),
                            "Asap" => Some("combo_asap"),
                            "Bebas Neue" => Some("combo_bebas_neue"),
                            "Source Code" => Some("combo_source_code"),
                            "Work" => Some("combo_work"),
                            "Wendy (Cursed)" => Some("combo_wendy_cursed"),
                            "None" => None,
                            _ => Some("wendy_combo"),
                            }
                        };
                        if let Some(font_name) = combo_font_for(choice_text.as_str()) {
                            actors.push(act!(text:
                                font(font_name): settext(combo_text.clone()):
                                align(0.5, 0.5):
                                xy(preview_x_for(primary_player_idx), current_row_y):
                                zoom(combo_zoom): horizalign(center):
                                diffuse(1.0, 1.0, 1.0, a):
                                z(102)
                            ));
                        }
                        if show_p2 && primary_player_idx != P2
                            && let Some(font_name) = combo_font_for(p2_text.as_str())
                        {
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
    for player_idx in 0..PLAYER_SLOTS {
        if !active[player_idx] {
            continue;
        }
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
