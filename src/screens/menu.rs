use crate::act;
// Screen navigation is handled in app.rs
use crate::core::input::{VirtualAction, InputEvent};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::logo::{self, LogoParams};
use crate::ui::components::menu_list::{self};
use crate::ui::components::{heart_bg, screen_bar};
use crate::game::song::get_song_cache;
use crate::core::network::{self, ConnectionStatus};

use crate::core::space::*;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;

const NORMAL_COLOR_HEX: &str = "#888888";

pub const OPTION_COUNT: usize = 3;
const MENU_OPTIONS: [&str; OPTION_COUNT] = ["GAMEPLAY", "OPTIONS", "EXIT"];

// --- CONSTANTS UPDATED FOR NEW ANIMATION-DRIVEN LAYOUT ---
//const MENU_BELOW_LOGO: f32 = 25.0;
//const MENU_ROW_SPACING: f32 = 23.0;

const MENU_BELOW_LOGO: f32 = 29.0;
const MENU_ROW_SPACING: f32 = 28.0;

const INFO_PX: f32 = 15.0;
const INFO_GAP: f32 = 5.0;
const INFO_MARGIN_ABOVE: f32 = 20.0;

pub struct State {
    pub selected_index: usize,
    pub active_color_index: i32,
    pub rainbow_mode: bool,
    bg: heart_bg::State,
}

pub fn init() -> State {
    State {
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX, // was 0
        rainbow_mode: false,
        bg: heart_bg::State::new(),
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

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

// Signature changed to accept the alpha_multiplier
pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let lp = LogoParams::default();
    let mut actors: Vec<Actor> = Vec::with_capacity(96);

    // 1) background component (never fades)
    let backdrop = if state.rainbow_mode { [1.0, 1.0, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] };
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: backdrop,
        alpha_mul: 1.0,
    }));

    // If fully faded, don't create the other actors
    if alpha_multiplier <= 0.0 {
        return actors;
    }

    // --- The rest of the function is the same, but uses the passed-in alpha_multiplier ---

    // 2) logo + info
    let info2_y_tl = lp.top_margin - INFO_MARGIN_ABOVE - INFO_PX;
    let info1_y_tl = info2_y_tl - INFO_PX - INFO_GAP;

    let logo_actors = logo::build_logo_default();
    for mut actor in logo_actors {
        if let Actor::Sprite { tint, .. } = &mut actor {
            tint[3] *= alpha_multiplier;
        }
        actors.push(actor);
    }

    let mut info_color = [1.0, 1.0, 1.0, 1.0];
    info_color[3] *= alpha_multiplier;

    // --- DYNAMICALLY CALCULATE AND DISPLAY SONG/PACK COUNT ---
    let version = env!("CARGO_PKG_VERSION");
    let song_cache = get_song_cache();
    let num_packs = song_cache.len();
    let num_songs: usize = song_cache.iter().map(|pack| pack.songs.len()).sum();
    let song_info_text = format!("{} songs in {} groups, X courses", num_songs, num_packs);


    // --- Create a single multi-line string and pass it to one text actor ---
    let combined_text = format!("DeadSync {}\n{}", version, song_info_text);

    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), info1_y_tl): zoom(0.8):
        font("miso"): settext(combined_text): horizalign(center):
        diffuse(info_color[0], info_color[1], info_color[2], info_color[3])
    ));

    // 3) menu list
    let base_y = lp.top_margin + lp.target_h + MENU_BELOW_LOGO;
    let mut selected = color::menu_selected_rgba(state.active_color_index);
    let mut normal   = color::rgba_hex(NORMAL_COLOR_HEX);
    selected[3] *= alpha_multiplier;
    normal[3] *= alpha_multiplier;

    // --- UPDATED PARAMS FOR THE NEW MENU LIST BUILDER ---
    let params = menu_list::MenuParams {
        options: &MENU_OPTIONS,
        selected_index: state.selected_index,
        start_center_y: base_y,
        row_spacing: MENU_ROW_SPACING,
        selected_color: selected,
        normal_color: normal,
        font: "wendy",
    };
    actors.extend(menu_list::build_vertical_menu(params));

    // --- footer bar ---
    let mut footer_fg = [1.0, 1.0, 1.0, 1.0];
    footer_fg[3] *= alpha_multiplier;

    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "EVENT MODE",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some("PRESS START"),
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: None,
        fg_color: footer_fg,
    }));

    // --- GrooveStats Info Pane (top-left) ---
    let mut groovestats_actors = Vec::new();
    let status = network::get_status();
    
    // Mimic the ActorFrame's zoom(0.8) which affects both size and position offsets.
    let frame_zoom = 0.8;
    let base_x = 10.0;
    let base_y = 15.0;

    let (main_text, services_to_list) = match status {
        ConnectionStatus::Pending => ("     GrooveStats".to_string(), Vec::new()),
        ConnectionStatus::Error(msg) => {
            let simplified_msg = match msg.as_str() {
                "Machine Offline" => "Machine Offline".to_string(),
                "Cannot Connect" => "Cannot Connect".to_string(),
                "Timed Out" => "Timed Out".to_string(),
                _ => "Failed to Load 😞".to_string(),
            };
            // When there is a connection error, SL shows the error message in Service1 and "❌ GrooveStats" as main text.
            ("GrooveStats not connected".to_string(), vec![simplified_msg])
        },
        ConnectionStatus::Connected(services) => {
            let mut disabled_services = Vec::new();
            if !services.get_scores {
                disabled_services.push("❌ Get Scores".to_string());
            }
            if !services.leaderboard {
                disabled_services.push("❌ Leaderboard".to_string());
            }
            if !services.auto_submit {
                disabled_services.push("❌ Auto-Submit".to_string());
            }
            
            let text = if disabled_services.is_empty() {
                "✔ GrooveStats".to_string()
            } else if disabled_services.len() == 3 {
                "❌ GrooveStats".to_string()
            } else {
                "⚠ GrooveStats".to_string()
            };

            let services_to_show = if disabled_services.len() == 3 { Vec::new() } else { disabled_services };

            (text, services_to_show)
        }
    };
    
    // Main status text
    groovestats_actors.push(act!(text: font("miso"): settext(main_text): align(0.0, 0.0): xy(base_x, base_y): zoom(frame_zoom): horizalign(left): z(200) ));

    // List of disabled/error services
    let line_height_offset = 18.0;
    for (i, service_text) in services_to_list.iter().enumerate() {
        groovestats_actors.push(act!(text: font("miso"): settext(service_text.clone()): align(0.0, 0.0): xy(base_x, base_y + (line_height_offset * (i as f32 + 1.0) * frame_zoom)): zoom(frame_zoom): horizalign(left): z(200)));
    }
    
    for actor in &mut groovestats_actors {
        if let Actor::Text { color, .. } = actor { color[3] *= alpha_multiplier; }
    }
    actors.extend(groovestats_actors);

    actors
}

// Event-driven virtual input handler
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed { return ScreenAction::None; }
    match ev.action {
        VirtualAction::p1_start => {
            crate::core::audio::play_sfx("assets/sounds/start.ogg");
            match state.selected_index {
                0 => ScreenAction::Navigate(Screen::SelectColor),
                1 => ScreenAction::Navigate(Screen::Options),
                2 => ScreenAction::Exit,
                _ => ScreenAction::None,
            }
        }
        VirtualAction::p1_back => ScreenAction::Exit,
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            let n = OPTION_COUNT as isize;
            let cur = state.selected_index as isize;
            state.selected_index = ((cur - 1 + n) % n) as usize;
            crate::core::audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            let n = OPTION_COUNT as isize;
            let cur = state.selected_index as isize;
            state.selected_index = ((cur + 1 + n) % n) as usize;
            crate::core::audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}
