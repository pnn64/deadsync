//! StepManiaX pad → player assignment screen.
//!
//! Walks the user through a press-a-panel flow: press a panel on the pad you
//! want as P1, then the pad you want as P2. The pressed pad's serial is pinned to
//! that player slot (see [`crate::engine::smx::set_player_assignment`]), which
//! decouples the assignment from the hardware P1/P2 jumper — the only way to fix
//! two pads that share a jumper or are physically installed swapped. The pads are
//! lit blue (P1) / red (P2) throughout so the user can see which is which.

use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::smx;
use crate::engine::space::{self, screen_center_x, screen_height, screen_width};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ScreenAction};
use deadsync_input::{InputEvent, VirtualAction};
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
    /// Fewer than two pads connected — assignment needs both.
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
    light_timer: f32,
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
        light_timer: 0.0,
    }
}

pub fn on_enter(state: &mut State) {
    state.return_screen = *PENDING_RETURN.lock().unwrap();
    state.p1_serial = None;
    state.p2_serial = None;
    state.prev_input = current_input();
    state.light_timer = 0.0;
    state.phase = if connected_count() >= 2 {
        Phase::AwaitP1
    } else {
        Phase::NeedTwoPads
    };
    apply_lights(state);
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    let connected = connected_count();
    // Keep the phase consistent with how many pads are present (hot-plug safe).
    if connected < 2 && state.phase != Phase::NeedTwoPads {
        state.phase = Phase::NeedTwoPads;
        state.p1_serial = None;
        state.p2_serial = None;
        apply_lights(state);
    } else if connected >= 2 && state.phase == Phase::NeedTwoPads {
        state.phase = Phase::AwaitP1;
        state.prev_input = current_input();
        apply_lights(state);
    }

    // Rising-edge press detection per slot (raw bitmask, independent of keymap).
    let input = current_input();
    let pressed = pressed_slot(state.prev_input, input);
    state.prev_input = input;

    match state.phase {
        Phase::AwaitP1 => {
            if let Some(serial) = pressed.and_then(pad_serial) {
                state.p1_serial = Some(serial);
                state.phase = Phase::AwaitP2;
                apply_lights(state);
            }
        }
        Phase::AwaitP2 => {
            // Accept only the *other* pad, not the one already chosen for P1.
            if let Some(serial) =
                accept_p2_serial(pressed.and_then(pad_serial), state.p1_serial.as_deref())
            {
                state.p2_serial = Some(serial);
                state.phase = Phase::Done;
                // Apply now: the SDK re-orders the slots immediately.
                crate::config::update_smx_pad_assignment(
                    state.p1_serial.clone(),
                    state.p2_serial.clone(),
                );
                apply_lights(state);
            }
        }
        Phase::NeedTwoPads | Phase::Done => {}
    }

    // Hold the indicator lights.
    state.light_timer += dt;
    if state.light_timer >= LIGHT_RESEND_INTERVAL {
        state.light_timer = 0.0;
        apply_lights(state);
    }
    None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back => exit(state),
        VirtualAction::p1_start | VirtualAction::p2_start if state.phase == Phase::Done => {
            exit(state)
        }
        _ => ScreenAction::None,
    }
}

/// Leave the screen, restoring the pads' automatic lighting first.
fn exit(state: &State) -> ScreenAction {
    smx::reenable_auto_lights();
    ScreenAction::Navigate(state.return_screen)
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn connected_count() -> usize {
    (0..2).filter(|&s| smx::get_info(s).connected).count()
}

fn current_input() -> [u16; 2] {
    let m = smx::manager();
    std::array::from_fn(|s| m.map_or(0, |m| m.get_input_state(s)))
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

/// Serial of the pad at `slot`, if connected with a real serial.
fn pad_serial(slot: usize) -> Option<String> {
    let info = smx::get_info(slot);
    (info.connected && !info.serial.is_empty()).then_some(info.serial)
}

/// First 4 hex chars of a serial, for a compact pad label (e.g. `40ea`).
fn serial_prefix(serial: &str) -> String {
    if serial.is_empty() {
        "????".to_owned()
    } else {
        serial.chars().take(4).collect()
    }
}

/// Drive the pad indicator lights for the current phase.
fn apply_lights(state: &State) {
    let mut colors: [Option<[u8; 3]>; 2] = [None, None];
    match state.phase {
        Phase::NeedTwoPads => {}
        Phase::AwaitP1 => {
            // Every connected pad glows blue — press the one you want as P1.
            for (s, c) in colors.iter_mut().enumerate() {
                if smx::get_info(s).connected {
                    *c = Some(smx::PLAYER1_LIGHT);
                }
            }
        }
        Phase::AwaitP2 => {
            // The chosen P1 pad stays blue; the other glows red.
            for (s, c) in colors.iter_mut().enumerate() {
                let info = smx::get_info(s);
                if !info.connected {
                    continue;
                }
                *c = if Some(&info.serial) == state.p1_serial.as_ref() {
                    Some(smx::PLAYER1_LIGHT)
                } else {
                    Some(smx::PLAYER2_LIGHT)
                };
            }
        }
        Phase::Done => {
            // Assignment applied: slot 0 = P1 (blue), slot 1 = P2 (red).
            colors = [Some(smx::PLAYER1_LIGHT), Some(smx::PLAYER2_LIGHT)];
        }
    }
    smx::set_player_lights(colors);
}

// ─── Rendering ───────────────────────────────────────────────────────────────

pub fn get_actors(state: &State, alpha_mul: f32) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(16);
    let screen_w = screen_width();
    let screen_h = screen_height();

    actors.extend(state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul,
    }));

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
    if smx::conflict_warning_active() {
        actors.push(act!(text:
            font("miso"):
            settext(tr("ScreenSmxAssignPads", "ConflictExplanation")):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_h * 0.30):
            zoom(0.72):
            maxwidth(screen_w * 0.8):
            horizalign(center):
            diffuse(1.0, 0.78, 0.2, 0.95 * alpha_mul):
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
    let blue = smx::PLAYER1_LIGHT;
    let red = smx::PLAYER2_LIGHT;
    let line = |label: &str, serial: &Option<String>, rgb: [u8; 3]| -> (String, [f32; 3]) {
        let val = serial
            .as_deref()
            .map_or_else(|| "(none)".to_owned(), |s| format!("SMX[{}]", serial_prefix(s)));
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
        line(&tr("ScreenSmxAssignPads", "Player1"), &state.p1_serial, blue),
        line(&tr("ScreenSmxAssignPads", "Player2"), &state.p2_serial, red),
    ];
    let base_y = screen_h * 0.56;
    for (i, (text, rgb)) in rows.iter().enumerate() {
        actors.push(act!(text:
            font("miso"):
            settext(text.clone()):
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

    actors
}

#[cfg(test)]
mod tests {
    use super::{accept_p2_serial, pressed_slot};

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
}
