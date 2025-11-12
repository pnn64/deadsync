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

// Keyboard input is handled centrally via the virtual dispatcher in app.rs
/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

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

pub struct Row {
    pub name: String,
    pub choices: Vec<String>,
    pub selected_choice_index: usize,
    pub help: Vec<String>,
    // Optional: map each choice to a FILE_DIFFICULTY_NAMES index (used for Stepchart)
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
    pub active_color_index: i32,
    pub speed_mod: SpeedMod,
    pub music_rate: f32,
    bg: heart_bg::State,
    pub nav_key_held_direction: Option<NavDirection>,
    pub nav_key_held_since: Option<Instant>,
    pub nav_key_last_scrolled_at: Option<Instant>,
    noteskin: Option<Noteskin>,
    preview_time: f32,
    preview_beat: f32,
    help_anim_time: f32,
}

fn build_rows(song: &SongData, speed_mod: &SpeedMod, selected_difficulty_index: usize, session_music_rate: f32) -> Vec<Row> {
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
            help: vec!["Change the way the arrows react to changing BPMs.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Speed Mod".to_string(),
            choices: vec![speed_mod_value_str], // Display only the current value
            selected_choice_index: 0,
            help: vec!["Adjust the speed at which arrows travel towards the targets.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Mini".to_string(),
            choices: vec![
                "0%".to_string(),
            ],
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
            choices: vec!["cel".to_string(), "metal".to_string(), "note".to_string()],
            selected_choice_index: 0,
            help: vec!["Change the appearance of the arrows.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Judgement Font".to_string(),
            choices: vec!["Love".to_string(), "Chromatic".to_string(), "ITG2".to_string()],
            selected_choice_index: 0,
            help: vec!["Pick your judgement font.".to_string()],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Combo Font".to_string(),
            choices: vec!["Wendy".to_string(), "Arial Rounded".to_string(), "Asap".to_string()],
            selected_choice_index: 0,
            help: vec![
                "Choose the font to count your combo. This font will also be used".to_string(),
                "for the Measure Counter if that is enabled.".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "Hold Judgement".to_string(),
            choices: vec!["Love".to_string(), "mute".to_string(), "ITG2".to_string()],
            selected_choice_index: 0,
            help: vec!["Change the judgement graphics displayed for hold notes.".to_string()],
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
            choices: vec!["0".to_string()],
            selected_choice_index: 0,
            help: vec![
                "Adjust the horizontal position of the notefield (relative to the".to_string(),
                "center).".to_string(),
            ],
            choice_difficulty_indices: None,
        },
        Row {
            name: "NoteField Offset Y".to_string(),
            choices: vec!["0".to_string()],
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
                // Calculate BPM with music rate applied
                let song_bpm = if (song.min_bpm - song.max_bpm).abs() < 1e-6 {
                    song.min_bpm
                } else {
                    song.min_bpm
                };
                let song_bpm = if song_bpm > 0.0 { song_bpm } else { 120.0 };
                let effective_bpm = song_bpm * session_music_rate as f64;
               
                // Format BPM: show one decimal only if it doesn't round to a whole number
                let bpm_str = if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
                    format!("{}", effective_bpm.round() as i32)
                } else {
                    format!("{:.1}", effective_bpm)
                };
               
                // Format: "Music Rate\nbpm: 120" (matches Simply Love's format from line 160)
                format!("Music Rate\nbpm: {}", bpm_str)
            },
            choices: vec![format!("{:.2}x", session_music_rate.clamp(0.5, 3.0))],
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
            choices: vec![
                "Gameplay".to_string(),
                "Choose a Different Song".to_string(),
                "Advanced Modifiers".to_string(),
                "Uncommon Modifiers".to_string(),
            ],
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

pub fn init(song: Arc<SongData>, chart_difficulty_index: usize, active_color_index: i32) -> State {
    let profile = crate::game::profile::get();
    let session_music_rate = crate::game::profile::get_session_music_rate();
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
    let mut rows = build_rows(&song, &speed_mod, chart_difficulty_index, session_music_rate);
    // Initialize Background Filter row from profile setting (Off, Dark, Darker, Darkest)
    if let Some(row) = rows.iter_mut().find(|r| r.name == "Background Filter") {
        row.selected_choice_index = match profile.background_filter {
            crate::game::profile::BackgroundFilter::Off => 0,
            crate::game::profile::BackgroundFilter::Dark => 1,
            crate::game::profile::BackgroundFilter::Darker => 2,
            crate::game::profile::BackgroundFilter::Darkest => 3,
        };
    }
    // Load noteskin for preview
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };
    let noteskin = noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style).ok();
    State {
        song,
        chart_difficulty_index,
        rows,
        selected_row: 0,
        prev_selected_row: 0,
        active_color_index,
        speed_mod,
        music_rate: session_music_rate,
        bg: heart_bg::State::new(),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        noteskin,
        preview_time: 0.0,
        preview_beat: 0.0,
        help_anim_time: 0.0,
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
        row.choices[0] = format!("{:.2}x", state.music_rate);
       
        // Update the row title to show the new BPM
        let song_bpm = if (state.song.min_bpm - state.song.max_bpm).abs() < 1e-6 {
            state.song.min_bpm
        } else {
            state.song.min_bpm
        };
        let song_bpm = if song_bpm > 0.0 { song_bpm } else { 120.0 };
        let effective_bpm = song_bpm * state.music_rate as f64;
       
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
            row.selected_choice_index =
                ((current_idx + delta + num_choices as isize) % num_choices as isize) as usize;
            // Changing the speed mod type should update the mod and the next row display
            if row.name == "Type of Speed Mod" {
                let new_type = match row.selected_choice_index {
                    0 => "X",
                    1 => "C",
                    2 => "M",
                    _ => "C",
                };
                state.speed_mod.mod_type = new_type.to_string();
                // Reset value to a default for the new type
                let new_value = match new_type {
                    "X" => 1.0,
                    "C" => 600.0,
                    "M" => 600.0,
                    _ => 600.0,
                };
                state.speed_mod.value = new_value;
                // Format the new value string
                let speed_mod_value_str = match new_type {
                    "X" => format!("{:.2}x", new_value),
                    "C" => format!("C{}", new_value as i32),
                    "M" => format!("M{}", new_value as i32),
                    _ => "".to_string(),
                };
                // Update the choices vec for the "Speed Mod" row.
                if let Some(speed_mod_row) = state.rows.get_mut(1) {
                    if speed_mod_row.name == "Speed Mod" {
                        speed_mod_row.choices[0] = speed_mod_value_str;
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
    if state.selected_row != state.prev_selected_row {
        // Direction-aware row change sounds
        match state.nav_key_held_direction {
            Some(NavDirection::Up) => audio::play_sfx("assets/sounds/prev_row.ogg"),
            Some(NavDirection::Down) => audio::play_sfx("assets/sounds/next_row.ogg"),
            _ => audio::play_sfx("assets/sounds/next_row.ogg"),
        }
        // Reset help reveal animation on row change
        state.help_anim_time = 0.0;
        state.prev_selected_row = state.selected_row;
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
            if num_rows > 0 && state.selected_row == num_rows - 1 {
                if let Some(what_comes_next_row) = state.rows.get(num_rows - 2) {
                    if what_comes_next_row.name == "What comes next?" {
                        match what_comes_next_row.selected_choice_index {
                            0 => return ScreenAction::Navigate(Screen::Gameplay),
                            1 => return ScreenAction::Navigate(Screen::SelectMusic),
                            _ => {}
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
    let speed_color = color::simply_love_rgba(state.active_color_index);
    
    // Calculate effective BPM based on speed mod type
    // IMPORTANT: Use the music rate to get the actual effective BPM
    let song_bpm = if (state.song.min_bpm - state.song.max_bpm).abs() < 1e-6 {
        state.song.min_bpm
    } else {
        state.song.min_bpm // Use min for variable BPM songs
    };
    let song_bpm = if song_bpm > 0.0 { song_bpm } else { 120.0 };
    let effective_song_bpm = song_bpm * state.music_rate as f64;
    
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
        align(0.0, 0.5): xy(speed_mod_x, speed_mod_y): zoom(0.5):
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
    const ANCHOR_ROW: usize = 4; // Keep selection on the 5th visible row
    const ROW_START_OFFSET: f32 = -164.0;
    const ROW_HEIGHT: f32 = 33.0;
    // Make the first column a bit wider to match SL
    const TITLE_BG_WIDTH: f32 = 140.0;
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
    let row_center_x = row_left + (row_width * 0.5);
    let title_bg_center_x = row_left + (TITLE_BG_WIDTH * 0.5);
    // Title text x: slightly less padding so text sits further left
    let title_x = row_left + widescale(8.0, 14.0);
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
            align(0.5, 0.5): xy(row_center_x, current_row_y):
            zoomto(row_width, frame_h):
            diffuse(bg_color[0], bg_color[1], bg_color[2], bg_color[3]):
            z(100)
        ));
        if !row.name.is_empty() {
            actors.push(act!(quad:
                align(0.5, 0.5): xy(title_bg_center_x, current_row_y):
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
                    align(0.0, 0.5): xy(title_x, current_row_y - 7.0): zoom(0.9):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(widescale(128.0, 120.0)):
                    z(101)
                ));
                // Second line (e.g., "bpm: 120") - smaller and slightly below
                actors.push(act!(text: font("miso"): settext(lines[1].to_string()):
                    align(0.0, 0.5): xy(title_x, current_row_y + 7.0): zoom(0.9):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(widescale(128.0, 120.0)):
                    z(101)
                ));
            } else {
                // Fallback for unexpected multi-line format
                actors.push(act!(text: font("miso"): settext(row.name.clone()):
                    align(0.0, 0.5): xy(title_x, current_row_y): zoom(0.9):
                    diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                    horizalign(left): maxwidth(widescale(128.0, 120.0)):
                    z(101)
                ));
            }
        } else {
            // Single-line title (normal case)
            actors.push(act!(text: font("miso"): settext(row.name.clone()):
                align(0.0, 0.5): xy(title_x, current_row_y): zoom(0.9):
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
            || row.name == "What comes next?";
        // Choice area: For single-choice rows (ShowOneInRow), use ItemsLongRowP1X positioning
        // For multi-choice rows (ShowAllInRow), use ItemsStartX positioning
        // ItemsLongRowP1X = WideScale(_screen.cx-100, _screen.cx-130) from Simply Love metrics
        // ItemsStartX = WideScale(146, 160) from Simply Love metrics
        let choice_inner_left = if show_all_choices_inline {
            row_left + TITLE_BG_WIDTH + widescale(24.0, 30.0) // Approximately matches ItemsStartX
        } else {
            screen_center_x() + widescale(-100.0, -130.0) // ItemsLongRowP1X for single-choice rows
        };
        if row.name.is_empty() {
            // Special case for the last "Exit" row
            let choice_text = &row.choices[row.selected_choice_index];
            let choice_color = if is_active { [1.0, 1.0, 1.0, 1.0] } else { sl_gray };
            actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                align(0.5, 0.5): xy(row_center_x, current_row_y): zoom(0.8):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                z(101)
            ));
            // Draw the selection cursor for the centered "Exit" text when active
            if is_active {
                let value_zoom = 0.8;
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
                        let pad_x = min_pad_x + (max_pad_x - min_pad_x) * t;
                        let border_w = widescale(2.0, 2.5);
                        let ring_w = draw_w + pad_x * 2.0;
                        let ring_h = draw_h + pad_y * 2.0;
                        let center_x = row_center_x; // Centered within the row
                        let left = center_x - ring_w * 0.5;
                        let right = center_x + ring_w * 0.5;
                        let top = current_row_y - ring_h * 0.5;
                        let bottom = current_row_y + ring_h * 0.5;
                        let mut ring_color = color::simply_love_rgba(state.active_color_index);
                        ring_color[3] = 1.0;
                        // Top, Bottom, Left, Right borders
                        actors.push(act!(quad: align(0.5, 0.5): xy(center_x, top + border_w * 0.5): zoomto(ring_w, border_w): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                        actors.push(act!(quad: align(0.5, 0.5): xy(center_x, bottom - border_w * 0.5): zoomto(ring_w, border_w): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                        actors.push(act!(quad: align(0.5, 0.5): xy(left + border_w * 0.5, current_row_y): zoomto(border_w, ring_h): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                        actors.push(act!(quad: align(0.5, 0.5): xy(right - border_w * 0.5, current_row_y): zoomto(border_w, ring_h): diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]): z(101)));
                    });
                });
            }
        } else if show_all_choices_inline {
            // Render every option horizontally; when active, all options should be white.
            // The selected option gets an underline (quad) drawn just below the text.
            let value_zoom = 0.8;
            let spacing = widescale(20.0, 24.0);
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
            // Draw underline under the selected option (always visible) — match text width exactly (no padding)
            if let Some(sel_x) = x_positions.get(row.selected_choice_index).copied() {
                let draw_w = widths.get(row.selected_choice_index).copied().unwrap_or(40.0);
                asset_manager.with_fonts(|_all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                        let border_w = widescale(2.0, 2.5); // thickness matches cursor bottom
                        let underline_w = draw_w; // exact text width
                        // Place just under the text baseline (slightly up from row bottom)
                        let offset = widescale(2.0, 3.0);
                        let underline_y = current_row_y + text_h * 0.5 + offset;
                        let mut line_color = color::simply_love_rgba(state.active_color_index);
                        line_color[3] = 1.0;
                        actors.push(act!(quad:
                            align(0.0, 0.5): // start at text's left edge
                            xy(sel_x, underline_y):
                            zoomto(underline_w, border_w):
                            diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                            z(101)
                        ));
                    });
                });
            }
            // Draw the 4-sided cursor ring around the selected option when this row is active
            if is_active {
                if let Some(sel_x) = x_positions.get(row.selected_choice_index).copied() {
                    let draw_w = widths.get(row.selected_choice_index).copied().unwrap_or(40.0);
                    asset_manager.with_fonts(|_all_fonts| {
                        asset_manager.with_font("miso", |metrics_font| {
                            let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                            let pad_y = widescale(6.0, 8.0);
                            let min_pad_x = widescale(2.0, 3.0);
                            let max_pad_x = widescale(22.0, 28.0);
                            let width_ref = widescale(180.0, 220.0);
                            let mut t = draw_w / width_ref;
                            if !t.is_finite() { t = 0.0; }
                            if t < 0.0 { t = 0.0; }
                            if t > 1.0 { t = 1.0; }
                            let pad_x = min_pad_x + (max_pad_x - min_pad_x) * t;
                            let border_w = widescale(2.0, 2.5);
                            let ring_w = draw_w + pad_x * 2.0;
                            let ring_h = text_h + pad_y * 2.0;
                            let left = sel_x - pad_x;
                            let right = left + ring_w;
                            let top = current_row_y - ring_h * 0.5;
                            let bottom = current_row_y + ring_h * 0.5;
                            let mut ring_color = color::simply_love_rgba(state.active_color_index);
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
            let choice_center_x = row_center_x - TITLE_BG_WIDTH / 2.0;
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
                    let value_zoom = 0.8;
                    let draw_w = text_w * value_zoom;
                    let draw_h = text_h * value_zoom;
                    actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                        align(0.5, 0.5): xy(choice_center_x, current_row_y): zoom(0.8):
                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                        z(101)
                    ));
                    // Encircling cursor around the active option value (programmatic border)
                    if is_active {
                        let pad_y = widescale(6.0, 8.0);
                        let min_pad_x = widescale(2.0, 3.0);
                        let max_pad_x = widescale(22.0, 28.0);
                        let width_ref = widescale(180.0, 220.0);
                        let t = (draw_w / width_ref).clamp(0.0, 1.0);
                        let pad_x = min_pad_x + (max_pad_x - min_pad_x) * t;
                        let border_w = widescale(2.0, 2.5);
                        let ring_w = draw_w + pad_x * 2.0;
                        let ring_h = draw_h + pad_y * 2.0;
                        let left = choice_center_x - draw_w / 2.0 - pad_x;
                        let right = choice_center_x + draw_w / 2.0 + pad_x;
                        let top = current_row_y - ring_h / 2.0;
                        let bottom = current_row_y + ring_h / 2.0;
                        let mut ring_color = color::simply_love_rgba(state.active_color_index);
                        ring_color[3] = 1.0;
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(choice_center_x, top + border_w * 0.5):
                            zoomto(ring_w, border_w):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(choice_center_x, bottom - border_w * 0.5):
                            zoomto(ring_w, border_w):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(left + border_w * 0.5, current_row_y):
                            zoomto(border_w, ring_h):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(right - border_w * 0.5, current_row_y):
                            zoomto(border_w, ring_h):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                    }
                    // Add previews (positioned to the right of the centered text)
                    let preview_offset = widescale(20.0, 25.0);
                    let preview_x = choice_center_x + draw_w / 2.0 + preview_offset;
                    // Add judgment preview for "Judgement Font" row showing Fantastic frame
                    if row.name == "Judgement Font" && choice_text == "Love" {
                        // Love judgment sprite is 2x7 (2 columns, 7 rows) at double resolution
                        // Fantastic is the first frame (top-left, column 0, row 0)
                        // Scale to 0.2x: Simply Love uses 0.4x, but our texture is doubleres, so 0.4 / 2 = 0.2
                        actors.push(act!(sprite("judgements/Love 2x7 (doubleres).png"):
                            align(0.0, 0.5):
                            xy(preview_x, current_row_y):
                            setstate(0):
                            zoom(0.2):
                            z(102)
                        ));
                    }
                    // Add hold judgment preview for "Hold Judgement" row showing both frames (Held and e.g. Let Go)
                    if row.name == "Hold Judgement" && choice_text == "Love" {
                        // Love hold judgment sprite is 1x2 (1 column, 2 rows) at double resolution
                        // Held is the first frame (top, row 0), second frame (bottom, row 1)
                        // Scale to 0.2x: Simply Love uses 0.4x, but our texture is doubleres, so 0.4 / 2 = 0.2
                        actors.push(act!(sprite("hold_judgements/Love 1x2 (doubleres).png"):
                            align(0.0, 0.5):
                            xy(preview_x, current_row_y):
                            setstate(0):
                            zoom(0.2):
                            z(102)
                        ));
                        let hold_spacing = 45.0; // Adjust this value as needed for spacing between the two sprites
                        let preview_x2 = preview_x + hold_spacing;
                        actors.push(act!(sprite("hold_judgements/Love 1x2 (doubleres).png"):
                            align(0.0, 0.5):
                            xy(preview_x2, current_row_y):
                            setstate(1):
                            zoom(0.2):
                            z(102)
                        ));
                    }
                    // Add noteskin preview for "NoteSkin" row showing animated 4th note
                    if row.name == "NoteSkin" && choice_text == "cel" {
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
                                const PREVIEW_SCALE: f32 = 0.4;
                                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                                
                                let scale = if height > 0.0 {
                                    target_height / height
                                } else {
                                    PREVIEW_SCALE
                                };
                                let final_width = width * scale;
                                let final_height = target_height;
                                
                                actors.push(act!(sprite(note_slot.texture_key().to_string()):
                                    align(0.0, 0.5):
                                    xy(preview_x, current_row_y):
                                    zoomto(final_width, final_height):
                                    rotationz(-note_slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
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
        diffuse(0.0, 0.0, 0.0, 0.8)
    ));
    if let Some(row) = state.rows.get(state.selected_row) {
        let help_text_color = color::simply_love_rgba(state.active_color_index);
        let wrap_width = help_box_w - 30.0; // padding
        let help_x = help_box_x + 15.0;
        
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
                    zoom(widescale(0.8, 0.85)):
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
                zoom(widescale(0.8, 0.85)):
                diffuse(help_text_color[0], help_text_color[1], help_text_color[2], 1.0):
                maxwidth(wrap_width): horizalign(left):
                z(101)
            ));
        }
    }
    actors
}
