use crate::act;
use crate::core::audio;
use crate::core::space::*;
use crate::game::song::SongData;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::assets::AssetManager;
use crate::ui::color;
use crate::ui::components::heart_bg;
use crate::ui::components::screen_bar::{
    self, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavDirection {
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
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
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
            help: vec!["Choose the font to count your combo. Also used for the measure counter if enabled.".to_string()],
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
            name: "Music Rate".to_string(),
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
            help: vec!["Go back and choose a different song or change additional modifiers.".to_string()],
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
    } else if row.name == "Music Rate" {
        let increment = 0.01f32;
        let min_rate = 0.05f32;
        let max_rate = 3.00f32;
        state.music_rate += delta as f32 * increment;
        state.music_rate = (state.music_rate / increment).round() * increment;
        state.music_rate = state.music_rate.clamp(min_rate, max_rate);
        row.choices[0] = format!("{:.2}x", state.music_rate);
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

pub fn handle_key_press(state: &mut State, e: &KeyEvent) -> ScreenAction {
    let num_rows = state.rows.len();
    let key_code = if let PhysicalKey::Code(code) = e.physical_key {
        code
    } else {
        return ScreenAction::None;
    };

    if e.state == ElementState::Pressed {
        if e.repeat {
            return ScreenAction::None;
        }

        match key_code {
            KeyCode::Escape => return ScreenAction::Navigate(Screen::SelectMusic),
            KeyCode::ArrowUp | KeyCode::KeyW => {
                if num_rows > 0 {
                    state.selected_row = (state.selected_row + num_rows - 1) % num_rows;
                }
                state.nav_key_held_direction = Some(NavDirection::Up);
                state.nav_key_held_since = Some(Instant::now());
                state.nav_key_last_scrolled_at = Some(Instant::now());
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                if num_rows > 0 {
                    state.selected_row = (state.selected_row + 1) % num_rows;
                }
                state.nav_key_held_direction = Some(NavDirection::Down);
                state.nav_key_held_since = Some(Instant::now());
                state.nav_key_last_scrolled_at = Some(Instant::now());
            }
            KeyCode::ArrowLeft | KeyCode::KeyA => {
                change_choice(state, -1);
                state.nav_key_held_direction = Some(NavDirection::Left);
                state.nav_key_held_since = Some(Instant::now());
                state.nav_key_last_scrolled_at = Some(Instant::now());
            }
            KeyCode::ArrowRight | KeyCode::KeyD => {
                change_choice(state, 1);
                state.nav_key_held_direction = Some(NavDirection::Right);
                state.nav_key_held_since = Some(Instant::now());
                state.nav_key_last_scrolled_at = Some(Instant::now());
            }
            KeyCode::Enter => {
                if num_rows > 0 && state.selected_row == num_rows - 1 {
                    // Last row is selected
                    if let Some(what_comes_next_row) = state.rows.get(num_rows - 2) {
                        if what_comes_next_row.name == "What comes next?" {
                            match what_comes_next_row.selected_choice_index {
                                0 => return ScreenAction::Navigate(Screen::Gameplay),
                                1 => return ScreenAction::Navigate(Screen::SelectMusic),
                                _ => {} // No-op
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    } else if e.state == ElementState::Released {
        let direction_to_clear = match key_code {
            KeyCode::ArrowUp | KeyCode::KeyW => Some(NavDirection::Up),
            KeyCode::ArrowDown | KeyCode::KeyS => Some(NavDirection::Down),
            KeyCode::ArrowLeft | KeyCode::KeyA => Some(NavDirection::Left),
            KeyCode::ArrowRight | KeyCode::KeyD => Some(NavDirection::Right),
            _ => None,
        };
        if state.nav_key_held_direction == direction_to_clear {
            state.nav_key_held_direction = None;
            state.nav_key_held_since = None;
            state.nav_key_last_scrolled_at = None;
        }
    }
    ScreenAction::None
}

pub fn update(state: &mut State, _dt: f32) {
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

    if state.selected_row != state.prev_selected_row {
        // Direction-aware row change sounds
        match state.nav_key_held_direction {
            Some(NavDirection::Up) => audio::play_sfx("assets/sounds/prev_row.ogg"),
            Some(NavDirection::Down) => audio::play_sfx("assets/sounds/next_row.ogg"),
            _ => audio::play_sfx("assets/sounds/next_row.ogg"),
        }
        state.prev_selected_row = state.selected_row;
    }
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
    let speed_mod_y = 48.0;
    let speed_mod_x = screen_center_x() + widescale(-77.0, -100.0);
    let speed_color = color::simply_love_rgba(state.active_color_index);
    let speed_text = if state.speed_mod.mod_type == "X" {
        format!("{:.2}x", state.speed_mod.value)
    } else {
        format!("{}{}", state.speed_mod.mod_type, state.speed_mod.value as i32)
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

        actors.push(act!(text: font("miso"): settext(row.name.clone()):
            align(0.0, 0.5): xy(title_x, current_row_y): zoom(0.9):
            diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
            horizalign(left): maxwidth(widescale(128.0, 120.0)):
            z(101)
        ));

        // Choice area: align more to the right, relative to the widened title column
        let choice_inner_left = row_left + TITLE_BG_WIDTH + widescale(24.0, 30.0);
        // Inactive option text color should be #808080 (alpha 1.0)
        let sl_gray = color::rgba_hex("#808080");

        // Some rows should display all choices inline
        let show_all_choices_inline = row.name == "Perspective"
            || row.name == "Background Filter"
            || row.name == "Stepchart"
            || row.name == "What comes next?";

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
            let choice_text = &row.choices[row.selected_choice_index];
            let choice_color = if is_active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                sl_gray
            };

            // Encircling cursor around the active option value (programmatic border)
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
                        let mut t = draw_w / width_ref;
                        if !t.is_finite() { t = 0.0; }
                        if t < 0.0 { t = 0.0; }
                        if t > 1.0 { t = 1.0; }
                        let pad_x = min_pad_x + (max_pad_x - min_pad_x) * t;
                        let border_w = widescale(2.0, 2.5);
                        let ring_w = draw_w + pad_x * 2.0;
                        let ring_h = draw_h + pad_y * 2.0;

                        let left = choice_inner_left - pad_x;
                        let right = left + ring_w;
                        let top = current_row_y - ring_h * 0.5;
                        let bottom = current_row_y + ring_h * 0.5;
                        let mut ring_color = color::simply_love_rgba(state.active_color_index);
                        ring_color[3] = 1.0;

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
                    });
                });
            }

            actors.push(act!(text: font("miso"): settext(choice_text.clone()):
                align(0.0, 0.5): xy(choice_inner_left, current_row_y): zoom(0.8):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                z(101)
            ));
        }
    }

    // ------------------- Description content (selected) -------------------
    actors.push(act!(quad:
        align(0.0, 1.0): xy(help_box_x, help_box_bottom_y):
        zoomto(help_box_w, help_box_h):
        diffuse(0.0, 0.0, 0.0, 0.8)
    ));

    if let Some(row) = state.rows.get(state.selected_row) {
        let help_text = row.help.join(" | ");
        let help_text_color = color::simply_love_rgba(state.active_color_index);
        let wrap_width = help_box_w - 30.0; // padding

        actors.push(act!(text:
            font("miso"): settext(help_text):
            align(0.0, 0.5):
            xy(help_box_x + 15.0, help_box_bottom_y - (help_box_h / 2.0)):
            // Slightly larger help text for readability
            zoom(widescale(0.8, 0.85)):
            diffuse(help_text_color[0], help_text_color[1], help_text_color[2], 1.0):
            maxwidth(wrap_width): horizalign(left)
        ));
    }

    actors
}
