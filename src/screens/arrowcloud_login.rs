//! Dedicated screen for the ArrowCloud QR device-login step.
//!
//! Mirrors Simply Love's `ScreenGrooveStatsLogin`
//! (`BGAnimations/ScreenGrooveStatsLogin underlay/default.lua`), which
//! sits between `ScreenSelectProfile` and `ScreenSelectColor` in the
//! boot-flow branch chain (`Scripts/SL-Branches.lua:78-80`).  Gated by
//! the `ArrowCloudQrLoginWhen` pref (Always/Sometimes/Disabled).
//!
//! The visual state machine + per-side worker is shared with future
//! entry points; both render the same overlay actors via
//! [`build_arrowcloud_login_overlay_actors`].

use std::sync::atomic::Ordering;

use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::options::arrowcloud_login::{
    ArrowCloudLoginUiState, build_arrowcloud_login_overlay_actors, create_arrowcloud_login_ui,
    poll_arrowcloud_login_ui,
};
use crate::screens::{Screen, ScreenAction};

const TRANSITION_IN_DURATION: f32 = 0.3;
const TRANSITION_OUT_DURATION: f32 = 0.3;

pub struct State {
    pub active_color_index: i32,
    pub(crate) ui: Option<ArrowCloudLoginUiState>,
    /// Animated heart/style background, matching SelectProfile /
    /// SelectColor.  The overlay panel is rendered on top of this with
    /// the standard 0.65-alpha black dimmer.
    bg: visual_style_bg::State,
    /// Tracks the L+R chord used as the Cancel input on three-key
    /// cabinets (which have no dedicated Back button).
    menu_lr_chord: screen_input::MenuLrChordTracker,
}

pub fn init() -> State {
    State {
        active_color_index: crate::config::get().simply_love_color,
        ui: None,
        bg: visual_style_bg::State::new(),
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    }
}

/// Called every time the app enters this screen.  Spawns a fresh
/// multi-side login UI (one worker per joined Local side) and discards
/// any previous instance.
pub fn on_enter(state: &mut State) {
    state.ui = Some(create_arrowcloud_login_ui());
    state.menu_lr_chord = screen_input::MenuLrChordTracker::default();
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

pub fn update(state: &mut State, _dt: f32) {
    if let Some(ui) = state.ui.as_mut() {
        poll_arrowcloud_login_ui(ui);
    }
}

/// Input mirrors `ScreenGrooveStatsLogin/default.lua:60-65`, with
/// deadsync-specific Back-to-title behavior:
///   * Start (or SELECT) → cancel workers, navigate to `SelectColor`
///                          (advance, even if no QR was scanned).
///   * Back  (or L+R chord on three-key cabinets) → cancel workers and
///                          return all the way to the title menu, same
///                          as Back on `SelectProfile`.
///
/// Any other input is consumed.
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    let three_key = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);
    let is_three_key_confirm = matches!(
        three_key,
        Some((_, screen_input::ThreeKeyMenuAction::Confirm))
    );
    let is_three_key_cancel = matches!(
        three_key,
        Some((_, screen_input::ThreeKeyMenuAction::Cancel))
    );
    let is_start = is_three_key_confirm
        || (ev.pressed
            && matches!(
                ev.action,
                VirtualAction::p1_start
                    | VirtualAction::p2_start
                    | VirtualAction::p1_select
                    | VirtualAction::p2_select
            ));
    let is_back = is_three_key_cancel
        || (ev.pressed
            && matches!(
                ev.action,
                VirtualAction::p1_back | VirtualAction::p2_back
            ));
    if is_start {
        if let Some(ui) = state.ui.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        state.ui = None;
        crate::engine::audio::play_sfx("assets/sounds/start.ogg");
        return ScreenAction::Navigate(Screen::SelectColor);
    }
    if is_back {
        if let Some(ui) = state.ui.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        state.ui = None;
        crate::engine::audio::play_sfx("assets/sounds/change.ogg");
        log::info!("ArrowCloud QR login cancelled — returning to title menu.");
        return ScreenAction::Navigate(Screen::Menu);
    }
    ScreenAction::None
}

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(32);

    // Animated heart background — matches SelectProfile / SelectColor.
    actors.extend(state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    if let Some(ui) = state.ui.as_ref() {
        let mut ui_actors = build_arrowcloud_login_overlay_actors(ui, state.active_color_index);
        for actor in &mut ui_actors {
            actor.mul_alpha(alpha_multiplier);
        }
        actors.extend(ui_actors);
    }
    actors
}
