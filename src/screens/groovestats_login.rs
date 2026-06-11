//! Dedicated screen for the GrooveStats QR device-login step.
//!
//! Mirrors Simply Love's `ScreenGrooveStatsLogin`
//! (`BGAnimations/ScreenGrooveStatsLogin underlay/default.lua`), which
//! sits between `ScreenSelectProfile` and `ScreenSelectColor` in the
//! boot-flow branch chain (`Scripts/SL-Branches.lua:78-80`).  Gated by
//! the `GrooveStatsQrLoginWhen` pref (Always/Sometimes/Disabled).
//!
//! When both this and the ArrowCloud variant are configured to auto-show,
//! GrooveStats runs first (`SelectProfile → GrooveStatsLogin →
//! ArrowCloudLogin → SelectColor`), matching SL's GrooveStats-first
//! Branch.AfterSelectProfile ordering.
//!
//! The visual state machine + per-side worker is shared with the
//! ArrowCloud variant; both render the same overlay actors via
//! [`build_qr_login_overlay_actors`].

use std::sync::atomic::Ordering;

use crate::config;
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::options::qr_login::{
    self, QrLoginUiState, build_qr_login_overlay_actors, create_groovestats_login_ui,
    create_groovestats_login_ui_for_profile, poll_qr_login_ui,
};
use crate::screens::{Screen, ScreenAction};
use deadsync_audio_stream as audio;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_present::actors::Actor;

const TRANSITION_IN_DURATION: f32 = 0.3;
const TRANSITION_OUT_DURATION: f32 = 0.3;

/// Optional per-profile scoping carried into the screen from Manage
/// Local Profiles' "Link GrooveStats" entry.
#[derive(Clone, Debug)]
pub struct ProfileTarget {
    pub id: String,
    pub display_name: String,
}

pub struct State {
    pub active_color_index: i32,
    pub(crate) ui: Option<QrLoginUiState>,
    /// `Some` when entered via Manage Local Profiles → Link GrooveStats,
    /// scoping the screen to a single profile (rather than P1/P2 sides).
    /// Cleared on dismiss so subsequent post-Select-Profile auto-flows
    /// don't accidentally inherit it.
    pub target_profile: Option<ProfileTarget>,
    bg: visual_style_bg::State,
    menu_lr_chord: screen_input::MenuLrChordTracker,
}

pub fn init() -> State {
    State {
        active_color_index: config::get().simply_love_color,
        ui: None,
        target_profile: None,
        bg: visual_style_bg::State::new(),
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    }
}

/// Called every time the app enters this screen.  Spawns a fresh login
/// UI — single-profile when `target_profile` is `Some`, multi-side
/// otherwise — and discards any previous instance.
pub fn on_enter(state: &mut State) {
    state.ui = Some(match state.target_profile.as_ref() {
        Some(target) => {
            create_groovestats_login_ui_for_profile(target.id.clone(), target.display_name.clone())
        }
        None => create_groovestats_login_ui(),
    });
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
        poll_qr_login_ui(ui);
    }
}

/// Input mirrors the ArrowCloud screen:
///   * Start (or SELECT) → cancel ws worker, advance to the next stop
///                          in the post-SelectProfile chain.
///   * Back  (or L+R chord on three-key cabinets) → cancel ws worker and
///                          return all the way to the title menu, same
///                          as Back on `SelectProfile`.
///
/// Advance target: if entered via Manage Local Profiles → return there.
/// Otherwise hand off to `ArrowCloudLogin` so its auto-show check + own
/// fall-through to `SelectColor` runs next.  This keeps the chain
/// (`SelectProfile → GrooveStats → ArrowCloud → SelectColor`) terminating
/// at SelectColor through the ArrowCloud screen's existing logic, even
/// when ArrowCloud's pref is `Disabled` (the app's ScreenAction handler
/// already collapses that case to SelectColor directly).
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
        || (ev.pressed && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back));
    let from_profile_menu = state.target_profile.is_some();
    if is_start {
        if let Some(ui) = state.ui.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        state.ui = None;
        state.target_profile = None;
        audio::play_sfx("assets/sounds/start.ogg");
        let next = if from_profile_menu {
            Screen::ManageLocalProfiles
        } else if qr_login::should_auto_show(config::get().arrowcloud_qr_login_when) {
            Screen::ArrowCloudLogin
        } else {
            Screen::SelectColor
        };
        return ScreenAction::Navigate(next);
    }
    if is_back {
        if let Some(ui) = state.ui.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        state.ui = None;
        state.target_profile = None;
        audio::play_sfx("assets/sounds/change.ogg");
        let next = if from_profile_menu {
            Screen::ManageLocalProfiles
        } else {
            Screen::Menu
        };
        log::info!("GrooveStats QR login cancelled — returning to {next:?}.");
        return ScreenAction::Navigate(next);
    }
    ScreenAction::None
}

pub fn push_actors(actors: &mut Vec<Actor>, state: &State, alpha_multiplier: f32) {
    actors.reserve(32);

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        },
    );

    if let Some(ui) = state.ui.as_ref() {
        let mut ui_actors = build_qr_login_overlay_actors(ui, state.active_color_index);
        for actor in &mut ui_actors {
            actor.mul_alpha(alpha_multiplier);
        }
        actors.extend(ui_actors);
    }
}

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(32);
    push_actors(&mut actors, state, alpha_multiplier);
    actors
}
