use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::downloads;
use crate::screens::components::select_music::screen_bars;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;

const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;
const VIEW_ROWS: usize = 6;
const LIST_LEFT_X: f32 = -240.0;
const LIST_TOP_Y: f32 = -240.0;
const ROW_STEP_Y: f32 = 55.0;
const BAR_WIDTH: f32 = 350.0;
const BAR_HEIGHT: f32 = 20.0;
const ENDPOINT_X: f32 = BAR_WIDTH;
const PERCENT_X: f32 = BAR_WIDTH + 50.0;
const AMOUNT_X: f32 = BAR_WIDTH + 60.0;
const SEPARATOR_WIDTH: f32 = 480.0;
const HINT_TEXT: &str = "Press Start, Back, or Select to dismiss.";
const EMPTY_TEXT: &str = "No Downloads to view";

#[derive(Debug, Clone)]
pub struct State {
    pub active_color_index: i32,
    pub scroll_index: usize,
}

pub fn init() -> State {
    State {
        active_color_index: 0,
        scroll_index: 0,
    }
}

pub fn update(state: &mut State, _delta_time: f32) -> Option<ScreenAction> {
    let len = downloads::snapshots().len();
    state.scroll_index = state.scroll_index.min(scroll_limit(len));
    None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::Navigate(Screen::SelectMusic)
        }
        VirtualAction::p1_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_menu_left => {
            move_scroll(state, -1);
            ScreenAction::None
        }
        VirtualAction::p1_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_menu_right => {
            move_scroll(state, 1);
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

fn move_scroll(state: &mut State, delta: isize) {
    let len = downloads::snapshots().len();
    let limit = scroll_limit(len);
    let next = (state.scroll_index as isize + delta).clamp(0, limit as isize) as usize;
    if next != state.scroll_index {
        state.scroll_index = next;
        audio::play_sfx("assets/sounds/change.ogg");
    }
}

fn scroll_limit(total: usize) -> usize {
    total.saturating_sub(VIEW_ROWS)
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let snapshots = downloads::snapshots();
    let (finished, total) = downloads::completion_counts();
    let mut actors = vec![act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0)
    )];
    actors.extend(screen_bars::build("DOWNLOADS"));
    actors.push(act!(text:
        font("wendy"): settext(HINT_TEXT):
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y() + 170.0):
        zoom(0.8): maxwidth(640.0): z(50)
    ));
    actors.push(act!(text:
        font("wendy"): settext(format!("{finished}/{total}")):
        align(1.0, 0.5): xy(screen_center_x() + 220.0, screen_center_y() + 170.0):
        zoom(0.8): z(50)
    ));

    if snapshots.is_empty() {
        actors.push(act!(text:
            font("wendy"): settext(EMPTY_TEXT):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoom(2.0): z(50)
        ));
        return actors;
    }

    let start = state.scroll_index.min(scroll_limit(snapshots.len()));
    for (slot, snapshot) in snapshots.iter().skip(start).take(VIEW_ROWS).enumerate() {
        let row_y = screen_center_y() + LIST_TOP_Y + ROW_STEP_Y * slot as f32;
        actors.extend(row_actors(slot + start, row_y, snapshot));
    }

    actors
}

fn row_actors(index: usize, row_y: f32, snapshot: &downloads::DownloadSnapshot) -> Vec<Actor> {
    let x = screen_center_x() + LIST_LEFT_X;
    let percent = percent(snapshot.current_bytes, snapshot.total_bytes);
    let fill_width = BAR_WIDTH * percent as f32 / 100.0;
    let amount_text = byte_amount_text(snapshot.current_bytes, snapshot.total_bytes);
    let mut actors = vec![
        act!(text:
            font("wendy"): settext(format!("{}. {}", index + 1, snapshot.name)):
            align(0.0, 0.5): xy(x, row_y):
            zoom(0.8): maxwidth(480.0): z(50)
        ),
        act!(quad:
            align(0.0, 0.5): xy(x, row_y + 25.0):
            zoomto(fill_width, BAR_HEIGHT):
            diffuse(1.0, 1.0, 1.0, if snapshot.complete { 1.0 } else { 0.8 }): z(45)
        ),
        act!(quad:
            align(0.5, 0.5): xy(x + ENDPOINT_X, row_y + 25.0):
            zoomto(3.0, BAR_HEIGHT):
            diffuse(1.0, 0.0, 0.0, 1.0): z(46)
        ),
        act!(text:
            font("wendy"): settext(format!("{percent}%")):
            align(1.0, 0.5): xy(x + PERCENT_X, row_y + 25.0):
            zoom(0.8): z(50)
        ),
        act!(text:
            font("wendy"): settext(amount_text):
            align(0.0, 0.5): xy(x + AMOUNT_X, row_y + 25.0):
            zoom(0.8): z(50)
        ),
        act!(quad:
            align(0.0, 0.5): xy(x, row_y + 40.0):
            zoomto(SEPARATOR_WIDTH, 1.0):
            diffuse(1.0, 1.0, 1.0, 0.7): z(44)
        ),
    ];
    if !snapshot.complete && fill_width > 0.0 {
        actors.push(act!(sprite("swoosh.png"):
            align(0.0, 0.5): xy(x, row_y + 25.0):
            zoomto(fill_width, BAR_HEIGHT):
            diffuse(1.0, 1.0, 1.0, 1.0):
            texcoordvelocity(-1.0, 0.0): z(47)
        ));
    }
    if snapshot.complete {
        let (text, color) = match snapshot.error_message.as_deref() {
            Some(message) => (format!("Error: {message}"), [1.0, 0.0, 0.0, 1.0]),
            None => ("Done!".to_string(), [0.0, 1.0, 0.0, 1.0]),
        };
        actors.push(act!(text:
            font("wendy"): settext(text):
            align(0.0, 0.5): xy(x, row_y + 25.0):
            zoom(0.8): maxwidth(BAR_WIDTH): z(48):
            diffuse(color[0], color[1], color[2], color[3])
        ));
    }
    actors
}

fn percent(current_bytes: u64, total_bytes: u64) -> u32 {
    if total_bytes == 0 {
        return 0;
    }
    (((current_bytes.min(total_bytes)) * 100) / total_bytes) as u32
}

fn byte_amount_text(current_bytes: u64, total_bytes: u64) -> String {
    let (suffix, divisor) = bytes_to_size(total_bytes);
    format!(
        "{}/{} {}",
        current_bytes / divisor,
        total_bytes / divisor,
        suffix
    )
}

fn bytes_to_size(bytes: u64) -> (&'static str, u64) {
    if bytes >= 1024 * 1024 {
        ("MiB", 1024 * 1024)
    } else if bytes >= 1024 {
        ("KiB", 1024)
    } else {
        ("bytes", 1)
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    (
        vec![act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0): z(1100):
            linear(TRANSITION_IN_DURATION): alpha(0.0):
            linear(0.0): visible(false)
        )],
        TRANSITION_IN_DURATION,
    )
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    (
        vec![act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.0): z(1200):
            linear(TRANSITION_OUT_DURATION): alpha(1.0)
        )],
        TRANSITION_OUT_DURATION,
    )
}
