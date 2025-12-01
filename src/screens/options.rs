use crate::act;
use crate::assets::AssetManager;
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
use crate::ui::font;

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
const LEFT_MARGIN_PX: f32 = 33.0;
const RIGHT_MARGIN_PX: f32 = 25.0;
const FIRST_ROW_TOP_MARGIN_PX: f32 = 18.0;
const BOTTOM_MARGIN_PX: f32 = 0.0;

/// Unscaled spec constants (we’ll uniformly scale).
const VISIBLE_ROWS: usize = 10; // how many rows are shown at once
// Match player_options.rs row height.
const ROW_H: f32 = 33.0;
const ROW_GAP: f32 = 2.5;
const LIST_W: f32 = 509.0;

const SEP_W: f32 = 2.5;     // gap/stripe between rows and description
const DESC_W: f32 = 292.0;  // description panel width (WideScale(287,292) in SL)
// derive description height from visible rows so it never includes a trailing gap
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

/// Left margin for row labels (in content-space pixels).
const TEXT_LEFT_PAD: f32 = 40.66;
/// Left margin for the heart icon (in content-space pixels).
const HEART_LEFT_PAD: f32 = 13.0;
/// Label text zoom, matched to the left column titles in player_options.rs.
const ITEM_TEXT_ZOOM: f32 = 0.88;

/// Heart sprite zoom for the options list rows.
/// This is a StepMania-style "zoom" factor applied to the native heart.png size.
const HEART_ZOOM: f32 = 0.026;

/// A simple item model with help text for the description box.
pub struct Item<'a> {
    name: &'a str,
    help: &'a [&'a str],
}

/// Description pane layout (mirrors Simply Love's ScreenOptionsService overlay).
/// Title and bullet list use separate top/side padding so they can be tuned independently.
const DESC_TITLE_TOP_PAD_PX: f32 = 9.75;      // padding from box top to title
const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5;     // left/right padding for title text
const DESC_BULLET_TOP_PAD_PX: f32 = 23.25;     // vertical gap between title and bullet list
const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5;    // left/right padding for bullet text
const DESC_BULLET_INDENT_PX: f32 = 10.0;      // extra indent for bullet marker + text
const DESC_TITLE_ZOOM: f32 = 1.0;            // title text zoom (roughly header-sized)
const DESC_BODY_ZOOM: f32 = 1.0;             // body/bullet text zoom (similar to help text)

pub const ITEMS: &[Item] = &[
    // Top-level ScreenOptionsService rows, ordered to match Simply Love's LineNames.
    Item {
        name: "System Options",
        help: &[
            "Adjust high-level settings like game type, theme, language, and more.",
            "Game",
            "Theme",
            "Language",
            "Announcer",
            "Default NoteSkin",
            "Editor Noteskin",
        ],
    },
    Item {
        name: "Configure Keyboard/Pad Mappings",
        help: &["Map keyboard keys, panels, menu buttons, etc. to game functions."],
    },
    Item {
        name: "Test Input",
        help: &["Test your dance pad/controller and menu buttons.\n\nIf one of your buttons is not mapped to a game function, it will appear here as \"not mapped\"."],
    },
    Item {
        name: "Input Options",
        help: &[
            "Adjust input options such as joystick automapping, dedicated menu buttons, and input debounce.",
            "AutoMap",
            "OnlyDedicatedMenu",
            "OptionsNav",
            "Debounce",
            "Three Button Navigation",
            "AxisFix",
        ],
    },
    Item {
        name: "Graphics/Sound Options",
        help: &[
            "Change screen aspect ratio, resolution, graphics quality, and miscellaneous sound options.",
            "Video Renderer",
            "DisplayMode",
            "DisplayAspectRatio",
            "DisplayResolution",
            "RefreshRate",
            "FullscreenType",
            "...",
        ],
    },
    Item {
        name: "Visual Options",
        help: &[
            "Change the way lyrics, backgrounds, etc. are displayed during gameplay; adjust overscan.",
            "Appearance Options",
            "Set Background Fit",
            "Overscan Adjustment",
            "CRT Test Patterns",
        ],
    },
    Item {
        name: "Arcade Options",
        help: &[
            "Change options typically associated with arcade games.",
            "Event",
            "Coin",
            "Coins Per Credit",
            "Maximum Credits",
            "Reset Coins At Startup",
            "Premium",
            "...",
        ],
    },
    Item {
        name: "View Bookkeeping Data",
        help: &["Check credits history"],
    },
    Item {
        name: "Advanced Options",
        help: &[
            "Adjust advanced settings for difficulty scaling, default fail type, song deletion, and more.",
            "DefaultFailType",
            "TimingWindowScale",
            "LifeDifficulty",
            "HiddenSongs",
            "EasterEggs",
            "AllowExtraStage",
            "...",
        ],
    },
    Item {
        name: "MenuTimer Options",
        help: &[
            "Turn the MenuTimer On or Off and set the MenuTimer values for various screens.",
            "MenuTimer",
            "GrooveStats Login",
            "Select Music",
            "Select Music Casual Mode",
            "Player Options",
            "Evaluation",
            "...",
        ],
    },
    Item {
        name: "USB Profile Options",
        help: &[
            "Adjust settings related to USB Profiles, including loading custom songs from USB sticks.",
            "USB Profiles",
            "CustomSongs",
            "Max Songs per USB",
            "Song Load Timeout",
            "Song Duration Limit",
            "Song File Size Limit",
        ],
    },
    Item {
        name: "Manage Local Profiles",
        help: &["Create, edit, and manage player profiles that are stored on this computer.\n\nYou'll need a keyboard to use this screen."],
    },
    Item {
        name: "Simply Love Options",
        help: &[
            "Adjust settings that only apply to this Simply Love theme.",
            "Visual Style",
            "Rainbow Mode",
            "MusicWheel Scroll Speed",
            "MusicWheel Style",
            "Preferred Style",
            "Default Game Mode",
            "...",
        ],
    },
    Item {
        name: "Tournament Mode Options",
        help: &[
            "Adjust settings to enforce for consistency during tournament play.",
            "Enable Tournament Mode",
            "Scoring System",
            "Step Stats",
            "Enforce No Cmod",
        ],
    },
    Item {
        name: "GrooveStats Options",
        help: &[
            "Manage GrooveStats settings.",
            "Enable GrooveStats",
            "Auto-Download Unlocks",
            "Separate Unlocks By Player",
            "Display GrooveStats QR Login",
        ],
    },
    Item {
        name: "StepMania Credits",
        help: &["Celebrate those who made StepMania possible."],
    },
    Item {
        name: "Clear Credits",
        help: &["Reset coin credits to 0."],
    },
    Item {
        name: "Reload Songs/Courses",
        help: &["Reload all songs and courses from disk without restarting."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main menu."],
    },
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
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL {
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
    // available height after the fixed top margin (inside content area),
    // and before an adjustable bottom margin.
    let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

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

pub fn get_actors(state: &State, asset_manager: &AssetManager, alpha_multiplier: f32) -> Vec<Actor> {
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

    // Active text color (for normal rows) – Simply Love uses row index + global color index.
    let col_active_text = color::simply_love_rgba(state.active_color_index + state.selected as i32);

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
        let row_mid_y   = row_y + 0.5 * ROW_H * s;
        let heart_x     = list_x + HEART_LEFT_PAD * s;
        let text_x_base = list_x + TEXT_LEFT_PAD * s;

        // Heart sprite (skip for Exit)
        if !is_exit {
            let heart_tint = if is_active { col_active_text } else { col_white };
            ui_actors.push(act!(sprite("heart.png"):
                align(0.0, 0.5):
                xy(heart_x, row_mid_y):
                zoom(HEART_ZOOM):
                diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
            ));
        }

        // Text (Miso)
        let text_x = if is_exit {
            // no heart => start at left pad
            text_x_base
        } else {
            text_x_base
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
            align(0.0, 0.5):
            xy(text_x, row_mid_y):
            zoom(ITEM_TEXT_ZOOM):
            diffuse(color_t[0], color_t[1], color_t[2], color_t[3]):
            font("miso"):
            settext(label):
            horizalign(left)
        ));
    }

    // ------------------- Description content (selected) -------------------
    let sel = state.selected.min(ITEMS.len() - 1);
    let item = &ITEMS[sel];

    // Match Simply Love's description box feel:
    // - explicit top/side padding for title and bullets so they can be tuned
    // - text zoom similar to other help text (player options, etc.)
    let mut cursor_y = list_y + DESC_TITLE_TOP_PAD_PX * s;
    let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
    let title_step_px = 20.0 * s; // approximate vertical advance for title line

    // Title/explanation text:
    // - For any item with help lines, use the first help line as the long explanation,
    //   with remaining lines rendered as the bullet list (if any).
    // - Fallback to the item name if there is no help text.
    let help = item.help;
    let (raw_title_text, bullet_lines): (&str, &[&str]) =
        if help.is_empty() {
            (item.name, &[][..])
        } else {
            (help[0], &help[1..])
        };

    // Word-wrapping using actual font metrics so the title respects the
    // description box's inner width and padding exactly.
    let wrapped_title = asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |miso_font| {
                let max_width_px = (DESC_W * s) - 2.0 * DESC_TITLE_SIDE_PAD_PX * s;
                let mut out = String::new();
                let mut is_first_output_line = true;

                for segment in raw_title_text.split('\n') {
                    let trimmed = segment.trim_end();
                    if trimmed.is_empty() {
                        // Preserve explicit blank lines (e.g. \"\\n\\n\" in help text)
                        // as an empty row between wrapped paragraphs.
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
                            font::measure_line_width_logical(miso_font, &candidate, all_fonts) as f32;
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
                    raw_title_text.to_string()
                } else {
                    out
                }
            })
        })
        .unwrap_or_else(|| raw_title_text.to_string());
    let title_lines = wrapped_title.lines().count().max(1) as f32;

    // Draw the wrapped explanation/title text.
    ui_actors.push(act!(text:
        align(0.0, 0.0):
        xy(desc_x + title_side_pad, cursor_y):
        zoom(DESC_TITLE_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"): settext(wrapped_title):
        horizalign(left)
    ));
    cursor_y += title_step_px * title_lines + DESC_BULLET_TOP_PAD_PX * s;

    // Optional bullet list (e.g. System Options: Game / Theme / Language / ...).
    if !bullet_lines.is_empty() {
        let mut bullet_text = String::new();
        let mut first = true;
        for line in bullet_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !first {
                bullet_text.push('\n');
            }
            // Ellipsis lines ("...") should not have a bullet to match Simply Love.
            if trimmed == "..." {
                bullet_text.push_str("...");
            } else {
                bullet_text.push('•');
                bullet_text.push(' ');
                bullet_text.push_str(line);
            }
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

    for actor in &mut ui_actors {
        apply_alpha_to_actor(actor, alpha_multiplier);
    }
    actors.extend(ui_actors);

    actors
}
