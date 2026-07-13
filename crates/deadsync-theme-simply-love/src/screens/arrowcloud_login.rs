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
//! [`build_qr_login_overlay_actors`].

use std::sync::atomic::Ordering;

use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::options::qr_login::{
    QrLoginUiState, build_qr_login_overlay_actors, create_arrowcloud_login_ui,
    create_arrowcloud_login_ui_for_profile, poll_qr_login_ui,
};
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::Actor;
use deadsync_input::{InputEvent, VirtualAction};

const TRANSITION_IN_DURATION: f32 = 0.3;
const TRANSITION_OUT_DURATION: f32 = 0.3;

/// Optional per-profile scoping carried into the screen from Manage
/// Local Profiles' "Link ArrowCloud" entry.
#[derive(Clone, Debug)]
pub struct ProfileTarget {
    pub id: String,
    pub display_name: String,
}

pub struct State {
    pub active_color_index: i32,
    pub(crate) ui: Option<QrLoginUiState>,
    /// `Some` when entered via Manage Local Profiles → Link ArrowCloud,
    /// scoping the screen to a single profile (rather than P1/P2 sides).
    /// Cleared on dismiss so subsequent post-Select-Profile auto-flows
    /// don't accidentally inherit it.
    pub target_profile: Option<ProfileTarget>,
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
            create_arrowcloud_login_ui_for_profile(target.id.clone(), target.display_name.clone())
        }
        None => create_arrowcloud_login_ui(),
    });
    state.menu_lr_chord = screen_input::MenuLrChordTracker::default();
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

pub fn update(state: &mut State, _dt: f32) -> Option<ThemeEffect> {
    state
        .ui
        .as_mut()
        .is_some_and(poll_qr_login_ui)
        .then_some(ThemeEffect::Runtime(
            crate::SimplyLoveRuntimeRequest::Online(
                crate::SimplyLoveOnlineRequest::RefreshArrowCloudStatus,
            ),
        ))
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
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
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
        let next = if from_profile_menu {
            Screen::ManageLocalProfiles
        } else {
            Screen::SelectColor
        };
        return crate::effects::sfx_then("assets/sounds/start.ogg", ThemeEffect::Navigate(next));
    }
    if is_back {
        if let Some(ui) = state.ui.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        state.ui = None;
        state.target_profile = None;
        let next = if from_profile_menu {
            Screen::ManageLocalProfiles
        } else {
            Screen::Menu
        };
        log::info!("ArrowCloud QR login cancelled — returning to {next:?}.");
        return crate::effects::sfx_then("assets/sounds/change.ogg", ThemeEffect::Navigate(next));
    }
    ThemeEffect::None
}

pub fn push_actors(actors: &mut Vec<Actor>, state: &State, alpha_multiplier: f32) {
    actors.reserve(32);

    // Animated heart background — matches SelectProfile / SelectColor.
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
