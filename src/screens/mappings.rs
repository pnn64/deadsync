use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{
    GamepadCodeBinding, InputBinding, InputEvent, InputSource, PadEvent, VirtualAction, get_keymap,
};
use crate::core::space::{screen_width, screen_height, widescale};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::components::{heart_bg, screen_bar};
use crate::ui::font;
use std::time::{Duration, Instant};
use winit::event::ElementState;
use winit::event::KeyEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

/* --------------------------- layout constants ---------------------------- */
/// Bars in `screen_bar.rs` use 32.0 px height.
const BAR_H: f32 = 32.0;

/// Screen-space margins (pixels, not scaled)
const LEFT_MARGIN_PX: f32 = 33.0;
const RIGHT_MARGIN_PX: f32 = 25.0;
const FIRST_ROW_TOP_MARGIN_PX: f32 = 18.0;
const BOTTOM_MARGIN_PX: f32 = 0.0;

/// Unscaled spec constants (we’ll uniformly scale).
const VISIBLE_ROWS: usize = 10;
const ROW_H: f32 = 33.0;
const ROW_GAP: f32 = 2.5;

/// Base widths (unscaled) for our custom layout.
const SIDE_W_BASE: f32 = 260.0;
const DESC_W_BASE: f32 = 260.0;
const SIDE_GAP_BASE: f32 = 35.0;
/// Extra vertical padding so the mappings table sits well below the top screen bar.
const TABLE_TOP_EXTRA_PX: f32 = 32.0;
/// Vertical offset (in px, unscaled) from the first row to the column headers
/// ("Primary / Secondary / Default"). Smaller values move these closer to the
/// items table; larger values push them upward toward the top bar.
const COLUMN_HEADER_OFFSET_PX: f32 = 7.0;
/// Vertical gap (in px, unscaled) between the Player labels ("Player 1/2")
/// and the column headers beneath them. Adjust this to move the Player labels
/// up or down together relative to the column header row.
const PLAYER_HEADER_GAP_PX: f32 = 19.0;

const DESC_BODY_ZOOM: f32 = 1.0;

/// Cursor tween duration for vertical movement.
const CURSOR_TWEEN_SECONDS: f32 = 0.1;

/// Spacing between inline items (for cursor ring sizing).
const INLINE_SPACING: f32 = 15.75;

/// Physical keys that are considered "default" and are not
/// accepted as candidates when capturing a new mapping.
const DEFAULT_PROTECTED_KEYS: &[KeyCode] = &[
    // P1 defaults (arrows + Enter/Escape)
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Enter,
    KeyCode::Escape,
    // P2 defaults (numpad directions + Start)
    KeyCode::Numpad8,
    KeyCode::Numpad2,
    KeyCode::Numpad4,
    KeyCode::Numpad6,
    KeyCode::NumpadEnter,
];

/// Logical mapping rows we expose in this prototype.
const NUM_MAPPING_ROWS: usize = 18;
const MAPPING_LABELS: [&str; NUM_MAPPING_ROWS] = [
    "MenuLeft",
    "MenuRight",
    "MenuUp",
    "MenuDown",
    "Start",
    "Select",
    "Back",
    "Restart",
    "Insert Coin",
    "Operator",
    "EffectUp",
    "EffectDown",
    "Left",
    "Right",
    "Up",
    "Down",
    "UpLeft",
    "UpRight",
];

/// Map each visual row to the underlying virtual actions for P1/P2.
#[inline(always)]
const fn row_actions(row_idx: usize) -> (Option<VirtualAction>, Option<VirtualAction>) {
    use VirtualAction::{p1_menu_left, p2_menu_left, p1_menu_right, p2_menu_right, p1_menu_up, p2_menu_up, p1_menu_down, p2_menu_down, p1_start, p2_start, p1_select, p2_select, p1_back, p2_back, p1_restart, p2_restart, p1_operator, p2_operator, p1_left, p2_left, p1_right, p2_right, p1_up, p2_up, p1_down, p2_down};
    match row_idx {
        // Menu navigation
        0 => (Some(p1_menu_left), Some(p2_menu_left)),
        1 => (Some(p1_menu_right), Some(p2_menu_right)),
        2 => (Some(p1_menu_up), Some(p2_menu_up)),
        3 => (Some(p1_menu_down), Some(p2_menu_down)),
        // System buttons
        4 => (Some(p1_start), Some(p2_start)),
        5 => (Some(p1_select), Some(p2_select)),
        6 => (Some(p1_back), Some(p2_back)),
        7 => (Some(p1_restart), Some(p2_restart)),
        // Insert Coin currently global-only; no per-player virtual action yet.
        8 => (None, None),
        // Operator
        9 => (Some(p1_operator), Some(p2_operator)),
        // EffectUp/EffectDown, UpLeft/UpRight reserved for future expansion.
        10 => (None, None),
        11 => (None, None),
        // Gameplay directions
        12 => (Some(p1_left), Some(p2_left)),
        13 => (Some(p1_right), Some(p2_right)),
        14 => (Some(p1_up), Some(p2_up)),
        15 => (Some(p1_down), Some(p2_down)),
        16 => (None, None),
        17 => (None, None),
        _ => (None, None),
    }
}

#[inline(always)]
fn ease_out_cubic(t: f32) -> f32 {
    let clamped = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    let u = 1.0 - clamped;
    (u * u).mul_add(-u, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
}

/// Which slot (player + primary/secondary) is currently focused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveSlot {
    P1Primary,
    P1Secondary,
    P2Primary,
    P2Secondary,
}

impl ActiveSlot {
    #[inline(always)]
    pub const fn next(self) -> Self {
        use ActiveSlot::{P1Primary, P1Secondary, P2Primary, P2Secondary};
        match self {
            P1Primary => P1Secondary,
            P1Secondary => P2Primary,
            P2Primary => P2Secondary,
            P2Secondary => P1Primary,
        }
    }

    #[inline(always)]
    pub const fn prev(self) -> Self {
        use ActiveSlot::{P1Primary, P2Secondary, P1Secondary, P2Primary};
        match self {
            P1Primary => P2Secondary,
            P1Secondary => P1Primary,
            P2Primary => P1Secondary,
            P2Secondary => P2Primary,
        }
    }
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    /// 0..NUM_MAPPING_ROWS-1 = mapping rows, `NUM_MAPPING_ROWS` = Exit.
    selected_row: usize,
    prev_selected_row: usize,
    active_slot: ActiveSlot,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    // Vertical tween when changing selected row
    cursor_row_anim_from_y: f32,
    cursor_row_anim_t: f32,
    cursor_row_anim_from_row: Option<usize>,
    // Horizontal tween when changing active slot within a row
    slot_anim_from: ActiveSlot,
    slot_anim_to: ActiveSlot,
    slot_anim_t: f32,
    // Capture state: when true, the active slot's value pulses and
    // navigation is locked until a non-default key is pressed.
    capture_active: bool,
    capture_row: Option<usize>,
    capture_slot: Option<ActiveSlot>,
    capture_pulse_t: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        selected_row: 0,
        prev_selected_row: 0,
        active_slot: ActiveSlot::P1Primary,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        cursor_row_anim_from_y: 0.0,
        cursor_row_anim_t: 1.0,
        cursor_row_anim_from_row: None,
        slot_anim_from: ActiveSlot::P1Primary,
        slot_anim_to: ActiveSlot::P1Primary,
        slot_anim_t: 1.0,
        capture_active: false,
        capture_row: None,
        capture_slot: None,
        capture_pulse_t: 0.0,
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

fn on_nav_press(state: &mut State, dir: NavDirection) {
    state.nav_key_held_direction = Some(dir);
    let now = Instant::now();
    state.nav_key_held_since = Some(now);
    state.nav_key_last_scrolled_at = Some(now);
}

fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

#[inline(always)]
const fn total_rows() -> usize {
    NUM_MAPPING_ROWS + 1 // + Exit row
}

fn move_selection(state: &mut State, dir: NavDirection) {
    let total = total_rows();
    if total == 0 {
        return;
    }
    let old = state.selected_row;
    let new = match dir {
        NavDirection::Up => {
            if state.selected_row == 0 {
                total.saturating_sub(1)
            } else {
                state.selected_row - 1
            }
        }
        NavDirection::Down => (state.selected_row + 1) % total,
    };
    if new != old {
        state.selected_row = new;
        // Reset row tween; update() will compute from_y based on layout.
        state.cursor_row_anim_t = 0.0;
        state.cursor_row_anim_from_row = Some(old);
        audio::play_sfx("assets/sounds/change.ogg");
    }
}

pub fn update(state: &mut State, dt: f32) {
    // Hold-to-scroll for Up/Down.
    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
        state.nav_key_held_direction,
        state.nav_key_held_since,
        state.nav_key_last_scrolled_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL
        {
            move_selection(state, direction);
            state.nav_key_last_scrolled_at = Some(now);
        }
    }
    // Start vertical cursor tween when the selected row changes.
    if state.selected_row != state.prev_selected_row {
        // Duplicate layout math needed to compute row centers (mirrors get_actors()).
        let sw = screen_width();
        let sh = screen_height();

        let content_top = BAR_H;
        let content_bottom = sh - BAR_H;
        let content_h = (content_bottom - content_top).max(0.0);

        let content_left = LEFT_MARGIN_PX;
        let content_right = sw - RIGHT_MARGIN_PX;
        let avail_w = (content_right - content_left).max(0.0);
        let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

        let total_w_base = SIDE_W_BASE.mul_add(2.0, DESC_W_BASE * 0.8) + SIDE_GAP_BASE * 2.0;
        let rows_h_base = (VISIBLE_ROWS as f32).mul_add(ROW_H, ((VISIBLE_ROWS - 1) as f32) * ROW_GAP);

        let s_w = if total_w_base > 0.0 {
            avail_w / total_w_base
        } else {
            1.0
        };
        let s_h = if rows_h_base > 0.0 {
            avail_h / rows_h_base
        } else {
            1.0
        };
        let s = s_w.min(s_h).max(0.0);

        let first_row_y = content_top + FIRST_ROW_TOP_MARGIN_PX + TABLE_TOP_EXTRA_PX;

        let total = total_rows();
        let anchor_row: usize = 4;
        let max_offset = total.saturating_sub(VISIBLE_ROWS);
        let offset_rows = if total <= VISIBLE_ROWS {
            0
        } else {
            state
                .selected_row
                .saturating_sub(anchor_row)
                .min(max_offset)
        };

        let prev_idx = state.prev_selected_row;
        let i_prev_vis = (prev_idx as isize) - (offset_rows as isize);
        let row_step = (ROW_H + ROW_GAP) * s;
        let from_y_center = (i_prev_vis as f32).mul_add(row_step, first_row_y) + 0.5 * ROW_H * s;
        state.cursor_row_anim_from_y = from_y_center;
        state.cursor_row_anim_t = 0.0;
        state.cursor_row_anim_from_row = Some(prev_idx);
        state.prev_selected_row = state.selected_row;
    }

    // Advance vertical row tween, if any.
    if state.cursor_row_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_row_anim_t =
                (state.cursor_row_anim_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_row_anim_t = 1.0;
        }
        if state.cursor_row_anim_t >= 1.0 {
            state.cursor_row_anim_from_row = None;
        }
    }

    // Advance horizontal slot tween, if any.
    if state.slot_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.slot_anim_t = (state.slot_anim_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.slot_anim_t = 1.0;
        }
    }

    // Advance capture pulse timer for the "heartbeat" animation.
    if state.capture_active {
        // Slightly faster than 1 Hz for a snappier heartbeat.
        state.capture_pulse_t += dt * 2.5 * std::f32::consts::PI;
        if !state.capture_pulse_t.is_finite() {
            state.capture_pulse_t = 0.0;
        }
    } else {
        state.capture_pulse_t = 0.0;
    }
}

/// Raw keyboard handler used only while capturing a new mapping.
pub fn handle_raw_key_event(state: &mut State, key_event: &KeyEvent) -> ScreenAction {
    if key_event.repeat {
        return ScreenAction::None;
    }

    let is_pressed = key_event.state == ElementState::Pressed;
    let PhysicalKey::Code(code) = key_event.physical_key else {
        return ScreenAction::None;
    };

    // If we're capturing, treat this as a candidate mapping; otherwise,
    // interpret arrows / Enter / Escape as navigation/back/capture.
    if state.capture_active {
        if !is_pressed {
            return ScreenAction::None;
        }
        // Default/protected keys do nothing while capturing; remain locked.
        if DEFAULT_PROTECTED_KEYS.contains(&code) {
            return ScreenAction::None;
        }

        // Map the captured key into the appropriate binding slot based on
        // the active row and slot, then persist to deadsync.ini with unique
        // keyboard bindings across all P1/P2 actions.
        if let (Some(row_idx), Some(slot)) = (state.capture_row, state.capture_slot) {
            let (p1_act_opt, p2_act_opt) = row_actions(row_idx);
            let action_opt = match slot {
                ActiveSlot::P1Primary | ActiveSlot::P1Secondary => p1_act_opt,
                ActiveSlot::P2Primary | ActiveSlot::P2Secondary => p2_act_opt,
            };

            if let Some(action) = action_opt {
                let index = match slot {
                    ActiveSlot::P1Primary | ActiveSlot::P2Primary => 1,
                    ActiveSlot::P1Secondary | ActiveSlot::P2Secondary => 2,
                };
                crate::config::update_keymap_binding_unique_keyboard(action, index, code);
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }

        // Any captured key ends capture.
        state.capture_active = false;
        state.capture_row = None;
        state.capture_slot = None;
        state.capture_pulse_t = 0.0;

        return ScreenAction::None;
    }

    // Not capturing: only arrow keys, Enter, and Escape drive navigation.
    match code {
        KeyCode::ArrowUp => {
            if is_pressed {
                move_selection(state, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        KeyCode::ArrowDown => {
            if is_pressed {
                move_selection(state, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        KeyCode::ArrowLeft => {
            if is_pressed && state.selected_row < NUM_MAPPING_ROWS {
                let old_slot = state.active_slot;
                let new_slot = state.active_slot.prev();
                if new_slot != old_slot {
                    state.active_slot = new_slot;
                    state.slot_anim_from = old_slot;
                    state.slot_anim_to = new_slot;
                    state.slot_anim_t = 0.0;
                }
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        KeyCode::ArrowRight => {
            if is_pressed && state.selected_row < NUM_MAPPING_ROWS {
                let old_slot = state.active_slot;
                let new_slot = state.active_slot.next();
                if new_slot != old_slot {
                    state.active_slot = new_slot;
                    state.slot_anim_from = old_slot;
                    state.slot_anim_to = new_slot;
                    state.slot_anim_t = 0.0;
                }
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        KeyCode::Enter => {
            if is_pressed {
                if state.selected_row == NUM_MAPPING_ROWS {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::Navigate(Screen::Options);
                }
                if state.selected_row < NUM_MAPPING_ROWS {
                    state.capture_active = true;
                    state.capture_row = Some(state.selected_row);
                    state.capture_slot = Some(state.active_slot);
                    state.capture_pulse_t = 0.0;
                    state.nav_key_held_direction = None;
                    state.nav_key_held_since = None;
                    state.nav_key_last_scrolled_at = None;
                    audio::play_sfx("assets/sounds/change_value.ogg");
                }
            }
        }
        KeyCode::Escape => {
            if is_pressed {
                return ScreenAction::Navigate(Screen::Options);
            }
        }
        _ => {}
    }

    ScreenAction::None
}

/// Raw gamepad handler used only while capturing a new mapping.
/// This consumes the first pressed gamepad element and writes it into
/// the appropriate binding slot for the active row/slot.
pub fn handle_raw_pad_event(state: &mut State, pad_event: &PadEvent) {
    if !state.capture_active {
        return;
    }

    // Only react to press edges; releases and pure axis motion are ignored.
    let binding_opt = match *pad_event {
        PadEvent::RawButton {
            id, code, pressed, ..
        } => {
            if !pressed {
                return;
            }
            let dev = usize::from(id);
            let code_u32 = code.into_u32();
            Some(InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32,
                device: Some(dev),
                uuid: None,
            }))
        }
        PadEvent::Dir { id, dir, pressed, .. } => {
            if !pressed {
                return;
            }
            let dev = usize::from(id);
            Some(InputBinding::PadDirOn { device: dev, dir })
        }
        PadEvent::RawAxis { .. } => None,
    };

    let Some(binding) = binding_opt else {
        return;
    };

    if let (Some(row_idx), Some(slot)) = (state.capture_row, state.capture_slot) {
        let (p1_act_opt, p2_act_opt) = row_actions(row_idx);
        let action_opt = match slot {
            ActiveSlot::P1Primary | ActiveSlot::P1Secondary => p1_act_opt,
            ActiveSlot::P2Primary | ActiveSlot::P2Secondary => p2_act_opt,
        };

        if let Some(action) = action_opt {
            let index = match slot {
                ActiveSlot::P1Primary | ActiveSlot::P2Primary => 1,
                ActiveSlot::P1Secondary | ActiveSlot::P2Secondary => 2,
            };
            crate::config::update_keymap_binding_unique_gamepad(action, index, binding);
            audio::play_sfx("assets/sounds/change_value.ogg");
        }
    }

    // Any captured pad input ends capture.
    state.capture_active = false;
    state.capture_row = None;
    state.capture_slot = None;
    state.capture_pulse_t = 0.0;
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    // While capturing, lock navigation and only allow backing out
    // of the screen; candidate keys are handled in handle_raw_key_event.
    if state.capture_active {
        if ev.action == VirtualAction::p1_back && ev.pressed {
            return ScreenAction::Navigate(Screen::Options);
        }
        return ScreenAction::None;
    }

    // Outside of capture, navigation on this screen is strictly keyboard-only.
    // Gamepad inputs should not move the cursor or activate UI here; they are
    // used only when explicitly capturing a new mapping.
    if ev.source == InputSource::Gamepad {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back if ev.pressed => {
            return ScreenAction::Navigate(Screen::Options);
        }
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if ev.pressed {
                move_selection(state, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if ev.pressed {
                move_selection(state, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if ev.pressed && state.selected_row < NUM_MAPPING_ROWS {
                let old_slot = state.active_slot;
                let new_slot = state.active_slot.prev();
                if new_slot != old_slot {
                    state.active_slot = new_slot;
                    state.slot_anim_from = old_slot;
                    state.slot_anim_to = new_slot;
                    state.slot_anim_t = 0.0;
                }
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if ev.pressed && state.selected_row < NUM_MAPPING_ROWS {
                let old_slot = state.active_slot;
                let new_slot = state.active_slot.next();
                if new_slot != old_slot {
                    state.active_slot = new_slot;
                    state.slot_anim_from = old_slot;
                    state.slot_anim_to = new_slot;
                    state.slot_anim_t = 0.0;
                }
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        VirtualAction::p1_start if ev.pressed => {
            if state.selected_row == NUM_MAPPING_ROWS {
                audio::play_sfx("assets/sounds/start.ogg");
                return ScreenAction::Navigate(Screen::Options);
            }

            // Begin capture on the currently focused slot in this row.
            if state.selected_row < NUM_MAPPING_ROWS {
                state.capture_active = true;
                state.capture_row = Some(state.selected_row);
                state.capture_slot = Some(state.active_slot);
                state.capture_pulse_t = 0.0;
                // Stop any held navigation so the list does not keep scrolling.
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        _ => {}
    }
    ScreenAction::None
}

/* -------------------------------- drawing -------------------------------- */

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Frame {
            background,
            children,
            ..
        } => {
            if let Some(crate::ui::actors::Background::Color(c)) = background {
                c[3] *= alpha;
            }
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Shadow { color, child, .. } => {
            color[3] *= alpha;
            apply_alpha_to_actor(child, alpha);
        }
    }
}

#[inline(always)]
fn slot_pulse_zoom_and_color(
    pulse_opt: Option<f32>,
    capture_slot: Option<ActiveSlot>,
    slot: ActiveSlot,
    base_zoom: f32,
    base_color: [f32; 4],
    col_white: [f32; 4],
) -> (f32, [f32; 4]) {
    let Some(pulse) = pulse_opt else {
        return (base_zoom, base_color);
    };
    if capture_slot != Some(slot) {
        return (base_zoom, base_color);
    }
    // Zoom out (shrink) instead of in: scale from 1.0 down to ~0.8.
    let scale = 0.20f32.mul_add(-pulse, 1.0);
    let brighten = 0.35 * pulse;
    let mut color = base_color;
    color[0] = (col_white[0] - base_color[0]).mul_add(brighten, base_color[0]);
    color[1] = (col_white[1] - base_color[1]).mul_add(brighten, base_color[1]);
    color[2] = (col_white[2] - base_color[2]).mul_add(brighten, base_color[2]);
    (base_zoom * scale, color)
}

#[inline(always)]
fn format_binding_for_display(binding: InputBinding) -> String {
    match binding {
        InputBinding::Key(code) => format!("{code:?}"),
        // Any-pad bindings
        InputBinding::PadDir(dir) => format!("Dir {dir:?}"),
        // Device-specific bindings, aligned with "Pad N Btn 0x.." style.
        InputBinding::PadDirOn { device, dir } => {
            format!("Pad {device} Dir {dir:?}")
        }
        InputBinding::GamepadCode(binding) => {
            let dev = binding.device.unwrap_or(0);
            // Display the full code but cropped at the first non-zero hex
            // digit for readability, e.g.:
            //   0x00000016 → "0x16"
            //   0x00010131 → "0x10131"
            let mut hex = format!("{:08X}", binding.code_u32);
            while hex.len() > 1 && hex.starts_with('0') {
                hex.remove(0);
            }
            let mut label = format!("Pad {dev} Btn 0x{hex}");
            // Soft max-width to avoid overflowing the column; in practice the
            // cropped code is short so this rarely triggers.
            const MAX_LABEL_CHARS: usize = 18;
            if label.len() > MAX_LABEL_CHARS {
                label.truncate(MAX_LABEL_CHARS);
            }
            label
        }
    }
}

pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(256);

    /* -------------------------- HEART BACKGROUND -------------------------- */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        // Keep hearts always visible for actor-only fades; UI rows fade separately.
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui_actors = Vec::new();

    /* ------------------------------ TOP BAR ------------------------------- */
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    ui_actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "KEYBOARD/PAD MAPPINGS",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        fg_color: FG,
    }));

    /* --------------------------- MAIN CONTENT UI -------------------------- */

    // Colors
    let col_active_bg = color::rgba_hex("#333333");
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];
    let col_white = [1.0, 1.0, 1.0, 1.0];
    let col_gray = color::rgba_hex("#808080");

    // Snapshot of current virtual keymap so defaults reflect deadsync.ini.
    let keymap = get_keymap();

    // Compute available content area between top/bottom bars and side margins.
    let sw = screen_width();
    let sh = screen_height();

    let content_top = BAR_H;
    let content_bottom = sh - BAR_H;
    let content_h = (content_bottom - content_top).max(0.0);

    let content_left = LEFT_MARGIN_PX;
    let content_right = sw - RIGHT_MARGIN_PX;
    let avail_w = (content_right - content_left).max(0.0);
    let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

    // Base layout extents (unscaled).
    let total_w_base = SIDE_W_BASE.mul_add(2.0, DESC_W_BASE * 0.8) + SIDE_GAP_BASE * 2.0;
    // Only VISIBLE_ROWS participate in vertical fit; the list scrolls inside.
    let rows_h_base = (VISIBLE_ROWS as f32).mul_add(ROW_H, ((VISIBLE_ROWS - 1) as f32) * ROW_GAP);

    let s_w = if total_w_base > 0.0 {
        avail_w / total_w_base
    } else {
        1.0
    };
    let s_h = if rows_h_base > 0.0 {
        avail_h / rows_h_base
    } else {
        1.0
    };
    let s = s_w.min(s_h).max(0.0);

    let desc_w = DESC_W_BASE * 0.8 * s;
    let side_w = SIDE_W_BASE * s;
    let gap = SIDE_GAP_BASE * s;

    let content_center_x = content_left + avail_w * 0.5;
    let first_row_y = content_top + FIRST_ROW_TOP_MARGIN_PX + TABLE_TOP_EXTRA_PX;

    let desc_x = content_center_x - desc_w * 0.5;
    let p1_side_x = desc_x - gap - side_w;
    let p2_side_x = desc_x + desc_w + gap;

    // Scrolling window (like PlayerOptions): only VISIBLE_ROWS rows shown.
    let total = total_rows();
    let anchor_row: usize = 4;
    let max_offset = total.saturating_sub(VISIBLE_ROWS);
    let offset_rows = if total <= VISIBLE_ROWS {
        0
    } else {
        state
            .selected_row
            .saturating_sub(anchor_row)
            .min(max_offset)
    };

    // Description height should end at the last visible mapping row (not including Exit).
    let mut visible_mapping_rows = 0_usize;
    for i_vis in 0..VISIBLE_ROWS {
        let row_idx = offset_rows + i_vis;
        if row_idx >= NUM_MAPPING_ROWS {
            break;
        }
        visible_mapping_rows += 1;
    }
    let desc_rows_h_base = if visible_mapping_rows == 0 {
        0.0
    } else {
        (visible_mapping_rows as f32).mul_add(ROW_H, ((visible_mapping_rows.saturating_sub(1)) as f32) * ROW_GAP)
    };
    let desc_h = desc_rows_h_base * s;

    // Description box (center) – height matched to visible mapping rows only.
    ui_actors.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, first_row_y):
        zoomto(desc_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));

    // Description content: per-row labels aligned with mapping rows.
    {
        let labels_center_x = desc_x + desc_w * 0.5;
        for i_vis in 0..VISIBLE_ROWS {
            let row_idx = offset_rows + i_vis;
            if row_idx >= NUM_MAPPING_ROWS {
                break;
            }
            let row_center_y =
                ((i_vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, first_row_y) + 0.5 * ROW_H * s;
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(labels_center_x, row_center_y):
                zoom(DESC_BODY_ZOOM):
                diffuse(1.0, 1.0, 1.0, 1.0):
                font("miso"): settext(MAPPING_LABELS[row_idx]):
                horizalign(center)
            ));
        }
    }

    // Side columns: three columns per side (Primary, Secondary, Default).
    let col_w = side_w / 3.0;
    let value_zoom = 0.9_f32;

    // Wendy-style column headers above each side's three columns.
    // First line: "Player 1"/"Player 2" centered over each side.
    // Second line: "Primary"/"Secondary"/"Default" per column.
    let header_sub_y = COLUMN_HEADER_OFFSET_PX.mul_add(-s, first_row_y);
    let header_main_y = PLAYER_HEADER_GAP_PX.mul_add(-s, header_sub_y);
    let p1_primary_x = p1_side_x + col_w * 0.5;
    let p1_secondary_x = p1_side_x + col_w * 1.5;
    let p1_default_x = p1_side_x + col_w * 2.5;
    let p2_primary_x = p2_side_x + col_w * 0.5;
    let p2_secondary_x = p2_side_x + col_w * 1.5;
    let p2_default_x = p2_side_x + col_w * 2.5;

    let header_zoom = 0.25_f32;
    let header_main_zoom = 0.65_f32;
    let p1_center_x = p1_side_x + side_w * 0.5;
    let p2_center_x = p2_side_x + side_w * 0.5;

    // Helper for computing the cursor center X for a given row index and slot.
    let slot_center_x_for_row = |row_idx: usize, slot: ActiveSlot| -> f32 {
        if row_idx >= total_rows() {
            content_center_x
        } else if row_idx == NUM_MAPPING_ROWS {
            // Exit row always uses the centered Exit label.
            content_center_x
        } else {
            match slot {
                ActiveSlot::P1Primary => p1_primary_x,
                ActiveSlot::P1Secondary => p1_secondary_x,
                ActiveSlot::P2Primary => p2_primary_x,
                ActiveSlot::P2Secondary => p2_secondary_x,
            }
        }
    };

    // Top line: Player labels (Wendy, white).
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p1_center_x, header_main_y):
        zoom(header_main_zoom):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("wendy"): settext("Player 1"):
        horizalign(center)
    ));
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p2_center_x, header_main_y):
        zoom(header_main_zoom):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("wendy"): settext("Player 2"):
        horizalign(center)
    ));

    // Column headers: Primary / Secondary / Default in decorative Wendy color.
    let mut header_dec = color::decorative_rgba(state.active_color_index);
    header_dec[3] = 1.0;

    // P1 headers
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p1_primary_x, header_sub_y):
        zoom(header_zoom):
        diffuse(header_dec[0], header_dec[1], header_dec[2], header_dec[3]):
        font("wendy"): settext("Primary"):
        horizalign(center)
    ));
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p1_secondary_x, header_sub_y):
        zoom(header_zoom):
        diffuse(header_dec[0], header_dec[1], header_dec[2], header_dec[3]):
        font("wendy"): settext("Secondary"):
        horizalign(center)
    ));
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p1_default_x, header_sub_y):
        zoom(header_zoom):
        diffuse(header_dec[0], header_dec[1], header_dec[2], header_dec[3]):
        font("wendy"): settext("Default"):
        horizalign(center)
    ));

    // P2 headers
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p2_primary_x, header_sub_y):
        zoom(header_zoom):
        diffuse(header_dec[0], header_dec[1], header_dec[2], header_dec[3]):
        font("wendy"): settext("Primary"):
        horizalign(center)
    ));
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p2_secondary_x, header_sub_y):
        zoom(header_zoom):
        diffuse(header_dec[0], header_dec[1], header_dec[2], header_dec[3]):
        font("wendy"): settext("Secondary"):
        horizalign(center)
    ));
    ui_actors.push(act!(text:
        align(0.5, 0.5):
        xy(p2_default_x, header_sub_y):
        zoom(header_zoom):
        diffuse(header_dec[0], header_dec[1], header_dec[2], header_dec[3]):
        font("wendy"): settext("Default"):
        horizalign(center)
    ));

    for i_vis in 0..VISIBLE_ROWS {
        let row_idx = offset_rows + i_vis;
        if row_idx >= total {
            break;
        }

        let is_exit = row_idx == total - 1;
        let row_y = ((i_vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, first_row_y);
        let row_mid_y = (0.5 * ROW_H).mul_add(s, row_y);
        let is_active = row_idx == state.selected_row;

        if !is_exit && row_idx >= NUM_MAPPING_ROWS {
            continue;
        }

        if !is_exit {
            let bg = if is_active {
                col_active_bg
            } else {
                col_inactive_bg
            };

            // Row backgrounds for P1 and P2 sides.
            ui_actors.push(act!(quad:
                align(0.0, 0.0):
                xy(p1_side_x, row_y):
                zoomto(side_w, ROW_H * s):
                diffuse(bg[0], bg[1], bg[2], bg[3])
            ));
            ui_actors.push(act!(quad:
                align(0.0, 0.0):
                xy(p2_side_x, row_y):
                zoomto(side_w, ROW_H * s):
                diffuse(bg[0], bg[1], bg[2], bg[3])
            ));

            // Label-style default columns (third column on each side).
            let default_bg_color = [0.0, 0.0, 0.0, 0.25];
            ui_actors.push(act!(quad:
                align(0.0, 0.0):
                xy(2.0f32.mul_add(col_w, p1_side_x), row_y):
                zoomto(col_w, ROW_H * s):
                diffuse(default_bg_color[0], default_bg_color[1], default_bg_color[2], default_bg_color[3])
            ));
            ui_actors.push(act!(quad:
                align(0.0, 0.0):
                xy(2.0f32.mul_add(col_w, p2_side_x), row_y):
                zoomto(col_w, ROW_H * s):
                diffuse(default_bg_color[0], default_bg_color[1], default_bg_color[2], default_bg_color[3])
            ));

            let (p1_act_opt, p2_act_opt) = row_actions(row_idx);
            // Config order: first = Default, second = Primary, third = Secondary.
            let p1_primary_text = p1_act_opt
                .and_then(|act| keymap.binding_at(act, 1)).map_or_else(|| "------".to_string(), format_binding_for_display);
            let p1_secondary_text = p1_act_opt
                .and_then(|act| keymap.binding_at(act, 2)).map_or_else(|| "------".to_string(), format_binding_for_display);
            let p2_primary_text = p2_act_opt
                .and_then(|act| keymap.binding_at(act, 1)).map_or_else(|| "------".to_string(), format_binding_for_display);
            let p2_secondary_text = p2_act_opt
                .and_then(|act| keymap.binding_at(act, 2)).map_or_else(|| "------".to_string(), format_binding_for_display);

            let p1_default_text = p1_act_opt
                .and_then(|act| keymap.first_key_binding(act))
                .map(|code| format!("{code:?}"))
                .unwrap_or_else(|| "------".to_string());
            let p2_default_text = p2_act_opt
                .and_then(|act| keymap.first_key_binding(act))
                .map(|code| format!("{code:?}"))
                .unwrap_or_else(|| "------".to_string());
            let active_value_color = if is_active { col_white } else { col_gray };

            // Heartbeat-style pulse for the slot currently being captured.
            let pulse_opt = if state.capture_active && state.capture_row == Some(row_idx) {
                let t = state.capture_pulse_t.sin().mul_add(0.5, 0.5);
                Some(t.clamp(0.0, 1.0))
            } else {
                None
            };

            let (p1_primary_zoom, p1_primary_color) = slot_pulse_zoom_and_color(
                pulse_opt,
                state.capture_slot,
                ActiveSlot::P1Primary,
                value_zoom,
                active_value_color,
                col_white,
            );
            let (p1_secondary_zoom, p1_secondary_color) = slot_pulse_zoom_and_color(
                pulse_opt,
                state.capture_slot,
                ActiveSlot::P1Secondary,
                value_zoom,
                active_value_color,
                col_white,
            );
            let (p2_primary_zoom, p2_primary_color) = slot_pulse_zoom_and_color(
                pulse_opt,
                state.capture_slot,
                ActiveSlot::P2Primary,
                value_zoom,
                active_value_color,
                col_white,
            );
            let (p2_secondary_zoom, p2_secondary_color) = slot_pulse_zoom_and_color(
                pulse_opt,
                state.capture_slot,
                ActiveSlot::P2Secondary,
                value_zoom,
                active_value_color,
                col_white,
            );

            // P1 columns: Primary, Secondary, Default.
            // P1 primary / secondary (editable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p1_primary_x, row_mid_y):
                zoom(p1_primary_zoom):
                diffuse(p1_primary_color[0], p1_primary_color[1], p1_primary_color[2], p1_primary_color[3]):
                font("miso"):
                settext(p1_primary_text.clone()):
                maxwidth(col_w * 0.8):
                horizalign(center)
            ));
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p1_secondary_x, row_mid_y):
                zoom(p1_secondary_zoom):
                diffuse(p1_secondary_color[0], p1_secondary_color[1], p1_secondary_color[2], p1_secondary_color[3]):
                font("miso"):
                settext(p1_secondary_text.clone()):
                maxwidth(col_w * 0.8):
                horizalign(center)
            ));

            // P1 default (non-selectable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p1_default_x, row_mid_y):
                zoom(value_zoom):
                diffuse(col_white[0], col_white[1], col_white[2], col_white[3]):
                font("miso"):
                settext(p1_default_text):
                maxwidth(col_w * 0.8):
                horizalign(center)
            ));

            // P2 primary / secondary (editable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p2_primary_x, row_mid_y):
                zoom(p2_primary_zoom):
                diffuse(p2_primary_color[0], p2_primary_color[1], p2_primary_color[2], p2_primary_color[3]):
                font("miso"):
                settext(p2_primary_text.clone()):
                maxwidth(col_w * 0.8):
                horizalign(center)
            ));
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p2_secondary_x, row_mid_y):
                zoom(p2_secondary_zoom):
                diffuse(p2_secondary_color[0], p2_secondary_color[1], p2_secondary_color[2], p2_secondary_color[3]):
                font("miso"):
                settext(p2_secondary_text.clone()):
                maxwidth(col_w * 0.8):
                horizalign(center)
            ));

            // P2 default (non-selectable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p2_default_x, row_mid_y):
                zoom(value_zoom):
                diffuse(col_white[0], col_white[1], col_white[2], col_white[3]):
                font("miso"):
                settext(p2_default_text):
                maxwidth(col_w * 0.8):
                horizalign(center)
            ));

            // Selection ring around active slot.
            if is_active {
                let center_x_target = slot_center_x_for_row(row_idx, state.active_slot);
                let mut center_x = center_x_target;
                let mut center_y = row_mid_y;

                // Base ring size (current fixed behavior) – used as an upper bound
                // so the cursor never grows larger than the existing standard.
                let base_ring_w = col_w * 0.9;
                let base_ring_h = ROW_H * s * 0.9;

                // Measure the active slot's text and adapt the ring size to it,
                // clamped so it never exceeds the base size.
                let (active_text, active_zoom) = match state.active_slot {
                    ActiveSlot::P1Primary => (&p1_primary_text, p1_primary_zoom),
                    ActiveSlot::P1Secondary => (&p1_secondary_text, p1_secondary_zoom),
                    ActiveSlot::P2Primary => (&p2_primary_text, p2_primary_zoom),
                    ActiveSlot::P2Secondary => (&p2_secondary_text, p2_secondary_zoom),
                };
                let ring_text_zoom = if state.capture_active {
                    value_zoom
                } else {
                    active_zoom
                };

                let border_w = widescale(2.0, 2.5);
                let mut ring_w = base_ring_w;
                let mut ring_h = base_ring_h;

                asset_manager.with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        let mut text_w =
                            font::measure_line_width_logical(metrics_font, active_text, all_fonts)
                                as f32;
                        if !text_w.is_finite() || text_w <= 0.0 {
                            text_w = 1.0;
                        }
                        let text_h = (metrics_font.height as f32).max(1.0);
                        let draw_w = text_w * ring_text_zoom;
                        let draw_h = text_h * ring_text_zoom;

                        let pad_y = widescale(6.0, 8.0);
                        let min_pad_x = widescale(2.0, 3.0);
                        let max_pad_x = widescale(22.0, 28.0);
                        let width_ref = widescale(180.0, 220.0);
                        let t = (draw_w / width_ref).clamp(0.0, 1.0);
                        let mut pad_x = (max_pad_x - min_pad_x).mul_add(t, min_pad_x);
                        // Ensure the ring does not invade adjacent inline column space.
                        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                        if pad_x > max_pad_by_spacing {
                            pad_x = max_pad_by_spacing;
                        }

                        let desired_w = draw_w + pad_x * 2.0;
                        let desired_h = draw_h + pad_y * 2.0;
                        ring_w = desired_w.min(base_ring_w);
                        ring_h = desired_h.min(base_ring_h);
                    });
                });

                // Vertical + (optional) diagonal tween between rows.
                if state.cursor_row_anim_t < 1.0 {
                    if let Some(from_row) = state.cursor_row_anim_from_row {
                        let t = ease_out_cubic(state.cursor_row_anim_t);
                        let from_x = slot_center_x_for_row(from_row, state.active_slot);
                        let from_y = state.cursor_row_anim_from_y;
                        center_x = (center_x_target - from_x).mul_add(t, from_x);
                        center_y = (row_mid_y - from_y).mul_add(t, from_y);
                    }
                } else if state.slot_anim_t < 1.0 {
                    // Horizontal tween within the current row when changing slots.
                    let t = ease_out_cubic(state.slot_anim_t);
                    let from_x = slot_center_x_for_row(row_idx, state.slot_anim_from);
                    let to_x = slot_center_x_for_row(row_idx, state.slot_anim_to);
                    center_x = (to_x - from_x).mul_add(t, from_x);
                }

                let left = center_x - ring_w * 0.5;
                let right = center_x + ring_w * 0.5;
                let top = center_y - ring_h * 0.5;
                let bottom = center_y + ring_h * 0.5;
                let mut ring_color = color::decorative_rgba(state.active_color_index);
                ring_color[3] = 1.0;

                ui_actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(center_x, top + border_w * 0.5):
                    zoomto(ring_w, border_w):
                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                    z(101)
                ));
                ui_actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(center_x, bottom - border_w * 0.5):
                    zoomto(ring_w, border_w):
                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                    z(101)
                ));
                ui_actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(left + border_w * 0.5, center_y):
                    zoomto(border_w, ring_h):
                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                    z(101)
                ));
                ui_actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(right - border_w * 0.5, center_y):
                    zoomto(border_w, ring_h):
                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                    z(101)
                ));
            }
        } else {
            // Exit row: full-width background across the content area and centered text,
            // similar in spirit to PlayerOptions.
            let exit_label = "Exit";
            let exit_y = row_mid_y;
            let choice_color = if is_active { col_white } else { col_gray };
            let exit_center_x = content_center_x;

            // Full-width background from content_left to content_right.
            let exit_row_left = content_left;
            let exit_row_width = (content_right - content_left).max(0.0);
            let exit_bg = if is_active {
                col_active_bg
            } else {
                col_inactive_bg
            };
            ui_actors.push(act!(quad:
                align(0.0, 0.0):
                xy(exit_row_left, row_y):
                zoomto(exit_row_width, ROW_H * s):
                diffuse(exit_bg[0], exit_bg[1], exit_bg[2], exit_bg[3])
            ));

            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(exit_center_x, exit_y):
                zoom(0.835):
                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                font("miso"):
                settext(exit_label):
                horizalign(center)
            ));

            if is_active {
                let value_zoom = 0.835_f32;
                asset_manager.with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        let mut text_w =
                            font::measure_line_width_logical(metrics_font, exit_label, all_fonts)
                                as f32;
                        if !text_w.is_finite() || text_w <= 0.0 {
                            text_w = 1.0;
                        }
                        let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                        let draw_w = text_w * value_zoom;
                        let draw_h = text_h;

                        let pad_y = widescale(6.0, 8.0);
                        let min_pad_x = widescale(2.0, 3.0);
                        let max_pad_x = widescale(22.0, 28.0);
                        let width_ref = widescale(180.0, 220.0);
                        let mut size_t = draw_w / width_ref;
                        if !size_t.is_finite() {
                            size_t = 0.0;
                        }
                        size_t = size_t.clamp(0.0, 1.0);
                        let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
                        let border_w = widescale(2.0, 2.5);
                        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                        if pad_x > max_pad_by_spacing {
                            pad_x = max_pad_by_spacing;
                        }
                        let mut ring_w = draw_w + pad_x * 2.0;
                        let mut ring_h = draw_h + pad_y * 2.0;

                        let mut center_x = exit_center_x;
                        let mut center_y = exit_y;

                        // Diagonal tween from the previous row's cursor center
                        // (mapping slot or the previous Exit) to this Exit row.
                        if state.cursor_row_anim_t < 1.0
                            && let Some(from_row) = state.cursor_row_anim_from_row {
                                let t = ease_out_cubic(state.cursor_row_anim_t);
                                let from_x = slot_center_x_for_row(from_row, state.active_slot);
                                let from_y = state.cursor_row_anim_from_y;
                                center_x = (exit_center_x - from_x).mul_add(t, from_x);
                                center_y = (exit_y - from_y).mul_add(t, from_y);

                                // Interpolate ring size from a mapping-sized
                                // cursor to the Exit-sized cursor.
                                let ring_w_from = col_w * 0.9;
                                let ring_h_from = ROW_H * s * 0.9;
                                let tsize = t;
                                ring_w = (ring_w - ring_w_from).mul_add(tsize, ring_w_from);
                                ring_h = (ring_h - ring_h_from).mul_add(tsize, ring_h_from);
                            }

                        let left = center_x - ring_w * 0.5;
                        let right = center_x + ring_w * 0.5;
                        let top = center_y - ring_h * 0.5;
                        let bottom = center_y + ring_h * 0.5;
                        let mut ring_color = color::decorative_rgba(state.active_color_index);
                        ring_color[3] = 1.0;

                        ui_actors.push(act!(quad:
                            align(0.5, 0.5):
                            xy(center_x, top + border_w * 0.5):
                            zoomto(ring_w, border_w):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                        ui_actors.push(act!(quad:
                            align(0.5, 0.5):
                            xy(center_x, bottom - border_w * 0.5):
                            zoomto(ring_w, border_w):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                        ui_actors.push(act!(quad:
                            align(0.5, 0.5):
                            xy(left + border_w * 0.5, center_y):
                            zoomto(border_w, ring_h):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                        ui_actors.push(act!(quad:
                            align(0.5, 0.5):
                            xy(right - border_w * 0.5, center_y):
                            zoomto(border_w, ring_h):
                            diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                            z(101)
                        ));
                    });
                });
            }
        }
    }

    let combined_alpha = alpha_multiplier;
    for actor in &mut ui_actors {
        apply_alpha_to_actor(actor, combined_alpha);
    }
    actors.extend(ui_actors);

    actors
}
