use crate::act;
use crate::core::space::*;
// Screen navigation is handled in app.rs via the dispatcher
use crate::core::audio;
use crate::screens::{Screen, ScreenAction};
use crate::core::input::{VirtualAction, InputEvent};
use std::time::{Duration, Instant};

use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::components::{heart_bg, screen_bar};
use crate::ui::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::actors;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

/// Bars in `screen_bar.rs` use 32.0 px height.
const BAR_H: f32 = 32.0;

/// Screen-space margins (pixels, not scaled)
const LEFT_MARGIN_PX: f32 = 25.0;
const RIGHT_MARGIN_PX: f32 = 17.0;
const FIRST_ROW_TOP_MARGIN_PX: f32 = 17.0;

/// Unscaled spec constants (we’ll uniformly scale).
const VISIBLE_ROWS: usize = 10; // how many rows are shown at once
const ROW_H: f32 = 55.0;
const ROW_GAP: f32 = 3.0;
const LIST_W: f32 = 721.0;

const SEP_W: f32 = 3.0;     // gap/stripe between rows and description
const DESC_W: f32 = 484.0;  // description panel width
// derive description height from visible rows so it never includes a trailing gap
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

/// Text sizing (unscaled). Picked to sit nicely inside 55px rows.
const TEXT_PX: f32 = 26.0;
const TEXT_LEFT_PAD: f32 = 19.0; // padding inside a row before the heart
const HEART_TEXT_GAP: f32 = 17.0;

/// Baseline nudge for row labels (screen pixels, not scaled)
const TEXT_BASELINE_NUDGE_PX: f32 = 1.0;

/// Heart native aspect (for aspect-correct scaling).
const HEART_NATIVE_W: f32 = 668.0;
const HEART_NATIVE_H: f32 = 566.0;
const HEART_ASPECT: f32 = HEART_NATIVE_W / HEART_NATIVE_H;

/// A simple item model with help text for the description box.
pub struct Item<'a> {
    name: &'a str,
    help: &'a [&'a str],
}

pub const ITEMS: &[Item] = &[
    Item { name: "System Options",                  help: &["Game", "Theme", "Language", "Announcer", "Default NoteSkin", "Editor Noteskin"] },
    Item { name: "Configure Keyboard/Pad Mappings", help: &["Bind keys/buttons for each player."] },
    Item { name: "Test Input",                      help: &["View live input state for debugging."] },
    Item { name: "Input Options",                   help: &["Debounce, menu buttons, coin mode…"] },
    Item { name: "Graphics/Sound Options",          help: &["Resolution, VSync, sound device…"] },
    Item { name: "Visual Options",                  help: &["Judgment, combo, lifebar, etc."] },
    Item { name: "Arcade Options",                  help: &["Coin mode, premium, attract mode…"] },
    Item { name: "View Bookkeeping Data",           help: &["Audit play counts, coins, uptime."] },
    Item { name: "Advanced Options",                help: &["Low-level engine toggles."] },
    Item { name: "MenuTimer Options",               help: &["Per-screen time limits."] },
    Item { name: "Network Options",                 help: &["Online features, matchmaking, latency…"] },
    Item { name: "Profiles",                        help: &["Create, select, and edit player profiles."] },
    Item { name: "Theme Options",                   help: &["UI skin, colorway, layout, accessibility."] },
    Item { name: "Data Management",                 help: &["Save data, screenshots, logs, cache."] },
    Item { name: "Service Options",                 help: &["Cabinet/service settings for operators."] },
    Item { name: "Credits",                         help: &["Project contributors and licenses."] },
    Item { name: "Exit",                            help: &["Return to the main menu."] },
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
}

pub struct State {
    pub selected: usize,
    prev_selected: usize,
    pub active_color_index: i32, // <-- ADDED
    bg: heart_bg::State,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
}

pub fn init() -> State {
    State {
        selected: 0,
        prev_selected: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX, // <-- ADDED
        bg: heart_bg::State::new(),

        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
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

/* --------------------------------- input --------------------------------- */

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

pub fn update(state: &mut State, _dt: f32) {
    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) =
        (state.nav_key_held_direction, state.nav_key_held_since, state.nav_key_last_scrolled_at)
    {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY {
            if now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL {
                let total = ITEMS.len();
                if total > 0 {
                    match direction {
                        NavDirection::Up => {
                            state.selected = if state.selected == 0 { total - 1 } else { state.selected - 1 };
                        }
                        NavDirection::Down => {
                            state.selected = (state.selected + 1) % total;
                        }
                    }
                    state.nav_key_last_scrolled_at = Some(now);
                }
            }
        }
    }

    if state.selected != state.prev_selected {
        audio::play_sfx("assets/sounds/change.ogg");
        state.prev_selected = state.selected;
    }
}

// Small helpers to let the app dispatcher manage hold-to-scroll without exposing fields
pub fn on_nav_press(state: &mut State, dir: NavDirection) {
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(Instant::now());
    state.nav_key_last_scrolled_at = Some(Instant::now());
}

pub fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    match ev.action {
        VirtualAction::p1_back if ev.pressed => return ScreenAction::Navigate(Screen::Menu),
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if ev.pressed {
                let total = ITEMS.len();
                if total > 0 {
                    state.selected = if state.selected == 0 { total - 1 } else { state.selected - 1 };
                }
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if ev.pressed {
                let total = ITEMS.len();
                if total > 0 {
                    state.selected = (state.selected + 1) % total;
                }
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_start if ev.pressed => {
            let total = ITEMS.len();
            if total > 0 && state.selected == total - 1 {
                return ScreenAction::Navigate(Screen::Menu);
            }
        }
        _ => {}
    }
    ScreenAction::None
}

/* --------------------------------- layout -------------------------------- */

/// content rect = full screen minus top & bottom bars.
/// We fit the (rows + separator + description) block inside that content rect,
/// honoring LEFT, RIGHT and TOP margins in *screen pixels*.
/// Returns (scale, origin_x, origin_y).
fn scaled_block_origin_with_margins() -> (f32, f32, f32) {
    let total_w = LIST_W + SEP_W + DESC_W;
    let total_h = DESC_H;

    let sw = screen_width();
    let sh = screen_height();

    // content area (between bars)
    let content_top = BAR_H;
    let content_bottom = sh - BAR_H;
    let content_h = (content_bottom - content_top).max(0.0);

    // available width between fixed left/right gutters
    let avail_w = (sw - LEFT_MARGIN_PX - RIGHT_MARGIN_PX).max(0.0);
    // available height after the fixed top margin (inside content area)
    let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX).max(0.0);

    // candidate scales
    let s_w = if total_w > 0.0 { avail_w / total_w } else { 1.0 };
    let s_h = if total_h > 0.0 { avail_h / total_h } else { 1.0 };
    let s = s_w.min(s_h).max(0.0);

    // X origin:
    // Right-align inside [LEFT..(sw-RIGHT)] so the description box ends exactly
    // RIGHT_MARGIN_PX from the screen edge.
    let ox = LEFT_MARGIN_PX + (avail_w - total_w * s).max(0.0);

    // Y origin is fixed under the top bar by the requested margin.
    let oy = content_top + FIRST_ROW_TOP_MARGIN_PX;

    (s, ox, oy)
}

/* -------------------------------- drawing -------------------------------- */

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Frame { background, children, .. } => {
            if let Some(actors::Background::Color(c)) = background {
                c[3] *= alpha;
            }
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Shadow { color, child, .. } => {
            // Apply alpha to the shadow tint and recurse to the child.
            color[3] *= alpha;
            apply_alpha_to_actor(child, alpha);
        }
    }
}

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(320);

    /* -------------------------- HEART BACKGROUND -------------------------- */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index, // <-- CHANGED
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui_actors = Vec::new();

    /* ------------------------------ TOP BAR ------------------------------- */
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    ui_actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "OPTIONS",
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

    // --- global colors ---
    let col_active_bg  = color::rgba_hex("#333333"); // active bg for normal rows

    // inactive bg = #071016 @ 0.8 alpha
    let base_inactive  = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];

    let col_white      = [1.0, 1.0, 1.0, 1.0];
    let col_black      = [0.0, 0.0, 0.0, 1.0];

    // Simply Love brand color (now uses the active theme color).
    let col_brand_bg   = color::simply_love_rgba(state.active_color_index); // <-- CHANGED

    // Active text color (for normal rows) – keep using a palette color keyed by selection.
    let col_active_text = color::simply_love_rgba(state.selected as i32);

    // --- scale & origin honoring fixed screen-space margins ---
    let (s, list_x, list_y) = scaled_block_origin_with_margins();

    // Geometry (scaled)
    let list_w = LIST_W * s;
    let sep_w  = SEP_W * s;
    let desc_w = DESC_W * s;
    let desc_h = DESC_H * s;

    // Separator immediately to the RIGHT of the rows, aligned to the FIRST row top
    ui_actors.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x + list_w, list_y):
        zoomto(sep_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3]) // #333333
    ));

    // Description box (RIGHT of separator), aligned to the first row top
    let desc_x = list_x + list_w + sep_w;
    ui_actors.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, list_y):
        zoomto(desc_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3]) // #333333
    ));

    // ---------------------------- Scrolling math ---------------------------
    let total_items = ITEMS.len();
    let anchor_row: usize = 4; // keep cursor near middle (5th visible row)
    let max_offset = total_items.saturating_sub(VISIBLE_ROWS);
    let offset_rows = if total_items <= VISIBLE_ROWS {
        0
    } else {
        state.selected.saturating_sub(anchor_row).min(max_offset)
    };

    // Row loop (backgrounds + content). We render the visible window.
    for i_vis in 0..VISIBLE_ROWS {
        let item_idx = offset_rows + i_vis;
        if item_idx >= total_items { break; }

        let row_y = list_y + (i_vis as f32) * (ROW_H + ROW_GAP) * s;

        let is_active = item_idx == state.selected;
        let is_exit   = item_idx == total_items - 1;

        // Row background width:
        // - Exit: always keep the 3px gap (even when active)
        // - Normal items: inactive keeps gap; active touches the separator
        let row_w = if is_exit {
            list_w - sep_w
        } else if is_active {
            list_w
        } else {
            list_w - sep_w
        };

        // Choose bg color with special case for active Exit row
        let bg = if is_active {
            if is_exit { col_brand_bg } else { col_active_bg }
        } else {
            col_inactive_bg
        };

        ui_actors.push(act!(quad:
            align(0.0, 0.0):
            xy(list_x, row_y):
            zoomto(row_w, ROW_H * s):
            diffuse(bg[0], bg[1], bg[2], bg[3])
        ));

        // Content placement inside row
        let row_mid_y = row_y + 0.5 * ROW_H * s;
        let text_h    = TEXT_PX * s;

        // Heart/icon sizing
        let heart_h = text_h;
        let heart_w = heart_h * HEART_ASPECT;

        // Left padding INSIDE the row
        let content_left = list_x + TEXT_LEFT_PAD * s;

        // Heart sprite (skip for Exit)
        if !is_exit {
            let heart_tint = if is_active { col_active_text } else { col_white };
            ui_actors.push(act!(sprite("heart.png"):
                align(0.0, 0.5):
                xy(content_left, row_mid_y):
                zoomto(heart_w, heart_h):
                diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
            ));
        }

        // Text (Miso)
        let text_x = if is_exit {
            // no heart => start at left pad
            content_left
        } else {
            // heart + gap
            content_left + heart_w + HEART_TEXT_GAP * s
        };

        let label = ITEMS[item_idx].name;

        // Exit text: white when inactive; black when active.
        let color_t = if is_exit {
            if is_active { col_black } else { col_white }
        } else if is_active {
            col_active_text
        } else {
            col_white
        };

        ui_actors.push(act!(text:
            align(0.0, 0.0):
            xy(text_x, row_mid_y - 0.5 * text_h + TEXT_BASELINE_NUDGE_PX):
            zoomtoheight(text_h):
            diffuse(color_t[0], color_t[1], color_t[2], color_t[3]):
            font("miso"):
            settext(label):
            horizalign(left)
        ));
    }

    // ------------------- Description content (selected) -------------------
    let sel = state.selected.min(ITEMS.len() - 1);
    let title_px = 28.0 * s;
    let body_px  = 28.0 * s;

    let desc_pad_x = 18.0 * s;
    let mut cursor_y = list_y + 18.0 * s;

    // Title (selected item name)
    ui_actors.push(act!(text:
        align(0.0, 0.0):
        xy(desc_x + desc_pad_x, cursor_y):
        zoomtoheight(title_px):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"): settext(ITEMS[sel].name):
        horizalign(left)
    ));
    cursor_y += title_px + 12.0 * s;

    // Help text
    for &line in ITEMS[sel].help {
        ui_actors.push(act!(text:
            align(0.0, 0.0):
            xy(desc_x + desc_pad_x + 12.0 * s, cursor_y):
            zoomtoheight(body_px):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"): settext(line):
            horizalign(left)
        ));
        cursor_y += body_px + 8.0 * s;
    }

    for actor in &mut ui_actors {
        apply_alpha_to_actor(actor, alpha_multiplier);
    }
    actors.extend(ui_actors);

    actors
}
