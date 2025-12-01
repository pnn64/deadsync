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

/* ----------------------------- cursor tweening ----------------------------- */
// Match Simply Love's CursorTweenSeconds for OptionRow cursor movement
const CURSOR_TWEEN_SECONDS: f32 = 0.1;
// Spacing between inline items in OptionRows (pixels at current zoom)
const INLINE_SPACING: f32 = 15.75;

#[inline(always)]
fn ease_out_cubic(t: f32) -> f32 {
    let clamped = if t < 0.0 { 0.0 } else if t > 1.0 { 1.0 } else { t };
    let u = 1.0 - clamped;
    1.0 - u * u * u
}

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
/// Width of the System Options submenu label column (content-space pixels).
const SUB_LABEL_COL_W: f32 = 142.5;
/// Left padding for text inside the System Options submenu label column.
const SUB_LABEL_TEXT_LEFT_PAD: f32 = 11.0;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsView {
    Main,
    SystemSubmenu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmenuTransition {
    None,
    FadeOutToSubmenu,
    FadeInSubmenu,
    FadeOutToMain,
    FadeInMain,
}

// Local fade timing when swapping between main options list and System Options submenu.
const SUBMENU_FADE_DURATION: f32 = 0.2;

pub struct SubRow<'a> {
    pub label: &'a str,
    pub choices: &'a [&'a str],
}

pub const SYSTEM_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: "Game",
        choices: &["dance", "pump"],
    },
    SubRow {
        label: "Theme",
        choices: &["Simply Love"],
    },
    SubRow {
        label: "Language",
        choices: &["English", "Japanese"],
    },
    SubRow {
        label: "Announcer",
        choices: &["None", "ITG"],
    },
    SubRow {
        label: "Default NoteSkin",
        choices: &["cel", "metal", "enchantment-v2", "devcel-2024-v3"],
    },
    SubRow {
        label: "Editor NoteSkin",
        choices: &["cel", "metal", "enchantment-v2", "devcel-2024-v3"],
    },
];

pub const SYSTEM_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: "Game",
        help: &["Select the default game type used by the engine."],
    },
    Item {
        name: "Theme",
        help: &["Choose which theme is active."],
    },
    Item {
        name: "Language",
        help: &["Select the active language for menus and prompts."],
    },
    Item {
        name: "Announcer",
        help: &["Enable or change the gameplay announcer."],
    },
    Item {
        name: "Default NoteSkin",
        help: &["Choose the default noteskin used in gameplay."],
    },
    Item {
        name: "Editor NoteSkin",
        help: &["Choose the noteskin used in the step editor."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub struct State {
    pub selected: usize,
    prev_selected: usize,
    pub active_color_index: i32, // <-- ADDED
    bg: heart_bg::State,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    view: OptionsView,
    submenu_transition: SubmenuTransition,
    submenu_fade_t: f32,
    content_alpha: f32,
    // System Options submenu state
    sub_selected: usize,
    sub_prev_selected: usize,
    sub_choice_indices: Vec<usize>,
    // Inline option cursor tween (left/right between items)
    cursor_anim_row: Option<usize>,
    cursor_anim_from_choice: usize,
    cursor_anim_to_choice: usize,
    cursor_anim_t: f32,
    // Vertical tween when changing selected row
    cursor_row_anim_from_y: f32,
    cursor_row_anim_t: f32,
    cursor_row_anim_from_row: Option<usize>,
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
        submenu_transition: SubmenuTransition::None,
        submenu_fade_t: 0.0,
        content_alpha: 1.0,
        view: OptionsView::Main,
        sub_selected: 0,
        sub_prev_selected: 0,
        sub_choice_indices: vec![0; SYSTEM_OPTIONS_ROWS.len()],
        cursor_anim_row: None,
        cursor_anim_from_choice: 0,
        cursor_anim_to_choice: 0,
        cursor_anim_t: 1.0,
        cursor_row_anim_from_y: 0.0,
        cursor_row_anim_t: 1.0,
        cursor_row_anim_from_row: None,
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

pub fn update(state: &mut State, dt: f32) {
    // ------------------------- local submenu fade ------------------------- //
    match state.submenu_transition {
        SubmenuTransition::None => {
            state.content_alpha = 1.0;
        }
        SubmenuTransition::FadeOutToSubmenu => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = 1.0 - state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                // Switch view to the System Options submenu, then fade it in.
                state.view = OptionsView::SystemSubmenu;
                state.sub_selected = 0;
                state.sub_prev_selected = 0;
                state.cursor_anim_row = None;
                state.cursor_anim_t = 1.0;
                state.cursor_row_anim_t = 1.0;
                state.cursor_row_anim_from_row = None;
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                state.submenu_transition = SubmenuTransition::FadeInSubmenu;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 0.0;
            }
        }
        SubmenuTransition::FadeInSubmenu => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                state.submenu_transition = SubmenuTransition::None;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 1.0;
            }
        }
        SubmenuTransition::FadeOutToMain => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = 1.0 - state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                // Return to the main options list and fade it in.
                state.view = OptionsView::Main;
                state.cursor_anim_row = None;
                state.cursor_anim_t = 1.0;
                state.cursor_row_anim_t = 1.0;
                state.cursor_row_anim_from_row = None;
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                state.submenu_transition = SubmenuTransition::FadeInMain;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 0.0;
            }
        }
        SubmenuTransition::FadeInMain => {
            let step = if SUBMENU_FADE_DURATION > 0.0 {
                dt / SUBMENU_FADE_DURATION
            } else {
                1.0
            };
            state.submenu_fade_t = (state.submenu_fade_t + step).min(1.0);
            state.content_alpha = state.submenu_fade_t;
            if state.submenu_fade_t >= 1.0 {
                state.submenu_transition = SubmenuTransition::None;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 1.0;
            }
        }
    }

    // While fading, freeze hold-to-scroll to avoid odd jumps.
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return;
    }

    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) =
        (state.nav_key_held_direction, state.nav_key_held_since, state.nav_key_last_scrolled_at)
    {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL {
                match state.view {
                    OptionsView::Main => {
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
                    OptionsView::SystemSubmenu => {
                        let total = SYSTEM_OPTIONS_ROWS.len() + 1; // + Exit row
                        if total > 0 {
                            match direction {
                                NavDirection::Up => {
                                    state.sub_selected = if state.sub_selected == 0 { total - 1 } else { state.sub_selected - 1 };
                                }
                                NavDirection::Down => {
                                    state.sub_selected = (state.sub_selected + 1) % total;
                                }
                            }
                            state.nav_key_last_scrolled_at = Some(now);
                        }
                    }
                }
            }
    }

    match state.view {
        OptionsView::Main => {
            if state.selected != state.prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");
                state.prev_selected = state.selected;
            }
        }
        OptionsView::SystemSubmenu => {
            if state.sub_selected != state.sub_prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");

                // Start a simple vertical cursor tween between rows in the submenu.
                let (s, _, list_y) = scaled_block_origin_with_margins();
                let prev_idx = state.sub_prev_selected;
                let from_y = list_y + (prev_idx as f32) * (ROW_H + ROW_GAP) * s;
                state.cursor_row_anim_from_y = from_y + 0.5 * ROW_H * s;
                state.cursor_row_anim_t = 0.0;
                state.cursor_row_anim_from_row = Some(prev_idx);

                state.sub_prev_selected = state.sub_selected;
            }
        }
    }

    // Advance cursor tween, if any (submenu only).
    if state.cursor_anim_row.is_some() && state.cursor_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_anim_t = (state.cursor_anim_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_anim_t = 1.0;
        }
        if state.cursor_anim_t >= 1.0 {
            state.cursor_anim_row = None;
        }
    }
    if state.cursor_row_anim_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_row_anim_t = (state.cursor_row_anim_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_row_anim_t = 1.0;
        }
        if state.cursor_row_anim_t >= 1.0 {
            state.cursor_row_anim_from_row = None;
        }
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

fn apply_submenu_choice_delta(state: &mut State, delta: isize) {
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return;
    }
    if state.view != OptionsView::SystemSubmenu {
        return;
    }
    let rows_len = SYSTEM_OPTIONS_ROWS.len();
    if rows_len == 0 {
        return;
    }
    let row_index = state.sub_selected;
    if row_index >= rows_len {
        // Exit row – no choices to change.
        return;
    }

    let choice_index = state.sub_choice_indices[row_index];
    let num_choices = SYSTEM_OPTIONS_ROWS[row_index].choices.len();
    if num_choices == 0 {
        return;
    }
    let cur = choice_index as isize;
    let n = num_choices as isize;
    let mut new_index = ((cur + delta).rem_euclid(n)) as usize;
    if new_index >= num_choices {
        new_index = num_choices.saturating_sub(1);
    }
    if new_index == choice_index {
        return;
    }

    state.sub_choice_indices[row_index] = new_index;
    audio::play_sfx("assets/sounds/change_value.ogg");

    // Begin cursor animation for inline Off/On choices.
    state.cursor_anim_row = Some(row_index);
    state.cursor_anim_from_choice = choice_index;
    state.cursor_anim_to_choice = new_index;
    state.cursor_anim_t = 0.0;
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    // Ignore new navigation while a local submenu fade is in progress.
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back if ev.pressed => {
            match state.view {
                OptionsView::Main => return ScreenAction::Navigate(Screen::Menu),
                OptionsView::SystemSubmenu => {
                    // Fade back to the main Options list.
                    state.submenu_transition = SubmenuTransition::FadeOutToMain;
                    state.submenu_fade_t = 0.0;
                }
            }
        }
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if ev.pressed {
                match state.view {
                    OptionsView::Main => {
                        let total = ITEMS.len();
                        if total > 0 {
                            state.selected = if state.selected == 0 { total - 1 } else { state.selected - 1 };
                        }
                    }
                    OptionsView::SystemSubmenu => {
                        let total = SYSTEM_OPTIONS_ROWS.len() + 1;
                        if total > 0 {
                            state.sub_selected = if state.sub_selected == 0 { total - 1 } else { state.sub_selected - 1 };
                        }
                    }
                }
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if ev.pressed {
                match state.view {
                    OptionsView::Main => {
                        let total = ITEMS.len();
                        if total > 0 {
                            state.selected = (state.selected + 1) % total;
                        }
                    }
                    OptionsView::SystemSubmenu => {
                        let total = SYSTEM_OPTIONS_ROWS.len() + 1;
                        if total > 0 {
                            state.sub_selected = (state.sub_selected + 1) % total;
                        }
                    }
                }
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if ev.pressed {
                apply_submenu_choice_delta(state, -1);
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if ev.pressed {
                apply_submenu_choice_delta(state, 1);
            }
        }
        VirtualAction::p1_start if ev.pressed => {
            match state.view {
                OptionsView::Main => {
                    let total = ITEMS.len();
                    if total == 0 {
                        return ScreenAction::None;
                    }
                    // Last row is Exit from the Options screen back to the main menu.
                    if state.selected == total - 1 {
                        return ScreenAction::Navigate(Screen::Menu);
                    }
                    // Enter System Options submenu when selecting the first row for now.
                    if state.selected == 0 {
                        state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                        state.submenu_fade_t = 0.0;
                    }
                }
                OptionsView::SystemSubmenu => {
                    let total = SYSTEM_OPTIONS_ROWS.len() + 1;
                    if total == 0 {
                        return ScreenAction::None;
                    }
                    // Exit row in the submenu: back to the main Options list.
                    if state.sub_selected == total - 1 {
                        state.submenu_transition = SubmenuTransition::FadeOutToMain;
                        state.submenu_fade_t = 0.0;
                    }
                }
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
    let is_fading_submenu = !matches!(state.submenu_transition, SubmenuTransition::None);

    /* -------------------------- HEART BACKGROUND -------------------------- */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index, // <-- CHANGED
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        // Only participate in global screen fades (alpha_multiplier), not local submenu fades.
        alpha_mul: alpha_multiplier,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui_actors = Vec::new();

    /* ------------------------------ TOP BAR ------------------------------- */
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let title_text = match state.view {
        OptionsView::Main => "OPTIONS",
        OptionsView::SystemSubmenu => "SYSTEM OPTIONS",
    };
    ui_actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: title_text,
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

    // -------------------------- Rows + Description -------------------------
    let mut selected_item: Option<&Item> = None;

    match state.view {
        OptionsView::Main => {
            // Active text color (for normal rows) – Simply Love uses row index + global color index.
            let col_active_text = color::simply_love_rgba(state.active_color_index + state.selected as i32);

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

            let sel = state.selected.min(ITEMS.len() - 1);
            selected_item = Some(&ITEMS[sel]);
        }
        OptionsView::SystemSubmenu => {
            // Active text color for submenu rows.
            let col_active_text = color::simply_love_rgba(state.active_color_index);
            // Inactive option text color should be #808080 (alpha 1.0), match player options.
            let sl_gray = color::rgba_hex("#808080");

            let total_rows = SYSTEM_OPTIONS_ROWS.len() + 1; // + Exit row
            let anchor_row: usize = 4;
            let max_offset = total_rows.saturating_sub(VISIBLE_ROWS);
            let offset_rows = if total_rows <= VISIBLE_ROWS {
                0
            } else {
                state.sub_selected.saturating_sub(anchor_row).min(max_offset)
            };

            let label_bg_w = SUB_LABEL_COL_W * s;
            let label_text_x = list_x + SUB_LABEL_TEXT_LEFT_PAD * s;

            // Helper to compute the cursor center X for a given submenu row index.
            let calc_row_center_x = |row_idx: usize| -> f32 {
                if row_idx >= total_rows {
                    return list_x + list_w * 0.5;
                }
                if row_idx >= SYSTEM_OPTIONS_ROWS.len() {
                    // Exit row is centered in the items column.
                    return list_x + list_w * 0.5;
                }
                let choices = SYSTEM_OPTIONS_ROWS[row_idx].choices;
                if choices.is_empty() {
                    return list_x + list_w * 0.5;
                }
                let value_zoom = 0.835_f32;
                let choice_inner_left = list_x + label_bg_w + 16.0 * s;
                let mut widths: Vec<f32> = Vec::with_capacity(choices.len());
                asset_manager.with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        for text in choices {
                            let mut w =
                                font::measure_line_width_logical(metrics_font, text, all_fonts)
                                    as f32;
                            if !w.is_finite() || w <= 0.0 {
                                w = 1.0;
                            }
                            widths.push(w * value_zoom);
                        }
                    });
                });
                if widths.is_empty() {
                    return list_x + list_w * 0.5;
                }
                let mut x_positions: Vec<f32> = Vec::with_capacity(widths.len());
                let mut x = choice_inner_left;
                for w in &widths {
                    x_positions.push(x);
                    x += *w + INLINE_SPACING;
                }
                let sel_idx = state
                    .sub_choice_indices
                    .get(row_idx)
                    .copied()
                    .unwrap_or(0)
                    .min(widths.len().saturating_sub(1));
                x_positions[sel_idx] + widths[sel_idx] * 0.5
            };

            // Helper to compute draw_w/draw_h (text box) for the selected item of a submenu row.
            let calc_row_dims = |row_idx: usize| -> (f32, f32) {
                let value_zoom = 0.835_f32;
                let mut out_w = 40.0_f32;
                let mut out_h = 16.0_f32;
                asset_manager.with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        out_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                        if row_idx >= SYSTEM_OPTIONS_ROWS.len() {
                            // Exit row
                            let text = "Exit";
                            let mut w =
                                font::measure_line_width_logical(metrics_font, text, all_fonts)
                                    as f32;
                            if !w.is_finite() || w <= 0.0 {
                                w = 1.0;
                            }
                            out_w = w * value_zoom;
                        } else {
                            let choices = SYSTEM_OPTIONS_ROWS[row_idx].choices;
                            if choices.is_empty() {
                                return;
                            }
                            let sel_idx = state
                                .sub_choice_indices
                                .get(row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(choices.len().saturating_sub(1));
                            let mut w =
                                font::measure_line_width_logical(metrics_font, choices[sel_idx], all_fonts)
                                    as f32;
                            if !w.is_finite() || w <= 0.0 {
                                w = 1.0;
                            }
                            out_w = w * value_zoom;
                        }
                    });
                });
                (out_w, out_h)
            };

            for i_vis in 0..VISIBLE_ROWS {
                let row_idx = offset_rows + i_vis;
                if row_idx >= total_rows {
                    break;
                }

                let row_y = list_y + (i_vis as f32) * (ROW_H + ROW_GAP) * s;
                let row_mid_y = row_y + 0.5 * ROW_H * s;

                let is_active = row_idx == state.sub_selected;
                let is_exit = row_idx == total_rows - 1;

                let row_w = if is_exit {
                    list_w - sep_w
                } else if is_active {
                    list_w
                } else {
                    list_w - sep_w
                };

                let bg = if is_active { col_active_bg } else { col_inactive_bg };

                ui_actors.push(act!(quad:
                    align(0.0, 0.0):
                    xy(list_x, row_y):
                    zoomto(row_w, ROW_H * s):
                    diffuse(bg[0], bg[1], bg[2], bg[3])
                ));

                if !is_exit && row_idx < SYSTEM_OPTIONS_ROWS.len() {
                    // Left label background column (matches player options style).
                    ui_actors.push(act!(quad:
                        align(0.0, 0.0):
                        xy(list_x, row_y):
                        zoomto(label_bg_w, ROW_H * s):
                        diffuse(0.0, 0.0, 0.0, 0.25)
                    ));

                    let label = SYSTEM_OPTIONS_ROWS[row_idx].label;
                    let title_color = if is_active {
                        let mut c = col_active_text;
                        c[3] = 1.0;
                        c
                    } else {
                        col_white
                    };

                    ui_actors.push(act!(text:
                        align(0.0, 0.5):
                        xy(label_text_x, row_mid_y):
                        zoom(ITEM_TEXT_ZOOM):
                        diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                        font("miso"):
                        settext(label):
                        horizalign(left)
                    ));

                    // Inline Off/On options in the items column.
                    let choices = SYSTEM_OPTIONS_ROWS[row_idx].choices;
                    if !choices.is_empty() {
                        let value_zoom = 0.835_f32;
                        let mut widths: Vec<f32> = Vec::with_capacity(choices.len());
                        asset_manager.with_fonts(|all_fonts| {
                            asset_manager.with_font("miso", |metrics_font| {
                                for text in choices {
                                    let mut w = font::measure_line_width_logical(metrics_font, text, all_fonts) as f32;
                                    if !w.is_finite() || w <= 0.0 {
                                        w = 1.0;
                                    }
                                    widths.push(w * value_zoom);
                                }
                            });
                        });

                        let choice_inner_left = list_x + label_bg_w + 16.0 * s;
                        let mut x_positions: Vec<f32> = Vec::with_capacity(choices.len());
                        {
                            let mut x = choice_inner_left;
                            for w in &widths {
                                x_positions.push(x);
                                x += *w + INLINE_SPACING;
                            }
                        }

                        let selected_choice = state.sub_choice_indices
                            .get(row_idx)
                            .copied()
                            .unwrap_or(0)
                            .min(choices.len().saturating_sub(1));

                        for (idx, choice) in choices.iter().enumerate() {
                            let x = x_positions.get(idx).copied().unwrap_or(choice_inner_left);
                            let choice_color = if is_active { col_white } else { sl_gray };

                            ui_actors.push(act!(text:
                                align(0.0, 0.5):
                                xy(x, row_mid_y):
                                zoom(value_zoom):
                                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                                font("miso"):
                                settext(*choice):
                                horizalign(left)
                            ));
                        }

                        // Underline the selected option when this row is active or inactive,
                        // matching the inline underline behavior from player_options.rs.
                        if let Some(sel_x) = x_positions.get(selected_choice).copied() {
                            let draw_w = widths.get(selected_choice).copied().unwrap_or(40.0);
                            asset_manager.with_fonts(|_all_fonts| {
                                asset_manager.with_font("miso", |metrics_font| {
                                    let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                                    let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                    let underline_w = draw_w.ceil();
                                    let offset = widescale(3.0, 4.0);
                                    let underline_y = row_mid_y + text_h * 0.5 + offset;
                                    let mut line_color = color::decorative_rgba(state.active_color_index);
                                    line_color[3] = 1.0;
                                    ui_actors.push(act!(quad:
                                        align(0.0, 0.5):
                                        xy(sel_x, underline_y):
                                        zoomto(underline_w, line_thickness):
                                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                        z(101)
                                    ));
                                });
                            });
                        }

                        // Encircling cursor ring around the active option when this row is active.
                        // During submenu fades, hide the ring to avoid exposing its construction.
                        if is_active && !widths.is_empty() && !is_fading_submenu {
                            let sel_idx = selected_choice.min(widths.len().saturating_sub(1));
                            if let Some(target_left_x) = x_positions.get(sel_idx).copied() {
                                let draw_w = widths.get(sel_idx).copied().unwrap_or(40.0);
                                asset_manager.with_fonts(|_all_fonts| {
                                    asset_manager.with_font("miso", |metrics_font| {
                                        let text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                                        let pad_y = widescale(6.0, 8.0);
                                        let min_pad_x = widescale(2.0, 3.0);
                                        let max_pad_x = widescale(22.0, 28.0);
                                        let width_ref = widescale(180.0, 220.0);
                                        let mut size_t_to = draw_w / width_ref;
                                        if !size_t_to.is_finite() { size_t_to = 0.0; }
                                        if size_t_to < 0.0 { size_t_to = 0.0; }
                                        if size_t_to > 1.0 { size_t_to = 1.0; }
                                        let mut pad_x_to = min_pad_x + (max_pad_x - min_pad_x) * size_t_to;
                                        let border_w = widescale(2.0, 2.5);
                                        // Cap pad so ring doesn't encroach neighbours.
                                        let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                                        if pad_x_to > max_pad_by_spacing { pad_x_to = max_pad_by_spacing; }
                                        let mut ring_w = draw_w + pad_x_to * 2.0;
                                        let mut ring_h = text_h + pad_y * 2.0;

                                        // Determine animated center X when tweening, otherwise snap to target.
                                        let mut center_x = target_left_x + draw_w * 0.5;
                                        // Vertical tween for row transitions
                                        let mut center_y = row_mid_y;
                                        if state.cursor_row_anim_t < 1.0 {
                                            let t = ease_out_cubic(state.cursor_row_anim_t);
                                            if let Some(from_row) = state.cursor_row_anim_from_row {
                                                let from_x = calc_row_center_x(from_row);
                                                center_x = from_x + (center_x - from_x) * t;
                                            }
                                            center_y = state.cursor_row_anim_from_y
                                                + (row_mid_y - state.cursor_row_anim_from_y) * t;
                                        }
                                        // Horizontal tween between choices within a row.
                                        if let Some(anim_row) = state.cursor_anim_row
                                            && anim_row == row_idx && state.cursor_anim_t < 1.0 {
                                                let from_idx = state.cursor_anim_from_choice.min(widths.len().saturating_sub(1));
                                                let to_idx = sel_idx.min(widths.len().saturating_sub(1));
                                                let from_center_x = x_positions[from_idx] + widths[from_idx] * 0.5;
                                                let to_center_x = x_positions[to_idx] + widths[to_idx] * 0.5;
                                                let t = ease_out_cubic(state.cursor_anim_t);
                                                center_x = from_center_x + (to_center_x - from_center_x) * t;
                                                // Also interpolate ring size from previous choice to current choice.
                                                let from_draw_w = widths[from_idx];
                                                let mut size_t_from = from_draw_w / width_ref;
                                                if !size_t_from.is_finite() { size_t_from = 0.0; }
                                                if size_t_from < 0.0 { size_t_from = 0.0; }
                                                if size_t_from > 1.0 { size_t_from = 1.0; }
                                                let mut pad_x_from = min_pad_x + (max_pad_x - min_pad_x) * size_t_from;
                                                let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                                                if pad_x_from > max_pad_by_spacing { pad_x_from = max_pad_by_spacing; }
                                                let ring_w_from = from_draw_w + pad_x_from * 2.0;
                                                let ring_h_from = text_h + pad_y * 2.0;
                                                ring_w = ring_w_from + (ring_w - ring_w_from) * t;
                                                ring_h = ring_h_from + (ring_h - ring_h_from) * t;
                                            }
                                        // If not horizontally tweening, but vertically tweening rows, interpolate size
                                        if state.cursor_row_anim_t < 1.0
                                            && (state.cursor_anim_row.is_none()
                                                || state.cursor_anim_row != Some(row_idx))
                                            && let Some(from_row) = state.cursor_row_anim_from_row {
                                                let (from_dw, from_dh) = calc_row_dims(from_row);
                                                let mut size_t_from = from_dw / width_ref;
                                                if !size_t_from.is_finite() { size_t_from = 0.0; }
                                                if size_t_from < 0.0 { size_t_from = 0.0; }
                                                if size_t_from > 1.0 { size_t_from = 1.0; }
                                                let mut pad_x_from = min_pad_x + (max_pad_x - min_pad_x) * size_t_from;
                                                let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
                                                if pad_x_from > max_pad_by_spacing { pad_x_from = max_pad_by_spacing; }
                                                let ring_w_from = from_dw + pad_x_from * 2.0;
                                                let ring_h_from = from_dh + pad_y * 2.0;
                                                let t = ease_out_cubic(state.cursor_row_anim_t);
                                                ring_w = ring_w_from + (ring_w - ring_w_from) * t;
                                                ring_h = ring_h_from + (ring_h - ring_h_from) * t;
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
                } else {
                    // Exit row: centered "Exit" text in the items column.
                    let label = "Exit";
                    let value_zoom = 0.835_f32;
                    let choice_color = if is_active { col_white } else { sl_gray };
                    let mut center_x = list_x + list_w * 0.5;
                    let mut center_y = row_mid_y;

                    ui_actors.push(act!(text:
                        align(0.5, 0.5):
                        xy(center_x, center_y):
                        zoom(value_zoom):
                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                        font("miso"):
                        settext(label):
                        horizalign(center)
                    ));

                    // Draw the selection cursor ring for the Exit row when active.
                    // During submenu fades, hide the ring to avoid exposing its construction.
                    if is_active && !is_fading_submenu {
                        asset_manager.with_fonts(|all_fonts| {
                            asset_manager.with_font("miso", |metrics_font| {
                                let mut text_w =
                                    font::measure_line_width_logical(metrics_font, label, all_fonts) as f32;
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
                                if size_t < 0.0 {
                                    size_t = 0.0;
                                }
                                if size_t > 1.0 {
                                    size_t = 1.0;
                                }
                                let mut pad_x = min_pad_x + (max_pad_x - min_pad_x) * size_t;
                                let border_w = widescale(2.0, 2.5);
                                let max_pad_by_spacing =
                                    (INLINE_SPACING - border_w).max(min_pad_x);
                                if pad_x > max_pad_by_spacing {
                                    pad_x = max_pad_by_spacing;
                                }
                                let mut ring_w = draw_w + pad_x * 2.0;
                                let mut ring_h = draw_h + pad_y * 2.0;

                                // Vertical tween for row transitions (and horizontal tween between rows).
                                if state.cursor_row_anim_t < 1.0 {
                                    let t = ease_out_cubic(state.cursor_row_anim_t);
                                    if let Some(from_row) = state.cursor_row_anim_from_row {
                                        // Interpolate X from previous row's cursor center to Exit center.
                                        let from_x = calc_row_center_x(from_row);
                                        center_x = from_x + (center_x - from_x) * t;
                                    }
                                    center_y = state.cursor_row_anim_from_y
                                        + (row_mid_y - state.cursor_row_anim_from_y) * t;

                                    // Interpolate ring size from previous row to Exit row.
                                    if let Some(from_row) = state.cursor_row_anim_from_row {
                                        let from_idx = from_row.min(total_rows.saturating_sub(2));
                                        if from_idx < SYSTEM_OPTIONS_ROWS.len() {
                                            // Approximate previous row dims by reusing current draw_w/draw_h.
                                            let from_draw_w = draw_w;
                                            let mut size_t_from = from_draw_w / width_ref;
                                            if !size_t_from.is_finite() {
                                                size_t_from = 0.0;
                                            }
                                            if size_t_from < 0.0 {
                                                size_t_from = 0.0;
                                            }
                                            if size_t_from > 1.0 {
                                                size_t_from = 1.0;
                                            }
                                            let mut pad_x_from =
                                                min_pad_x + (max_pad_x - min_pad_x) * size_t_from;
                                            let max_pad_by_spacing =
                                                (INLINE_SPACING - border_w).max(min_pad_x);
                                            if pad_x_from > max_pad_by_spacing {
                                                pad_x_from = max_pad_by_spacing;
                                            }
                                            let ring_w_from = from_draw_w + pad_x_from * 2.0;
                                            let ring_h_from = draw_h + pad_y * 2.0;
                                            let tsize = ease_out_cubic(state.cursor_row_anim_t);
                                            ring_w =
                                                ring_w_from + (ring_w - ring_w_from) * tsize;
                                            ring_h =
                                                ring_h_from + (ring_h - ring_h_from) * tsize;
                                        }
                                    }
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
                                    xy((left + right) * 0.5, top + border_w * 0.5):
                                    zoomto(ring_w, border_w):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                ui_actors.push(act!(quad:
                                    align(0.5, 0.5):
                                    xy((left + right) * 0.5, bottom - border_w * 0.5):
                                    zoomto(ring_w, border_w):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                ui_actors.push(act!(quad:
                                    align(0.5, 0.5):
                                    xy(left + border_w * 0.5, (top + bottom) * 0.5):
                                    zoomto(border_w, ring_h):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                                ui_actors.push(act!(quad:
                                    align(0.5, 0.5):
                                    xy(right - border_w * 0.5, (top + bottom) * 0.5):
                                    zoomto(border_w, ring_h):
                                    diffuse(ring_color[0], ring_color[1], ring_color[2], ring_color[3]):
                                    z(101)
                                ));
                            });
                        });
                    }
                }
            }

            // Description items for the submenu
            let total_rows = SYSTEM_OPTIONS_ROWS.len() + 1;
            let sel = state.sub_selected.min(total_rows.saturating_sub(1));
            let item = if sel < SYSTEM_OPTIONS_ROWS.len() {
                &SYSTEM_OPTIONS_ITEMS[sel]
            } else {
                &SYSTEM_OPTIONS_ITEMS[SYSTEM_OPTIONS_ITEMS.len().saturating_sub(1)]
            };
            selected_item = Some(item);
        }
    }

    // ------------------- Description content (selected) -------------------
    if let Some(item) = selected_item {
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
    }

    let combined_alpha = alpha_multiplier * state.content_alpha;
    for actor in &mut ui_actors {
        apply_alpha_to_actor(actor, combined_alpha);
    }
    actors.extend(ui_actors);

    actors
}
