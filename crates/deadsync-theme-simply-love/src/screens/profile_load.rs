use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, machine_font_key};
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_profile as profile_data;

// Simply Love: BGAnimations/ScreenProfileLoad overlay.lua
const TWEENTIME: f32 = 0.325;
const SWOOSH_H: f32 = 50.0;
const SWOOSH_W_PAD: f32 = 100.0;
const CONTINUE_DELAY: f32 = 0.1;
const MIN_SHOW_SECS: f32 = TWEENTIME * 3.0 + CONTINUE_DELAY;

pub struct State {
    pub active_color_index: i32,
    elapsed: f32,
    ready: bool,
    next_screen: Screen,
}

pub fn init() -> State {
    State {
        active_color_index: deadlib_present::color::DEFAULT_COLOR_INDEX,
        elapsed: 0.0,
        ready: false,
        next_screen: Screen::SelectMusic,
    }
}

pub fn on_enter(state: &mut State, play_mode: profile_data::PlayMode) {
    state.elapsed = 0.0;
    state.ready = false;
    state.next_screen = match play_mode {
        profile_data::PlayMode::Marathon => Screen::SelectCourse,
        profile_data::PlayMode::Regular => Screen::SelectMusic,
    };
}

#[inline(always)]
pub fn sync_ready(state: &mut State, ready: bool) {
    state.ready = ready;
}

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    state.elapsed += dt.max(0.0);
    if state.elapsed >= MIN_SHOW_SECS && state.ready {
        return Some(ThemeEffect::Navigate(state.next_screen));
    }
    None
}

pub fn handle_input(_: &mut State, _: &deadsync_input::InputEvent) -> ThemeEffect {
    ThemeEffect::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    (vec![], 0.0)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    (vec![], 0.0)
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    _: &State,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(4);
    let w = screen_width();
    let h = screen_height();
    let cx = screen_center_x();
    let cy = screen_center_y();

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(0.0)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(100.0):
        sleep(TWEENTIME):
        linear(TWEENTIME): alpha(1.0)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        diffuse(1.0, 1.0, 1.0, 1.0):
        zoomto(w + SWOOSH_W_PAD, SWOOSH_H):
        fadeleft(0.1): faderight(0.1):
        cropright(1.0):
        z(101.0):
        linear(TWEENTIME): cropright(0.0):
        sleep(TWEENTIME):
        linear(TWEENTIME): cropleft(1.0)
    ));
    actors.push(act!(text:
        font(machine_font_key(visual_policy.machine_font, FontRole::Header)): settext(tr("Common", "Loading")):
        align(0.5, 0.5): xy(cx, cy):
        zoom(0.6):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(102.0)
    ));
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(4);
    push_actors(&mut actors, state, Default::default());
    actors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_gates_the_theme_owned_redirect() {
        let mut state = init();
        on_enter(&mut state, profile_data::PlayMode::Regular);
        assert!(update(&mut state, MIN_SHOW_SECS).is_none());

        sync_ready(&mut state, true);
        assert!(matches!(
            update(&mut state, 0.0),
            Some(ThemeEffect::Navigate(Screen::SelectMusic))
        ));
    }

    #[test]
    fn marathon_mode_redirects_to_select_course() {
        let mut state = init();
        on_enter(&mut state, profile_data::PlayMode::Marathon);
        sync_ready(&mut state, true);
        assert!(matches!(
            update(&mut state, MIN_SHOW_SECS),
            Some(ThemeEffect::Navigate(Screen::SelectCourse))
        ));
    }
}
