use crate::act;
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::profile;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use std::sync::mpsc;

// Simply Love: BGAnimations/ScreenProfileLoad overlay.lua
const TWEENTIME: f32 = 0.325;
const SWOOSH_H: f32 = 50.0;
const SWOOSH_W_PAD: f32 = 100.0;
const CONTINUE_DELAY: f32 = 0.1;
const MIN_SHOW_SECS: f32 = TWEENTIME * 3.0 + CONTINUE_DELAY;

pub struct State {
    pub active_color_index: i32,
    elapsed: f32,
    rx: Option<mpsc::Receiver<PreparedState>>,
    prepared_select_music: Option<crate::screens::select_music::State>,
    prepared_select_course: Option<crate::screens::select_course::State>,
    next_screen: Screen,
}

enum PreparedState {
    Music(crate::screens::select_music::State),
    Course(crate::screens::select_course::State),
}

pub fn init() -> State {
    State {
        active_color_index: crate::ui::color::DEFAULT_COLOR_INDEX,
        elapsed: 0.0,
        rx: None,
        prepared_select_music: None,
        prepared_select_course: None,
        next_screen: Screen::SelectMusic,
    }
}

pub fn on_enter(state: &mut State) {
    state.elapsed = 0.0;
    state.prepared_select_music = None;
    state.prepared_select_course = None;
    state.rx = None;
    state.next_screen = match profile::get_session_play_mode() {
        profile::PlayMode::Marathon => Screen::SelectCourse,
        profile::PlayMode::Regular => Screen::SelectMusic,
    };

    let (tx, rx) = mpsc::channel();
    let play_mode = profile::get_session_play_mode();
    std::thread::spawn(move || {
        let prepared = match play_mode {
            profile::PlayMode::Marathon => {
                PreparedState::Course(crate::screens::select_course::init())
            }
            profile::PlayMode::Regular => {
                PreparedState::Music(crate::screens::select_music::init())
            }
        };
        let _ = tx.send(prepared);
    });
    state.rx = Some(rx);
}

pub fn take_prepared_select_music(
    state: &mut State,
) -> Option<crate::screens::select_music::State> {
    state.prepared_select_music.take()
}

pub fn take_prepared_select_course(
    state: &mut State,
) -> Option<crate::screens::select_course::State> {
    state.prepared_select_course.take()
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    state.elapsed += dt.max(0.0);

    if state.prepared_select_music.is_none()
        && state.prepared_select_course.is_none()
        && let Some(rx) = &state.rx
    {
        match rx.try_recv() {
            Ok(PreparedState::Music(sm)) => {
                state.prepared_select_music = Some(sm);
                state.next_screen = Screen::SelectMusic;
                state.rx = None;
            }
            Ok(PreparedState::Course(sc)) => {
                state.prepared_select_course = Some(sc);
                state.next_screen = Screen::SelectCourse;
                state.rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                // Defensive fallback: avoid hanging on a failed loader thread.
                match state.next_screen {
                    Screen::SelectCourse => {
                        state.prepared_select_course = Some(crate::screens::select_course::init());
                    }
                    _ => {
                        state.prepared_select_music = Some(crate::screens::select_music::init());
                        state.next_screen = Screen::SelectMusic;
                    }
                }
                state.rx = None;
            }
        }
    }

    if state.elapsed >= MIN_SHOW_SECS
        && (state.prepared_select_music.is_some() || state.prepared_select_course.is_some())
    {
        return Some(ScreenAction::Navigate(state.next_screen));
    }
    None
}

pub fn handle_input(_: &mut State, _: &crate::core::input::InputEvent) -> ScreenAction {
    ScreenAction::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    (vec![], 0.0)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    (vec![], 0.0)
}

pub fn get_actors(_: &State) -> Vec<Actor> {
    let w = screen_width();
    let h = screen_height();
    let cx = screen_center_x();
    let cy = screen_center_y();

    vec![
        // Backdrop (ScreenWithMenuElements background is effectively black here).
        act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(0.0)
        ),
        // FadeToBlack
        act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.0):
            z(100.0):
            sleep(TWEENTIME):
            linear(TWEENTIME): alpha(1.0)
        ),
        // HorizontalWhiteSwoosh
        act!(quad:
            align(0.5, 0.5): xy(cx, cy):
            diffuse(1.0, 1.0, 1.0, 1.0):
            zoomto(w + SWOOSH_W_PAD, SWOOSH_H):
            fadeleft(0.1): faderight(0.1):
            cropright(1.0):
            z(101.0):
            linear(TWEENTIME): cropright(0.0):
            sleep(TWEENTIME):
            linear(TWEENTIME): cropleft(1.0)
        ),
        // "Common Bold" (Simply Love) -> Wendy small.
        act!(text:
            font("wendy"): settext("Loading"):
            align(0.5, 0.5): xy(cx, cy):
            zoom(0.6):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(102.0)
        ),
    ]
}
