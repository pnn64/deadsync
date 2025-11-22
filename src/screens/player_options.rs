use crate::act;
use crate::core::audio;
use crate::core::space::*;
use crate::game::song::SongData;
use crate::screens::{Screen, ScreenAction};
use crate::core::input::{VirtualAction, InputEvent};
use crate::ui::actors::Actor;
use crate::assets::AssetManager;
use crate::ui::color;
use crate::ui::components::heart_bg;
use crate::ui::components::screen_bar::{
    self, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::game::parsing::noteskin::{self, Noteskin, Quantization, NUM_QUANTIZATIONS};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* ----------------------------- cursor tweening ----------------------------- */
// Match Simply Love's CursorTweenSeconds for OptionRow cursor movement
const CURSOR_TWEEN_SECONDS: f32 = 0.1;
// Spacing between inline items in OptionRows (pixels at current zoom)
const INLINE_SPACING: f32 = 15.75;

#[inline(always)]
fn ease_out_cubic(t: f32) -> f32 {
    let clamped = if t < 0.0 { 0.0 } else if t > 1.0 { 1.0 } else { t };
    let u = 1.0 - clamped;
    1.0 - u * u * u
}

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

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

pub struct Row {
    pub name: String,
    pub choices: Vec<String>,
    pub selected_choice_index: usize,
    pub help: Vec<String>,
    pub choice_difficulty_indices: Option<Vec<usize>>,
}

pub struct SpeedMod {
    pub mod_type: String, // "X", "C", "M"
    pub value: f32,
}

pub struct State {
    pub song: Arc<SongData>,
    pub chart_difficulty_index: usize,
    pub rows: Vec<Row>,
    pub selected_row: usize,
    pub prev_selected_row: usize,
    // For Scroll row: bitmask of which options are enabled.
    // 0 => Normal scroll (no special modifier).
    pub scroll_active_mask: u8,
    // For FA+ Options row: bitmask of which options are enabled.
    // bit0 = Display FA+ Window, bit1 = Display EX Score, bit2 = Display FA+ Pane.
    pub fa_plus_active_mask: u8,
    pub active_color_index: i32,
    pub speed_mod: SpeedMod,
    pub music_rate: f32,
    pub current_pane: OptionsPane,
    bg: heart_bg::State,
    pub nav_key_held_direction: Option<NavDirection>,
    pub nav_key_held_since: Option<Instant>,
    pub nav_key_last_scrolled_at: Option<Instant>,
    noteskin: Option<Noteskin>,
    preview_time: f32,
    preview_beat: f32,
    help_anim_time: f32,
    // Combo preview state (for Combo Font row)
    combo_preview_count: u32,
    combo_preview_elapsed: f32,
    // Inline option cursor tween (left/right between items)
    cursor_anim_row: Option<usize>,
    cursor_anim_from_choice: usize,
    cursor_anim_to_choice: usize,
    cursor_anim_t: f32,
    // Vertical tween when changing selected row
    cursor_row_anim_from_y: f32,
    cursor_row_anim_t: f32,
    cursor_row_anim_from_row: Option<usize>,
}

// Format music rate like Simply Love wants:
fn fmt_music_rate(rate: f32) -> String {
    let scaled = (rate * 100.0).round() as i32;
    let int_part = scaled / 100;
    let frac2 = (scaled % 100).abs();
    if frac2 == 0 {
        format!("{}", int_part)
    } else if frac2 % 10 == 0 {
        format!("{}.{}", int_part, frac2 / 10)
    } else {
        format!("{}.{:02}", int_part, frac2)
    }
}

// Prefer #DISPLAYBPM for reference BPM (use max of range or single value); fallback to song.max_bpm, then 120.
fn reference_bpm_for_song(song: &SongData) -> f32 {
    let s = song.display_bpm.trim();
    let from_display = if !s.is_empty() && s != "*" {
        if let Some((_, max_str)) = s.split_once(':') {
            max_str.trim().parse::<f32>().ok()
        } else if let Some((_, max_str)) = s.split_once('-') {
            max_str.trim().parse::<f32>().ok()
        } else {
            s.parse::<f32>().ok()
        }
    } else { None };
    let bpm = from_display.unwrap_or_else(|| song.max_bpm as f32);
    if bpm.is_finite() && bpm > 0.0 { bpm } else { 120.0 }
}

#[inline(always)]
fn round_to_step(x: f32, step: f32) -> f32 {
    if !x.is_finite() || !step.is_finite() || step <= 0.0 { return x; }
    (x / step).round() * step
}

fn what_comes_next_choices(pane: OptionsPane) -> Vec<String> {
    match pane {
        OptionsPane::Main => vec![
            "Gameplay".to_string(),
            "Choose a Different Song".to_string(),
            "Advanced Modifiers".to_string(),
            "Uncommon Modifiers".to_string(),
        ],
        OptionsPane::Advanced => vec![
            "Gameplay".to_string(),
            "Choose a Different Song".to_string(),
            "Main Modifiers".to_string(),
            "Uncommon Modifiers".to_string(),
        ],
        OptionsPane::Uncommon => vec![
            "Gameplay".to_string(),
            "Choose a Different Song".to_string(),
            "Main Modifiers".to_string(),
            "Advanced Modifiers".to_string(),
        ],
    }
}

fn build_main_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    selected_difficulty_index: usize,
    session_music_rate: f32,
) -> Vec<Row> {
    let speed_mod_value_str = match speed_mod.mod_type.as_str() {
        "X" => format!("{:.2}x", speed_mod.value),
        "C" => format!("C{}", speed_mod.value as i32),
        "M" => format!("M{}", speed_mod.value as i32),
        _ => "".to_string(),
    };
    // Build Stepchart choices from the song's dance-single charts, ordered Beginner..Challenge
    let mut stepchart_choices: Vec<String> = Vec::with_capacity(5);
    let mut stepchart_choice_indices: Vec<usize> = Vec::with_capacity(5);
    for (i, file_name) in crate::ui::color::FILE_DIFFICULTY_NAMES.iter().enumerate() {
        if let Some(chart) = song
            .charts
            .iter()
            .find(|c| c.chart_type.eq_ignore_ascii_case("dance-single") && c.difficulty.eq_ignore_ascii_case(file_name))
        {
            let display_name = crate::ui::color::DISPLAY_DIFFICULTY_NAMES[i];
            stepchart_choices.push(format!("{} {}", display_name, chart.meter));
            stepchart_choice_indices.push(i);
        }
    }
    // Fallback if none found (defensive; SelectMusic filters to dance-single songs)
    if stepchart_choices.is_empty() {
        stepchart_choices.push("(Current)".to_string());
        stepchart_choice_indices.push(selected_difficulty_index.min(crate::ui::color::FILE_DIFFICULTY_NAMES.len() - 1));
    }
    let initial_stepchart_choice_index = stepchart_choice_indices
        .iter()
        .position(|&idx| idx == selected_difficulty_index)
        .unwrap_or(0);
    vec![
        Row {
            name: "Type of Speed Mod".to_string(),
            choices: vec![
                "X (multiplier)".to_string(),
                "C (constant)".to_string(),
                "M (maximum)".to_string(),
            ],
            selected_choice_index: match speed_mod.mod_type.as_str() {
                "X" => 0,
                "C" => 1,
                "M" => 2,
                _ => 1, // Default to C
            },
            help: vec!["Change the way arrows react to changing BPMs.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Speed Mod".to_string(),
            choices: vec![speed_mod_value_str], // Display only the current value
            selected_choice_index: 0,
            help: vec!["Adjust the speed at which arrows travel toward the targets.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Mini".to_string(),
            choices: (-100..=150).map(|v| format!("{}%", v)).collect(),
            selected_choice_index: 0,
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
            selected_choice_index: 0,
            help: vec!["Change the viewing angle of the arrow stream.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteSkin".to_string(),
            choices: vec![
                "cel".to_string(),
                "metal".to_string(),
                "enchantment-v2".to_string(),
                "devcel-2024-v3".to_string(),
            ],
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 3,
            help: vec![
                "Darken the underside of the playing field.".to_string(),
                "This will partially obscure background art.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteField Offset X".to_string(),
            choices: (0..=50).map(|v| v.to_string()).collect(),
            selected_choice_index: 0,
            help: vec![
                "Adjust the horizontal position of the notefield (relative to the".to_string(),
                "center).".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteField Offset Y".to_string(),
            choices: (-50..=50).map(|v| v.to_string()).collect(),
            selected_choice_index: 0,
            help: vec!["Adjust the vertical position of the notefield.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Visual Delay".to_string(),
            choices: vec!["0ms".to_string()],
            selected_choice_index: 0,
            help: vec![
                "Player specific visual delay. Negative values shifts the arrows".to_string(),
                "upwards, while positive values move them down.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: {
                let reference_bpm = reference_bpm_for_song(song);
                let effective_bpm = (reference_bpm as f64) * session_music_rate as f64;
                let bpm_str = if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
                    format!("{}", effective_bpm.round() as i32)
                } else {
                    format!("{:.1}", effective_bpm)
                };
                format!("Music Rate\nbpm: {}", bpm_str)
            },
            choices: vec![fmt_music_rate(session_music_rate.clamp(0.5, 3.0))],
            selected_choice_index: 0,
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
            choices: what_comes_next_choices(OptionsPane::Main),
            selected_choice_index: 0,
            help: vec![
                "Go back and choose a different song or change additional".to_string(),
                "modifiers.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "".to_string(),
            choices: vec!["Exit".to_string()],
            selected_choice_index: 0,
            help: vec!["".to_string()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_advanced_rows() -> Vec<Row> {
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
            selected_choice_index: 0,
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
            ],
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 11, // S by default, matching screenshot
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
            selected_choice_index: 0,
            help: vec!["Decide what happens if you fall behind your target score.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Gameplay Extras".to_string(),
            choices: vec![
                "Flash Column for Miss".to_string(),
                "Subtractive Scoring".to_string(),
                "Pacemaker".to_string(),
                "Density Graph at Top".to_string(),
            ],
            selected_choice_index: 0,
            help: vec!["Extra feedback helpers shown during gameplay.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Gameplay Extras (More)".to_string(),
            choices: vec![
                "Judgment Tilt".to_string(),
                "Column Cues".to_string(),
                "Display Scorebox".to_string(),
            ],
            selected_choice_index: 0,
            help: vec!["Additional visual effects, cues, and score display options.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Judgment Tilt Intensity".to_string(),
            choices: vec![
                "1".to_string(),
                "1.5".to_string(),
                "2".to_string(),
                "2.5".to_string(),
                "3".to_string(),
            ],
            selected_choice_index: 0,
            help: vec!["How strongly to tilt judgments left/right.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Error Bar".to_string(),
            choices: vec![
                "None".to_string(),
                "Colorful".to_string(),
                "Monochrome".to_string(),
                "Text".to_string(),
            ],
            selected_choice_index: 0,
            help: vec!["Choose the style for the timing error bar or disable it.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Error Bar Trim".to_string(),
            choices: vec![
                "Off".to_string(),
                "Great".to_string(),
                "Excellent".to_string(),
            ],
            selected_choice_index: 0,
            help: vec!["Set the worst timing window that the error bar will show.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Error Bar Options".to_string(),
            choices: vec![
                "Move Up".to_string(),
                "Multi-Tick".to_string(),
            ],
            selected_choice_index: 0,
            help: vec![
                "Adjust where the error bar appears and whether it shows multiple tick marks.".to_string(),
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
            selected_choice_index: 0,
            help: vec![
                "Display a count of how long you have been streaming a specific type of note.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Measure Counter Options".to_string(),
            choices: vec![
                "Move Left".to_string(),
                "Move Up".to_string(),
                "Hide Lookahead".to_string(),
            ],
            selected_choice_index: 0,
            help: vec![
                "Change how the Measure Counter is positioned and whether it hides upcoming notes.".to_string(),
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
            selected_choice_index: 0,
            help: vec!["Display horizontal lines on the notefield to indicate quantization.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Early Decent/Way Off Options".to_string(),
            choices: vec![
                "Hide Judgments".to_string(),
                "Hide NoteField Flash".to_string(),
            ],
            selected_choice_index: 0,
            help: vec!["Set how early Decent and Way Off judgments are visually represented.".to_string()],
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
            selected_choice_index: 0,
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
                "Display FA+ Pane".to_string(),
            ],
            selected_choice_index: 0,
            help: vec![
                "Toggle FA+ style timing window display and EX scoring visuals.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "What comes next?".to_string(),
            choices: what_comes_next_choices(OptionsPane::Advanced),
            selected_choice_index: 0,
            help: vec![
                "Jump to gameplay, another modifier pane,".to_string(),
                "or back to song select.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "".to_string(),
            choices: vec!["Exit".to_string()],
            selected_choice_index: 0,
            help: vec!["".to_string()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_uncommon_rows() -> Vec<Row> {
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
            help: vec!["Fade or hide incoming arrows in unusual ways.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Attacks".to_string(),
            choices: vec![
                "Off".to_string(),
                "On".to_string(),
                "Random".to_string(),
            ],
            selected_choice_index: 0,
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
            selected_choice_index: 0,
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
            selected_choice_index: 0,
            help: vec!["Control how cabinet lights react during gameplay.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "What comes next?".to_string(),
            choices: what_comes_next_choices(OptionsPane::Uncommon),
            selected_choice_index: 0,
            help: vec![
                "Jump to gameplay, another modifier pane,".to_string(),
                "or back to song select.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "".to_string(),
            choices: vec!["Exit".to_string()],
            selected_choice_index: 0,
            help: vec!["".to_string()],
            choice_difficulty_indices: None,
        },
    ]
}

fn build_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    selected_difficulty_index: usize,
    session_music_rate: f32,
    pane: OptionsPane,
) -> Vec<Row> {
    match pane {
        OptionsPane::Main => build_main_rows(song, speed_mod, selected_difficulty_index, session_music_rate),
        OptionsPane::Advanced => build_advanced_rows(),
        OptionsPane::Uncommon => build_uncommon_rows(),
    }
}

fn apply_profile_defaults(rows: &mut [Row]) -> (u8, u8) {
    let profile = crate::game::profile::get();
    let mut scroll_active_mask: u8 = 0;
    let mut fa_plus_active_mask: u8 = 0;
    // Initialize Background Filter row from profile setting (Off, Dark, Darker, Darkest)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Background Filter") {
        row.selected_choice_index = match profile.background_filter {
            crate::game::profile::BackgroundFilter::Off => 0,
            crate::game::profile::BackgroundFilter::Dark => 1,
            crate::game::profile::BackgroundFilter::Darker => 2,
            crate::game::profile::BackgroundFilter::Darkest => 3,
        };
    }
    // Initialize Judgment Font row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Judgment Font") {
        row.selected_choice_index = match profile.judgment_graphic {
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
        row.selected_choice_index = match profile.noteskin {
            crate::game::profile::NoteSkin::Cel => 0,
            crate::game::profile::NoteSkin::Metal => 1,
            crate::game::profile::NoteSkin::EnchantmentV2 => 2,
            crate::game::profile::NoteSkin::DevCel2024V3 => 3,
        };
    }
    // Initialize Combo Font row from profile setting
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Combo Font") {
        row.selected_choice_index = match profile.combo_font {
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
        row.selected_choice_index = match profile.hold_judgment_graphic {
            crate::game::profile::HoldJudgmentGraphic::Love => 0,
            crate::game::profile::HoldJudgmentGraphic::Mute => 1,
            crate::game::profile::HoldJudgmentGraphic::ITG2 => 2,
            crate::game::profile::HoldJudgmentGraphic::None => 3,
        };
    }
    // Initialize Mini row from profile (range -100..150, stored as percent).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Mini") {
        let val = profile.mini_percent.clamp(-100, 150);
        let needle = format!("{}%", val);
        if let Some(idx) = row.choices.iter().position(|c| c == &needle) {
            row.selected_choice_index = idx;
        }
    }
    // Initialize NoteField Offset X from profile (0..50, non-negative; P1 uses negative sign at render time)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "NoteField Offset X") {
        let val = profile.note_field_offset_x.clamp(0, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index = idx;
        }
    }
    // Initialize NoteField Offset Y from profile (-50..50)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "NoteField Offset Y") {
        let val = profile.note_field_offset_y.clamp(-50, 50);
        let val_str = val.to_string();
        if let Some(idx) = row.choices.iter().position(|c| c == &val_str) {
            row.selected_choice_index = idx;
        }
    }
    // Initialize FA+ Options row from profile (three independent toggles).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "FA+ Options") {
        // Cursor always starts on the first option; toggled state is reflected visually.
        row.selected_choice_index = 0;
    }
    if profile.show_fa_plus_window {
        fa_plus_active_mask |= 1u8 << 0;
    }
    if profile.show_ex_score {
        fa_plus_active_mask |= 1u8 << 1;
    }
    if profile.show_fa_plus_pane {
        fa_plus_active_mask |= 1u8 << 2;
    }

    // Initialize Scroll row from profile setting (multi-choice toggle group).
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Scroll") {
        use crate::game::profile::ScrollOption;
        // Map profile flags onto row choice indices.
        if profile.scroll_option.contains(ScrollOption::Reverse) {
            if let Some(idx) = row.choices.iter().position(|c| c == "Reverse") {
                if idx < 8 {
                    scroll_active_mask |= 1u8 << (idx as u8);
                }
            }
        }
        if profile.scroll_option.contains(ScrollOption::Split) {
            if let Some(idx) = row.choices.iter().position(|c| c == "Split") {
                if idx < 8 {
                    scroll_active_mask |= 1u8 << (idx as u8);
                }
            }
        }
        if profile.scroll_option.contains(ScrollOption::Alternate) {
            if let Some(idx) = row.choices.iter().position(|c| c == "Alternate") {
                if idx < 8 {
                    scroll_active_mask |= 1u8 << (idx as u8);
                }
            }
        }
        if profile.scroll_option.contains(ScrollOption::Cross) {
            if let Some(idx) = row.choices.iter().position(|c| c == "Cross") {
                if idx < 8 {
                    scroll_active_mask |= 1u8 << (idx as u8);
                }
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
            row.selected_choice_index = first_idx;
        } else {
            row.selected_choice_index = 0;
        }
    }
    (scroll_active_mask, fa_plus_active_mask)
}

pub fn init(song: Arc<SongData>, chart_difficulty_index: usize, active_color_index: i32) -> State {
    let session_music_rate = crate::game::profile::get_session_music_rate();
    let profile = crate::game::profile::get();
    let speed_mod = match profile.scroll_speed {
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
    let mut rows = build_rows(&song, &speed_mod, chart_difficulty_index, session_music_rate, OptionsPane::Main);
    let (scroll_active_mask, fa_plus_active_mask) = apply_profile_defaults(&mut rows);
    // Load noteskin for preview based on profile setting
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };
    let noteskin_path = match profile.noteskin {
        crate::game::profile::NoteSkin::Cel => "assets/noteskins/cel/dance-single.txt",
        crate::game::profile::NoteSkin::Metal => "assets/noteskins/metal/dance-single.txt",
        crate::game::profile::NoteSkin::EnchantmentV2 => "assets/noteskins/enchantment-v2/dance-single.txt",
        crate::game::profile::NoteSkin::DevCel2024V3 => "assets/noteskins/devcel-2024-v3/dance-single.txt",
    };
    let noteskin = noteskin::load(Path::new(noteskin_path), &style)
        .ok()
        .or_else(|| noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style).ok())
        .or_else(|| noteskin::load(Path::new("assets/noteskins/fallback.txt"), &style).ok());
    State {
        song,
        chart_difficulty_index,
        rows,
        selected_row: 0,
        prev_selected_row: 0,
        scroll_active_mask,
        fa_plus_active_mask,
        active_color_index,
        speed_mod,
        music_rate: session_music_rate,
        current_pane: OptionsPane::Main,
        bg: heart_bg::State::new(),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        noteskin,
        preview_time: 0.0,
        preview_beat: 0.0,
        help_anim_time: 0.0,
        combo_preview_count: 0,
        combo_preview_elapsed: 0.0,
        cursor_anim_row: None,
        cursor_anim_from_choice: 0,
        cursor_anim_to_choice: 0,
        cursor_anim_t: 1.0,
        cursor_row_anim_from_y: 0.0,
        cursor_row_anim_t: 1.0,
        cursor_row_anim_from_row: None,
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

fn change_choice(state: &mut State, delta: isize) {
    let row = &mut state.rows[state.selected_row];
    if row.name == "Speed Mod" {
        let speed_mod = &mut state.speed_mod;
        let (upper, increment) = match speed_mod.mod_type.as_str() {
            "X" => (20.0, 0.05),
            "C" | "M" => (2000.0, 5.0),
            _ => (1.0, 0.1),
        };
        speed_mod.value += delta as f32 * increment;
        speed_mod.value = (speed_mod.value / increment).round() * increment;
        speed_mod.value = speed_mod.value.clamp(increment, upper);
        let speed_mod_value_str = match speed_mod.mod_type.as_str() {
            "X" => format!("{:.2}x", speed_mod.value),
            "C" => format!("C{}", speed_mod.value as i32),
            "M" => format!("M{}", speed_mod.value as i32),
            _ => "".to_string(),
        };
        row.choices[0] = speed_mod_value_str;
        audio::play_sfx("assets/sounds/change_value.ogg");
    } else if row.name.starts_with("Music Rate") {
        let increment = 0.01f32;
        let min_rate = 0.05f32;
        let max_rate = 3.00f32;
        state.music_rate += delta as f32 * increment;
        state.music_rate = (state.music_rate / increment).round() * increment;
        state.music_rate = state.music_rate.clamp(min_rate, max_rate);
        row.choices[0] = fmt_music_rate(state.music_rate);
       
        // Update the row title to show the new BPM using reference BPM
        let reference_bpm = reference_bpm_for_song(&state.song);
        let effective_bpm = (reference_bpm as f64) * state.music_rate as f64;

        // Format BPM: show one decimal only if it doesn't round to a whole number
        let bpm_str = if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
            format!("{}", effective_bpm.round() as i32)
        } else {
            format!("{:.1}", effective_bpm)
        };
       
        row.name = format!("Music Rate\nbpm: {}", bpm_str);
       
        audio::play_sfx("assets/sounds/change_value.ogg");
        // Update session music rate immediately so SelectMusic will match on return
        crate::game::profile::set_session_music_rate(state.music_rate);
        // If a preview is playing, adjust its rate without restarting it
        audio::set_music_rate(state.music_rate);
    } else {
        let num_choices = row.choices.len();
        if num_choices > 0 {
            let current_idx = row.selected_choice_index as isize;
            let new_index = ((current_idx + delta + num_choices as isize) % num_choices as isize) as usize;
            // Begin cursor animation if this row is inline and the choice actually changes
            let is_inline_row = row.name == "Perspective"
                || row.name == "Background Filter"
                || row.name == "Stepchart"
                || row.name == "What comes next?"
                || row.name == "Turn"
                || row.name == "Scroll"
                || row.name == "Hide"
                || row.name == "LifeMeter Type"
                || row.name == "Data Visualizations"
                || row.name.starts_with("Gameplay Extras")
                || row.name == "Judgment Tilt Intensity"
                || row.name == "Error Bar"
                || row.name == "Error Bar Trim"
                || row.name == "Error Bar Options"
                || row.name == "Measure Counter"
                || row.name == "Measure Counter Options"
                || row.name == "Measure Lines"
                || row.name == "Early Decent/Way Off Options"
                || row.name == "Timing Windows"
                || row.name == "FA+ Options"
                || row.name == "Insert"
                || row.name == "Remove"
                || row.name == "Holds"
                || row.name == "Accel Effects"
                || row.name == "Visual Effects"
                || row.name == "Appearance Effects"
                || row.name == "Attacks"
                || row.name == "Characters"
                || row.name == "Hide Light Type";
            let prev_choice = row.selected_choice_index;
            row.selected_choice_index = new_index;
            if is_inline_row && prev_choice != new_index {
                state.cursor_anim_row = Some(state.selected_row);
                state.cursor_anim_from_choice = prev_choice;
                state.cursor_anim_to_choice = new_index;
                state.cursor_anim_t = 0.0;
            }
            // Changing the speed mod type should update the mod and the next row display
            if row.name == "Type of Speed Mod" {
                let new_type = match row.selected_choice_index { 0 => "X", 1 => "C", 2 => "M", _ => "C" };
                let old_type = state.speed_mod.mod_type.clone();
                let old_value = state.speed_mod.value;

                // Determine target effective BPM label we want to preserve when switching types.
                let reference_bpm = reference_bpm_for_song(&state.song);
                let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
                let target_bpm: f32 = match old_type.as_str() {
                    // C/M store a BPM; keep that BPM label when switching types
                    "C" | "M" => old_value,
                    // From X: infer current displayed X### as the target bpm
                    "X" => (reference_bpm * rate * old_value).round(),
                    _ => 600.0,
                };

                // Compute new value for selected type, matching target_bpm as closely as possible
                let new_value = match new_type {
                    // For X: pick nearest 0.05 step to hit target bpm label
                    "X" => {
                        let denom = reference_bpm * rate;
                        let raw = if denom.is_finite() && denom > 0.0 { target_bpm / denom } else { 1.0 };
                        let stepped = round_to_step(raw, 0.05);
                        stepped.clamp(0.05, 20.0)
                    }
                    // C and M are BPM-style values; snap to nearest 5 BPM like the UI increments
                    "C" | "M" => {
                        let stepped = round_to_step(target_bpm, 5.0);
                        stepped.clamp(5.0, 2000.0)
                    }
                    _ => 600.0,
                };

                state.speed_mod.mod_type = new_type.to_string();
                state.speed_mod.value = new_value;

                // Update the choices vec for the "Speed Mod" row.
                if let Some(speed_mod_row) = state.rows.get_mut(1) {
                    if speed_mod_row.name == "Speed Mod" {
                        speed_mod_row.choices[0] = match new_type {
                            "X" => format!("{:.2}x", new_value),
                            "C" => format!("C{}", new_value as i32),
                            "M" => format!("M{}", new_value as i32),
                            _ => String::new(),
                        };
                    }
                }
            } else if row.name == "Background Filter" {
                // Persist the new filter level to the profile
                let setting = match row.selected_choice_index {
                    0 => crate::game::profile::BackgroundFilter::Off,
                    1 => crate::game::profile::BackgroundFilter::Dark,
                    2 => crate::game::profile::BackgroundFilter::Darker,
                    3 => crate::game::profile::BackgroundFilter::Darkest,
                    _ => crate::game::profile::BackgroundFilter::Darkest,
                };
                crate::game::profile::update_background_filter(setting);
            } else if row.name == "Mini" {
                // Persist Mini% selection to the profile.
                if let Some(choice) = row.choices.get(row.selected_choice_index) {
                    let trimmed = choice.trim_end_matches('%');
                    if let Ok(val) = trimmed.parse::<i32>() {
                        crate::game::profile::update_mini_percent(val);
                    }
                }
            } else if row.name == "NoteField Offset X" {
                if let Some(choice) = row.choices.get(row.selected_choice_index) {
                    if let Ok(raw) = choice.parse::<i32>() {
                        crate::game::profile::update_notefield_offset_x(raw);
                    }
                }
            } else if row.name == "NoteField Offset Y" {
                if let Some(choice) = row.choices.get(row.selected_choice_index) {
                    if let Ok(raw) = choice.parse::<i32>() {
                        crate::game::profile::update_notefield_offset_y(raw);
                    }
                }
            } else if row.name == "Judgment Font" {
                // Persist tap judgment font selection to the profile
                let setting = match row.selected_choice_index {
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
                crate::game::profile::update_judgment_graphic(setting);
            } else if row.name == "Combo Font" {
                // Persist combo font selection to the profile
                let setting = match row.selected_choice_index {
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
                crate::game::profile::update_combo_font(setting);
            } else if row.name == "Hold Judgment" {
                // Persist hold judgment graphic selection to profile
                let setting = match row.selected_choice_index {
                    0 => crate::game::profile::HoldJudgmentGraphic::Love,
                    1 => crate::game::profile::HoldJudgmentGraphic::Mute,
                    2 => crate::game::profile::HoldJudgmentGraphic::ITG2,
                    3 => crate::game::profile::HoldJudgmentGraphic::None,
                    _ => crate::game::profile::HoldJudgmentGraphic::Love,
                };
                crate::game::profile::update_hold_judgment_graphic(setting);
            } else if row.name == "NoteSkin" {
                // Persist noteskin selection to profile and reload preview noteskin
                let setting = match row.selected_choice_index {
                    0 => crate::game::profile::NoteSkin::Cel,
                    1 => crate::game::profile::NoteSkin::Metal,
                    2 => crate::game::profile::NoteSkin::EnchantmentV2,
                    3 => crate::game::profile::NoteSkin::DevCel2024V3,
                    _ => crate::game::profile::NoteSkin::Cel,
                };
                crate::game::profile::update_noteskin(setting);
                let style = noteskin::Style { num_cols: 4, num_players: 1 };
                let path_str = match setting {
                    crate::game::profile::NoteSkin::Cel => "assets/noteskins/cel/dance-single.txt",
                    crate::game::profile::NoteSkin::Metal => "assets/noteskins/metal/dance-single.txt",
                    crate::game::profile::NoteSkin::EnchantmentV2 => "assets/noteskins/enchantment-v2/dance-single.txt",
                    crate::game::profile::NoteSkin::DevCel2024V3 => "assets/noteskins/devcel-2024-v3/dance-single.txt",
                };
                state.noteskin = noteskin::load(Path::new(path_str), &style)
                    .ok()
                    .or_else(|| noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style).ok())
                    .or_else(|| noteskin::load(Path::new("assets/noteskins/fallback.txt"), &style).ok());
            } else if row.name == "Stepchart" {
                // Update the state's difficulty index to match the newly selected choice
                if let Some(diff_indices) = &row.choice_difficulty_indices {
                    if let Some(&difficulty_idx) = diff_indices.get(row.selected_choice_index) {
                        state.chart_difficulty_index = difficulty_idx;
                    }
                }
            }
            audio::play_sfx("assets/sounds/change_value.ogg");
        }
    }
}

// Public wrapper so app dispatcher can invoke a single step change without exposing internals.
pub fn apply_choice_delta(state: &mut State, delta: isize) {
    change_choice(state, delta);
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs
pub fn update(state: &mut State, dt: f32) {
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
    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
        state.nav_key_held_direction,
        state.nav_key_held_since,
        state.nav_key_last_scrolled_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY {
            if now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL {
                let total_rows = state.rows.len();
                if total_rows > 0 {
                    match direction {
                        NavDirection::Up => {
                            state.selected_row = (state.selected_row + total_rows - 1) % total_rows
                        }
                        NavDirection::Down => state.selected_row = (state.selected_row + 1) % total_rows,
                        NavDirection::Left => {
                            change_choice(state, -1);
                        }
                        NavDirection::Right => {
                            change_choice(state, 1);
                        }
                    }
                    state.nav_key_last_scrolled_at = Some(now);
                }
            }
        }
    }
    // Advance the help reveal animation timer
    state.help_anim_time += dt;

    // If the Combo Font row is active, tick the preview combo once per second
    if let Some(row) = state.rows.get(state.selected_row) {
        if row.name == "Combo Font" {
            state.combo_preview_elapsed += dt;
            if state.combo_preview_elapsed >= 1.0 {
                // Advance by one per second
                state.combo_preview_elapsed -= 1.0;
                state.combo_preview_count = state.combo_preview_count.saturating_add(1);
            }
        } else {
            // Pause ticking when not on the Combo Font row
            state.combo_preview_elapsed = 0.0;
        }
    }
    if state.selected_row != state.prev_selected_row {
        // Direction-aware row change sounds
        match state.nav_key_held_direction {
            Some(NavDirection::Up) => audio::play_sfx("assets/sounds/prev_row.ogg"),
            Some(NavDirection::Down) => audio::play_sfx("assets/sounds/next_row.ogg"),
            _ => audio::play_sfx("assets/sounds/next_row.ogg"),
        }
        // Start vertical cursor tween from previous row's Y to new row's Y
        // Duplicate row layout math used in get_actors() to compute Y centers.
        let total_rows = state.rows.len();
        // constants must mirror get_actors()
        let frame_h = 33.0_f32;                // ROW_HEIGHT
        let anchor_row = 5_usize;              // ANCHOR_ROW
        let visible_rows = 10_usize;           // VISIBLE_ROWS
        let first_row_center_y = screen_center_y() + (-164.0); // ROW_START_OFFSET
        let help_box_h = 40.0_f32;
        let help_box_bottom_y = screen_height() - 36.0;
        let help_top_y = help_box_bottom_y - help_box_h;
        let n_rows_f = visible_rows as f32;
        let mut row_gap = if n_rows_f > 0.0 {
            (help_top_y - first_row_center_y - ((n_rows_f - 0.5) * frame_h)) / n_rows_f
        } else { 0.0 };
        if !row_gap.is_finite() { row_gap = 0.0; }
        if row_gap < 0.0 { row_gap = 0.0; }
        let max_offset = total_rows.saturating_sub(visible_rows);
        let offset_rows = if total_rows <= visible_rows {
            0
        } else {
            state.selected_row.saturating_sub(anchor_row).min(max_offset)
        };
        let prev_idx = state.prev_selected_row;
        let i_prev_vis = (prev_idx as isize) - (offset_rows as isize);
        let from_y = first_row_center_y + (i_prev_vis as f32) * (frame_h + row_gap);
        state.cursor_row_anim_from_y = from_y;
        state.cursor_row_anim_t = 0.0;
        state.cursor_row_anim_from_row = Some(prev_idx);
        // Reset help reveal animation on row change
        state.help_anim_time = 0.0;
        state.prev_selected_row = state.selected_row;
    }

    // Advance cursor tween, if any
    if state.cursor_anim_row.is_some() && state.cursor_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_anim_t = (state.cursor_anim_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_anim_t = 1.0;
        }
        if state.cursor_anim_t >= 1.0 {
            state.cursor_anim_row = None;
        }
    }
    // Advance vertical row tween, if any
    if state.cursor_row_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_row_anim_t = (state.cursor_row_anim_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_row_anim_t = 1.0;
        }
        if state.cursor_row_anim_t >= 1.0 {
            state.cursor_row_anim_from_row = None;
        }
    }
}

// Helpers for hold-to-scroll controlled by the app dispatcher
pub fn on_nav_press(state: &mut State, dir: NavDirection) {
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(Instant::now());
    state.nav_key_last_scrolled_at = Some(Instant::now());
}

pub fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

fn toggle_scroll_row(state: &mut State) {
    let row_index = state.selected_row;
    if let Some(row) = state.rows.get(row_index) {
        if row.name != "Scroll" {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index;
    let bit = if choice_index < 8 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.scroll_active_mask & bit) != 0 {
        state.scroll_active_mask &= !bit;
    } else {
        state.scroll_active_mask |= bit;
    }

    // Rebuild the ScrollOption bitmask from the active choices.
    use crate::game::profile::ScrollOption;
    let mut setting = ScrollOption::Normal;
    if state.scroll_active_mask != 0 {
        if (state.scroll_active_mask & (1u8 << 0)) != 0 {
            setting = setting.union(ScrollOption::Reverse);
        }
        if (state.scroll_active_mask & (1u8 << 1)) != 0 {
            setting = setting.union(ScrollOption::Split);
        }
        if (state.scroll_active_mask & (1u8 << 2)) != 0 {
            setting = setting.union(ScrollOption::Alternate);
        }
        if (state.scroll_active_mask & (1u8 << 3)) != 0 {
            setting = setting.union(ScrollOption::Cross);
        }
    }
    crate::game::profile::update_scroll_option(setting);
    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn toggle_fa_plus_row(state: &mut State) {
    let row_index = state.selected_row;
    if let Some(row) = state.rows.get(row_index) {
        if row.name != "FA+ Options" {
            return;
        }
    } else {
        return;
    }

    let choice_index = state.rows[row_index].selected_choice_index;
    let bit = if choice_index < 3 {
        1u8 << (choice_index as u8)
    } else {
        0
    };
    if bit == 0 {
        return;
    }

    // Toggle this bit in the local mask.
    if (state.fa_plus_active_mask & bit) != 0 {
        state.fa_plus_active_mask &= !bit;
    } else {
        state.fa_plus_active_mask |= bit;
    }

    // Persist back to profile.
    let window_enabled = (state.fa_plus_active_mask & (1u8 << 0)) != 0;
    let ex_enabled = (state.fa_plus_active_mask & (1u8 << 1)) != 0;
    let pane_enabled = (state.fa_plus_active_mask & (1u8 << 2)) != 0;
    crate::game::profile::update_show_fa_plus_window(window_enabled);
    crate::game::profile::update_show_ex_score(ex_enabled);
    crate::game::profile::update_show_fa_plus_pane(pane_enabled);

    audio::play_sfx("assets/sounds/change_value.ogg");
}

fn switch_to_pane(state: &mut State, pane: OptionsPane) {
    if state.current_pane == pane {
        return;
    }
    let mut rows = build_rows(
        &state.song,
        &state.speed_mod,
        state.chart_difficulty_index,
        state.music_rate,
        pane,
    );
    let (scroll_active_mask, fa_plus_active_mask) = apply_profile_defaults(&mut rows);
    state.rows = rows;
    state.scroll_active_mask = scroll_active_mask;
    state.fa_plus_active_mask = fa_plus_active_mask;
    state.current_pane = pane;
    state.selected_row = 0;
    state.prev_selected_row = 0;
    state.cursor_anim_row = None;
    state.cursor_anim_t = 1.0;
    state.cursor_row_anim_t = 1.0;
    state.cursor_row_anim_from_row = None;
    state.help_anim_time = 0.0;
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match ev.action {
        VirtualAction::p1_back if ev.pressed => return ScreenAction::Navigate(Screen::SelectMusic),
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if let Some(_) = state.rows.get(0) {
                if ev.pressed {
                    let num_rows = state.rows.len();
                    state.selected_row = (state.selected_row + num_rows - 1) % num_rows;
                    on_nav_press(state, NavDirection::Up);
                } else {
                    on_nav_release(state, NavDirection::Up);
                }
            }
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if let Some(_) = state.rows.get(0) {
                if ev.pressed {
                    let num_rows = state.rows.len();
                    state.selected_row = (state.selected_row + 1) % num_rows;
                    on_nav_press(state, NavDirection::Down);
                } else {
                    on_nav_release(state, NavDirection::Down);
                }
            }
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if ev.pressed { apply_choice_delta(state, -1); on_nav_press(state, NavDirection::Left); }
            else { on_nav_release(state, NavDirection::Left); }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if ev.pressed { apply_choice_delta(state, 1); on_nav_press(state, NavDirection::Right); }
            else { on_nav_release(state, NavDirection::Right); }
        }
        VirtualAction::p1_start if ev.pressed => {
            let num_rows = state.rows.len();
            if num_rows == 0 {
                // Nothing to do.
            } else if matches!(
                state.rows.get(state.selected_row),
                Some(row) if row.name == "Scroll"
            ) {
                // Scroll row uses Start as a toggle for the currently focused option.
                toggle_scroll_row(state);
            } else if matches!(
                state.rows.get(state.selected_row),
                Some(row) if row.name == "FA+ Options"
            ) {
                // FA+ Options row uses Start as a toggle for the currently focused option.
                toggle_fa_plus_row(state);
            } else if state.selected_row == num_rows - 1 {
                if let Some(what_comes_next_row) = state.rows.get(num_rows - 2) {
                    if what_comes_next_row.name == "What comes next?" {
                        if let Some(choice) = what_comes_next_row
                            .choices
                            .get(what_comes_next_row.selected_choice_index)
                        {
                            match choice.as_str() {
                                "Gameplay" => return ScreenAction::Navigate(Screen::Gameplay),
                                "Choose a Different Song" => {
                                    return ScreenAction::Navigate(Screen::SelectMusic)
                                }
                                "Advanced Modifiers" => {
                                    switch_to_pane(state, OptionsPane::Advanced);
                                }
                                "Uncommon Modifiers" => {
                                    switch_to_pane(state, OptionsPane::Uncommon);
                                }
                                "Main Modifiers" => {
                                    switch_to_pane(state, OptionsPane::Main);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    ScreenAction::None
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);
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
    }));
    // Speed Mod Helper Display (from overlay.lua)
    // Shows the effective scroll speed (e.g., "X390" for 3.25x on 120 BPM)
    let speed_mod_y = 48.0;
    let speed_mod_x = screen_center_x() + widescale(-77.0, -100.0);
    // All previews (judgment, hold, noteskin, combo) share this center line.
    // Tweak these to dial in parity with Simply Love.
    const PREVIEW_CENTER_OFFSET_NORMAL: f32 = 80.75; // 4:3
    const PREVIEW_CENTER_OFFSET_WIDE: f32 = 98.75; // 16:9
    let preview_center_x = speed_mod_x + widescale(PREVIEW_CENTER_OFFSET_NORMAL, PREVIEW_CENTER_OFFSET_WIDE);
    let speed_color = color::simply_love_rgba(state.active_color_index);
   
    // Calculate effective BPM for display. For X-mod parity with gameplay, use reference BPM.
    let reference_bpm = reference_bpm_for_song(&state.song);
    let effective_song_bpm = (reference_bpm as f64) * state.music_rate as f64;
   
    let speed_text = match state.speed_mod.mod_type.as_str() {
        "X" => {
            // For X-mod, show the effective BPM accounting for music rate
            // (e.g., "X390" for 3.25x on 120 BPM at 1.0x rate)
            let effective_bpm = (state.speed_mod.value * effective_song_bpm as f32).round() as i32;
            format!("X{}", effective_bpm)
        }
        "C" => format!("C{}", state.speed_mod.value as i32),
        "M" => format!("M{}", state.speed_mod.value as i32),
        _ => format!("{:.2}x", state.speed_mod.value),
    };
   
    actors.push(act!(text: font("wendy"): settext(speed_text):
        align(0.5, 0.5): xy(speed_mod_x, speed_mod_y): zoom(0.5):
        diffuse(speed_color[0], speed_color[1], speed_color[2], 1.0):
        z(121)
    ));
    /* ---------- SHARED GEOMETRY (rows aligned to help box) ---------- */
    // Help Text Box (from underlay.lua) — define this first so rows can match its width/left.
    let help_box_h = 40.0;
    let help_box_w = widescale(614.0, 792.0);
    let help_box_x = widescale(13.0, 30.666);
    let help_box_bottom_y = screen_height() - 36.0;
    // --- Row Layout Constants & Scrolling ---
    const VISIBLE_ROWS: usize = 10;
    const ANCHOR_ROW: usize = 5; // Keep selection on the 5th visible row
    const ROW_START_OFFSET: f32 = -164.0;
    const ROW_HEIGHT: f32 = 33.0;
    const TITLE_BG_WIDTH: f32 = 127.0;
    let total_rows = state.rows.len();
    let max_offset = total_rows.saturating_sub(VISIBLE_ROWS);
    let offset_rows = if total_rows <= VISIBLE_ROWS {
        0
    } else {
        state.selected_row.saturating_sub(ANCHOR_ROW).min(max_offset)
    };
    let frame_h = ROW_HEIGHT;
    // Compute dynamic row gap so the space between the last visible
    // row and the help box equals all other inter-row gaps.
    // Derivation (using row centers):
    //   help_top = y0 + (N - 0.5)*H + N*gap  =>  gap = (help_top - y0 - (N - 0.5)*H)/N
    // where y0 is the first row center, H is row height, N is number of rows.
    let first_row_center_y = screen_center_y() + ROW_START_OFFSET;
    let help_top_y = help_box_bottom_y - help_box_h;
    // Use VISIBLE_ROWS for gap calculation
    let n_rows_f = VISIBLE_ROWS as f32;
    let mut row_gap = if n_rows_f > 0.0 {
        (help_top_y - first_row_center_y - ((n_rows_f - 0.5) * frame_h)) / n_rows_f
    } else {
        0.0
    };
    if !row_gap.is_finite() { row_gap = 0.0; }
    if row_gap < 0.0 { row_gap = 0.0; }
    // Make row frame LEFT and WIDTH exactly match the help box.
    let row_left = help_box_x;
    let row_width = help_box_w;
    //let row_center_x = row_left + (row_width * 0.5);
    let title_zoom = 0.88;
    // Title text x: slightly less padding so text sits further left
    let title_x = row_left + widescale(7.0, 13.0);
    // Helper to compute the cursor center X for a given row index.
    let calc_row_center_x = |row_idx: usize| -> f32 {
        if row_idx >= state.rows.len() { return speed_mod_x; }
        let r = &state.rows[row_idx];
        if r.name.is_empty() {
            // Exit row aligns with Speed Mod helper
            return speed_mod_x;
        }
        let is_inline = r.name == "Perspective"
            || r.name == "Background Filter"
            || r.name == "Stepchart"
            || r.name == "What comes next?"
            || r.name == "Turn"
            || r.name == "Scroll"
            || r.name == "Hide"
            || r.name == "LifeMeter Type"
            || r.name == "Data Visualizations"
            || r.name.starts_with("Gameplay Extras")
            || r.name == "Judgment Tilt Intensity"
            || r.name == "Insert"
            || r.name == "Remove"
            || r.name == "Holds"
            || r.name == "Accel Effects"
            || r.name == "Visual Effects"
            || r.name == "Appearance Effects"
            || r.name == "Attacks"
            || r.name == "Characters"
            || r.name == "Hide Light Type";
        if is_inline {
            let value_zoom = 0.835_f32;
            let spacing = 15.75_f32;
            let choice_inner_left = widescale(162.0, 176.0);
            let mut widths: Vec<f32> = Vec::with_capacity(r.choices.len());
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    for text in &r.choices {
                        let mut w = crate::ui::font::measure_line_width_logical(metrics_font, text, all_fonts) as f32;
                        if !w.is_finite() || w <= 0.0 { w = 1.0; }
                        widths.push(w * value_zoom);
                    }
                });
            });
            if widths.is_empty() { return speed_mod_x; }
            let mut x_positions: Vec<f32> = Vec::with_capacity(widths.len());
            let mut x = choice_inner_left;
            for w in &widths {
                x_positions.push(x);
                x += *w + spacing;
            }
            let sel = r.selected_choice_index.min(widths.len().saturating_sub(1));
            return x_positions[sel] + widths[sel] * 0.5;
        } else {
            // Single value rows: default to Speed Mod helper X, except Music Rate centered in items column
            let mut cx = speed_mod_x;
            if r.name.starts_with("Music Rate") {
                let item_col_left = row_left + TITLE_BG_WIDTH;
                let item_col_w = row_width - TITLE_BG_WIDTH;
                cx = item_col_left + item_col_w * 0.5;
            }
            return cx;
        }
    };

    // Helper to compute draw_w/draw_h (text box) for the selected item of a row
    let calc_row_dims = |row_idx: usize| -> (f32, f32) {
        let value_zoom = 0.835_f32;
        let mut out_w = 40.0_f32;
        let mut out_h = 16.0_f32;
        if row_idx >= state.rows.len() {
            // Fallback; overridden below when font metrics are available
            return (out_w, out_h);
        }
        let r = &state.rows[row_idx];
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |metrics_font| {
                out_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                if r.choices.is_empty() {
                    out_w = 40.0;
                    return;
                }
                // For inline rows, measure the selected choice; single-value rows do the same
                let sel = r.selected_choice_index.min(r.choices.len() - 1);
                let mut w = crate::ui::font::measure_line_width_logical(metrics_font, &r.choices[sel], all_fonts) as f32;
                if !w.is_finite() || w <= 0.0 { w = 1.0; }
                out_w = w * value_zoom;
            });
        });
        (out_w, out_h)
    };

    for i_vis in 0..VISIBLE_ROWS {
        let item_idx = offset_rows + i_vis;
        if item_idx >= total_rows {
            break;
        }
        let current_row_y = first_row_center_y + (i_vis as f32) * (frame_h + row_gap);
        let is_active = item_idx == state.selected_row;
        let row = &state.rows[item_idx];
        let active_bg = color::rgba_hex("#333333");
        let inactive_bg_base = color::rgba_hex("#071016");
        let bg_color = if is_active {
            active_bg
        } else {
            [inactive_bg_base[0], inactive_bg_base[1], inactive_bg_base[2], 0.8]
        };
        // Row background — matches help box width & left
        actors.push(act!(quad:
            align(0.0, 0.5): xy(row_left, current_row_y):
            zoomto(row_width, frame_h):
            diffuse(bg_color[0], bg_color[1], bg_color[2], bg_color[3]):
            z(100)
        ));
        if !row.name.is_empty() {
            actors.push(act!(quad:
                align(0.0, 0.5): xy(row_left, current_row_y):
                zoomto(TITLE_BG_WIDTH, frame_h):
                diffuse(0.0, 0.0, 0.0, 0.25):
                z(101)
            ));
        }
        // Left column (row titles)
        let title_color = if is_active {
            let mut c = color::simply_love_rgba(state.active_color_index);
            c[3] = 1.0;
            c
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        // Handle multi-line row titles (e.g., "Music Rate\nbpm: 120")
        if row.name.contains('\n') {
            let lines: Vec<&str> = row.name.split('\n').collect();
            if lines.len() == 2 {
                // First line (e.g., "Music Rate")
                actors.push(act!(text: font("miso"): settext(lines[0].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y - 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(widescale(128.0, 120.0)):
                    z(101)
                ));
                // Second line (e.g., "bpm: 120") - smaller and slightly below
                actors.push(act!(text: font("miso"): settext(lines[1].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y + 7.0): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(widescale(128.0, 120.0)):
                    z(101)
                ));
            } else {
                // Fallback for unexpected multi-line format
                actors.push(act!(text: font("miso"): settext(row.name.clone()):
                    align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(widescale(128.0, 120.0)):
                    z(101)
                ));
            }
        } else {
            // Single-line title (normal case)
            actors.push(act!(text: font("miso"): settext(row.name.clone()):
                align(0.0, 0.5): xy(title_x, current_row_y): zoom(title_zoom):
                diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                horizalign(left): maxwidth(widescale(128.0, 120.0)):
                z(101)
            ));
        }
        // Inactive option text color should be #808080 (alpha 1.0)
        let sl_gray = color::rgba_hex("#808080");
        // Some rows should display all choices inline
        let show_all_choices_inline = row.name == "Perspective"
            || row.name == "Background Filter"
            || row.name == "Stepchart"
            || row.name == "What comes next?"
            || row.name == "Turn"
            || row.name == "Scroll"
            || row.name == "Hide"
            || row.name == "LifeMeter Type"
            || row.name == "Data Visualizations"
            || row.name.starts_with("Gameplay Extras")
            || row.name == "Judgment Tilt Intensity"
            || row.name == "FA+ Options"
            || row.name == "Insert"
            || row.name == "Remove"
            || row.name == "Holds"
            || row.name == "Accel Effects"
            || row.name == "Visual Effects"
            || row.name == "Appearance Effects"
            || row.name == "Attacks"
            || row.name == "Characters"
            || row.name == "Hide Light Type";
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
            let choice_text = &row.choices[row.selected_choice_index];
            let choice_color = if is_active { [1.0, 1.0, 1.0, 1.0] } else { sl_gray };
            // Align Exit horizontally with other single-value options (Speed Mod line)
            let choice_center_x = speed_mod_x;
            actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(0.835):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                z(101)
            ));
            // Draw the selection cursor for the centered "Exit" text when active
            if is_active {
                let value_zoom = 0.835;
                asset_manager.with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        let mut text_w = crate::ui::font::measure_line_width_logical(metrics_font, choice_text, all_fonts) as f32;
                        if !text_w.is_finite() || text_w <= 0.0 { text_w = 1.0; }
                        let text_h = (metrics_font.height as f32).max(1.0);
                        let draw_w = text_w * value_zoom;
                        let draw_h = text_h * value_zoom;
                        let pad_y = widescale(6.0, 8.0);
                        let min_pad_x = widescale(2.0, 3.0);
                        let max_pad_x = widescale(22.0, 28.0);
                        let width_ref = widescale(180.0, 220.0);
                        let t = (draw_w / width_ref).clamp(0.0, 1.0);
                        let mut pad_x = min_pad_x + (max_pad_x - min_pad_x) * t;
                        let border_w = widescale(2.0, 2.5);
                        // Cap pad so the ring never invades adjacent inline item space
                        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                        if pad_x > max_pad_by_spacing { pad_x = max_pad_by_spacing; }
                        let mut ring_w = draw_w + pad_x * 2.0;
                        let mut ring_h = draw_h + pad_y * 2.0;
                        let mut center_x = choice_center_x; // Align with single-value line
                        // Vertical tween for row transitions
                        let mut center_y = current_row_y;
                        if state.cursor_row_anim_t < 1.0 {
                            let t = ease_out_cubic(state.cursor_row_anim_t);
                            // If we have a previous row index, interpolate X from that row's cursor center
                            if let Some(from_row) = state.cursor_row_anim_from_row {
                                let from_x = calc_row_center_x(from_row);
                                center_x = from_x + (center_x - from_x) * t;
                            }
                            center_y = state.cursor_row_anim_from_y + (current_row_y - state.cursor_row_anim_from_y) * t;
                        }
                        // Interpolate ring size between previous row and this row when vertically tweening
                        if state.cursor_row_anim_t < 1.0 {
                            if let Some(from_row) = state.cursor_row_anim_from_row {
                                let (from_dw, from_dh) = calc_row_dims(from_row);
                                let tsize = (from_dw / width_ref).clamp(0.0, 1.0);
                                let mut pad_x_from = min_pad_x + (max_pad_x - min_pad_x) * tsize;
                                let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                                if pad_x_from > max_pad_by_spacing { pad_x_from = max_pad_by_spacing; }
                                let ring_w_from = from_dw + pad_x_from * 2.0;
                                let ring_h_from = from_dh + pad_y * 2.0;
                                let t = ease_out_cubic(state.cursor_row_anim_t);
                                ring_w = ring_w_from + (ring_w - ring_w_from) * t;
                                ring_h = ring_h_from + (ring_h - ring_h_from) * t;
                            }
                        }
                        let left = center_x - ring_w * 0.5;
                        let right = center_x + ring_w * 0.5;
                        let top = center_y - ring_h * 0.5;
                        let bottom = center_y + ring_h * 0.5;
                        let mut ring_color = color::decorative_rgba(state.active_color_index);
                        ring_color[3] = 1.0;
                        // Top, Bottom, Left, Right borders
                        actors.push(act!(quad: align(0.5, 0.5): xy(center_x, top + border_w * 0.5): zoomto(ring_w, border_w): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                        actors.push(act!(quad: align(0.5, 0.5): xy(center_x, bottom - border_w * 0.5): zoomto(ring_w, border_w): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                        actors.push(act!(quad: align(0.5, 0.5): xy(left + border_w * 0.5, center_y): zoomto(border_w, ring_h): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                        actors.push(act!(quad: align(0.5, 0.5): xy(right - border_w * 0.5, center_y): zoomto(border_w, ring_h): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                    });
                });
            }
        } else if show_all_choices_inline {
            // Render every option horizontally; when active, all options should be white.
            // The active option gets an underline (quad) drawn just below the text.
            let value_zoom = 0.835;
            let spacing = 15.75;
            // First pass: measure widths to lay out options inline
            let mut widths: Vec<f32> = Vec::with_capacity(row.choices.len());
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    for text in &row.choices {
                        let mut w = crate::ui::font::measure_line_width_logical(metrics_font, text, all_fonts) as f32;
                        if !w.is_finite() || w <= 0.0 { w = 1.0; }
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
                let mask = state.scroll_active_mask;
                if mask != 0 {
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            asset_manager.with_fonts(|_all_fonts| {
                                asset_manager.with_font("miso", |metrics_font| {
                                    let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                                    let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                    let underline_w = draw_w.ceil();
                                    let offset = widescale(3.0, 4.0);
                                    let underline_y = current_row_y + text_h * 0.5 + offset;
                                    let mut line_color = color::decorative_rgba(state.active_color_index);
                                    line_color[3] = 1.0;
                                    actors.push(act!(quad:
                                        align(0.0, 0.5):
                                        xy(sel_x, underline_y):
                                        zoomto(underline_w, line_thickness):
                                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                        z(101)
                                    ));
                                });
                            });
                        }
                    }
                }
            } else if row.name == "FA+ Options" {
                let mask = state.fa_plus_active_mask;
                if mask != 0 {
                    for idx in 0..row.choices.len() {
                        let bit = 1u8 << (idx as u8);
                        if (mask & bit) == 0 {
                            continue;
                        }
                        if let Some(sel_x) = x_positions.get(idx).copied() {
                            let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                            asset_manager.with_fonts(|_all_fonts| {
                                asset_manager.with_font("miso", |metrics_font| {
                                    let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                                    let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                    let underline_w = draw_w.ceil();
                                    let offset = widescale(3.0, 4.0);
                                    let underline_y = current_row_y + text_h * 0.5 + offset;
                                    let mut line_color = color::decorative_rgba(state.active_color_index);
                                    line_color[3] = 1.0;
                                    actors.push(act!(quad:
                                        align(0.0, 0.5):
                                        xy(sel_x, underline_y):
                                        zoomto(underline_w, line_thickness):
                                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                        z(101)
                                    ));
                                });
                            });
                        }
                    }
                }
            } else {
                let idx = row.selected_choice_index;
                if let Some(sel_x) = x_positions.get(idx).copied() {
                    let draw_w = widths.get(idx).copied().unwrap_or(40.0);
                    asset_manager.with_fonts(|_all_fonts| {
                        asset_manager.with_font("miso", |metrics_font| {
                            let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                            let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                            let underline_w = draw_w.ceil();
                            let offset = widescale(3.0, 4.0);
                            let underline_y = current_row_y + text_h * 0.5 + offset;
                            let mut line_color = color::decorative_rgba(state.active_color_index);
                            line_color[3] = 1.0;
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(sel_x, underline_y):
                                zoomto(underline_w, line_thickness):
                                diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                z(101)
                            ));
                        });
                    });
                }
            }
            // Draw the 4-sided cursor ring around the selected option when this row is active.
            // If a tween is in progress for this row, animate the ring's X position (SL's CursorTweenSeconds).
            if is_active {
                let sel_idx = row.selected_choice_index;
                if let Some(target_left_x) = x_positions.get(sel_idx).copied() {
                    let draw_w = widths.get(sel_idx).copied().unwrap_or(40.0);
                    asset_manager.with_fonts(|_all_fonts| {
                        asset_manager.with_font("miso", |metrics_font| {
                            let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                            let pad_y = widescale(6.0, 8.0);
                            let min_pad_x = widescale(2.0, 3.0);
                            let max_pad_x = widescale(22.0, 28.0);
                            let width_ref = widescale(180.0, 220.0);
                            let mut size_t_to = draw_w / width_ref;
                            if !size_t_to.is_finite() { size_t_to = 0.0; }
                            if size_t_to < 0.0 { size_t_to = 0.0; }
                            if size_t_to > 1.0 { size_t_to = 1.0; }
                            let mut pad_x_to = min_pad_x + (max_pad_x - min_pad_x) * size_t_to;
                            let border_w = widescale(2.0, 2.5);
                            // Cap pad so ring doesn't encroach neighbors
                            let max_pad_by_spacing = (spacing - border_w).max(min_pad_x);
                            if pad_x_to > max_pad_by_spacing { pad_x_to = max_pad_by_spacing; }
                            let mut ring_w = draw_w + pad_x_to * 2.0;
                            let mut ring_h = text_h + pad_y * 2.0;

                            // Determine animated center X when tweening, otherwise snap to target.
                            let mut center_x = target_left_x + draw_w * 0.5;
                            // Vertical tween for row transitions
                            let mut center_y = current_row_y;
                            if state.cursor_row_anim_t < 1.0 {
                                let t = ease_out_cubic(state.cursor_row_anim_t);
                                if let Some(from_row) = state.cursor_row_anim_from_row {
                                    let from_x = calc_row_center_x(from_row);
                                    center_x = from_x + (center_x - from_x) * t;
                                }
                                center_y = state.cursor_row_anim_from_y + (current_row_y - state.cursor_row_anim_from_y) * t;
                            }
                            if let Some(anim_row) = state.cursor_anim_row {
                                if anim_row == item_idx && state.cursor_anim_t < 1.0 {
                                    let from_idx = state.cursor_anim_from_choice.min(widths.len().saturating_sub(1));
                                    let to_idx = sel_idx.min(widths.len().saturating_sub(1));
                                    let from_center_x = x_positions[from_idx] + widths[from_idx] * 0.5;
                                    let to_center_x = x_positions[to_idx] + widths[to_idx] * 0.5;
                                    let t = ease_out_cubic(state.cursor_anim_t);
                                    center_x = from_center_x + (to_center_x - from_center_x) * t;
                                    // Also interpolate ring size from previous choice to current choice
                                    let from_draw_w = widths[from_idx];
                                    let mut size_t_from = from_draw_w / width_ref;
                                    if !size_t_from.is_finite() { size_t_from = 0.0; }
                                    if size_t_from < 0.0 { size_t_from = 0.0; }
                                    if size_t_from > 1.0 { size_t_from = 1.0; }
                                    let mut pad_x_from = min_pad_x + (max_pad_x - min_pad_x) * size_t_from;
                                    let max_pad_by_spacing = (spacing - border_w).max(min_pad_x);
                                    if pad_x_from > max_pad_by_spacing { pad_x_from = max_pad_by_spacing; }
                                    let ring_w_from = from_draw_w + pad_x_from * 2.0;
                                    let ring_h_from = text_h + pad_y * 2.0;
                                    ring_w = ring_w_from + (ring_w - ring_w_from) * t;
                                    ring_h = ring_h_from + (ring_h - ring_h_from) * t;
                                }
                            }
                            // If not horizontally tweening, but vertically tweening rows, interpolate size
                            if state.cursor_row_anim_t < 1.0 && (state.cursor_anim_row.is_none() || state.cursor_anim_row != Some(item_idx)) {
                                if let Some(from_row) = state.cursor_row_anim_from_row {
                                    let (from_dw, from_dh) = calc_row_dims(from_row);
                                    let mut size_t_from = from_dw / width_ref;
                                    if !size_t_from.is_finite() { size_t_from = 0.0; }
                                    if size_t_from < 0.0 { size_t_from = 0.0; }
                                    if size_t_from > 1.0 { size_t_from = 1.0; }
                                    let mut pad_x_from = min_pad_x + (max_pad_x - min_pad_x) * size_t_from;
                                    let max_pad_by_spacing = (spacing - border_w).max(min_pad_x);
                                    if pad_x_from > max_pad_by_spacing { pad_x_from = max_pad_by_spacing; }
                                    let ring_w_from = from_dw + pad_x_from * 2.0;
                                    let ring_h_from = from_dh + pad_y * 2.0;
                                    let t = ease_out_cubic(state.cursor_row_anim_t);
                                    ring_w = ring_w_from + (ring_w - ring_w_from) * t;
                                    ring_h = ring_h_from + (ring_h - ring_h_from) * t;
                                }
                            }

                            let left = center_x - ring_w * 0.5;
                            let right = center_x + ring_w * 0.5;
                            let top = center_y - ring_h * 0.5;
                            let bottom = center_y + ring_h * 0.5;
                            let mut ring_color = color::decorative_rgba(state.active_color_index);
                            ring_color[3] = 1.0;
                            // Top border
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy((left + right) * 0.5, top + border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            // Bottom border
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy((left + right) * 0.5, bottom - border_w * 0.5):
                                zoomto(ring_w, border_w):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            // Left border
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(left + border_w * 0.5, (top + bottom) * 0.5):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                            // Right border
                            actors.push(act!(quad:
                                align(0.5, 0.5): xy(right - border_w * 0.5, (top + bottom) * 0.5):
                                zoomto(border_w, ring_h):
                                diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                z(101)
                            ));
                        });
                    });
                }
            }
            // Draw each option's text (active row: all white; inactive: #808080)
            for (idx, text) in row.choices.iter().enumerate() {
                let x = x_positions.get(idx).copied().unwrap_or(choice_inner_left);
                let color_rgba = if is_active { [1.0, 1.0, 1.0, 1.0] } else { sl_gray };
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
            let mut choice_center_x = speed_mod_x;
            if row.name.starts_with("Music Rate") {
                let item_col_left = row_left + TITLE_BG_WIDTH;
                let item_col_w = row_width - TITLE_BG_WIDTH;
                choice_center_x = item_col_left + item_col_w * 0.5;
            }
            let choice_text = &row.choices[row.selected_choice_index];
            let choice_color = if is_active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                sl_gray
            };
            asset_manager.with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |metrics_font| {
                    let mut text_w = crate::ui::font::measure_line_width_logical(metrics_font, choice_text, all_fonts) as f32;
                    if !text_w.is_finite() || text_w <= 0.0 { text_w = 1.0; }
                    let text_h = (metrics_font.height as f32).max(1.0);
                    let value_zoom = 0.835;
                    let draw_w = text_w * value_zoom;
                    let draw_h = text_h * value_zoom;
                    actors.push(act!(text: font("miso"): settext(choice_text.clone()):
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
                    let mut line_color = color::decorative_rgba(state.active_color_index);
                    line_color[3] = 1.0;
                    actors.push(act!(quad:
                        align(0.0, 0.5): // start at text's left edge
                        xy(underline_left_x, underline_y):
                        zoomto(underline_w, line_thickness):
                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                        z(101)
                    ));
                    // Encircling cursor around the active option value (programmatic border)
                    if is_active {
                        let pad_y = widescale(6.0, 8.0);
                        let min_pad_x = widescale(2.0, 3.0);
                        let max_pad_x = widescale(22.0, 28.0);
                        let width_ref = widescale(180.0, 220.0);
                        let t = (draw_w / width_ref).clamp(0.0, 1.0);
                        let mut pad_x = min_pad_x + (max_pad_x - min_pad_x) * t;
                        let border_w = widescale(2.0, 2.5);
                        // Cap pad for single-value rows too (consistency)
                        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                        if pad_x > max_pad_by_spacing { pad_x = max_pad_by_spacing; }
                        let mut ring_w = draw_w + pad_x * 2.0;
                        let mut ring_h = draw_h + pad_y * 2.0;
                        let mut center_x = choice_center_x;
                        // Vertical tween for row transitions
                        let mut center_y = current_row_y;
                        if state.cursor_row_anim_t < 1.0 {
                            let t = ease_out_cubic(state.cursor_row_anim_t);
                            if let Some(from_row) = state.cursor_row_anim_from_row {
                                let from_x = calc_row_center_x(from_row);
                                center_x = from_x + (center_x - from_x) * t;
                            }
                            center_y = state.cursor_row_anim_from_y + (current_row_y - state.cursor_row_anim_from_y) * t;
                        }
                        // Interpolate ring size between previous row and this row when vertically tweening
                        if state.cursor_row_anim_t < 1.0 {
                            if let Some(from_row) = state.cursor_row_anim_from_row {
                                let (from_dw, from_dh) = calc_row_dims(from_row);
                                let tsize = (from_dw / width_ref).clamp(0.0, 1.0);
                                let pad_x_from = min_pad_x + (max_pad_x - min_pad_x) * tsize;
                                let ring_w_from = from_dw + pad_x_from * 2.0;
                                let ring_h_from = from_dh + pad_y * 2.0;
                                let t = ease_out_cubic(state.cursor_row_anim_t);
                                ring_w = ring_w_from + (ring_w - ring_w_from) * t;
                                ring_h = ring_h_from + (ring_h - ring_h_from) * t;
                            }
                        }
                        let left = center_x - ring_w * 0.5;
                        let right = center_x + ring_w * 0.5;
                        let top = center_y - ring_h / 2.0;
                        let bottom = center_y + ring_h / 2.0;
                        let mut ring_color = color::decorative_rgba(state.active_color_index);
                        ring_color[3] = 1.0;
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
                    // Add previews (centered on a shared vertical line)
                    // Add judgment preview for "Judgment Font" row showing Fantastic frame of the selected font
                    if row.name == "Judgment Font" {
                        let texture_key = match choice_text.as_str() {
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
                        };
                        if let Some(texture) = texture_key {
                            // Fantastic is the first frame (top-left, column 0, row 0)
                            // Scale to 0.2x: Simply Love uses 0.4x, but our texture is doubleres, so 0.4 / 2 = 0.2
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_center_x, current_row_y):
                                setstate(0):
                                zoom(0.225):
                                z(102)
                            ));
                        }
                    }
                    // Add hold judgment preview for "Hold Judgment" row showing both frames (Held and Let Go)
                    if row.name == "Hold Judgment" {
                        let texture_key = match choice_text.as_str() {
                            "Love" => Some("hold_judgements/Love 1x2 (doubleres).png"),
                            "mute" => Some("hold_judgements/mute 1x2 (doubleres).png"),
                            "ITG2" => Some("hold_judgements/ITG2 1x2 (doubleres).png"),
                            "None" => None,
                            _ => None,
                        };
                        if let Some(texture) = texture_key {
                            // 1x2 doubleres: row 0 = Held, row 1 = Let Go.
                            // Match Simply Love's spacing: each sprite is offset horizontally by
                            // width * 0.4 from the center, after applying our preview zoom.
                            let zoom = 0.225;
                            let tex_w = crate::assets::texture_dims(texture)
                                .map(|meta| meta.w.max(1) as f32)
                                .unwrap_or(128.0);
                            let center_offset = tex_w * zoom * 0.4;

                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_center_x - center_offset, current_row_y):
                                setstate(0):
                                zoom(zoom):
                                z(102)
                            ));
                            actors.push(act!(sprite(texture):
                                align(0.5, 0.5):
                                xy(preview_center_x + center_offset, current_row_y):
                                setstate(1):
                                zoom(zoom):
                                z(102)
                            ));
                        }
                    }
                    // Add noteskin preview for "NoteSkin" row showing animated 4th note
                    if row.name == "NoteSkin" {
                        if let Some(ns) = &state.noteskin {
                            // Render a 4th note (Quantization::Q4th = 0) for column 2 (Up arrow)
                            // In dance-single: Left=0, Down=1, Up=2, Right=3
                            let note_idx = 2 * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
                            if let Some(note_slot) = ns.notes.get(note_idx) {
                                // Get the current animation frame using preview_time and preview_beat
                                let frame = note_slot.frame_index(state.preview_time, state.preview_beat);
                                let uv = note_slot.uv_for_frame(frame);
                               
                                // Scale the note to match Simply Love's 0.4x preview zoom
                                // Note: cel noteskin textures are NOT doubleres, so we use 0.4x directly
                                let size = note_slot.size();
                                let width = size[0].max(1) as f32;
                                let height = size[1].max(1) as f32;
                               
                                // Target size: 64px is the gameplay size, so 0.4x of that is 25.6px
                                const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;
                                const PREVIEW_SCALE: f32 = 0.45;
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                               
                                let scale = if height > 0.0 {
                                    target_height / height
                                } else {
                                    PREVIEW_SCALE
                                };
                                let final_width = width * scale;
                                let final_height = target_height;
                               
                                actors.push(act!(sprite(note_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(preview_center_x, current_row_y):
                                    zoomto(final_width, final_height):
                                    rotationz(-note_slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    z(102)
                                ));
                            }
                        }
                    }
                    // Add combo preview for "Combo Font" row showing ticking numbers
                    if row.name == "Combo Font" {
                        let combo_text = state.combo_preview_count.to_string();
                        let combo_zoom = 0.45;
                        let font_name_opt = match choice_text.as_str() {
                            "Wendy" => Some("wendy_combo"),
                            "Arial Rounded" => Some("combo_arial_rounded"),
                            "Asap" => Some("combo_asap"),
                            "Bebas Neue" => Some("combo_bebas_neue"),
                            "Source Code" => Some("combo_source_code"),
                            "Work" => Some("combo_work"),
                            "Wendy (Cursed)" => Some("combo_wendy_cursed"),
                            "None" => None,
                            _ => Some("wendy_combo"),
                        };
                        if let Some(font_name) = font_name_opt {
                            actors.push(act!(text:
                                font(font_name): settext(combo_text):
                                align(0.5, 0.5):
                                xy(preview_center_x, current_row_y):
                                zoom(combo_zoom): horizalign(center):
                                diffuse(1.0, 1.0, 1.0, 1.0):
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
        diffuse(0.0, 0.0, 0.0, 0.8)
    ));
    if let Some(row) = state.rows.get(state.selected_row) {
        let help_text_color = color::simply_love_rgba(state.active_color_index);
        let wrap_width = help_box_w - 30.0; // padding
        let help_x = help_box_x + 12.0;
       
        // Calculate reveal fraction (0.0 to 1.0 over 0.5 seconds)
        const REVEAL_DURATION: f32 = 0.5;
        let num_help_lines = if row.help.len() > 1 { row.help.len() } else { 1 };
        let time_per_line = if num_help_lines > 0 { REVEAL_DURATION / num_help_lines as f32 } else { REVEAL_DURATION };
       
        // Handle multi-line help text (similar to multi-line row titles)
        if row.help.len() > 1 {
            // Multiple help lines - render them vertically stacked
            let line_spacing = 12.0; // Spacing between help lines
            let total_height = (row.help.len() as f32 - 1.0) * line_spacing;
            let start_y = help_box_bottom_y - (help_box_h / 2.0) - (total_height / 2.0);
           
            for (i, help_line) in row.help.iter().enumerate() {
                // Sequential letter-by-letter reveal per line
                let start_time = i as f32 * time_per_line;
                let end_time = start_time + time_per_line;
                let anim_time = state.help_anim_time;
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
               
                let line_y = start_y + (i as f32 * line_spacing);
                actors.push(act!(text:
                    font("miso"): settext(visible_text):
                    align(0.0, 0.5):
                    xy(help_x, line_y):
                    zoom(0.825):
                    diffuse(help_text_color[0], help_text_color[1], help_text_color[2], 1.0):
                    maxwidth(wrap_width): horizalign(left):
                    z(101)
                ));
            }
        } else {
            // Single help line (normal case)
            let help_text = row.help.join(" | ");
            // Letter-by-letter reveal
            let char_count = help_text.chars().count();
            let fraction = (state.help_anim_time / REVEAL_DURATION).clamp(0.0, 1.0);
            let visible_chars = ((char_count as f32 * fraction).round() as usize).min(char_count);
            let visible_text: String = help_text.chars().take(visible_chars).collect();
           
            actors.push(act!(text:
                font("miso"): settext(visible_text):
                align(0.0, 0.5):
                xy(help_x, help_box_bottom_y - (help_box_h / 2.0)):
                zoom(0.825):
                diffuse(help_text_color[0], help_text_color[1], help_text_color[2], 1.0):
                maxwidth(wrap_width): horizalign(left):
                z(101)
            ));
        }
    }
    actors
}
