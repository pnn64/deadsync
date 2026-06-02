//! Generic FSR pad configuration screen.
//!
//! Shows every connected FSR pad (SMX, FSRIO, …) side by side as groups of
//! L/D/U/R bars with live sensor values and an editable threshold. Navigation
//! is keyboard / dedicated-menu-button only (Left/Right moves the cursor across
//! all bars, Up/Down adjusts the focused threshold) so stepping on the pad to
//! test a sensor never moves the selection.
//!
//! Two views:
//! * **Simple** — one bar per button (L/D/U/R) editing every sensor in that
//!   button to a single threshold. This is what you land on.
//! * **Advanced** — press Start on the pad under the cursor to drill into it:
//!   per-sensor thresholds, per-sensor enable/disable, and the "Extra Advanced"
//!   pad-level controls (auto-recalibration, panel debounce).

use crate::act;
use crate::engine::input::fsr::{PAD_BUTTON_COUNT, PadDeviceId, PadView, SensorView};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height};
use crate::screens::components::shared::visual_style_bg;
use crate::screens::{Screen, ScreenAction};
use deadsync_input::{InputEvent, InputSource, VirtualAction};

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const THRESHOLD_STEP: u16 = 5;

// Debounce editing (microseconds). Displayed as decimal milliseconds.
const DEBOUNCE_DEFAULT_US: u16 = 4000;
const DEBOUNCE_MIN_US: u16 = 500;
const DEBOUNCE_MAX_US: u16 = 25000;
const DEBOUNCE_STEP_US: u16 = 1000; // coarse = 1.0 ms
const DEBOUNCE_FINE_US: u16 = 100; //  fine = 0.1 ms

// ── Simple-view bar geometry ──
const BAR_WIDTH: f32 = 48.0;
const BAR_GAP: f32 = 24.0;
const BAR_HEIGHT: f32 = 140.0;
const PAD_GAP: f32 = 70.0;

// ── Advanced-view geometry ──
const ADV_BAR_W: f32 = 20.0;
const ADV_BAR_GAP: f32 = 6.0;
const ADV_GROUP_GAP: f32 = 44.0;
const ADV_BAR_HEIGHT: f32 = 138.0;
const ADV_TOP_Y: f32 = 132.0;

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
/// Caution color for the Extra Advanced section.
const CAUTION_TEXT: [f32; 4] = [1.0, 0.45, 0.2, 1.0];
/// "On" color for enable / auto-recal indicators.
const ON_TEXT: [f32; 4] = [0.30, 0.95, 0.45, 1.0];
/// Dimmed color for an "off" / disabled indicator.
const OFF_TEXT: [f32; 4] = [0.6, 0.6, 0.65, 0.9];

struct Theme {
    frame: [f32; 4],
    fill_idle: [f32; 4],
}

/// A pending pad-config edit for the app loop to apply to hardware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadCommand {
    /// Threshold edit. `sensor: None` applies to every sensor in the button
    /// (Simple mode); `Some(i)` targets one sensor (Advanced mode).
    Threshold {
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    },
    /// Enable/disable a single sensor (Advanced mode).
    SensorEnabled {
        device: PadDeviceId,
        button: usize,
        sensor: usize,
        enabled: bool,
    },
    /// Toggle auto-recalibration for the whole pad (Extra Advanced).
    AutoRecalibration { device: PadDeviceId, enabled: bool },
    /// Set the per-panel debounce in microseconds (Extra Advanced).
    Debounce { device: PadDeviceId, micros: u16 },
}

/// Outcome of an edit, so the screen / overlay caller knows when to leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditResult {
    /// The event was handled within the screen (edit, nav, enter/leave Advanced).
    Handled,
    /// Back was pressed at the top (Simple) level — the caller should exit.
    ExitToParent,
    /// The user confirmed saving the selected pad as a profile. The caller
    /// should `take_save` the draft and perform the capture + store.
    SaveRequested,
}

/// In-progress "save this pad as a profile" entry (in-session overlay only).
#[derive(Clone, Debug, Default)]
pub struct SaveDraft {
    pub name: String,
    pub set_default: bool,
}

/// Max length of a saved pad-config profile name.
const MAX_PROFILE_NAME_LEN: usize = 24;

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
    /// When set, the Advanced view is open for this specific pad.
    advanced: Option<PadDeviceId>,
    /// Focus index into the Advanced view's focusable targets.
    adv_sel: usize,
    /// Queued edits drained by the app loop each frame.
    pending: Vec<PadCommand>,
    /// When set, the "save this pad as a profile" name-entry box is open.
    saving: Option<SaveDraft>,
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
/// screen is active), applying the active pad filter. Keeps selections in range.
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

    // Drop back to Simple if the Advanced pad disappeared; otherwise clamp the
    // Advanced focus against the (possibly changed) target list.
    match state.advanced {
        Some(dev) if pad_index(state, dev).is_some() => {
            let n = advanced_targets(state).len();
            if n == 0 {
                state.advanced = None;
                state.adv_sel = 0;
            } else if state.adv_sel >= n {
                state.adv_sel = n - 1;
            }
        }
        Some(_) => {
            state.advanced = None;
            state.adv_sel = 0;
        }
        None => {}
    }
}

/// Drain queued edits so the app loop can apply them to hardware.
pub fn take_commands(state: &mut State) -> Vec<PadCommand> {
    std::mem::take(&mut state.pending)
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

/// `fine` (Shift held) makes adjustments use the fine step instead of coarse.
pub fn handle_input(state: &mut State, ev: &InputEvent, fine: bool) -> ScreenAction {
    match apply_edit(state, ev, fine) {
        EditResult::ExitToParent => {
            ScreenAction::Navigate(state.return_screen.unwrap_or(Screen::Options))
        }
        // Saving is only reachable from the in-session overlay; the standalone
        // Options screen never enters save mode, so this is inert here.
        EditResult::Handled | EditResult::SaveRequested => ScreenAction::None,
    }
}

/// Apply an edit for a press. Shared by the full screen and the Song Select
/// overlay. Returns whether Back at the top level asked to exit.
pub fn apply_edit(state: &mut State, ev: &InputEvent, fine: bool) -> EditResult {
    if !ev.pressed {
        return EditResult::Handled;
    }
    // Only keyboard or dedicated menu controls drive the UI; ignore raw pad
    // panels so testing a sensor doesn't move the cursor or change values.
    if ev.source == InputSource::Gamepad && !is_menu_control(ev.action) {
        return EditResult::Handled;
    }

    // Name-entry box for saving the selected pad as a profile (text comes via
    // the raw-key path; here we handle the menu controls).
    if state.saving.is_some() {
        if is_back(ev.action) {
            state.saving = None;
        } else if is_start(ev.action) {
            if state.saving.as_ref().is_some_and(|d| !d.name.trim().is_empty()) {
                return EditResult::SaveRequested;
            }
        } else if matches!(ui_action(ev.action), Some(UiAction::Raise | UiAction::Lower)) {
            if let Some(d) = state.saving.as_mut() {
                d.set_default = !d.set_default;
            }
        }
        return EditResult::Handled;
    }

    let total = total_bars(state);
    if total == 0 {
        // Allow Back to leave even with nothing connected.
        if is_back(ev.action) {
            return EditResult::ExitToParent;
        }
        return EditResult::Handled;
    }

    if state.advanced.is_some() {
        if is_back(ev.action) {
            state.advanced = None;
            state.adv_sel = 0;
        } else {
            apply_advanced_edit(state, ev, fine);
        }
        return EditResult::Handled;
    }

    // Simple view.
    if is_back(ev.action) {
        return EditResult::ExitToParent;
    }
    if is_start(ev.action) {
        enter_advanced(state);
        return EditResult::Handled;
    }
    let step = if fine { 1 } else { THRESHOLD_STEP as i32 };
    match ui_action(ev.action) {
        Some(UiAction::PrevBar) => state.selected = (state.selected + total - 1) % total,
        Some(UiAction::NextBar) => state.selected = (state.selected + 1) % total,
        Some(UiAction::Raise) => adjust_simple_threshold(state, step),
        Some(UiAction::Lower) => adjust_simple_threshold(state, -step),
        None => {}
    }
    EditResult::Handled
}

/// Open the "save this pad as a profile" name-entry box (in-session overlay).
/// No-op in Advanced view, with no pads, or if already saving.
pub fn begin_save(state: &mut State) {
    if state.advanced.is_some() || state.pads.is_empty() || state.saving.is_some() {
        return;
    }
    state.saving = Some(SaveDraft::default());
}

pub fn is_saving(state: &State) -> bool {
    state.saving.is_some()
}

/// Whether the save box currently has a non-blank name (ready to confirm).
pub fn save_name_nonempty(state: &State) -> bool {
    state.saving.as_ref().is_some_and(|d| !d.name.trim().is_empty())
}

/// Take the confirmed save draft, clearing save mode.
pub fn take_save(state: &mut State) -> Option<SaveDraft> {
    state.saving.take()
}

/// Feed a raw key into the name-entry box. `backspace` removes the last char;
/// otherwise printable characters from `text` are appended. Returns whether
/// save mode is active (i.e. the event was consumed by the name box).
pub fn save_key_input(state: &mut State, backspace: bool, text: Option<&str>) -> bool {
    let Some(draft) = state.saving.as_mut() else {
        return false;
    };
    if backspace {
        draft.name.pop();
    } else if let Some(text) = text {
        for c in text.chars() {
            if (c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
                && draft.name.chars().count() < MAX_PROFILE_NAME_LEN
            {
                draft.name.push(c);
            }
        }
    }
    true
}

/// The pad device the cursor is currently on (for the overlay's save action).
pub fn selected_device(state: &State) -> Option<PadDeviceId> {
    let pad_idx = state.selected / PAD_BUTTON_COUNT;
    state.pads.get(pad_idx).map(|p| p.device_id)
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    });
    actors.extend(build_content(state, false));
    actors
}

/// Render the title, pads/messages, and instructions without a background.
/// `as_overlay` adjusts z-order and the footer hints for the Song Select overlay.
pub fn build_content(state: &State, as_overlay: bool) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(160);
    let theme = theme(state.active_color_index);
    // As an overlay we draw above Song Select; as a screen we start near 0.
    let zb = if as_overlay { 1450.0 } else { 0.0 };

    let advanced_pad = state.advanced.and_then(|dev| pad_index(state, dev));
    let title = if advanced_pad.is_some() {
        "CONFIGURE PADS  -  ADVANCED"
    } else {
        "CONFIGURE PADS"
    };
    actors.push(act!(text:
        font("miso"):
        settext(title):
        align(0.5, 0.0):
        xy(screen_center_x(), 36.0):
        zoom(1.2):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.95):
        z(20.0 + zb)
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
            z(20.0 + zb)
        ));
        actors.push(act!(text:
            font("miso"):
            settext("If you have FSR pads, enable \"Use FSRs\" in Options > Input."):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y() + 14.0):
            zoom(0.7):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.8):
            z(20.0 + zb)
        ));
        push_footer(&mut actors, Footer::Simple { as_overlay, advanced_available: false }, zb);
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
            z(20.0 + zb)
        ));
        push_footer(&mut actors, Footer::Simple { as_overlay, advanced_available: false }, zb);
        return actors;
    }

    if let Some(pad_idx) = advanced_pad {
        build_advanced(&mut actors, state, pad_idx, &theme, zb);
    } else {
        build_simple(&mut actors, state, &theme, as_overlay, zb);
    }
    if let Some(draft) = &state.saving {
        push_save_box(&mut actors, state, draft, zb);
    }
    actors
}

/// Modal name-entry box drawn over the Simple view while saving a pad profile.
fn push_save_box(actors: &mut Vec<Actor>, state: &State, draft: &SaveDraft, zb: f32) {
    let cx = screen_center_x();
    let cy = screen_center_y();
    // High z so it sits above everything (overlay base z is ~1450).
    let z = 60.0 + zb;
    // Dim + panel.
    push_quad(actors, cx, cy - 90.0, 520.0, 180.0, [0.0, 0.0, 0.0, 0.92], z);
    let pad_name = selected_device(state)
        .and_then(|dev| state.pads.iter().find(|p| p.device_id == dev))
        .map_or("", |p| p.device_name.as_str());
    actors.push(act!(text:
        font("miso"): settext(format!("Save {pad_name} as profile")): align(0.5, 0.5):
        xy(cx, cy - 64.0): zoom(0.85): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.95): z(z + 1.0)
    ));
    // Name with a blinking-ish caret (static underscore is fine).
    let shown = if draft.name.is_empty() {
        "_".to_owned()
    } else {
        format!("{}_", draft.name)
    };
    actors.push(act!(text:
        font("miso"): settext(shown): align(0.5, 0.5):
        xy(cx, cy - 30.0): zoom(1.0): horizalign(center):
        diffuse(SELECTED_TEXT[0], SELECTED_TEXT[1], SELECTED_TEXT[2], SELECTED_TEXT[3]): z(z + 1.0)
    ));
    let (def_label, def_color) = if draft.set_default {
        ("Set as default: ON", ON_TEXT)
    } else {
        ("Set as default: off", OFF_TEXT)
    };
    actors.push(act!(text:
        font("miso"): settext(def_label.to_owned()): align(0.5, 0.5):
        xy(cx, cy): zoom(0.7): horizalign(center):
        diffuse(def_color[0], def_color[1], def_color[2], def_color[3]): z(z + 1.0)
    ));
    actors.push(act!(text:
        font("miso"): settext("Up/Down - toggle default".to_owned()): align(0.5, 0.5):
        xy(cx, cy + 28.0): zoom(0.62): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.8): z(z + 1.0)
    ));
    actors.push(act!(text:
        font("miso"): settext("Press &START; to save, &BACK; to cancel".to_owned()): align(0.5, 0.5):
        xy(cx, cy + 50.0): zoom(0.62): horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.8): z(z + 1.0)
    ));
}

// ─── Simple view ─────────────────────────────────────────────────────────────

fn build_simple(actors: &mut Vec<Actor>, state: &State, theme: &Theme, as_overlay: bool, zb: f32) {
    let group_w = BAR_WIDTH * PAD_BUTTON_COUNT as f32 + BAR_GAP * (PAD_BUTTON_COUNT - 1) as f32;
    let panel_w = group_w + 34.0;
    let total_w = panel_w * state.pads.len() as f32 + PAD_GAP * (state.pads.len() - 1) as f32;
    let panel_h = BAR_HEIGHT + 140.0;
    // Nudge up so the 4-line footer ("...Select Panel") never clips the boxes.
    let top_y = screen_center_y() - panel_h * 0.5 - 14.0;
    let mut panel_cx = screen_center_x() - total_w * 0.5 + panel_w * 0.5;

    for (pad_idx, pad) in state.pads.iter().enumerate() {
        push_frame(actors, panel_cx, top_y, panel_w, panel_h, theme.frame, 10.0 + zb);
        actors.push(act!(text:
            font("miso"):
            settext(pad.device_name.clone()):
            align(0.5, 0.0):
            xy(panel_cx, top_y + 14.0):
            zoom(0.82):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.9):
            z(12.0 + zb)
        ));

        let track_y = top_y + 84.0;
        let left = panel_cx - group_w * 0.5 + BAR_WIDTH * 0.5;
        for (btn_idx, button) in pad.buttons.iter().enumerate() {
            let x = left + btn_idx as f32 * (BAR_WIDTH + BAR_GAP);
            let selected = state.selected == pad_idx * PAD_BUTTON_COUNT + btn_idx;
            let scale = button.value_scale;
            // A pending whole-button edit shows a single value; otherwise show the
            // live per-sensor range ("200-230") so Advanced edits are visible here.
            let (threshold_label, threshold_norm) =
                if let Some(v) = pending_simple_threshold(state, pad.device_id, btn_idx) {
                    (v.to_string(), normalize(v, scale))
                } else {
                    let (mn, mx) = sensor_threshold_range(button);
                    let label = if mn == mx {
                        mx.to_string()
                    } else {
                        format!("{mn}-{mx}")
                    };
                    (label, normalize(mx, scale))
                };
            if pad.simple_per_sensor_bars {
                // Load cells: show all four corner readings (numbered) sharing one threshold.
                push_value_cluster(
                    actors,
                    x,
                    track_y,
                    button.label,
                    &button.sensors,
                    scale,
                    threshold_label,
                    threshold_norm,
                    button.active,
                    theme,
                    selected,
                    11.0 + zb,
                );
            } else {
                // A panel with every sensor disabled in Advanced reads as off.
                let disabled =
                    !button.sensors.is_empty() && button.sensors.iter().all(|s| !s.enabled);
                push_bar(
                    actors,
                    button.label,
                    button.aggregate_value,
                    normalize(button.aggregate_value, scale),
                    threshold_label,
                    threshold_norm,
                    button.active,
                    disabled,
                    x,
                    track_y,
                    theme,
                    selected,
                    11.0 + zb,
                );
            }
        }
        panel_cx += panel_w + PAD_GAP;
    }

    let selected_pad = state.pads.get(state.selected / PAD_BUTTON_COUNT);
    let advanced_available = selected_pad.is_some_and(|p| p.supports_advanced);
    push_footer(
        actors,
        Footer::Simple {
            as_overlay,
            advanced_available,
        },
        zb,
    );
}

// ─── Advanced view ───────────────────────────────────────────────────────────

fn build_advanced(actors: &mut Vec<Actor>, state: &State, pad_idx: usize, theme: &Theme, zb: f32) {
    let pad = &state.pads[pad_idx];
    let device = pad.device_id;
    let targets = advanced_targets(state);
    let focused = targets.get(state.adv_sel).copied();

    actors.push(act!(text:
        font("miso"):
        settext(pad.device_name.clone()):
        align(0.5, 0.0):
        xy(screen_center_x(), 66.0):
        zoom(0.9):
        horizalign(center):
        diffuse(1.0, 1.0, 1.0, 0.92):
        z(20.0 + zb)
    ));

    // Total width across the (variable-width) button groups.
    let group_widths: Vec<f32> = pad
        .buttons
        .iter()
        .map(|b| group_width(b.sensors.len()))
        .collect();
    let total_w: f32 =
        group_widths.iter().sum::<f32>() + ADV_GROUP_GAP * (PAD_BUTTON_COUNT - 1) as f32;
    let mut group_left = screen_center_x() - total_w * 0.5;
    let top_y = ADV_TOP_Y;

    for (btn_idx, button) in pad.buttons.iter().enumerate() {
        let gw = group_widths[btn_idx];
        let group_cx = group_left + gw * 0.5;
        // Button label above its sensor group.
        actors.push(act!(text:
            font("miso"):
            settext(button.label.to_string()):
            align(0.5, 1.0):
            xy(group_cx, top_y - 22.0):
            zoom(0.95):
            horizalign(center):
            diffuse(1.0, 1.0, 1.0, 0.95):
            z(22.0 + zb)
        ));

        for (s, sensor) in button.sensors.iter().enumerate() {
            let x = group_left + ADV_BAR_W * 0.5 + s as f32 * (ADV_BAR_W + ADV_BAR_GAP);
            let fw = sensor.firmware_index;
            let threshold =
                current_sensor_threshold(state, device, btn_idx, fw).unwrap_or(sensor.raw_threshold);
            let enabled = current_sensor_enabled(state, device, btn_idx, fw, sensor.enabled);
            let is_focused = focused == Some(AdvTarget::Sensor { button: btn_idx, sensor: s });
            let bar_label = sensor.label.map_or_else(|| (s + 1).to_string(), str::to_owned);
            push_sensor_bar(
                actors,
                x,
                top_y,
                bar_label,
                sensor.value_norm,
                sensor.active && enabled,
                threshold,
                normalize(threshold, button.max_raw_threshold),
                enabled,
                pad.supports_sensor_toggle,
                is_focused,
                theme,
                23.0 + zb,
            );
        }
        group_left += gw + ADV_GROUP_GAP;
    }

    // Extra Advanced section (pad-level), anchored just above the footer so it
    // doesn't leave a big gap under the sensor grid.
    let mut ey = screen_height() - 150.0;
    if pad.auto_recalibration.is_some() || pad.debounce_micros.is_some() {
        actors.push(act!(text:
            font("miso"):
            settext("Extra Advanced - only change these if you know what they do"):
            align(0.5, 0.5):
            xy(screen_center_x(), ey):
            zoom(0.62):
            horizalign(center):
            diffuse(CAUTION_TEXT[0], CAUTION_TEXT[1], CAUTION_TEXT[2], CAUTION_TEXT[3]):
            z(22.0 + zb)
        ));
        ey += 26.0;
    }
    if pad.auto_recalibration.is_some() {
        let on = current_auto_recal(state, device, pad.auto_recalibration.unwrap_or(true));
        let focused_here = focused == Some(AdvTarget::AutoRecal);
        push_setting_row(actors, "Auto-recalibration", if on { "ON" } else { "OFF" }, on, focused_here, ey, zb);
        ey += 24.0;
    }
    if pad.debounce_micros.is_some() {
        let us = current_debounce(state, device, pad.debounce_micros.unwrap_or(DEBOUNCE_DEFAULT_US));
        let focused_here = focused == Some(AdvTarget::Debounce);
        push_setting_row(actors, "Debounce", &format_ms(us), true, focused_here, ey, zb);
    }

    push_footer(actors, Footer::Advanced { supports_toggle: pad.supports_sensor_toggle }, zb);
}

#[allow(clippy::too_many_arguments)]
fn push_sensor_bar(
    actors: &mut Vec<Actor>,
    x: f32,
    y: f32,
    sensor_label: String,
    value_norm: f32,
    active: bool,
    raw_threshold: u16,
    threshold_norm: f32,
    enabled: bool,
    supports_toggle: bool,
    selected: bool,
    theme: &Theme,
    z: f32,
) {
    let value_norm = value_norm.clamp(0.0, 1.0);
    let threshold_norm = threshold_norm.clamp(0.0, 1.0);

    push_quad(actors, x, y, ADV_BAR_W, ADV_BAR_HEIGHT, TRACK_COLOR, z);

    let fill_h = value_norm * ADV_BAR_HEIGHT;
    if fill_h > 0.0 {
        let mut fill = if active { ACTIVE_FILL } else { theme.fill_idle };
        if !enabled {
            fill[3] *= 0.35; // dim a disabled sensor's fill
        }
        push_quad(actors, x, y + ADV_BAR_HEIGHT - fill_h, ADV_BAR_W, fill_h, fill, z + 1.0);
    }

    let threshold_h = 2.0_f32;
    let threshold_y = y + (1.0 - threshold_norm) * ADV_BAR_HEIGHT - threshold_h * 0.5;
    push_quad(actors, x, threshold_y, ADV_BAR_W, threshold_h, THRESHOLD_COLOR, z + 2.0);

    if selected {
        let ox = x - (ADV_BAR_W + 8.0) * 0.5;
        let oy = y - 16.0;
        let ow = ADV_BAR_W + 8.0;
        let oh = ADV_BAR_HEIGHT + 44.0;
        let t = 2.0_f32;
        let o = [1.0, 1.0, 1.0, 1.0];
        push_quad(actors, x, oy, ow, t, o, z + 2.5);
        push_quad(actors, x, oy + oh - t, ow, t, o, z + 2.5);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(ox, oy): zoomto(t, oh): diffuse(o[0], o[1], o[2], o[3]): z(z + 2.5)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(ox + ow - t, oy): zoomto(t, oh): diffuse(o[0], o[1], o[2], o[3]): z(z + 2.5)
        ));
    }

    let text_color = if selected { SELECTED_TEXT } else { [1.0, 1.0, 1.0, 0.95] };
    // Threshold value above the bar.
    actors.push(act!(text:
        font("miso"): settext(raw_threshold.to_string()): align(0.5, 1.0):
        xy(x, y - 2.0): zoom(0.5): horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
    ));
    // Sensor identifier (edge label, or 1-based number) directly below the bar.
    actors.push(act!(text:
        font("miso"): settext(sensor_label): align(0.5, 0.0):
        xy(x, y + ADV_BAR_HEIGHT + 4.0): zoom(0.5): horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
    ));
    // Enable indicator under the identifier (only where the backend supports it).
    if supports_toggle {
        let (label, c) = if enabled { ("ON", ON_TEXT) } else { ("off", OFF_TEXT) };
        actors.push(act!(text:
            font("miso"): settext(label.to_string()): align(0.5, 0.0):
            xy(x, y + ADV_BAR_HEIGHT + 17.0): zoom(0.46): horizalign(center):
            diffuse(c[0], c[1], c[2], c[3]): z(z + 3.0)
        ));
    }
}

fn push_setting_row(
    actors: &mut Vec<Actor>,
    label: &str,
    value: &str,
    value_on: bool,
    selected: bool,
    y: f32,
    zb: f32,
) {
    let cx = screen_center_x();
    let label_color = if selected { SELECTED_TEXT } else { [1.0, 1.0, 1.0, 0.92] };
    actors.push(act!(text:
        font("miso"): settext(format!("{label}:")): align(1.0, 0.5):
        xy(cx - 8.0, y): zoom(0.75): horizalign(right):
        diffuse(label_color[0], label_color[1], label_color[2], label_color[3]): z(22.0 + zb)
    ));
    let vc = if !value_on {
        OFF_TEXT
    } else if selected {
        SELECTED_TEXT
    } else {
        ON_TEXT
    };
    actors.push(act!(text:
        font("miso"): settext(value.to_string()): align(0.0, 0.5):
        xy(cx + 8.0, y): zoom(0.75): horizalign(left):
        diffuse(vc[0], vc[1], vc[2], vc[3]): z(22.0 + zb)
    ));
}

// ─── Edit logic ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum UiAction {
    PrevBar,
    NextBar,
    Raise,
    Lower,
}

/// One focusable item in the Advanced view, traversed by Left/Right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdvTarget {
    Sensor { button: usize, sensor: usize },
    AutoRecal,
    Debounce,
}

fn enter_advanced(state: &mut State) {
    let pad_idx = state.selected / PAD_BUTTON_COUNT;
    if let Some(pad) = state.pads.get(pad_idx) {
        // Load-cell pads are Simple-only.
        if !pad.supports_advanced {
            return;
        }
        state.advanced = Some(pad.device_id);
        state.adv_sel = 0;
    }
}

/// Build the Advanced focus list (column-major: a button's sensors, then the
/// next button, then the pad-level Extra Advanced controls).
fn advanced_targets(state: &State) -> Vec<AdvTarget> {
    let Some(dev) = state.advanced else {
        return Vec::new();
    };
    let Some(idx) = pad_index(state, dev) else {
        return Vec::new();
    };
    let pad = &state.pads[idx];
    let mut targets = Vec::with_capacity(18);
    for (b, button) in pad.buttons.iter().enumerate() {
        for s in 0..button.sensors.len() {
            targets.push(AdvTarget::Sensor { button: b, sensor: s });
        }
    }
    if pad.auto_recalibration.is_some() {
        targets.push(AdvTarget::AutoRecal);
    }
    if pad.debounce_micros.is_some() {
        targets.push(AdvTarget::Debounce);
    }
    targets
}

fn apply_advanced_edit(state: &mut State, ev: &InputEvent, fine: bool) {
    let targets = advanced_targets(state);
    if targets.is_empty() {
        return;
    }
    if state.adv_sel >= targets.len() {
        state.adv_sel = targets.len() - 1;
    }
    let Some(dev) = state.advanced else { return };

    if is_start(ev.action) {
        toggle_focused(state, dev, targets[state.adv_sel]);
        return;
    }
    match ui_action(ev.action) {
        Some(UiAction::PrevBar) => {
            state.adv_sel = (state.adv_sel + targets.len() - 1) % targets.len();
        }
        Some(UiAction::NextBar) => {
            state.adv_sel = (state.adv_sel + 1) % targets.len();
        }
        Some(UiAction::Raise) => edit_focused(state, dev, targets[state.adv_sel], true, fine),
        Some(UiAction::Lower) => edit_focused(state, dev, targets[state.adv_sel], false, fine),
        None => {}
    }
}

fn edit_focused(state: &mut State, dev: PadDeviceId, target: AdvTarget, up: bool, fine: bool) {
    match target {
        AdvTarget::Sensor { button, sensor } => {
            let step = if fine { 1 } else { THRESHOLD_STEP as i32 };
            adjust_sensor_threshold(state, dev, button, sensor, if up { step } else { -step });
        }
        AdvTarget::AutoRecal => {
            queue_unique(state, PadCommand::AutoRecalibration { device: dev, enabled: up });
        }
        AdvTarget::Debounce => {
            let Some(pad) = pad_by_device(state, dev) else { return };
            let live = pad.debounce_micros.unwrap_or(DEBOUNCE_DEFAULT_US);
            let current = current_debounce(state, dev, live);
            let step = if fine { DEBOUNCE_FINE_US } else { DEBOUNCE_STEP_US } as i32;
            let next = (i32::from(current) + if up { step } else { -step })
                .clamp(i32::from(DEBOUNCE_MIN_US), i32::from(DEBOUNCE_MAX_US)) as u16;
            if next != current {
                queue_unique(state, PadCommand::Debounce { device: dev, micros: next });
            }
        }
    }
}

fn toggle_focused(state: &mut State, dev: PadDeviceId, target: AdvTarget) {
    match target {
        AdvTarget::Sensor { button, sensor: disp } => {
            let Some(pad) = pad_by_device(state, dev) else { return };
            if !pad.supports_sensor_toggle {
                return;
            }
            let Some(sv) = pad.buttons.get(button).and_then(|b| b.sensors.get(disp)) else {
                return;
            };
            let fw = sv.firmware_index;
            let current = current_sensor_enabled(state, dev, button, fw, sv.enabled);
            queue_unique(
                state,
                PadCommand::SensorEnabled {
                    device: dev,
                    button,
                    sensor: fw,
                    enabled: !current,
                },
            );
        }
        AdvTarget::AutoRecal => {
            let Some(pad) = pad_by_device(state, dev) else { return };
            let live = pad.auto_recalibration.unwrap_or(true);
            let current = current_auto_recal(state, dev, live);
            queue_unique(
                state,
                PadCommand::AutoRecalibration {
                    device: dev,
                    enabled: !current,
                },
            );
        }
        AdvTarget::Debounce => {}
    }
}

fn adjust_simple_threshold(state: &mut State, delta: i32) {
    let pad_idx = state.selected / PAD_BUTTON_COUNT;
    let button = state.selected % PAD_BUTTON_COUNT;
    let Some(pad) = state.pads.get(pad_idx) else {
        return;
    };
    let bar = &pad.buttons[button];
    let device = pad.device_id;
    let current = pending_simple_threshold(state, device, button).unwrap_or(bar.aggregate_threshold);
    let next = (i32::from(current) + delta).clamp(
        i32::from(bar.min_raw_threshold),
        i32::from(bar.max_raw_threshold),
    ) as u16;
    if next == current {
        return;
    }
    queue_unique(
        state,
        PadCommand::Threshold {
            device,
            button,
            sensor: None,
            value: next,
        },
    );
}

fn adjust_sensor_threshold(
    state: &mut State,
    dev: PadDeviceId,
    button: usize,
    disp: usize,
    delta: i32,
) {
    let Some(pad) = pad_by_device(state, dev) else {
        return;
    };
    let Some(bar) = pad.buttons.get(button) else {
        return;
    };
    let (min, max) = (bar.min_raw_threshold, bar.max_raw_threshold);
    let Some(sv) = bar.sensors.get(disp) else {
        return;
    };
    let fw = sv.firmware_index;
    let live = sv.raw_threshold;
    let current = current_sensor_threshold(state, dev, button, fw).unwrap_or(live);
    let next = (i32::from(current) + delta).clamp(i32::from(min), i32::from(max)) as u16;
    if next == current {
        return;
    }
    queue_unique(
        state,
        PadCommand::Threshold {
            device: dev,
            button,
            sensor: Some(fw),
            value: next,
        },
    );
}

/// Replace any queued command of the same kind/target so a value can't pile up
/// multiple times in one frame, then push the new one.
fn queue_unique(state: &mut State, cmd: PadCommand) {
    state.pending.retain(|c| !same_target(c, &cmd));
    state.pending.push(cmd);
}

fn same_target(a: &PadCommand, b: &PadCommand) -> bool {
    use PadCommand::*;
    match (a, b) {
        (
            Threshold { device: d1, button: b1, sensor: s1, .. },
            Threshold { device: d2, button: b2, sensor: s2, .. },
        ) => d1 == d2 && b1 == b2 && s1 == s2,
        (
            SensorEnabled { device: d1, button: b1, sensor: s1, .. },
            SensorEnabled { device: d2, button: b2, sensor: s2, .. },
        ) => d1 == d2 && b1 == b2 && s1 == s2,
        (AutoRecalibration { device: d1, .. }, AutoRecalibration { device: d2, .. }) => d1 == d2,
        (Debounce { device: d1, .. }, Debounce { device: d2, .. }) => d1 == d2,
        _ => false,
    }
}

// ─── Pending / live value lookups ──────────────────────────────────────────────

/// Most recently queued Simple-mode (whole-button) threshold for a pad+button.
fn pending_simple_threshold(state: &State, device: PadDeviceId, button: usize) -> Option<u16> {
    state.pending.iter().rev().find_map(|c| match *c {
        PadCommand::Threshold { device: d, button: b, sensor: None, value }
            if d == device && b == button =>
        {
            Some(value)
        }
        _ => None,
    })
}

/// Queued per-sensor threshold, if any.
fn current_sensor_threshold(
    state: &State,
    device: PadDeviceId,
    button: usize,
    sensor: usize,
) -> Option<u16> {
    state.pending.iter().rev().find_map(|c| match *c {
        PadCommand::Threshold { device: d, button: b, sensor: Some(s), value }
            if d == device && b == button && s == sensor =>
        {
            Some(value)
        }
        _ => None,
    })
}

fn current_sensor_enabled(
    state: &State,
    device: PadDeviceId,
    button: usize,
    sensor: usize,
    live: bool,
) -> bool {
    state
        .pending
        .iter()
        .rev()
        .find_map(|c| match *c {
            PadCommand::SensorEnabled { device: d, button: b, sensor: s, enabled }
                if d == device && b == button && s == sensor =>
            {
                Some(enabled)
            }
            _ => None,
        })
        .unwrap_or(live)
}

fn current_auto_recal(state: &State, device: PadDeviceId, live: bool) -> bool {
    state
        .pending
        .iter()
        .rev()
        .find_map(|c| match *c {
            PadCommand::AutoRecalibration { device: d, enabled } if d == device => Some(enabled),
            _ => None,
        })
        .unwrap_or(live)
}

fn current_debounce(state: &State, device: PadDeviceId, live: u16) -> u16 {
    state
        .pending
        .iter()
        .rev()
        .find_map(|c| match *c {
            PadCommand::Debounce { device: d, micros } if d == device => Some(micros),
            _ => None,
        })
        .unwrap_or(live)
}

fn pad_index(state: &State, device: PadDeviceId) -> Option<usize> {
    state.pads.iter().position(|p| p.device_id == device)
}

fn pad_by_device(state: &State, device: PadDeviceId) -> Option<&PadView> {
    state.pads.iter().find(|p| p.device_id == device)
}

fn total_bars(state: &State) -> usize {
    state.pads.len() * PAD_BUTTON_COUNT
}

/// Min/max live threshold across a button's sensors (for the Simple-view
/// range display). Empty buttons report `(0, 0)`.
fn sensor_threshold_range(button: &crate::engine::input::fsr::ButtonView) -> (u16, u16) {
    let mut mn = u16::MAX;
    let mut mx = 0u16;
    for s in &button.sensors {
        mn = mn.min(s.raw_threshold);
        mx = mx.max(s.raw_threshold);
    }
    if button.sensors.is_empty() {
        return (0, 0);
    }
    (mn, mx)
}

fn group_width(sensors: usize) -> f32 {
    if sensors == 0 {
        return ADV_BAR_W;
    }
    sensors as f32 * ADV_BAR_W + (sensors - 1) as f32 * ADV_BAR_GAP
}

fn format_ms(micros: u16) -> String {
    format!("{:.1} ms", micros as f32 / 1000.0)
}

fn normalize(value: u16, max: u16) -> f32 {
    if max == 0 {
        return 0.0;
    }
    (value as f32 / max as f32).clamp(0.0, 1.0)
}

// ─── Input mapping ─────────────────────────────────────────────────────────────

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

const fn is_back(act: VirtualAction) -> bool {
    matches!(act, VirtualAction::p1_back | VirtualAction::p2_back)
}

const fn is_start(act: VirtualAction) -> bool {
    matches!(act, VirtualAction::p1_start | VirtualAction::p2_start)
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

/// Controls allowed from a gamepad source (so a pad's menu buttons work but
/// stepping on its panels doesn't move the cursor).
const fn is_menu_control(act: VirtualAction) -> bool {
    is_dedicated_menu_action(act) || is_back(act) || is_start(act)
}

// ─── Footer ──────────────────────────────────────────────────────────────────

enum Footer {
    Simple {
        as_overlay: bool,
        advanced_available: bool,
    },
    Advanced { supports_toggle: bool },
}

fn push_footer(actors: &mut Vec<Actor>, footer: Footer, zb: f32) {
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
            z(20.0 + zb)
        ));
    };
    match footer {
        Footer::Simple {
            as_overlay,
            advanced_available,
        } => {
            line(actors, "Left/Right - Select Panel".to_owned(), bottom - 94.0);
            line(
                actors,
                format!("Up/Down - Threshold +/- {THRESHOLD_STEP} (Shift +/- 1)"),
                bottom - 70.0,
            );
            // In-session overlay: combine Advanced + Save on one line (Save uses
            // the session's player/profile context). Standalone screen: just
            // Advanced (no profile to save to).
            let action_line = match (as_overlay, advanced_available) {
                (true, true) => Some("&START; Advanced    &SELECT; Save profile".to_owned()),
                (true, false) => Some("Press &SELECT; to save this pad as a profile".to_owned()),
                (false, true) => Some("Press &START; for Advanced (per-sensor)".to_owned()),
                (false, false) => None,
            };
            if let Some(action_line) = action_line {
                line(actors, action_line, bottom - 46.0);
            }
            let back = if as_overlay {
                "Press &BACK; to return to Song Select"
            } else {
                "Press &BACK; to return to Options"
            };
            line(actors, back.to_owned(), bottom - 22.0);
        }
        Footer::Advanced { supports_toggle } => {
            line(actors, "Left/Right - Select   Up/Down - Adjust (Shift = fine)".to_owned(), bottom - 70.0);
            if supports_toggle {
                line(
                    actors,
                    "Press &START; to toggle the selected sensor on/off".to_owned(),
                    bottom - 46.0,
                );
            }
            line(
                actors,
                "Press &BACK; to return to the simple view".to_owned(),
                bottom - 22.0,
            );
        }
    }
}

// ─── Shared drawing ────────────────────────────────────────────────────────────

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

/// Simple-view renderer for load-cell panels: draws each corner sensor as a
/// thin value bar (numbered 1-N) sharing one panel threshold line, inside the
/// same slot a single Simple bar would occupy.
#[allow(clippy::too_many_arguments)]
fn push_value_cluster(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    label: &str,
    sensors: &[SensorView],
    value_scale: u16,
    threshold_label: String,
    threshold_norm: f32,
    button_active: bool,
    theme: &Theme,
    selected: bool,
    z: f32,
) {
    let threshold_norm = threshold_norm.clamp(0.0, 1.0);
    let n = sensors.len().max(1);
    let thin_w = 9.0_f32;
    let gap = 3.0_f32;
    let total = n as f32 * thin_w + (n - 1) as f32 * gap;
    let start_left = x_center - total * 0.5;

    for (i, sensor) in sensors.iter().enumerate() {
        let bx = start_left + thin_w * 0.5 + i as f32 * (thin_w + gap);
        push_quad(actors, bx, y, thin_w, BAR_HEIGHT, TRACK_COLOR, z);
        let vn = normalize(sensor.raw_value, value_scale);
        let fill_h = vn * BAR_HEIGHT;
        if fill_h > 0.0 {
            let fill = if sensor.active { ACTIVE_FILL } else { theme.fill_idle };
            push_quad(actors, bx, y + BAR_HEIGHT - fill_h, thin_w, fill_h, fill, z + 1.0);
        }
        // Sensor number (1-based) below its bar.
        let nc = if selected { SELECTED_TEXT } else { [1.0, 1.0, 1.0, 0.9] };
        actors.push(act!(text:
            font("miso"): settext((i + 1).to_string()): align(0.5, 0.0):
            xy(bx, y + BAR_HEIGHT + 6.0): zoom(0.5): horizalign(center):
            diffuse(nc[0], nc[1], nc[2], nc[3]): z(z + 3.0)
        ));
    }

    // One shared threshold line across the whole cluster.
    let threshold_h = 3.0_f32;
    let threshold_y = y + (1.0 - threshold_norm) * BAR_HEIGHT - threshold_h * 0.5;
    push_quad(actors, x_center, threshold_y, BAR_WIDTH, threshold_h, THRESHOLD_COLOR, z + 2.0);

    if selected {
        let ox = x_center - (BAR_WIDTH + 12.0) * 0.5;
        let oy = y - 34.0;
        let ow = BAR_WIDTH + 12.0;
        let oh = BAR_HEIGHT + 70.0;
        let t = 2.0_f32;
        let o = [1.0, 1.0, 1.0, 1.0];
        push_quad(actors, x_center, oy, ow, t, o, z + 2.5);
        push_quad(actors, x_center, oy + oh - t, ow, t, o, z + 2.5);
        actors.push(act!(quad:
            align(0.0, 0.0): xy(ox, oy): zoomto(t, oh): diffuse(o[0], o[1], o[2], o[3]): z(z + 2.5)
        ));
        actors.push(act!(quad:
            align(0.0, 0.0): xy(ox + ow - t, oy): zoomto(t, oh): diffuse(o[0], o[1], o[2], o[3]): z(z + 2.5)
        ));
    }

    let text_color = if selected { SELECTED_TEXT } else { [1.0, 1.0, 1.0, 0.95] };
    // Shared threshold value above the line.
    actors.push(act!(text:
        font("miso"): settext(threshold_label): align(0.5, 1.0):
        xy(x_center, threshold_y - 3.0): zoom(0.68): horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
    ));
    // Button label below the cluster.
    let label_color = if button_active { ACTIVE_FILL } else { text_color };
    actors.push(act!(text:
        font("miso"): settext(label.to_string()): align(0.5, 0.0):
        xy(x_center, y + BAR_HEIGHT + 20.0): zoom(1.0): horizalign(center):
        diffuse(label_color[0], label_color[1], label_color[2], label_color[3]): z(z + 3.0)
    ));
}

#[allow(clippy::too_many_arguments)]
fn push_bar(
    actors: &mut Vec<Actor>,
    label: &str,
    raw_value: u16,
    value_norm: f32,
    threshold_label: String,
    threshold_norm: f32,
    active: bool,
    disabled: bool,
    x: f32,
    y: f32,
    theme: &Theme,
    selected: bool,
    z: f32,
) {
    let value_norm = value_norm.clamp(0.0, 1.0);
    let threshold_norm = threshold_norm.clamp(0.0, 1.0);
    let text_color = if selected {
        SELECTED_TEXT
    } else {
        [1.0, 1.0, 1.0, 0.95]
    };

    // Single muted track background.
    push_quad(actors, x, y, BAR_WIDTH, BAR_HEIGHT, TRACK_COLOR, z);

    if selected {
        let ox = x - (BAR_WIDTH + 12.0) * 0.5;
        let oy = y - 42.0;
        let ow = BAR_WIDTH + 12.0;
        let oh = BAR_HEIGHT + 78.0;
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

    // Whole panel disabled (every sensor turned off in Advanced): just say so.
    if disabled {
        let off = if selected { SELECTED_TEXT } else { CAUTION_TEXT };
        actors.push(act!(text:
            font("miso"): settext("OFF"): align(0.5, 0.5):
            xy(x, y + BAR_HEIGHT * 0.5): zoom(0.82): horizalign(center):
            diffuse(off[0], off[1], off[2], off[3]): z(z + 3.0)
        ));
        actors.push(act!(text:
            font("miso"): settext(label.to_string()): align(0.5, 0.0):
            xy(x, y + BAR_HEIGHT + 8.0): zoom(1.0): horizalign(center):
            diffuse(0.6, 0.6, 0.65, 0.9): z(z + 3.0)
        ));
        return;
    }

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

    // Current pressure value, kept high above the bar so a near-max threshold
    // number doesn't clip into it.
    actors.push(act!(text:
        font("miso"): settext(raw_value.to_string()): align(0.5, 1.0):
        xy(x, y - 20.0): zoom(0.92): horizalign(center):
        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
    ));
    // Threshold number sits above its line, except near the top where it would
    // collide with the pressure value — then drop it just below the line.
    if threshold_norm > 0.9 {
        actors.push(act!(text:
            font("miso"): settext(threshold_label): align(0.5, 0.0):
            xy(x, threshold_y + threshold_h + 2.0): zoom(0.68): horizalign(center):
            diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
        ));
    } else {
        actors.push(act!(text:
            font("miso"): settext(threshold_label): align(0.5, 1.0):
            xy(x, threshold_y - 3.0): zoom(0.68): horizalign(center):
            diffuse(text_color[0], text_color[1], text_color[2], text_color[3]): z(z + 3.0)
        ));
    }
    let label_color = if active { ACTIVE_FILL } else { text_color };
    actors.push(act!(text:
        font("miso"): settext(label.to_string()): align(0.5, 0.0):
        xy(x, y + BAR_HEIGHT + 8.0): zoom(1.0): horizalign(center):
        diffuse(label_color[0], label_color[1], label_color[2], label_color[3]): z(z + 3.0)
    ));
}
