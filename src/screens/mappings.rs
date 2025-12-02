use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::*;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::{heart_bg, screen_bar};
use crate::ui::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::font;
use std::time::{Duration, Instant};

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

/// Description pane layout.
const DESC_TITLE_TOP_PAD_PX: f32 = 9.75;
const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_TOP_PAD_PX: f32 = 23.25;
const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_INDENT_PX: f32 = 10.0;
const DESC_TITLE_ZOOM: f32 = 1.0;
const DESC_BODY_ZOOM: f32 = 1.0;

/// Cursor tween duration for vertical movement.
const CURSOR_TWEEN_SECONDS: f32 = 0.1;

/// Spacing between inline items (for cursor ring sizing).
const INLINE_SPACING: f32 = 15.75;

/// Cardinal directions we expose in this prototype.
const NUM_MAPPING_ROWS: usize = 4;
const MAPPING_LABELS: [&str; NUM_MAPPING_ROWS] = ["Left", "Down", "Up", "Right"];

/// Static description lines for the center box.
const DESC_TITLE: &str = "Configure mappings for these directions:";
const DESC_LINES: [&str; NUM_MAPPING_ROWS] = ["Left", "Down", "Up", "Right"];

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
    1.0 - u * u * u
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
    pub fn next(self) -> Self {
        use ActiveSlot::*;
        match self {
            P1Primary => P1Secondary,
            P1Secondary => P2Primary,
            P2Primary => P2Secondary,
            P2Secondary => P1Primary,
        }
    }

    #[inline(always)]
    pub fn prev(self) -> Self {
        use ActiveSlot::*;
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
    /// 0..NUM_MAPPING_ROWS-1 = mapping rows, NUM_MAPPING_ROWS = Exit.
    selected_row: usize,
    active_slot: ActiveSlot,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    // Vertical tween when changing selected row
    cursor_row_anim_from_y: f32,
    cursor_row_anim_t: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        selected_row: 0,
        active_slot: ActiveSlot::P1Primary,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        cursor_row_anim_from_y: 0.0,
        cursor_row_anim_t: 1.0,
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
fn total_rows() -> usize {
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

    // Advance vertical row tween.
    if state.cursor_row_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_row_anim_t = (state.cursor_row_anim_t + dt / CURSOR_TWEEN_SECONDS)
                .min(1.0);
        } else {
            state.cursor_row_anim_t = 1.0;
        }
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
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
                state.active_slot = state.active_slot.prev();
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if ev.pressed && state.selected_row < NUM_MAPPING_ROWS {
                state.active_slot = state.active_slot.next();
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
        }
        VirtualAction::p1_start if ev.pressed => {
            if state.selected_row == NUM_MAPPING_ROWS {
                audio::play_sfx("assets/sounds/start.ogg");
                return ScreenAction::Navigate(Screen::Options);
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
        Actor::Frame { background, children, .. } => {
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
        alpha_mul: alpha_multiplier,
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
        fg_color: FG,
    }));

    /* --------------------------- MAIN CONTENT UI -------------------------- */

    // Colors
    let col_active_bg = color::rgba_hex("#333333");
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] =
        [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];
    let col_white = [1.0, 1.0, 1.0, 1.0];
    let col_gray = color::rgba_hex("#808080");

    // Compute available content area between top/bottom bars and side margins.
    let sw = screen_width();
    let sh = screen_height();

    let content_top = BAR_H;
    let content_bottom = sh - BAR_H;
    let content_h = (content_bottom - content_top).max(0.0);

    let content_left = LEFT_MARGIN_PX;
    let content_right = sw - RIGHT_MARGIN_PX;
    let avail_w = (content_right - content_left).max(0.0);
    let avail_h =
        (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

    // Base layout extents (unscaled).
    let total_w_base =
        SIDE_W_BASE * 2.0 + DESC_W_BASE * 0.8 + SIDE_GAP_BASE * 2.0;
    let rows_h_base = (NUM_MAPPING_ROWS as f32) * ROW_H
        + ((NUM_MAPPING_ROWS.saturating_sub(1)) as f32) * ROW_GAP;

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
    let first_row_y = content_top + FIRST_ROW_TOP_MARGIN_PX;

    let desc_x = content_center_x - desc_w * 0.5;
    let p1_side_x = desc_x - gap - side_w;
    let p2_side_x = desc_x + desc_w + gap;

    let rows_h = rows_h_base * s;
    let desc_h = rows_h;

    // Description box (center).
    ui_actors.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, first_row_y):
        zoomto(desc_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));

    // Description content: title + bullet list.
    {
        let mut cursor_y = first_row_y + DESC_TITLE_TOP_PAD_PX * s;
        let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
        let title_step_px = 20.0 * s;

        let wrapped_title = asset_manager
            .with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |miso_font| {
                    let max_width_px =
                        (desc_w / s - 2.0 * DESC_TITLE_SIDE_PAD_PX).max(0.0) * s;
                    let mut out = String::new();
                    let mut is_first_output_line = true;

                    for segment in DESC_TITLE.split('\n') {
                        let trimmed = segment.trim_end();
                        if trimmed.is_empty() {
                            if !is_first_output_line {
                                out.push('\n');
                            }
                            continue;
                        }

                        let mut current_line = String::new();
                        for word in trimmed.split_whitespace() {
                            let candidate = if current_line.is_empty() {
                                word.to_owned()
                            } else {
                                let mut tmp = current_line.clone();
                                tmp.push(' ');
                                tmp.push_str(word);
                                tmp
                            };

                            let logical_w =
                                font::measure_line_width_logical(
                                    miso_font,
                                    &candidate,
                                    all_fonts,
                                ) as f32;
                            let pixel_w = logical_w * DESC_TITLE_ZOOM * s;

                            if !current_line.is_empty() && pixel_w > max_width_px {
                                if !is_first_output_line {
                                    out.push('\n');
                                }
                                out.push_str(&current_line);
                                is_first_output_line = false;
                                current_line.clear();
                                current_line.push_str(word);
                            } else {
                                current_line = candidate;
                            }
                        }

                        if !current_line.is_empty() {
                            if !is_first_output_line {
                                out.push('\n');
                            }
                            out.push_str(&current_line);
                            is_first_output_line = false;
                        }
                    }

                    if out.is_empty() {
                        DESC_TITLE.to_string()
                    } else {
                        out
                    }
                })
            })
            .unwrap_or_else(|| DESC_TITLE.to_string());
        let title_lines = wrapped_title.lines().count().max(1) as f32;

        ui_actors.push(act!(text:
            align(0.0, 0.0):
            xy(desc_x + title_side_pad, cursor_y):
            zoom(DESC_TITLE_ZOOM):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"): settext(wrapped_title):
            horizalign(left)
        ));
        cursor_y += title_step_px * title_lines + DESC_BULLET_TOP_PAD_PX * s;

        if !DESC_LINES.is_empty() {
            let mut bullet_text = String::new();
            let mut first = true;
            for line in DESC_LINES {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !first {
                    bullet_text.push('\n');
                }
                bullet_text.push('•');
                bullet_text.push(' ');
                bullet_text.push_str(trimmed);
                first = false;
            }
            let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
            let bullet_x = desc_x + bullet_side_pad + DESC_BULLET_INDENT_PX * s;
            ui_actors.push(act!(text:
                align(0.0, 0.0):
                xy(bullet_x, cursor_y):
                zoom(DESC_BODY_ZOOM):
                diffuse(1.0, 1.0, 1.0, 1.0):
                font("miso"): settext(bullet_text):
                horizalign(left)
            ));
        }
    }

    // Side columns: three columns per side (Primary, Secondary, Default).
    let col_w = side_w / 3.0;
    let value_zoom = 0.9_f32;
    let total_rows = total_rows();

    for row_idx in 0..total_rows {
        let is_exit = row_idx == total_rows - 1;
        let row_y =
            first_row_y + (row_idx as f32) * (ROW_H + ROW_GAP) * s;
        let row_mid_y = row_y + 0.5 * ROW_H * s;
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
                xy(p1_side_x + 2.0 * col_w, row_y):
                zoomto(col_w, ROW_H * s):
                diffuse(default_bg_color[0], default_bg_color[1], default_bg_color[2], default_bg_color[3])
            ));
            ui_actors.push(act!(quad:
                align(0.0, 0.0):
                xy(p2_side_x + 2.0 * col_w, row_y):
                zoomto(col_w, ROW_H * s):
                diffuse(default_bg_color[0], default_bg_color[1], default_bg_color[2], default_bg_color[3])
            ));

            // For now, every slot shows the placeholder value "------".
            let value_text = "------";
            let active_value_color = if is_active {
                col_white
            } else {
                col_gray
            };

            // P1 columns: Primary, Secondary, Default.
            let p1_primary_x = p1_side_x + col_w * 0.5;
            let p1_secondary_x = p1_side_x + col_w * 1.5;
            let p1_default_x = p1_side_x + col_w * 2.5;

            // P2 columns: Primary, Secondary, Default.
            let p2_primary_x = p2_side_x + col_w * 0.5;
            let p2_secondary_x = p2_side_x + col_w * 1.5;
            let p2_default_x = p2_side_x + col_w * 2.5;

            // P1 primary / secondary (editable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p1_primary_x, row_mid_y):
                zoom(value_zoom):
                diffuse(active_value_color[0], active_value_color[1], active_value_color[2], active_value_color[3]):
                font("miso"):
                settext(value_text):
                horizalign(center)
            ));
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p1_secondary_x, row_mid_y):
                zoom(value_zoom):
                diffuse(active_value_color[0], active_value_color[1], active_value_color[2], active_value_color[3]):
                font("miso"):
                settext(value_text):
                horizalign(center)
            ));

            // P1 default (non-selectable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p1_default_x, row_mid_y):
                zoom(value_zoom):
                diffuse(col_white[0], col_white[1], col_white[2], col_white[3]):
                font("miso"):
                settext(value_text):
                horizalign(center)
            ));

            // P2 primary / secondary (editable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p2_primary_x, row_mid_y):
                zoom(value_zoom):
                diffuse(active_value_color[0], active_value_color[1], active_value_color[2], active_value_color[3]):
                font("miso"):
                settext(value_text):
                horizalign(center)
            ));
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p2_secondary_x, row_mid_y):
                zoom(value_zoom):
                diffuse(active_value_color[0], active_value_color[1], active_value_color[2], active_value_color[3]):
                font("miso"):
                settext(value_text):
                horizalign(center)
            ));

            // P2 default (non-selectable).
            ui_actors.push(act!(text:
                align(0.5, 0.5):
                xy(p2_default_x, row_mid_y):
                zoom(value_zoom):
                diffuse(col_white[0], col_white[1], col_white[2], col_white[3]):
                font("miso"):
                settext(value_text):
                horizalign(center)
            ));

            // Selection ring around active slot.
            if is_active {
                let mut center_x = match state.active_slot {
                    ActiveSlot::P1Primary => p1_primary_x,
                    ActiveSlot::P1Secondary => p1_secondary_x,
                    ActiveSlot::P2Primary => p2_primary_x,
                    ActiveSlot::P2Secondary => p2_secondary_x,
                };
                let mut center_y = row_mid_y;

                if state.cursor_row_anim_t < 1.0 {
                    let t = ease_out_cubic(state.cursor_row_anim_t);
                    center_y = state.cursor_row_anim_from_y
                        + (row_mid_y - state.cursor_row_anim_from_y) * t;
                }

                let ring_w = col_w * 0.9;
                let ring_h = ROW_H * s * 0.9;
                let border_w = widescale(2.0, 2.5);

                let left = center_x - ring_w * 0.5;
                let right = center_x + ring_w * 0.5;
                let top = center_y - ring_h * 0.5;
                let bottom = center_y + ring_h * 0.5;
                let mut ring_color =
                    color::decorative_rgba(state.active_color_index);
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
            // Exit row centered below everything, similar spirit to PlayerOptions.
            let exit_label = "Exit";
            let exit_y = row_mid_y;
            let choice_color = if is_active {
                col_white
            } else {
                col_gray
            };
            let exit_center_x = content_center_x;

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
                            font::measure_line_width_logical(
                                metrics_font,
                                exit_label,
                                all_fonts,
                            ) as f32;
                        if !text_w.is_finite() || text_w <= 0.0 {
                            text_w = 1.0;
                        }
                        let text_h =
                            (metrics_font.height as f32).max(1.0) * value_zoom;
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
                        let mut pad_x =
                            min_pad_x + (max_pad_x - min_pad_x) * size_t;
                        let border_w = widescale(2.0, 2.5);
                        let max_pad_by_spacing =
                            (INLINE_SPACING - border_w).max(min_pad_x);
                        if pad_x > max_pad_by_spacing {
                            pad_x = max_pad_by_spacing;
                        }
                        let mut ring_w = draw_w + pad_x * 2.0;
                        let mut ring_h = draw_h + pad_y * 2.0;

                        let mut center_x = exit_center_x;
                        let mut center_y = exit_y;
                        if state.cursor_row_anim_t < 1.0 {
                            let t = ease_out_cubic(state.cursor_row_anim_t);
                            center_y = state.cursor_row_anim_from_y
                                + (exit_y - state.cursor_row_anim_from_y) * t;
                        }

                        let left = center_x - ring_w * 0.5;
                        let right = center_x + ring_w * 0.5;
                        let top = center_y - ring_h * 0.5;
                        let bottom = center_y + ring_h * 0.5;
                        let mut ring_color =
                            color::decorative_rgba(state.active_color_index);
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
