//! StepManiaX pad → player assignment screen.
//!
//! Walks the user through a press-a-panel flow: press a panel on the pad you
//! want as P1, then the pad you want as P2. The theme owns this interaction and
//! emits shell requests that pin the selected serials and drive indicator lights.

use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{self, screen_center_x, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_theme::views::SmxAssignmentView;
use std::sync::Mutex;

/// Screen to return to when the assignment screen finishes/cancels. Set by the
/// caller right before navigating in (so the Options entry returns to Options and
/// the auto-prompt returns to the Menu), then consumed by [`on_enter`].
static PENDING_RETURN: Mutex<Screen> = Mutex::new(Screen::Options);

/// Set where the assignment screen should return to on exit. Call immediately
/// before navigating to [`Screen::SmxAssignPads`].
pub fn set_pending_return(screen: Screen) {
    *PENDING_RETURN.lock().unwrap() = screen;
}

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/// How often to re-send the indicator lights so the pads hold their colour
/// (a one-shot `set_lights` can otherwise lapse back to auto-lighting).
const LIGHT_RESEND_INTERVAL: f32 = 0.25;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Fewer than two pads connected; assignment needs both.
    NeedTwoPads,
    /// Waiting for a panel press to pick the P1 pad.
    AwaitP1,
    /// Waiting for a panel press (on the other pad) to pick the P2 pad.
    AwaitP2,
    /// Both chosen and applied; press Start to finish.
    Done,
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    /// Screen to return to on finish/cancel (Options for the manual entry; the
    /// caller can override for the auto-prompt path).
    return_screen: Screen,
    phase: Phase,
    /// Previous per-slot input bitmask, for rising-edge press detection.
    prev_input: [u16; 2],
    p1_serial: Option<String>,
    p2_serial: Option<String>,
    p1_label: Option<String>,
    p2_label: Option<String>,
    conflict_warning: bool,
    conflict_rgb: [f32; 3],
    player_rgb: [[u8; 3]; 2],
    light_timer: f32,
    lights_pending: bool,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        return_screen: Screen::Options,
        phase: Phase::NeedTwoPads,
        prev_input: [0, 0],
        p1_serial: None,
        p2_serial: None,
        p1_label: None,
        p2_label: None,
        conflict_warning: false,
        conflict_rgb: [0.0; 3],
        player_rgb: [[0; 3]; 2],
        light_timer: 0.0,
        lights_pending: false,
    }
}

pub fn on_enter(state: &mut State, view: &SmxAssignmentView) {
    state.return_screen = *PENDING_RETURN.lock().unwrap();
    state.p1_serial = None;
    state.p2_serial = None;
    state.p1_label = None;
    state.p2_label = None;
    sync_view(state, view);
    state.prev_input = current_input(view);
    state.light_timer = 0.0;
    state.phase = if connected_count(view) >= 2 {
        Phase::AwaitP1
    } else {
        Phase::NeedTwoPads
    };
    state.lights_pending = true;
}

pub fn update(state: &mut State, dt: f32, view: &SmxAssignmentView) -> Option<ThemeEffect> {
    sync_view(state, view);
    let connected = connected_count(view);
    let mut effects = Vec::with_capacity(2);
    // Keep the phase consistent with how many pads are present (hot-plug safe).
    if connected < 2 && state.phase != Phase::NeedTwoPads {
        state.phase = Phase::NeedTwoPads;
        state.p1_serial = None;
        state.p2_serial = None;
        state.p1_label = None;
        state.p2_label = None;
        state.lights_pending = true;
    } else if connected >= 2 && state.phase == Phase::NeedTwoPads {
        state.phase = Phase::AwaitP1;
        state.prev_input = current_input(view);
        state.lights_pending = true;
    }

    // Rising-edge press detection per slot (raw bitmask, independent of keymap).
    let input = current_input(view);
    let pressed = pressed_slot(state.prev_input, input);
    state.prev_input = input;

    match state.phase {
        Phase::AwaitP1 => {
            if let Some((serial, label)) = pressed.and_then(|slot| pad_identity(view, slot)) {
                state.p1_serial = Some(serial);
                state.p1_label = Some(label);
                state.phase = Phase::AwaitP2;
                state.lights_pending = true;
            }
        }
        Phase::AwaitP2 => {
            // Accept only the *other* pad, not the one already chosen for P1.
            let identity = pressed.and_then(|slot| pad_identity(view, slot));
            if let Some((serial, label)) = identity.filter(|(serial, _)| {
                accept_p2_serial(Some(serial.clone()), state.p1_serial.as_deref()).is_some()
            }) {
                state.p2_serial = Some(serial);
                state.p2_label = Some(label);
                state.phase = Phase::Done;
                effects.push(hardware_effect(
                    crate::SimplyLoveHardwareRequest::AssignSmxPads {
                        p1_serial: state.p1_serial.clone(),
                        p2_serial: state.p2_serial.clone(),
                    },
                ));
                state.lights_pending = true;
            }
        }
        Phase::NeedTwoPads | Phase::Done => {}
    }

    // Hold the indicator lights.
    state.light_timer += dt;
    if state.light_timer >= LIGHT_RESEND_INTERVAL {
        state.light_timer = 0.0;
        state.lights_pending = true;
    }
    if state.lights_pending {
        state.lights_pending = false;
        effects.push(hardware_effect(
            crate::SimplyLoveHardwareRequest::SetSmxPlayerLights(light_colors(state, view)),
        ));
    }
    match effects.len() {
        0 => None,
        1 => effects.pop(),
        _ => Some(ThemeEffect::Batch(effects)),
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if !ev.pressed {
        return ThemeEffect::None;
    }
    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back => exit(state),
        VirtualAction::p1_start | VirtualAction::p2_start if state.phase == Phase::Done => {
            exit(state)
        }
        _ => ThemeEffect::None,
    }
}

/// Leave the screen. The options StepManiaX page re-drives the pad lights itself,
/// so only restore auto-lighting when returning somewhere that won't (e.g. the
/// Menu), to avoid a one-frame flicker on the handoff.
fn exit(state: &State) -> ThemeEffect {
    let navigate = ThemeEffect::Navigate(state.return_screen);
    if state.return_screen == Screen::Options {
        navigate
    } else {
        ThemeEffect::Batch(vec![
            hardware_effect(crate::SimplyLoveHardwareRequest::ReenableSmxAutoLights),
            navigate,
        ])
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn hardware_effect(request: crate::SimplyLoveHardwareRequest) -> ThemeEffect {
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Hardware(request))
}

fn sync_view(state: &mut State, view: &SmxAssignmentView) {
    state.conflict_warning = view.conflict_warning;
    state.conflict_rgb = view.conflict_rgb;
    state.player_rgb = view.player_rgb;
}

fn connected_count(view: &SmxAssignmentView) -> usize {
    view.pads.iter().filter(|pad| pad.connected).count()
}

fn current_input(view: &SmxAssignmentView) -> [u16; 2] {
    std::array::from_fn(|slot| view.pads[slot].input_state)
}

/// Rising-edge press detection: the first slot whose raw input bitmask gained a
/// set bit since the previous frame, if any.
fn pressed_slot(prev: [u16; 2], cur: [u16; 2]) -> Option<usize> {
    (0..2).find(|&s| (cur[s] & !prev[s]) != 0)
}

/// In `AwaitP2`, accept the pressed pad's serial only if it is a real serial for
/// the *other* pad (different from the P1 choice); otherwise ignore the press.
fn accept_p2_serial(pressed: Option<String>, p1: Option<&str>) -> Option<String> {
    match pressed {
        Some(s) if Some(s.as_str()) != p1 => Some(s),
        _ => None,
    }
}

/// Serial and display label of the pad at `slot`, if it is assignable.
fn pad_identity(view: &SmxAssignmentView, slot: usize) -> Option<(String, String)> {
    let pad = view.pads.get(slot)?;
    (pad.connected && !pad.serial.is_empty()).then(|| (pad.serial.clone(), pad.label.clone()))
}

/// Indicator-light intent for the current phase.
fn light_colors(state: &State, view: &SmxAssignmentView) -> [Option<[u8; 3]>; 2] {
    let mut colors: [Option<[u8; 3]>; 2] = [None, None];
    match state.phase {
        Phase::NeedTwoPads => {}
        Phase::AwaitP1 => {
            // Every connected pad glows blue; press the one you want as P1.
            for (s, c) in colors.iter_mut().enumerate() {
                if view.pads[s].connected {
                    *c = Some(state.player_rgb[0]);
                }
            }
        }
        Phase::AwaitP2 => {
            // The chosen P1 pad stays blue; the other glows red.
            for (s, c) in colors.iter_mut().enumerate() {
                let pad = &view.pads[s];
                if !pad.connected {
                    continue;
                }
                *c = if Some(&pad.serial) == state.p1_serial.as_ref() {
                    Some(state.player_rgb[0])
                } else {
                    Some(state.player_rgb[1])
                };
            }
        }
        Phase::Done => {
            // Assignment applied: slot 0 = P1 (blue), slot 1 = P2 (red).
            colors = [Some(state.player_rgb[0]), Some(state.player_rgb[1])];
        }
    }
    colors
}

// ─── Rendering ───────────────────────────────────────────────────────────────

pub fn push_actors(actors: &mut Vec<Actor>, state: &State, alpha_mul: f32) {
    actors.reserve(16);
    let screen_w = screen_width();
    let screen_h = screen_height();

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul,
        },
    );

    // Title.
    let title = tr("ScreenSmxAssignPads", "HeaderText");
    let title_font = current_machine_font_key_for_text(FontRole::Header, &title);
    let title_scale = if space::is_wide() { 0.6 } else { 0.5 };
    actors.push(act!(text:
        font(title_font):
        settext(title):
        align(0.5, 0.5):
        xy(screen_center_x(), 30.0):
        zoom(title_scale):
        maxwidth(screen_w * 0.85):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.96 * alpha_mul):
        z(85)
    ));

    // When there's an unresolved same-jumper conflict (incl. the case that
    // auto-opened this screen), explain why the user is here.
    if state.conflict_warning {
        let amber = state.conflict_rgb;
        actors.push(act!(text:
            font("miso"):
            settext(tr("ScreenSmxAssignPads", "ConflictExplanation")):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_h * 0.30):
            zoom(0.72):
            maxwidth(screen_w * 0.8):
            horizalign(center):
            diffuse(amber[0], amber[1], amber[2], 0.95 * alpha_mul):
            strokecolor(0.0, 0.0, 0.0, 0.75 * alpha_mul):
            shadowlength(1.0):
            z(86)
        ));
    }

    // Prompt for the current phase.
    let prompt = match state.phase {
        Phase::NeedTwoPads => tr("ScreenSmxAssignPads", "NeedTwoPads"),
        Phase::AwaitP1 => tr("ScreenSmxAssignPads", "PressForP1"),
        Phase::AwaitP2 => tr("ScreenSmxAssignPads", "PressForP2"),
        Phase::Done => tr("ScreenSmxAssignPads", "Assigned"),
    };
    actors.push(act!(text:
        font("miso"):
        settext(prompt):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_h * 0.42):
        zoom(0.95):
        maxwidth(screen_w * 0.85):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.95 * alpha_mul):
        strokecolor(0.0, 0.0, 0.0, 0.75 * alpha_mul):
        shadowlength(1.0):
        z(86)
    ));

    // Chosen-so-far lines (P1 blue, P2 red), with the pad's serial prefix.
    let blue = state.player_rgb[0];
    let red = state.player_rgb[1];
    let line = |label: &str, pad_label: &Option<String>, rgb: [u8; 3]| -> (String, [f32; 3]) {
        let val = pad_label.as_deref().unwrap_or("(none)");
        (
            format!("{label}: {val}"),
            [
                rgb[0] as f32 / 255.0,
                rgb[1] as f32 / 255.0,
                rgb[2] as f32 / 255.0,
            ],
        )
    };
    let rows = [
        line(&tr("ScreenSmxAssignPads", "Player1"), &state.p1_label, blue),
        line(&tr("ScreenSmxAssignPads", "Player2"), &state.p2_label, red),
    ];
    let base_y = screen_h * 0.56;
    for (i, (text, rgb)) in rows.into_iter().enumerate() {
        actors.push(act!(text:
            font("miso"):
            settext(text):
            align(0.5, 0.5):
            xy(screen_center_x(), base_y + i as f32 * 40.0):
            zoom(0.9):
            maxwidth(screen_w * 0.8):
            horizalign(center):
            diffuse(rgb[0], rgb[1], rgb[2], 0.95 * alpha_mul):
            strokecolor(0.0, 0.0, 0.0, 0.75 * alpha_mul):
            shadowlength(1.0):
            z(86)
        ));
    }

    // Footer help.
    let footer = match state.phase {
        Phase::Done => tr("ScreenSmxAssignPads", "ControlsDone"),
        _ => tr("ScreenSmxAssignPads", "Controls"),
    };
    actors.push(act!(text:
        font("miso"):
        settext(footer):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_h - 22.0):
        zoom(0.74):
        maxwidth(screen_w * 0.92):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.74 * alpha_mul):
        z(90)
    ));
}

pub fn get_actors(state: &State, alpha_mul: f32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(16);
    push_actors(&mut actors, state, alpha_mul);
    actors
}

#[cfg(test)]
mod tests {
    use super::{accept_p2_serial, exit, init, on_enter, pressed_slot, update};
    use crate::screens::ThemeEffect;
    use crate::{SimplyLoveHardwareRequest, SimplyLoveRuntimeRequest};
    use deadsync_theme::views::{SmxAssignmentPadView, SmxAssignmentView};

    fn assignment_view(input: [u16; 2]) -> SmxAssignmentView {
        SmxAssignmentView {
            pads: std::array::from_fn(|slot| SmxAssignmentPadView {
                connected: true,
                serial: format!("SERIAL{slot}"),
                label: format!("SMX[S{slot}]"),
                input_state: input[slot],
                ..SmxAssignmentPadView::default()
            }),
            can_swap: true,
            conflict_warning: true,
            conflict_rgb: [1.0, 0.5, 0.0],
            player_rgb: [[0, 0, 255], [255, 0, 0]],
        }
    }

    #[test]
    fn pressed_slot_detects_rising_edge_only() {
        // No change: nothing pressed.
        assert_eq!(pressed_slot([0, 0], [0, 0]), None);
        assert_eq!(pressed_slot([0b1, 0], [0b1, 0]), None);
        // A newly set bit on slot 0.
        assert_eq!(pressed_slot([0, 0], [0b10, 0]), Some(0));
        // A newly set bit on slot 1 only.
        assert_eq!(pressed_slot([0, 0], [0, 0b1]), Some(1));
        // A released bit is not a press.
        assert_eq!(pressed_slot([0b1, 0], [0, 0]), None);
        // Slot 0 wins when both gain a bit on the same frame.
        assert_eq!(pressed_slot([0, 0], [0b1, 0b1]), Some(0));
    }

    #[test]
    fn accept_p2_serial_requires_the_other_pad() {
        // Same pad as P1: rejected.
        assert_eq!(accept_p2_serial(Some("A".to_owned()), Some("A")), None);
        // Different pad: accepted.
        assert_eq!(
            accept_p2_serial(Some("B".to_owned()), Some("A")),
            Some("B".to_owned())
        );
        // No press: nothing to accept.
        assert_eq!(accept_p2_serial(None, Some("A")), None);
    }

    #[test]
    fn assignment_uses_prepared_input_and_emits_shell_requests() {
        let idle = assignment_view([0, 0]);
        let mut state = init();
        on_enter(&mut state, &idle);
        assert!(matches!(
            update(&mut state, 0.0, &idle),
            Some(ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Hardware(
                SimplyLoveHardwareRequest::SetSmxPlayerLights(_)
            )))
        ));

        let p1 = assignment_view([1, 0]);
        assert!(matches!(
            update(&mut state, 0.0, &p1),
            Some(ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Hardware(
                SimplyLoveHardwareRequest::SetSmxPlayerLights(_)
            )))
        ));
        let released = assignment_view([0, 0]);
        assert!(update(&mut state, 0.0, &released).is_none());

        let p2 = assignment_view([0, 1]);
        let Some(ThemeEffect::Batch(effects)) = update(&mut state, 0.0, &p2) else {
            panic!("P2 selection should assign pads and refresh lights");
        };
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Hardware(
                SimplyLoveHardwareRequest::AssignSmxPads {
                    p1_serial,
                    p2_serial,
                }
            )) if p1_serial.as_deref() == Some("SERIAL0")
                && p2_serial.as_deref() == Some("SERIAL1")
        ));
        assert!(matches!(
            effects[1],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Hardware(
                SimplyLoveHardwareRequest::SetSmxPlayerLights(_)
            ))
        ));
    }

    #[test]
    fn non_options_exit_restores_shell_owned_auto_lights() {
        let mut state = init();
        state.return_screen = crate::screens::Screen::Menu;
        let ThemeEffect::Batch(effects) = exit(&state) else {
            panic!("menu return should restore lights before navigation");
        };
        assert!(matches!(
            effects[0],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Hardware(
                SimplyLoveHardwareRequest::ReenableSmxAutoLights
            ))
        ));
        assert!(matches!(
            effects[1],
            ThemeEffect::Navigate(crate::screens::Screen::Menu)
        ));
    }
}
