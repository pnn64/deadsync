//! Generic FSR pad configuration screen.
//!
//! Shows every connected FSR pad (SMX, FSRIO, …) side by side as groups of
//! L/D/U/R bars with live sensor values and an editable threshold. Navigation
//! is keyboard / dedicated-menu-button only (Left/Right moves the cursor across
//! all bars, Up/Down adjusts the focused threshold) so stepping on the pad to
//! test a sensor never moves the selection.
//!
//! Phase 1 renders one bar per button using the button's aggregate value /
//! threshold (Simple mode). Per-sensor (Advanced) rendering is a later phase.

use crate::act;
use crate::engine::input::fsr::{PAD_BUTTON_COUNT, PadDeviceId, PadView};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height};
use crate::screens::components::shared::visual_style_bg;
use crate::screens::{Screen, ScreenAction};
use deadsync_input::{InputEvent, InputSource, VirtualAction};

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const THRESHOLD_STEP: u16 = 5;

const BAR_WIDTH: f32 = 48.0;
const BAR_GAP: f32 = 24.0;
const BAR_HEIGHT: f32 = 160.0;
const PAD_GAP: f32 = 70.0;
const PANEL_BG: [f32; 4] = [0.0, 0.0, 0.0, 0.68];
const PANEL_BORDER_H: f32 = 3.0;
/// Muted background of an unfilled bar.
const TRACK_COLOR: [f32; 4] = [0.12, 0.12, 0.16, 0.85];
/// Fill color when the panel is currently activated (real pad input state).
const ACTIVE_FILL: [f32; 4] = [0.30, 0.95, 0.45, 0.95];
/// The activation-threshold line.
const THRESHOLD_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Highlight color for the currently selected bar's text (reads on black and green).
const SELECTED_TEXT: [f32; 4] = [1.0, 0.55, 0.1, 1.0];

struct Theme {
    frame: [f32; 4],
    fill_idle: [f32; 4],
}

/// A pending threshold edit for the app loop to apply via `Monitor::set_threshold`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThresholdCommand {
    pub device: PadDeviceId,
    pub button: usize,
    pub sensor: Option<usize>,
    pub threshold: u16,
}

/// Which pads to show, based on play context when opened.
#[derive(Clone, Copy, Default)]
pub enum PadFilter {
    /// Show every connected pad (e.g. opened from Options).
    #[default]
    All,
    /// Show only the pads for the given player sides (e.g. opened mid-play).
    Sides { p1: bool, p2: bool },
}

#[derive(Default)]
pub struct State {
    pub active_color_index: i32,
    pads: Vec<PadView>,
    /// Flat bar index across every pad: `pad = selected / 4`, `button = selected % 4`.
    selected: usize,
    pending: Option<ThresholdCommand>,
    /// Screen to return to on Back. Set when navigating in; defaults to Options.
    return_screen: Option<Screen>,
    filter: PadFilter,
    bg: visual_style_bg::State,
}

/// Set where Back returns to (e.g. Song Select when opened from its menu).
pub fn set_return_screen(state: &mut State, screen: Screen) {
    state.return_screen = Some(screen);
}

/// Set which pads to show (defaults to all). Apply before the next `set_pads`.
pub fn set_filter(state: &mut State, filter: PadFilter) {
    state.filter = filter;
}

pub fn init() -> State {
    State::default()
}

/// Replace the live pad snapshot (called by the app loop each frame while this
/// screen is active), applying the active pad filter. Keeps the selection in range.
pub fn set_pads(state: &mut State, pads: Vec<PadView>) {
    state.pads = pads
        .into_iter()
        .filter(|p| match state.filter {
            PadFilter::All => true,
            PadFilter::Sides { p1, p2 } => {
                if p.is_player2 {
                    p2
                } else {
                    p1
                }
            }
        })
        .collect();
    let total = total_bars(state);
    if total == 0 {
        state.selected = 0;
    } else if state.selected >= total {
        state.selected = total - 1;
    }
}

/// Take any pending threshold edit so the app loop can apply it to hardware.
pub fn take_command(state: &mut State) -> Option<ThresholdCommand> {
    state.pending.take()
}

pub fn update(_state: &mut State, _dt: f32) -> Option<ScreenAction> {
    None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    (Vec::new(), TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    (Vec::new(), TRANSITION_OUT_DURATION)
}

/// `fine` (Shift held) adjusts thresholds by 1 instead of `THRESHOLD_STEP`.
pub fn handle_input(state: &mut State, ev: &InputEvent, fine: bool) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    if matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back) {
        return ScreenAction::Navigate(state.return_screen.unwrap_or(Screen::Options));
    }
    // Only keyboard or dedicated menu buttons drive the UI; ignore raw pad
    // panels so testing a sensor doesn't move the cursor.
    if ev.source == InputSource::Gamepad && !is_dedicated_menu_action(ev.action) {
        return ScreenAction::None;
    }
    let total = total_bars(state);
    if total == 0 {
        return ScreenAction::None;
    }
    let step = if fine { 1 } else { THRESHOLD_STEP as i32 };
    match ui_action(ev.action) {
        Some(UiAction::PrevBar) => {
            state.selected = (state.selected + total - 1) % total;
        }
        Some(UiAction::NextBar) => {
            state.selected = (state.selected + 1) % total;
        }
        Some(UiAction::Raise) => adjust_threshold(state, step),
        Some(UiAction::Lower) => adjust_threshold(state, -step),
        None => {}
    }
    ScreenAction::None
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(128);
    let theme = theme(state.active_color_index);

    actors.extend(state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    actors.push(act!(text:
        font("miso"):
        settext("CONFIGURE PADS"):
        align(0.5, 0.0):
        xy(screen_center_x(), 36.0):
        zoom(1.2):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.95):
        z(20)
    ));

    if !crate::config::get().use_fsrs {
        actors.push(act!(text:
            font("miso"):
            settext("FSR support is off."):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y() - 16.0):
            zoom(1.0):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.9):
            z(20)
        ));
        actors.push(act!(text:
            font("miso"):
            settext("If you have FSR pads, enable \"Use FSRs\" in Options > Input."):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y() + 14.0):
            zoom(0.7):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.8):
            z(20)
        ));
        push_footer(&mut actors);
        return actors;
    }

    if state.pads.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext("No FSR pads detected."):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y()):
            zoom(1.0):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.85):
            z(20)
        ));
        push_footer(&mut actors);
        return actors;
    }

    let group_w = BAR_WIDTH * PAD_BUTTON_COUNT as f32 + BAR_GAP * (PAD_BUTTON_COUNT - 1) as f32;
    let panel_w = group_w + 34.0;
    let total_w = panel_w * state.pads.len() as f32 + PAD_GAP * (state.pads.len() - 1) as f32;
    let panel_h = BAR_HEIGHT + 140.0;
    let top_y = screen_center_y() - panel_h * 0.5;
    let mut panel_cx = screen_center_x() - total_w * 0.5 + panel_w * 0.5;

    for (pad_idx, pad) in state.pads.iter().enumerate() {
        push_frame(&mut actors, panel_cx, top_y, panel_w, panel_h, theme.frame, 10.0);
        actors.push(act!(text:
            font("miso"):
            settext(pad.device_name.clone()):
            align(0.5, 0.0):
            xy(panel_cx, top_y + 14.0):
            zoom(0.82):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.9):
            z(12)
        ));

        let track_y = top_y + 84.0;
        let left = panel_cx - group_w * 0.5 + BAR_WIDTH * 0.5;
        for (btn_idx, button) in pad.buttons.iter().enumerate() {
            let x = left + btn_idx as f32 * (BAR_WIDTH + BAR_GAP);
            let selected = state.selected == pad_idx * PAD_BUTTON_COUNT + btn_idx;
            let threshold = pending_threshold(state, pad.device_id, btn_idx)
                .unwrap_or(button.aggregate_threshold);
            push_bar(
                &mut actors,
                button.label,
                button.aggregate_value,
                normalize(button.aggregate_value, button.max_raw_threshold),
                threshold,
                normalize(threshold, button.max_raw_threshold),
                button.active,
                x,
                track_y,
                &theme,
                selected,
                11.0,
            );
        }
        panel_cx += panel_w + PAD_GAP;
    }

    push_footer(&mut actors);
    actors
}

// ─── Internal ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum UiAction {
    PrevBar,
    NextBar,
    Raise,
    Lower,
}

const fn ui_action(act: VirtualAction) -> Option<UiAction> {
    match act {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => Some(UiAction::PrevBar),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => Some(UiAction::NextBar),
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => Some(UiAction::Raise),
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => Some(UiAction::Lower),
        _ => None,
    }
}

const fn is_dedicated_menu_action(act: VirtualAction) -> bool {
    matches!(
        act,
        VirtualAction::p1_menu_up
            | VirtualAction::p1_menu_down
            | VirtualAction::p1_menu_left
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_menu_up
            | VirtualAction::p2_menu_down
            | VirtualAction::p2_menu_left
            | VirtualAction::p2_menu_right
    )
}

fn total_bars(state: &State) -> usize {
    state.pads.len() * PAD_BUTTON_COUNT
}

/// Current pending threshold for a specific pad+button, if one is queued.
fn pending_threshold(state: &State, device: PadDeviceId, button: usize) -> Option<u16> {
    state
        .pending
        .filter(|c| c.device == device && c.button == button)
        .map(|c| c.threshold)
}

fn adjust_threshold(state: &mut State, delta: i32) {
    let pad_idx = state.selected / PAD_BUTTON_COUNT;
    let button = state.selected % PAD_BUTTON_COUNT;
    let Some(pad) = state.pads.get(pad_idx) else {
        return;
    };
    let bar = &pad.buttons[button];
    let device = pad.device_id;
    let current = pending_threshold(state, device, button).unwrap_or(bar.aggregate_threshold);
    let next = (i32::from(current) + delta).clamp(
        i32::from(bar.min_raw_threshold),
        i32::from(bar.max_raw_threshold),
    ) as u16;
    if next == current {
        return;
    }
    // Simple mode: apply to every sensor in the button (`sensor: None`).
    state.pending = Some(ThresholdCommand {
        device,
        button,
        sensor: None,
        threshold: next,
    });
}

fn normalize(value: u16, max: u16) -> f32 {
    if max == 0 {
        return 0.0;
    }
    (value as f32 / max as f32).clamp(0.0, 1.0)
}

fn push_footer(actors: &mut Vec<Actor>) {
    let cx = screen_center_x();
    let bottom = screen_height();
    let line = |actors: &mut Vec<Actor>, text: String, y: f32| {
        actors.push(act!(text:
            font("miso"):
            settext(text):
            align(0.5, 0.5):
            xy(cx, y):
            zoom(0.7):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.85):
            z(20)
        ));
    };
    line(actors, "Left/Right - Select Panel".to_owned(), bottom - 70.0);
    line(
        actors,
        format!("Up/Down - Threshold +/- {THRESHOLD_STEP} (Shift +/- 1)"),
        bottom - 46.0,
    );
    line(
        actors,
        "Press &BACK; to return to Options".to_owned(),
        bottom - 22.0,
    );
}

fn theme(active_color_index: i32) -> Theme {
    Theme {
        frame: with_alpha(color::decorative_rgba(active_color_index - 2), 0.95),
        fill_idle: with_alpha(color::decorative_rgba(active_color_index), 0.95),
    }
}

fn with_alpha(mut rgba: [f32; 4], alpha: f32) -> [f32; 4] {
    rgba[3] = alpha;
    rgba
}

fn push_quad(actors: &mut Vec<Actor>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], z: f32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    actors.push(act!(quad:
        align(0.5, 0.0):
        xy(x, y):
        zoomto(w, h):
        diffuse(color[0], color[1], color[2], color[3]):
        z(z)
    ));
}

fn push_frame(
    actors: &mut Vec<Actor>,
    center_x: f32,
    top_y: f32,
    panel_w: f32,
    panel_h: f32,
    frame_color: [f32; 4],
    z: f32,
) {
    let left = center_x - panel_w * 0.5;
    let right = center_x + panel_w * 0.5;
    push_quad(actors, center_x, top_y, panel_w, panel_h, PANEL_BG, z);
    push_quad(actors, center_x, top_y, panel_w, PANEL_BORDER_H, frame_color, z + 1.0);
    push_quad(
        actors,
        center_x,
        top_y + panel_h - PANEL_BORDER_H,
        panel_w,
        PANEL_BORDER_H,
        frame_color,
        z + 1.0,
    );
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(left, top_y):
        zoomto(PANEL_BORDER_H, panel_h):
        diffuse(frame_color[0], frame_color[1], frame_color[2], frame_color[3]):
        z(z + 1.0)
    ));
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(right - PANEL_BORDER_H, top_y):
        zoomto(PANEL_BORDER_H, panel_h):
        diffuse(frame_color[0], frame_color[1], frame_color[2], frame_color[3]):
        z(z + 1.0)
    ));
}

#[allow(clippy::too_many_arguments)]
fn push_bar(
    actors: &mut Vec<Actor>,
    label: &str,
    raw_value: u16,
    value_norm: f32,
    raw_threshold: u16,
    threshold_norm: f32,
    active: bool,
    x: f32,
    y: f32,
    theme: &Theme,
    selected: bool,
    z: f32,
) {
    let value_norm = value_norm.clamp(0.0, 1.0);
    let threshold_norm = threshold_norm.clamp(0.0, 1.0);

    // Single muted track background.
    push_quad(actors, x, y, BAR_WIDTH, BAR_HEIGHT, TRACK_COLOR, z);

    // Value fill rising from the bottom; turns green while the panel is
    // actually activated (real pad input state, which uses the firmware's
    // low/high hysteresis).
    let fill_h = value_norm * BAR_HEIGHT;
    if fill_h > 0.0 {
        let fill_color = if active { ACTIVE_FILL } else { theme.fill_idle };
        push_quad(actors, x, y + BAR_HEIGHT - fill_h, BAR_WIDTH, fill_h, fill_color, z + 1.0);
    }

    // Activation-threshold line.
    let threshold_h = 3.0_f32;
    let threshold_y = y + (1.0 - threshold_norm) * BAR_HEIGHT - threshold_h * 0.5;
    push_quad(actors, x, threshold_y, BAR_WIDTH, threshold_h, THRESHOLD_COLOR, z + 2.0);

    if selected {
        let ox = x - (BAR_WIDTH + 12.0) * 0.5;
        let oy = y - 34.0;
        let ow = BAR_WIDTH + 12.0;
        let oh = BAR_HEIGHT + 70.0;
        let t = 2.0_f32;
        let outline = [1.0, 1.0, 1.0, 1.0];
        push_quad(actors, x, oy, ow, t, outline, z + 2.5);
        push_quad(actors, x, oy + oh - t, ow, t, outline, z + 2.5);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(ox, oy): zoomto(t, oh):
            diffuse(outline[0], outline[1], outline[2], outline[3]): z(z + 2.5)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(ox + ow - t, oy): zoomto(t, oh):
            diffuse(outline[0], outline[1], outline[2], outline[3]): z(z + 2.5)
        ));
    }

    let text_color = if selected {
        SELECTED_TEXT
    } else {
        [1.0, 1.0, 1.0, 0.95]
    };
    actors.push(act!(text:
        font("miso"): settext(raw_value.to_string()): align(0.5, 1.0):
        xy(x, y - 10.0): zoom(0.92): horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
    ));
    actors.push(act!(text:
        font("miso"): settext(raw_threshold.to_string()): align(0.5, 1.0):
        xy(x, threshold_y - 3.0): zoom(0.68): horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
    ));
    let label_color = if active { ACTIVE_FILL } else { text_color };
    actors.push(act!(text:
        font("miso"): settext(label.to_string()): align(0.5, 0.0):
        xy(x, y + BAR_HEIGHT + 8.0): zoom(1.0): horizalign(center):
        diffuse(label_color[0], label_color[1], label_color[2], label_color[3]): z(z + 3.0)
    ));
}
