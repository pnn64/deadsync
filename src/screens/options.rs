use crate::act;
use crate::assets::AssetManager;
use crate::core::gfx::BackendType;
use crate::core::space::*;
// Screen navigation is handled in app.rs via the dispatcher
use crate::core::audio;
use crate::config::{self, DisplayMode, FullscreenType};
use crate::screens::{Screen, ScreenAction};
use crate::core::input::{VirtualAction, InputEvent};
use std::time::{Duration, Instant};
use std::borrow::Cow;

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

// Match Simply Love operator menu ranges (±1000 ms) for these calibrations.
const GLOBAL_OFFSET_MIN_MS: i32 = -1000;
const GLOBAL_OFFSET_MAX_MS: i32 = 1000;
const VISUAL_DELAY_MIN_MS: i32 = -1000;
const VISUAL_DELAY_MAX_MS: i32 = 1000;

// --- Monitor & Video Mode Data Structures ---

#[derive(Clone, Debug)]
pub struct VideoModeSpec {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_millihertz: u32,
}

#[derive(Clone, Debug)]
pub struct MonitorSpec {
    pub name: String,
    pub modes: Vec<VideoModeSpec>,
}

#[inline(always)]
fn ease_out_cubic(t: f32) -> f32 {
    let clamped = if t < 0.0 { 0.0 } else if t > 1.0 { 1.0 } else { t };
    let u = 1.0 - clamped;
    1.0 - u * u * u
}

#[inline(always)]
fn format_ms(value: i32) -> String {
    // Positive values omit a '+' and compact to the Simply Love "Nms" style.
    format!("{}ms", value)
}

#[inline(always)]
fn adjust_ms_value(value: &mut i32, delta: isize, min: i32, max: i32) -> bool {
    let new_value = (*value + delta as i32).clamp(min, max);
    if new_value == *value {
        false
    } else {
        *value = new_value;
        true
    }
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
/// Left padding for inline option values in the System Options submenu (content-space pixels).
const SUB_INLINE_ITEMS_LEFT_PAD: f32 = 13.0;
/// Horizontal offset (content-space pixels) for single-value submenu items
/// (e.g. Language and Exit) within the items column.
const SUB_SINGLE_VALUE_CENTER_OFFSET: f32 = -43.0;

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
pub enum SubmenuKind {
    System,
    GraphicsSound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsView {
    Main,
    Submenu(SubmenuKind),
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
    pub inline: bool, // whether to lay out choices inline (vs single centered value)
}

pub const SYSTEM_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: "Game",
        choices: &["dance", "pump"],
        inline: true,
    },
    SubRow {
        label: "Theme",
        choices: &["Simply Love"],
        inline: true,
    },
    SubRow {
        label: "Language",
        choices: &["English", "Japanese"],
        inline: false, // single centered value (no inline tween)
    },
    SubRow {
        label: "Announcer",
        choices: &["None", "ITG"],
        inline: true,
    },
    SubRow {
        label: "Default NoteSkin",
        choices: &["cel", "metal", "enchantment-v2", "devcel-2024-v3"],
        inline: true,
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
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

#[cfg(target_os = "windows")]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::DirectX, "DirectX (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(not(target_os = "windows"))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];

#[cfg(target_os = "windows")]
const VIDEO_RENDERER_LABELS: &[&str] = &[
    VIDEO_RENDERER_OPTIONS[0].1,
    VIDEO_RENDERER_OPTIONS[1].1,
    VIDEO_RENDERER_OPTIONS[2].1,
    VIDEO_RENDERER_OPTIONS[3].1,
    VIDEO_RENDERER_OPTIONS[4].1,
    VIDEO_RENDERER_OPTIONS[5].1,
];
#[cfg(not(target_os = "windows"))]
const VIDEO_RENDERER_LABELS: &[&str] = &[
    VIDEO_RENDERER_OPTIONS[0].1,
    VIDEO_RENDERER_OPTIONS[1].1,
    VIDEO_RENDERER_OPTIONS[2].1,
    VIDEO_RENDERER_OPTIONS[3].1,
    VIDEO_RENDERER_OPTIONS[4].1,
];

const VIDEO_RENDERER_ROW_INDEX: usize = 0;
const DISPLAY_MODE_ROW_INDEX: usize = 1;
const DISPLAY_ASPECT_RATIO_ROW_INDEX: usize = 2;
const DISPLAY_RESOLUTION_ROW_INDEX: usize = 3;
const REFRESH_RATE_ROW_INDEX: usize = 4;
const FULLSCREEN_TYPE_ROW_INDEX: usize = 5;

const DEFAULT_RESOLUTION_CHOICES: &[(u32, u32)] = &[
    (1920, 1080),
    (1600, 900),
    (1280, 720),
    (1024, 768),
    (800, 600),
];

fn build_display_mode_choices(monitor_specs: &[MonitorSpec]) -> Vec<String> {
    if monitor_specs.is_empty() {
        return vec!["Screen 1".to_string(), "Windowed".to_string()];
    }
    let mut out = Vec::with_capacity(monitor_specs.len() + 1);
    for spec in monitor_specs {
        out.push(spec.name.clone());
    }
    out.push("Windowed".to_string());
    out
}

pub const GRAPHICS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: "Video Renderer",
        choices: VIDEO_RENDERER_LABELS,
        inline: true,
    },
    SubRow {
        label: "Display Mode",
        choices: &["Windowed", "Fullscreen", "Borderless"], // Replaced dynamically
        inline: true,
    },
    SubRow {
        label: "Display Aspect Ratio",
        choices: &["16:9", "16:10", "4:3", "1:1"],
        inline: true,
    },
    SubRow {
        label: "Display Resolution",
        choices: &["1920x1080", "1600x900", "1280x720", "1024x768", "800x600"], // Replaced dynamically
        inline: true,
    },
    SubRow {
        label: "Refresh Rate",
        choices: &["Default", "60 Hz", "75 Hz", "120 Hz", "144 Hz", "165 Hz", "240 Hz", "360 Hz"], // Replaced dynamically
        inline: true,
    },
    SubRow {
        label: "Fullscreen Type",
        choices: &["Exclusive", "Borderless"],
        inline: true,
    },
    SubRow {
        label: "High Resolution Textures",
        choices: &["Auto", "Force Off", "Force On"],
        inline: true,
    },
    SubRow {
        label: "Max Texture Resolution",
        choices: &["256", "512", "1024", "2048"],
        inline: true,
    },
    SubRow {
        label: "Smooth Lines",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "CelShade Models",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Delayed Texture Delete",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Vsync",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Fast Note Rendering",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Show Stats",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Attract Sound Frequency",
        choices: &["Never", "Always", "2 Times", "3 Times", "4 Times", "5 Times"],
        inline: true,
    },
    SubRow {
        label: "Sound Volume",
        choices: &["Silent", "10%", "25%", "50%", "75%", "100%"],
        inline: true,
    },
    SubRow {
        label: "Preferred Sample Rate",
        choices: &["Default", "44100 Hz", "48000 Hz"],
        inline: true,
    },
    SubRow {
        label: "Enable Attack Sounds",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Enable Mine Hit Sound",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Global Offset (ms)",
        choices: &["0 ms"],
        inline: false,
    },
    SubRow {
        label: "Visual Delay (ms)",
        choices: &["0 ms"],
        inline: false,
    },
    SubRow {
        label: "Default Sync Offset",
        choices: &["NULL", "ITG"],
        inline: true,
    },
    SubRow {
        label: "RateMod Preserves Pitch",
        choices: &["Off", "On"],
        inline: true,
    },
];

pub const GRAPHICS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: "Video Renderer",
        help: &["Select the rendering backend."],
    },
    Item {
        name: "Display Mode",
        help: &["Choose how the window is presented on screen."],
    },
    Item {
        name: "Display Aspect Ratio",
        help: &["Set the aspect ratio used for rendering."],
    },
    Item {
        name: "Display Resolution",
        help: &["Pick a rendering resolution."],
    },
    Item {
        name: "Refresh Rate",
        help: &["Pick a target display refresh rate."],
    },
    Item {
        name: "Fullscreen Type",
        help: &["Choose between exclusive or borderless fullscreen."],
    },
    Item {
        name: "High Resolution Textures",
        help: &["Control use of high resolution textures."],
    },
    Item {
        name: "Max Texture Resolution",
        help: &["Cap the maximum texture resolution."],
    },
    Item {
        name: "Smooth Lines",
        help: &["Toggle antialiasing for vector lines."],
    },
    Item {
        name: "CelShade Models",
        help: &["Toggle cel shading for 3D models."],
    },
    Item {
        name: "Delayed Texture Delete",
        help: &["Delay texture deletion to reduce hitches."],
    },
    Item {
        name: "Vsync",
        help: &["Enable vertical sync."],
    },
    Item {
        name: "Fast Note Rendering",
        help: &["Use fast note rendering optimizations."],
    },
    Item {
        name: "Show Stats",
        help: &["Display rendering statistics overlay."],
    },
    Item {
        name: "Attract Sound Frequency",
        help: &["Control how often attract-mode sounds play."],
    },
    Item {
        name: "Sound Volume",
        help: &["Set the master sound volume for gameplay."],
    },
    Item {
        name: "Preferred Sample Rate",
        help: &["Select an audio output sample rate."],
    },
    Item {
        name: "Enable Attack Sounds",
        help: &["Play sounds for attacks."],
    },
    Item {
        name: "Enable Mine Hit Sound",
        help: &["Play a sound when mines are hit."],
    },
    Item {
        name: "Global Offset (ms)",
        help: &["Apply a global audio timing offset in 1 ms steps."],
    },
    Item {
        name: "Visual Delay (ms)",
        help: &["Apply a visual timing offset in 1 ms steps."],
    },
    Item {
        name: "Default Sync Offset",
        help: &["Choose the sync profile used for judgments."],
    },
    Item {
        name: "RateMod Preserves Pitch",
        help: &["Keep pitch constant when rate mods are active."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

fn submenu_rows(kind: SubmenuKind) -> &'static [SubRow<'static>] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ROWS,
        SubmenuKind::GraphicsSound => GRAPHICS_OPTIONS_ROWS,
    }
}

fn submenu_items(kind: SubmenuKind) -> &'static [Item<'static>] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ITEMS,
        SubmenuKind::GraphicsSound => GRAPHICS_OPTIONS_ITEMS,
    }
}

fn submenu_title(kind: SubmenuKind) -> &'static str {
    match kind {
        SubmenuKind::System => "SYSTEM OPTIONS",
        SubmenuKind::GraphicsSound => "GRAPHICS/SOUND OPTIONS",
    }
}

fn backend_to_renderer_choice_index(backend: BackendType) -> usize {
    VIDEO_RENDERER_OPTIONS
        .iter()
        .position(|(b, _)| *b == backend)
        .unwrap_or(0)
}

fn renderer_choice_index_to_backend(idx: usize) -> BackendType {
    VIDEO_RENDERER_OPTIONS
        .get(idx)
        .map(|(backend, _)| *backend)
        .unwrap_or_else(|| VIDEO_RENDERER_OPTIONS[0].0)
}

fn selected_video_renderer(state: &State) -> BackendType {
    let choice_idx = state
        .sub_choice_indices_graphics
        .get(VIDEO_RENDERER_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    renderer_choice_index_to_backend(choice_idx)
}

fn fullscreen_type_to_choice_index(fullscreen_type: FullscreenType) -> usize {
    match fullscreen_type {
        FullscreenType::Exclusive => 0,
        FullscreenType::Borderless => 1,
    }
}

fn choice_index_to_fullscreen_type(idx: usize) -> FullscreenType {
    match idx {
        1 => FullscreenType::Borderless,
        _ => FullscreenType::Exclusive,
    }
}

fn selected_fullscreen_type(state: &State) -> FullscreenType {
    state
        .sub_choice_indices_graphics
        .get(FULLSCREEN_TYPE_ROW_INDEX)
        .copied()
        .map(choice_index_to_fullscreen_type)
        .unwrap_or(FullscreenType::Exclusive)
}

fn selected_display_mode(state: &State) -> DisplayMode {
    let display_choice = state
        .sub_choice_indices_graphics
        .get(DISPLAY_MODE_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    let windowed_idx = state.display_mode_choices.len().saturating_sub(1);
    if windowed_idx == 0 || display_choice >= windowed_idx {
        DisplayMode::Windowed
    } else {
        DisplayMode::Fullscreen(selected_fullscreen_type(state))
    }
}

fn selected_display_monitor(state: &State) -> usize {
    let display_choice = state
        .sub_choice_indices_graphics
        .get(DISPLAY_MODE_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    let windowed_idx = state
        .display_mode_choices
        .len()
        .saturating_sub(1);
    if windowed_idx == 0 || display_choice >= windowed_idx {
        0
    } else {
        display_choice.min(windowed_idx.saturating_sub(1))
    }
}

fn ensure_display_mode_choices(state: &mut State) {
    let monitor_count = state.monitor_specs.len();
    state.display_mode_choices = build_display_mode_choices(&state.monitor_specs);
    // If current selection is out of bounds, reset it.
    if let Some(idx) = state.sub_choice_indices_graphics.get_mut(DISPLAY_MODE_ROW_INDEX) {
        if *idx >= state.display_mode_choices.len() {
            *idx = 0;
        }
    }
    // Also re-run logic that depends on the selected monitor.
    let current_res = selected_resolution(state);
    rebuild_resolution_choices(state, current_res.0, current_res.1);
}

pub fn update_monitor_specs(state: &mut State, specs: Vec<MonitorSpec>) {
    state.monitor_specs = specs;
    ensure_display_mode_choices(state);
    // Keep the Display Mode row aligned with the actual current mode after monitors refresh.
    set_display_mode_row_selection(
        state,
        state.monitor_specs.len(),
        state.display_mode_at_load,
        state.display_monitor_at_load,
    );
}

fn set_display_mode_row_selection(
    state: &mut State,
    _monitor_count: usize, // Ignored, we use stored monitor_specs now
    mode: DisplayMode,
    monitor: usize,
) {
    // Ensure choices are up to date.
    ensure_display_mode_choices(state);
    let windowed_idx = state.display_mode_choices.len().saturating_sub(1);
    let idx = match mode {
        DisplayMode::Windowed => windowed_idx,
        DisplayMode::Fullscreen(_) => {
            let max_idx = windowed_idx.saturating_sub(1);
            if max_idx == 0 {
                0
            } else {
                monitor.min(max_idx)
            }
        }
    };
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(DISPLAY_MODE_ROW_INDEX)
    {
        *slot = idx;
    }
    // Re-trigger resolution rebuild based on the potentially new monitor selection.
    let current_res = selected_resolution(state);
    rebuild_resolution_choices(state, current_res.0, current_res.1);
}

fn selected_aspect_label(state: &State) -> &'static str {
    let idx = state
        .sub_choice_indices_graphics
        .get(DISPLAY_ASPECT_RATIO_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    GRAPHICS_OPTIONS_ROWS
        .get(DISPLAY_ASPECT_RATIO_ROW_INDEX)
        .and_then(|row| row.choices.get(idx))
        .copied()
        .unwrap_or("16:9")
}

fn push_unique_resolution(target: &mut Vec<(u32, u32)>, width: u32, height: u32) {
    if !target.iter().any(|&(w, h)| w == width && h == height) {
        target.push((width, height));
    }
}

fn preset_resolutions_for_aspect(label: &str) -> Vec<(u32, u32)> {
    match label.to_ascii_lowercase().as_str() {
        "16:9" => vec![(1280, 720), (1600, 900), (1920, 1080)],
        "16:10" => vec![(1280, 800), (1440, 900), (1680, 1050), (1920, 1200)],
        "4:3" => vec![(640, 480), (800, 600), (1024, 768), (1280, 960), (1600, 1200)],
        "1:1" => vec![(342, 342), (456, 456), (608, 608), (810, 810), (1080, 1080)],
        _ => DEFAULT_RESOLUTION_CHOICES.to_vec(),
    }
}

fn aspect_matches(width: u32, height: u32, label: &str) -> bool {
    let ratio = width as f32 / height as f32;
    match label {
        "16:9" => (ratio - 1.7777).abs() < 0.05,
        "16:10" => (ratio - 1.6).abs() < 0.05,
        "4:3" => (ratio - 1.3333).abs() < 0.05,
        "1:1" => (ratio - 1.0).abs() < 0.05,
        _ => true,
    }
}

fn selected_resolution(state: &State) -> (u32, u32) {
    let idx = state
        .sub_choice_indices_graphics
        .get(DISPLAY_RESOLUTION_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    state
        .resolution_choices
        .get(idx)
        .copied()
        .or_else(|| state.resolution_choices.get(0).copied())
        .unwrap_or((state.display_width_at_load, state.display_height_at_load))
}

fn rebuild_refresh_rate_choices(state: &mut State) {
    if matches!(selected_display_mode(state), DisplayMode::Windowed) {
        state.refresh_rate_choices = vec![0];
        if let Some(slot) = state.sub_choice_indices_graphics.get_mut(REFRESH_RATE_ROW_INDEX) {
            *slot = 0;
        }
        return;
    }

    let (width, height) = selected_resolution(state);
    let mon_idx = selected_display_monitor(state);
    let mut rates = Vec::new();
    
    // Default choice is always available (0).
    rates.push(0);

    if let Some(spec) = state.monitor_specs.get(mon_idx) {
        let mut supported_rates: Vec<u32> = spec.modes.iter()
            .filter(|m| m.width == width && m.height == height)
            .map(|m| m.refresh_rate_millihertz)
            .collect();
        supported_rates.sort();
        supported_rates.dedup();
        rates.extend(supported_rates);
    }
    
    // Add common fallback rates if list is empty (besides Default)
    if rates.len() == 1 {
        rates.extend_from_slice(&[60000, 75000, 120000, 144000, 165000, 240000]);
    }
    
    // Preserve current selection if possible, else default to "Default".
    let current_rate = if let Some(idx) = state.sub_choice_indices_graphics.get(REFRESH_RATE_ROW_INDEX) {
        state.refresh_rate_choices.get(*idx).copied().unwrap_or(0)
    } else {
        0
    };
    
    state.refresh_rate_choices = rates;
    
    if let Some(slot) = state.sub_choice_indices_graphics.get_mut(REFRESH_RATE_ROW_INDEX) {
        *slot = state.refresh_rate_choices.iter().position(|&r| r == current_rate).unwrap_or(0);
    }
}

fn rebuild_resolution_choices(state: &mut State, width: u32, height: u32) {
    let aspect_label = selected_aspect_label(state);
    let mon_idx = selected_display_monitor(state);
    
    let mut list = Vec::new();
    
    // 1. Gather resolutions from the selected monitor spec.
    if let Some(spec) = state.monitor_specs.get(mon_idx) {
        let mut modes: Vec<(u32, u32)> = spec.modes.iter()
            .map(|m| (m.width, m.height))
            .collect();
        modes.sort();
        modes.dedup();
        
        for (w, h) in modes {
            if aspect_matches(w, h, aspect_label) {
                list.push((w, h));
            }
        }
    }
    
    // 2. If list is empty (e.g. no monitor data or Aspect filter too strict), use presets.
    if list.is_empty() {
        list = preset_resolutions_for_aspect(aspect_label);
    }
    
    // 3. Ensure the currently requested/active resolution is in the list so we don't lose it.
    push_unique_resolution(&mut list, width, height);
    
    // Sort descending by width then height (typical UI preference).
    list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    
    state.resolution_choices = list;
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(DISPLAY_RESOLUTION_ROW_INDEX)
    {
        *slot = state
            .resolution_choices
            .iter()
            .position(|&(w, h)| w == width && h == height)
            .unwrap_or(0);
    }
    
    // Rebuild refresh rates since available rates depend on resolution.
    rebuild_refresh_rate_choices(state);
}

fn row_choices<'a>(
    state: &'a State,
    kind: SubmenuKind,
    rows: &'a [SubRow<'a>],
    row_idx: usize,
) -> Vec<Cow<'a, str>> {
    if let Some(row) = rows.get(row_idx) {
        if matches!(kind, SubmenuKind::GraphicsSound) {
            if row.label == "Display Mode" {
                return state
                    .display_mode_choices
                    .iter()
                    .cloned()
                    .map(Cow::Owned)
                    .collect();
            }
            if row.label == "Display Resolution" {
                return state
                    .resolution_choices
                    .iter()
                    .map(|&(w, h)| Cow::Owned(format!("{w}x{h}")))
                    .collect();
            }
            if row.label == "Refresh Rate" {
                return state.refresh_rate_choices.iter().map(|&mhz| {
                    if mhz == 0 {
                        Cow::Borrowed("Default")
                    } else {
                        // Format nicely: 60000 -> "60 Hz", 59940 -> "59.94 Hz"
                        let hz = mhz as f32 / 1000.0;
                        if (hz.fract()).abs() < 0.01 {
                            Cow::Owned(format!("{:.0}Hz", hz))
                        } else {
                            Cow::Owned(format!("{:.2}Hz", hz))
                        }
                    }
                }).collect();
            }
        }
    }
    rows.get(row_idx)
        .map(|row| row.choices.iter().map(|c| Cow::Borrowed(*c)).collect())
        .unwrap_or_default()
}

pub struct State {
    pub selected: usize,
    prev_selected: usize,
    pub active_color_index: i32, // <-- ADDED
    bg: heart_bg::State,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    nav_lr_held_direction: Option<isize>,
    nav_lr_held_since: Option<Instant>,
    nav_lr_last_adjusted_at: Option<Instant>,
    view: OptionsView,
    submenu_transition: SubmenuTransition,
    pending_submenu_kind: Option<SubmenuKind>,
    submenu_fade_t: f32,
    content_alpha: f32,
    // Submenu state
    sub_selected: usize,
    sub_prev_selected: usize,
    sub_choice_indices_system: Vec<usize>,
    sub_choice_indices_graphics: Vec<usize>,
    global_offset_ms: i32,
    visual_delay_ms: i32,
    video_renderer_at_load: BackendType,
    display_mode_at_load: DisplayMode,
    display_monitor_at_load: usize,
    display_width_at_load: u32,
    display_height_at_load: u32,
    display_mode_choices: Vec<String>,
    resolution_choices: Vec<(u32, u32)>,
    refresh_rate_choices: Vec<u32>, // New: stored in millihertz
    // Hardware info
    pub monitor_specs: Vec<MonitorSpec>,
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
    let cfg = config::get();
    let mut state = State {
        selected: 0,
        prev_selected: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX, // <-- ADDED
        bg: heart_bg::State::new(),

        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        nav_lr_held_direction: None,
        nav_lr_held_since: None,
        nav_lr_last_adjusted_at: None,
        submenu_transition: SubmenuTransition::None,
        pending_submenu_kind: None,
        submenu_fade_t: 0.0,
        content_alpha: 1.0,
        view: OptionsView::Main,
        sub_selected: 0,
        sub_prev_selected: 0,
        sub_choice_indices_system: vec![0; SYSTEM_OPTIONS_ROWS.len()],
        sub_choice_indices_graphics: vec![0; GRAPHICS_OPTIONS_ROWS.len()],
        global_offset_ms: {
            let ms = (cfg.global_offset_seconds * 1000.0).round() as i32;
            ms.clamp(GLOBAL_OFFSET_MIN_MS, GLOBAL_OFFSET_MAX_MS)
        },
        visual_delay_ms: 0,
        video_renderer_at_load: cfg.video_renderer,
        display_mode_at_load: cfg.display_mode(),
        display_monitor_at_load: cfg.display_monitor,
        display_width_at_load: cfg.display_width,
        display_height_at_load: cfg.display_height,
        display_mode_choices: build_display_mode_choices(&[]),
        resolution_choices: Vec::new(),
        refresh_rate_choices: Vec::new(),
        monitor_specs: Vec::new(),
        cursor_anim_row: None,
        cursor_anim_from_choice: 0,
        cursor_anim_to_choice: 0,
        cursor_anim_t: 1.0,
        cursor_row_anim_from_y: 0.0,
        cursor_row_anim_t: 1.0,
        cursor_row_anim_from_row: None,
    };

    sync_video_renderer(&mut state, cfg.video_renderer);
    sync_display_mode(
        &mut state,
        cfg.display_mode(),
        cfg.fullscreen_type,
        cfg.display_monitor,
        1,
    );
    sync_display_resolution(&mut state, cfg.display_width, cfg.display_height);
    state
}

fn submenu_choice_indices<'a>(state: &'a State, kind: SubmenuKind) -> &'a [usize] {
    match kind {
        SubmenuKind::System => &state.sub_choice_indices_system,
        SubmenuKind::GraphicsSound => &state.sub_choice_indices_graphics,
    }
}

fn submenu_choice_indices_mut<'a>(state: &'a mut State, kind: SubmenuKind) -> &'a mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_choice_indices_system,
        SubmenuKind::GraphicsSound => &mut state.sub_choice_indices_graphics,
    }
}

pub fn sync_video_renderer(state: &mut State, renderer: BackendType) {
    state.video_renderer_at_load = renderer;
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(VIDEO_RENDERER_ROW_INDEX)
    {
        *slot = backend_to_renderer_choice_index(renderer);
    }
}

pub fn sync_display_mode(
    state: &mut State,
    mode: DisplayMode,
    fullscreen_type: FullscreenType,
    monitor: usize,
    monitor_count: usize,
) {
    state.display_mode_at_load = mode;
    state.display_monitor_at_load = monitor;
    set_display_mode_row_selection(state, monitor_count, mode, monitor);
    let target_type = match mode {
        DisplayMode::Fullscreen(ft) => ft,
        DisplayMode::Windowed => fullscreen_type,
    };
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(FULLSCREEN_TYPE_ROW_INDEX)
    {
        *slot = fullscreen_type_to_choice_index(target_type);
    }
}

pub fn sync_display_resolution(state: &mut State, width: u32, height: u32) {
    rebuild_resolution_choices(state, width, height);
    state.display_width_at_load = width;
    state.display_height_at_load = height;
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

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    let mut pending_action: Option<ScreenAction> = None;
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
                // Switch view to the target submenu, then fade it in.
                let target_kind = state.pending_submenu_kind.unwrap_or(SubmenuKind::System);
                state.view = OptionsView::Submenu(target_kind);
                state.pending_submenu_kind = None;
                state.sub_selected = 0;
                state.sub_prev_selected = 0;
                state.cursor_anim_row = None;
                state.cursor_anim_t = 1.0;
                state.cursor_row_anim_t = 1.0;
                state.cursor_row_anim_from_row = None;
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                state.nav_lr_held_direction = None;
                state.nav_lr_held_since = None;
                state.nav_lr_last_adjusted_at = None;
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
            let leaving_graphics =
                matches!(state.view, OptionsView::Submenu(SubmenuKind::GraphicsSound));
            let (desired_renderer, desired_display_mode, desired_resolution, desired_monitor) =
                if leaving_graphics {
                    (
                        Some(selected_video_renderer(state)),
                        Some(selected_display_mode(state)),
                        Some(selected_resolution(state)),
                        Some(selected_display_monitor(state)),
                    )
                } else {
                    (None, None, None, None)
                };
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
                state.pending_submenu_kind = None;
                state.cursor_anim_row = None;
                state.cursor_anim_t = 1.0;
                state.cursor_row_anim_t = 1.0;
                state.cursor_row_anim_from_row = None;
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
                state.nav_key_last_scrolled_at = None;
                state.nav_lr_held_direction = None;
                state.nav_lr_held_since = None;
                state.nav_lr_last_adjusted_at = None;
                state.submenu_transition = SubmenuTransition::FadeInMain;
                state.submenu_fade_t = 0.0;
                state.content_alpha = 0.0;

                let mut renderer_change: Option<BackendType> = None;
                let mut display_mode_change: Option<DisplayMode> = None;
                let mut resolution_change: Option<(u32, u32)> = None;
                let mut monitor_change: Option<usize> = None;

                if let Some(renderer) = desired_renderer {
                    if renderer != state.video_renderer_at_load {
                        renderer_change = Some(renderer);
                    }
                }
                if let Some(display_mode) = desired_display_mode {
                    if display_mode != state.display_mode_at_load {
                        display_mode_change = Some(display_mode);
                    }
                }
                if let Some(monitor) = desired_monitor {
                    if monitor != state.display_monitor_at_load {
                        monitor_change = Some(monitor);
                    }
                }
                if let Some((w, h)) = desired_resolution {
                    if w != state.display_width_at_load || h != state.display_height_at_load {
                        resolution_change = Some((w, h));
                    }
                }

                if renderer_change.is_some()
                    || display_mode_change.is_some()
                    || monitor_change.is_some()
                    || resolution_change.is_some()
                {
                    pending_action = Some(ScreenAction::ChangeGraphics {
                        renderer: renderer_change,
                        display_mode: display_mode_change,
                        monitor: monitor_change,
                        resolution: resolution_change,
                    });
                }
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
        return pending_action;
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
                    OptionsView::Submenu(kind) => {
                        let total = submenu_rows(kind).len() + 1; // + Exit row
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

    if let (Some(delta_lr), Some(held_since), Some(last_adjusted)) =
        (state.nav_lr_held_direction, state.nav_lr_held_since, state.nav_lr_last_adjusted_at)
    {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_adjusted) >= NAV_REPEAT_SCROLL_INTERVAL
        {
            if matches!(state.view, OptionsView::Submenu(_)) {
                apply_submenu_choice_delta(state, delta_lr);
                state.nav_lr_last_adjusted_at = Some(now);
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
        OptionsView::Submenu(kind) => {
            if state.sub_selected != state.sub_prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");

                // Start a simple vertical cursor tween between rows in the submenu.
                let (s, _, list_y) = scaled_block_origin_with_margins();
                let prev_idx = state.sub_prev_selected;
                let total_rows = submenu_rows(kind).len() + 1;
                let offset_prev = scroll_offset(prev_idx, total_rows);
                let prev_vis_idx = prev_idx.saturating_sub(offset_prev);
                let from_y = list_y + (prev_vis_idx as f32) * (ROW_H + ROW_GAP) * s;
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

    pending_action
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

fn on_lr_press(state: &mut State, delta: isize) {
    let now = Instant::now();
    state.nav_lr_held_direction = Some(delta);
    state.nav_lr_held_since = Some(now);
    state.nav_lr_last_adjusted_at = Some(now);
}

fn on_lr_release(state: &mut State, delta: isize) {
    if state.nav_lr_held_direction == Some(delta) {
        state.nav_lr_held_direction = None;
        state.nav_lr_held_since = None;
        state.nav_lr_last_adjusted_at = None;
    }
}

fn apply_submenu_choice_delta(state: &mut State, delta: isize) {
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return;
    }
    let kind = match state.view {
        OptionsView::Submenu(k) => k,
        _ => return,
    };
    let rows = submenu_rows(kind);
    let rows_len = rows.len();
    if rows_len == 0 {
        return;
    }
    let row_index = state.sub_selected;
    if row_index >= rows_len {
        // Exit row – no choices to change.
        return;
    }

    if let Some(row) = rows.get(row_index) {
        if matches!(kind, SubmenuKind::GraphicsSound) {
            match row.label {
                "Global Offset (ms)" => {
                    if adjust_ms_value(
                        &mut state.global_offset_ms,
                        delta,
                        GLOBAL_OFFSET_MIN_MS,
                        GLOBAL_OFFSET_MAX_MS,
                    ) {
                        config::update_global_offset(state.global_offset_ms as f32 / 1000.0);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                    }
                    return;
                }
                "Visual Delay (ms)" => {
                    if adjust_ms_value(
                        &mut state.visual_delay_ms,
                        delta,
                        VISUAL_DELAY_MIN_MS,
                        VISUAL_DELAY_MAX_MS,
                    ) {
                        audio::play_sfx("assets/sounds/change_value.ogg");
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    let mut pending_resolution_list: Option<Vec<(u32, u32)>> = None;
    let mut prev_choice_index: Option<usize> = None;
    let mut new_choice_index: Option<usize> = None;
    let choices = row_choices(state, kind, rows, row_index);
    let num_choices = choices.len();
    if num_choices == 0 {
        return;
    }
    {
        let choice_indices = submenu_choice_indices_mut(state, kind);
        if row_index >= choice_indices.len() {
            return;
        }
        let choice_index = choice_indices[row_index].min(num_choices.saturating_sub(1));
        let cur = choice_index as isize;
        let n = num_choices as isize;
        let mut new_index = ((cur + delta).rem_euclid(n)) as usize;
        if new_index >= num_choices {
            new_index = num_choices.saturating_sub(1);
        }
        if new_index == choice_index {
            return;
        }

        choice_indices[row_index] = new_index;
        prev_choice_index = Some(choice_index);
        new_choice_index = Some(new_index);
        audio::play_sfx("assets/sounds/change_value.ogg");

        if matches!(kind, SubmenuKind::GraphicsSound) {
            let row = &rows[row_index];
            if row.label == "Display Aspect Ratio" {
                // If Aspect Ratio changed, rebuild resolutions
                let aspect_label = row.choices.get(new_index).copied().unwrap_or("16:9");
                let (cur_w, cur_h) = selected_resolution(state);
                // We'll queue a rebuild
                rebuild_resolution_choices(state, cur_w, cur_h);
                // Also reset resolution selection to best match? 
                // rebuild_resolution_choices already tries to keep current.
            }
            if row.label == "Display Resolution" {
                // If resolution changed, update refresh rates
                rebuild_refresh_rate_choices(state);
            }
            if row.label == "Display Mode" {
                // If display mode changed (e.g. Screen 1 -> Screen 2), update refresh/resolution lists
                // We treat this as a monitor change if it's not "Windowed"
                let (cur_w, cur_h) = selected_resolution(state);
                rebuild_resolution_choices(state, cur_w, cur_h);
            }
        }
    }

    if let Some(list) = pending_resolution_list {
        state.resolution_choices = list;
    }

    // Begin cursor animation when changing inline options, but treat the Language row
    // as a single-value row (no horizontal tween; value changes in-place).
    if let (Some(choice_index), Some(new_index)) = (prev_choice_index, new_choice_index) {
        let is_language_row = rows
            .get(row_index)
            .map(|r| r.label == "Language")
            .unwrap_or(false);
        let is_inline_row = rows
            .get(row_index)
            .map(|r| r.inline)
            .unwrap_or(true);
        if is_inline_row && !is_language_row {
            state.cursor_anim_row = Some(row_index);
            state.cursor_anim_from_choice = choice_index;
            state.cursor_anim_to_choice = new_index;
            state.cursor_anim_t = 0.0;
        } else {
            state.cursor_anim_row = None;
            state.cursor_anim_t = 1.0;
        }
    }
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
                OptionsView::Submenu(_) => {
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
                    OptionsView::Submenu(kind) => {
                        let total = submenu_rows(kind).len() + 1;
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
                    OptionsView::Submenu(kind) => {
                        let total = submenu_rows(kind).len() + 1;
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
                on_lr_press(state, -1);
            } else {
                on_lr_release(state, -1);
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if ev.pressed {
                apply_submenu_choice_delta(state, 1);
                on_lr_press(state, 1);
            } else {
                on_lr_release(state, 1);
            }
        }
        VirtualAction::p1_start if ev.pressed => {
            match state.view {
                OptionsView::Main => {
                    let total = ITEMS.len();
                    if total == 0 {
                        return ScreenAction::None;
                    }
                    let sel = state.selected.min(total - 1);
                    let item = &ITEMS[sel];

                    // Route based on the selected row label.
                    match item.name {
                        // Enter System Options submenu.
                        "System Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::System);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        // Enter Graphics/Sound Options submenu.
                        "Graphics/Sound Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::GraphicsSound);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        // Navigate to the new mappings screen.
                        "Configure Keyboard/Pad Mappings" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Mappings);
                        }
                        // Navigate to Test Input screen.
                        "Test Input" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Input);
                        }
                        // Exit from Options back to Menu.
                        "Exit" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Menu);
                        }
                        _ => {}
                    }
                }
                OptionsView::Submenu(kind) => {
                    let total = submenu_rows(kind).len() + 1;
                    if total == 0 {
                        return ScreenAction::None;
                    }
                    // Exit row in the submenu: back to the main Options list.
                    if state.sub_selected == total - 1 {
                        audio::play_sfx("assets/sounds/start.ogg");
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

#[inline(always)]
fn scroll_offset(selected: usize, total_rows: usize) -> usize {
    let anchor_row: usize = 4; // keep cursor near middle (5th visible row)
    let max_offset = total_rows.saturating_sub(VISIBLE_ROWS);
    if total_rows <= VISIBLE_ROWS {
        0
    } else {
        selected.saturating_sub(anchor_row).min(max_offset)
    }
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

pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(320);
    let is_fading_submenu = !matches!(state.submenu_transition, SubmenuTransition::None);

    /* -------------------------- HEART BACKGROUND -------------------------- */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index, // <-- CHANGED
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        // Keep hearts always visible for actor-only fades (Options/Menu/Mappings);
        // local submenu fades are handled via content_alpha on UI actors only.
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui_actors = Vec::new();

    /* ------------------------------ TOP BAR ------------------------------- */
    const FG: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let title_text = match state.view {
        OptionsView::Main => "OPTIONS",
        OptionsView::Submenu(kind) => submenu_title(kind),
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
    let selected_item: Option<&Item>;

    match state.view {
        OptionsView::Main => {
            // Active text color (for normal rows) – Simply Love uses row index + global color index.
            let col_active_text = color::simply_love_rgba(state.active_color_index + state.selected as i32);

            // ---------------------------- Scrolling math ---------------------------
            let total_items = ITEMS.len();
            let offset_rows = scroll_offset(state.selected, total_items);

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
        OptionsView::Submenu(kind) => {
            let rows = submenu_rows(kind);
            let choice_indices = submenu_choice_indices(state, kind);
            let items = submenu_items(kind);
            // Active text color for submenu rows.
            let col_active_text = color::simply_love_rgba(state.active_color_index);
            // Inactive option text color should be #808080 (alpha 1.0), match player options.
            let sl_gray = color::rgba_hex("#808080");

            let total_rows = rows.len() + 1; // + Exit row
            let offset_rows = scroll_offset(state.sub_selected, total_rows);

            let label_bg_w = SUB_LABEL_COL_W * s;
            let label_text_x = list_x + SUB_LABEL_TEXT_LEFT_PAD * s;

            // Helper to compute the cursor center X for a given submenu row index.
            let calc_row_center_x = |row_idx: usize| -> f32 {
                if row_idx >= total_rows {
                    return list_x + list_w * 0.5;
                }
                if row_idx >= rows.len() {
                    // Exit row: center within the items column (row width minus label column),
                    // matching how single-value rows like Music Rate are centered in player_options.rs.
                    let item_col_left = list_x + label_bg_w;
                    let item_col_w = list_w - label_bg_w;
                    return item_col_left + item_col_w * 0.5 + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                }
                let row = &rows[row_idx];
                // Non-inline rows behave as single-value rows: keep the cursor centered
                // on the center of the available items column (row width minus label column).
                if !row.inline {
                    let item_col_left = list_x + label_bg_w;
                    let item_col_w = list_w - label_bg_w;
                    return item_col_left + item_col_w * 0.5 + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                }
                let choices = row_choices(state, kind, rows, row_idx);
                if choices.is_empty() {
                    return list_x + list_w * 0.5;
                }
                let value_zoom = 0.835_f32;
                let choice_inner_left = list_x + label_bg_w + SUB_INLINE_ITEMS_LEFT_PAD * s;
                let mut widths: Vec<f32> = Vec::with_capacity(choices.len());
                asset_manager.with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |metrics_font| {
                        for text in choices {
                            let mut w =
                                font::measure_line_width_logical(metrics_font, text.as_ref(), all_fonts)
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
                let sel_idx = choice_indices
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
                        if row_idx >= rows.len() {
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
                            let choices = rows[row_idx].choices;
                            if choices.is_empty() {
                                return;
                            }
                            let sel_idx = choice_indices
                                .get(row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(choices.len().saturating_sub(1));
                            let choice_text = if rows[row_idx].label == "Global Offset (ms)" {
                                format_ms(state.global_offset_ms)
                            } else if rows[row_idx].label == "Visual Delay (ms)" {
                                format_ms(state.visual_delay_ms)
                            } else {
                                choices[sel_idx].to_string()
                            };
                            let mut w =
                                font::measure_line_width_logical(metrics_font, &choice_text, all_fonts)
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

                if !is_exit && row_idx < rows.len() {
                    // Left label background column (matches player options style).
                    ui_actors.push(act!(quad:
                        align(0.0, 0.0):
                        xy(list_x, row_y):
                        zoomto(label_bg_w, ROW_H * s):
                        diffuse(0.0, 0.0, 0.0, 0.25)
                    ));

                    let row = &rows[row_idx];
                    let inline_row = row.inline;
                    let label = row.label;
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

                    // Inline Off/On options in the items column (or a single centered value if inline == false).
                    let mut choice_texts: Vec<Cow<'_, str>> = row_choices(state, kind, rows, row_idx);
                    if !choice_texts.is_empty() {
                        let value_zoom = 0.835_f32;
                        if row.label == "Global Offset (ms)" {
                            let formatted = Cow::Owned(format_ms(state.global_offset_ms));
                            choice_texts[0] = formatted;
                        } else if row.label == "Visual Delay (ms)" {
                            let formatted = Cow::Owned(format_ms(state.visual_delay_ms));
                            choice_texts[0] = formatted;
                        }

                        let mut widths: Vec<f32> = Vec::with_capacity(choice_texts.len());
                        asset_manager.with_fonts(|all_fonts| {
                            asset_manager.with_font("miso", |metrics_font| {
                                for text in &choice_texts {
                                    let mut w = font::measure_line_width_logical(metrics_font, text.as_ref(), all_fonts) as f32;
                                    if !w.is_finite() || w <= 0.0 {
                                        w = 1.0;
                                    }
                                    widths.push(w * value_zoom);
                                }
                            });
                        });

                        let selected_choice = choice_indices
                            .get(row_idx)
                            .copied()
                            .unwrap_or(0)
                            .min(choice_texts.len().saturating_sub(1));
                        let mut selected_left_x: Option<f32> = None;

                        let choice_inner_left = list_x + label_bg_w + SUB_INLINE_ITEMS_LEFT_PAD * s;
                        let mut x_positions: Vec<f32> = Vec::with_capacity(choice_texts.len());
                        if inline_row {
                            let mut x = choice_inner_left;
                            for w in &widths {
                                x_positions.push(x);
                                x += *w + INLINE_SPACING;
                            }
                        }

                        if inline_row {
                            for (idx, choice) in choice_texts.iter().enumerate() {
                                let x = x_positions.get(idx).copied().unwrap_or(choice_inner_left);
                                let is_choice_selected = idx == selected_choice;
                                if is_choice_selected {
                                    selected_left_x = Some(x);
                                }

                                let choice_color = if is_active { col_white } else { sl_gray };
                                ui_actors.push(act!(text:
                                    align(0.0, 0.5):
                                    xy(x, row_mid_y):
                                    zoom(value_zoom):
                                    diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                                    font("miso"):
                                    settext(choice.as_ref()):
                                    horizalign(left)
                                ));
                            }
                        } else {
                            let choice_color = if is_active { col_white } else { sl_gray };
                            let choice_center_x = calc_row_center_x(row_idx);
                            let choice_text = choice_texts
                                .get(selected_choice)
                                .map(|c| c.as_ref())
                                .unwrap_or("??");
                            let draw_w = widths.get(selected_choice).copied().unwrap_or(40.0);
                            selected_left_x = Some(choice_center_x - draw_w * 0.5);
                            ui_actors.push(act!(text:
                                align(0.5, 0.5):
                                xy(choice_center_x, row_mid_y):
                                zoom(value_zoom):
                                diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                                font("miso"):
                                settext(choice_text):
                                horizalign(center)
                            ));
                        }

                        // Underline the selected option when this row is active or inactive,
                        // matching the inline underline behavior from player_options.rs.
                        if let Some(sel_left_x) = selected_left_x {
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
                                    let underline_left_x = sel_left_x;
                                    ui_actors.push(act!(quad:
                                        align(0.0, 0.5):
                                        xy(underline_left_x, underline_y):
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
                            if let Some(mut target_left_x) = selected_left_x {
                                let draw_w = widths.get(sel_idx).copied().unwrap_or(40.0);
                                if !inline_row {
                                    let cx = calc_row_center_x(row_idx);
                                    target_left_x = cx - draw_w * 0.5;
                                }
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
                                        // Cap pad so ring doesn't encroach neighbors.
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
                                        if inline_row && let Some(anim_row) = state.cursor_anim_row
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
                    let mut center_x = calc_row_center_x(row_idx);
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
                                        if from_idx < rows.len() {
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
            let total_rows = rows.len() + 1;
            let sel = state.sub_selected.min(total_rows.saturating_sub(1));
            let item = if sel < rows.len() {
                &items[sel]
            } else {
                &items[items.len().saturating_sub(1)]
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
