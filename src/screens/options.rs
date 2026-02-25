use crate::act;
use crate::assets::AssetManager;
use crate::core::display::{self, MonitorSpec};
use crate::core::gfx::BackendType;
use crate::core::space::{screen_height, screen_width, widescale};
// Screen navigation is handled in app.rs via the dispatcher
use crate::config::{
    self, BreakdownStyle, DefaultFailType, DisplayMode, FullscreenType,
    SelectMusicPatternInfoMode, SimpleIni,
};
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
#[cfg(target_os = "windows")]
use crate::core::input::WindowsPadBackend;
use crate::game::parsing::{noteskin as noteskin_parser, simfile as song_loading};
use crate::game::{profile, scores};
use crate::screens::{Screen, ScreenAction};
use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::screens::components::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::screens::components::{heart_bg, screen_bar};
use crate::ui::actors;
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::font;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

/* ----------------------------- cursor tweening ----------------------------- */
// Simply Love metrics.ini uses 0.1 for both [ScreenOptions] TweenSeconds and CursorTweenSeconds.
// ScreenOptionsService rows inherit OptionRow tween behavior, so keep both aligned at 0.1.
const SL_OPTION_ROW_TWEEN_SECONDS: f32 = 0.1;
const CURSOR_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;
const ROW_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;
// Spacing between inline items in OptionRows (pixels at current zoom)
const INLINE_SPACING: f32 = 15.75;

// Match Simply Love operator menu ranges (±1000 ms) for these calibrations.
const GLOBAL_OFFSET_MIN_MS: i32 = -1000;
const GLOBAL_OFFSET_MAX_MS: i32 = 1000;
const VISUAL_DELAY_MIN_MS: i32 = -1000;
const VISUAL_DELAY_MAX_MS: i32 = 1000;
const VOLUME_MIN_PERCENT: i32 = 0;
const VOLUME_MAX_PERCENT: i32 = 100;

// --- Monitor & Video Mode Data Structures ---

#[derive(Clone, Copy, Debug)]
struct RowTween {
    from_y: f32,
    to_y: f32,
    from_a: f32,
    to_a: f32,
    t: f32,
}

impl RowTween {
    #[inline(always)]
    fn y(&self) -> f32 {
        (self.to_y - self.from_y).mul_add(self.t, self.from_y)
    }

    #[inline(always)]
    fn a(&self) -> f32 {
        (self.to_a - self.from_a).mul_add(self.t, self.from_a)
    }
}

#[inline(always)]
fn format_ms(value: i32) -> String {
    // Positive values omit a '+' and compact to the Simply Love "Nms" style.
    format!("{value}ms")
}

#[inline(always)]
fn format_percent(value: i32) -> String {
    format!("{value}%")
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

const SEP_W: f32 = 2.5; // gap/stripe between rows and description
const DESC_W: f32 = 292.0; // description panel width (WideScale(287,292) in SL)
// derive description height from visible rows so it never includes a trailing gap
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

/// Left margin for row labels (in content-space pixels).
const TEXT_LEFT_PAD: f32 = 40.66;
/// Left margin for the heart icon (in content-space pixels).
const HEART_LEFT_PAD: f32 = 13.0;
/// Label text zoom, matched to the left column titles in `player_options.rs`.
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

/// Description pane layout (mirrors Simply Love's `ScreenOptionsService` overlay).
/// Title and bullet list use separate top/side padding so they can be tuned independently.
const DESC_TITLE_TOP_PAD_PX: f32 = 9.75; // padding from box top to title
const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5; // left/right padding for title text
const DESC_BULLET_TOP_PAD_PX: f32 = 23.25; // vertical gap between title and bullet list
const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5; // left/right padding for bullet text
const DESC_BULLET_INDENT_PX: f32 = 10.0; // extra indent for bullet marker + text
const DESC_TITLE_ZOOM: f32 = 1.0; // title text zoom (roughly header-sized)
const DESC_BODY_ZOOM: f32 = 1.0; // body/bullet text zoom (similar to help text)

pub const ITEMS: &[Item] = &[
    // Top-level ScreenOptionsService rows, ordered to match Simply Love's LineNames.
    Item {
        name: "System Options",
        help: &[
            "Adjust high-level settings like game type, theme, language, and more.",
            "Game",
            "Theme",
            "Language",
            "Default NoteSkin",
        ],
    },
    Item {
        name: "Graphics Options",
        help: &[
            "Change screen aspect ratio, resolution, graphics quality, and timing visuals.",
            "Video Renderer",
            "DisplayMode",
            "DisplayAspectRatio",
            "DisplayResolution",
            "RefreshRate",
            "FullscreenType",
            "Wait for VSync",
            "Show Stats",
            "Visual Delay",
        ],
    },
    Item {
        name: "Sound Options",
        help: &[
            "Adjust audio output settings and feedback sounds.",
            "Master Volume",
            "SFX Volume",
            "Music Volume",
            "Audio Sample Rate",
            "Mine Sounds",
            "Global Offset",
            "Rate Mod Preserves Pitch",
        ],
    },
    Item {
        name: "Input Options",
        help: &[
            "Configure control mappings and input diagnostics.",
            "Configure Keyboard/Pad Mappings",
            "Test Input",
            "Input Options",
        ],
    },
    Item {
        name: "Machine Options",
        help: &[
            "Choose which startup and post-session screens are shown.",
            "Select Profile",
            "Select Color",
            "Select Style",
            "Select Play Mode",
            "Eval Summary",
            "Name Entry",
            "Gameover Screen",
            "Menu Music",
            "Keyboard Features",
        ],
    },
    Item {
        name: "Gameplay Options",
        help: &[
            "Adjust gameplay presentation settings.",
            GAMEPLAY_ROW_BG_BRIGHTNESS,
            GAMEPLAY_ROW_CENTERED_P1,
            GAMEPLAY_ROW_ZMOD_RATING_BOX,
            GAMEPLAY_ROW_BPM_DECIMAL,
        ],
    },
    Item {
        name: "Select Music Options",
        help: &[
            "Adjust behavior and display for the Select Music screen.",
            "Show Banners",
            "Show Breakdown",
            "Show Native Language",
            "Music Wheel Speed",
            "Show CDTitles",
            "Show Music Wheel Grades",
            "Show Music Wheel Lamps",
            "Show Pattern Info",
            "Music Previews",
            "Show Gameplay Timer",
            "Show Rivals",
        ],
    },
    Item {
        name: "Advanced Options",
        help: &[
            "Adjust machine-level fail and cache/parsing behavior.",
            "Default Fail Type",
            "Banner Cache",
            "Banner Cache Color Depth",
            "Banner Cache Min Dimension",
            "Banner Cache Pow2",
            "Banner Cache Scale Divisor",
            "Song Parsing Threads",
            "Cache Songs",
            "Fast Load",
        ],
    },
    Item {
        name: "Course Options",
        help: &["Adjust options related to course selection and course play behavior."],
    },
    Item {
        name: "Manage Local Profiles",
        help: &[
            "Create, edit, and manage player profiles that are stored on this computer.\n\nYou'll need a keyboard to use this screen.",
        ],
    },
    Item {
        name: "GrooveStats Options",
        help: &[
            "Manage GrooveStats settings.",
            "Enable GrooveStats",
            "Auto Populate GS Scores",
        ],
    },
    Item {
        name: "Arrow Cloud Options",
        help: &["Configure Arrow Cloud integration and related display behavior."],
    },
    Item {
        name: "Score Import",
        help: &[
            "Import online score data for a selected endpoint/profile and pack scope.",
            "API Endpoint",
            "Profile",
            "Pack",
            "Start",
        ],
    },
    Item {
        name: "Reload Songs/Courses",
        help: &["Reload all songs and courses from disk without restarting."],
    },
    Item {
        name: "Credits",
        help: &["View deadsync and project credits."],
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
    Graphics,
    Input,
    InputBackend,
    Machine,
    Advanced,
    Gameplay,
    Sound,
    SelectMusic,
    GrooveStats,
    ScoreImport,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReloadPhase {
    Songs,
    Courses,
}

#[derive(Debug)]
enum ReloadMsg {
    Phase(ReloadPhase),
    Song { pack: String, song: String },
    Course { group: String, course: String },
    Done,
}

struct ReloadUiState {
    phase: ReloadPhase,
    line2: String,
    line3: String,
    done: bool,
    rx: std::sync::mpsc::Receiver<ReloadMsg>,
}

impl ReloadUiState {
    fn new(rx: std::sync::mpsc::Receiver<ReloadMsg>) -> Self {
        Self {
            phase: ReloadPhase::Songs,
            line2: String::new(),
            line3: String::new(),
            done: false,
            rx,
        }
    }
}

#[derive(Clone, Debug)]
struct ScoreImportProfileConfig {
    id: String,
    display_name: String,
    gs_api_key: String,
    gs_username: String,
    ac_api_key: String,
}

#[derive(Clone, Debug)]
struct ScoreImportSelection {
    endpoint: scores::ScoreImportEndpoint,
    profile: ScoreImportProfileConfig,
    pack_group: Option<String>,
    pack_label: String,
    only_missing_gs_scores: bool,
}

#[derive(Debug)]
enum ScoreImportMsg {
    Progress(scores::ScoreImportProgress),
    Done(Result<scores::ScoreBulkImportSummary, String>),
}

struct ScoreImportUiState {
    endpoint: scores::ScoreImportEndpoint,
    profile_name: String,
    pack_label: String,
    total_charts: usize,
    processed_charts: usize,
    imported_scores: usize,
    missing_scores: usize,
    failed_requests: usize,
    detail_line: String,
    done: bool,
    done_message: String,
    done_since: Option<Instant>,
    cancel_requested: Arc<AtomicBool>,
    rx: std::sync::mpsc::Receiver<ScoreImportMsg>,
}

impl ScoreImportUiState {
    fn new(
        endpoint: scores::ScoreImportEndpoint,
        profile_name: String,
        pack_label: String,
        cancel_requested: Arc<AtomicBool>,
        rx: std::sync::mpsc::Receiver<ScoreImportMsg>,
    ) -> Self {
        Self {
            endpoint,
            profile_name,
            pack_label,
            total_charts: 0,
            processed_charts: 0,
            imported_scores: 0,
            missing_scores: 0,
            failed_requests: 0,
            detail_line: "Preparing score import...".to_string(),
            done: false,
            done_message: String::new(),
            done_since: None,
            cancel_requested,
            rx,
        }
    }
}

#[derive(Clone, Debug)]
struct ScoreImportConfirmState {
    selection: ScoreImportSelection,
    active_choice: u8, // 0 = Yes, 1 = No
}

// Local fade timing when swapping between main options list and System Options submenu.
const SUBMENU_FADE_DURATION: f32 = 0.2;

pub struct SubRow<'a> {
    pub label: &'a str,
    pub choices: &'a [&'a str],
    pub inline: bool, // whether to lay out choices inline (vs single centered value)
}

const GS_ROW_ENABLE: &str = "Enable GrooveStats";
const GS_ROW_AUTO_POPULATE: &str = "Auto Populate GS Scores";
const INPUT_ROW_CONFIGURE_MAPPINGS: &str = "Configure Keyboard/Pad Mappings";
const INPUT_ROW_TEST: &str = "Test Input";
const INPUT_ROW_OPTIONS: &str = "Input Options";
const INPUT_ROW_BACKEND: &str = "Gamepad Backend";
#[cfg(target_os = "windows")]
const INPUT_BACKEND_CHOICES: &[&str] = &["W32 Raw Input", "WGI"];
#[cfg(target_os = "macos")]
const INPUT_BACKEND_CHOICES: &[&str] = &["macOS IOHID"];
#[cfg(all(unix, not(target_os = "macos")))]
const INPUT_BACKEND_CHOICES: &[&str] = &["Linux evdev"];
#[cfg(not(any(target_os = "windows", unix)))]
const INPUT_BACKEND_CHOICES: &[&str] = &["Platform Default"];
#[cfg(target_os = "windows")]
const INPUT_BACKEND_INLINE: bool = true;
#[cfg(not(target_os = "windows"))]
const INPUT_BACKEND_INLINE: bool = false;
const SELECT_MUSIC_ROW_SHOW_BANNERS: &str = "Show Banners";
const SELECT_MUSIC_ROW_SHOW_BREAKDOWN: &str = "Show Breakdown";
const SELECT_MUSIC_ROW_BREAKDOWN_STYLE: &str = "Breakdown Style";
const SELECT_MUSIC_ROW_NATIVE_LANGUAGE: &str = "Show Native Language";
const SELECT_MUSIC_ROW_WHEEL_SPEED: &str = "Music Wheel Speed";
const SELECT_MUSIC_ROW_CDTITLES: &str = "Show CDTitles";
const SELECT_MUSIC_ROW_WHEEL_GRADES: &str = "Show Music Wheel Grades";
const SELECT_MUSIC_ROW_WHEEL_LAMPS: &str = "Show Music Wheel Lamps";
const SELECT_MUSIC_ROW_PATTERN_INFO: &str = "Show Pattern Info";
const SELECT_MUSIC_ROW_PREVIEWS: &str = "Music Previews";
const SELECT_MUSIC_ROW_PREVIEW_LOOP: &str = "Loop Music";
const SELECT_MUSIC_ROW_GAMEPLAY_TIMER: &str = "Show Gameplay Timer";
const SELECT_MUSIC_ROW_SHOW_RIVALS: &str = "Show Rivals";
const MACHINE_ROW_SELECT_PROFILE: &str = "Select Profile";
const MACHINE_ROW_SELECT_COLOR: &str = "Select Color";
const MACHINE_ROW_SELECT_STYLE: &str = "Select Style";
const MACHINE_ROW_SELECT_PLAY_MODE: &str = "Select Play Mode";
const MACHINE_ROW_EVAL_SUMMARY: &str = "Eval Summary";
const MACHINE_ROW_NAME_ENTRY: &str = "Name Entry";
const MACHINE_ROW_GAMEOVER: &str = "Gameover Screen";
const MACHINE_ROW_MENU_MUSIC: &str = "Menu Music";
const MACHINE_ROW_KEYBOARD_FEATURES: &str = "Keyboard Features";
const ADVANCED_ROW_DEFAULT_FAIL_TYPE: &str = "Default Fail Type";
const ADVANCED_ROW_BANNER_CACHE: &str = "Banner Cache";
const ADVANCED_ROW_BANNER_COLOR_DEPTH: &str = "Banner Cache Color Depth";
const ADVANCED_ROW_BANNER_MIN_DIMENSION: &str = "Banner Cache Min Dimension";
const ADVANCED_ROW_BANNER_POW2: &str = "Banner Cache Pow2";
const ADVANCED_ROW_BANNER_SCALE_DIVISOR: &str = "Banner Cache Scale Divisor";
const ADVANCED_ROW_SONG_PARSING_THREADS: &str = "Song Parsing Threads";
const ADVANCED_ROW_CACHE_SONGS: &str = "Cache Songs";
const ADVANCED_ROW_FAST_LOAD: &str = "Fast Load";
const GAMEPLAY_ROW_BG_BRIGHTNESS: &str = "BG Brightness";
const GAMEPLAY_ROW_CENTERED_P1: &str = "Centered P1 Notefield";
const GAMEPLAY_ROW_ZMOD_RATING_BOX: &str = "Zmod Rating Box";
const GAMEPLAY_ROW_BPM_DECIMAL: &str = "Show Decimal in BPM";
const SCORE_IMPORT_ROW_ENDPOINT: &str = "API Endpoint";
const SCORE_IMPORT_ROW_PROFILE: &str = "Profile";
const SCORE_IMPORT_ROW_PACK: &str = "Pack";
const SCORE_IMPORT_ROW_ONLY_MISSING: &str = "Only Missing GS Scores";
const SCORE_IMPORT_ROW_START: &str = "Start";
const SCORE_IMPORT_ALL_PACKS: &str = "All";
const SCORE_IMPORT_DONE_OVERLAY_SECONDS: f32 = 1.5;
const SCORE_IMPORT_ROW_ENDPOINT_INDEX: usize = 0;
const SCORE_IMPORT_ROW_PROFILE_INDEX: usize = 1;
const SCORE_IMPORT_ROW_PACK_INDEX: usize = 2;
const SCORE_IMPORT_ROW_ONLY_MISSING_INDEX: usize = 3;

fn discover_system_noteskin_choices() -> Vec<String> {
    let mut names = noteskin_parser::discover_itg_skins("dance");
    if names.is_empty() {
        names.push(profile::NoteSkin::DEFAULT_NAME.to_string());
    }
    names
}

pub const SYSTEM_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: "Game",
        choices: &["dance"],
        inline: false,
    },
    SubRow {
        label: "Theme",
        choices: &["Simply Love"],
        inline: false,
    },
    SubRow {
        label: "Language",
        choices: &["English"],
        inline: false,
    },
    SubRow {
        label: "Default NoteSkin",
        choices: &[profile::NoteSkin::DEFAULT_NAME],
        inline: false,
    },
];

pub const SYSTEM_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: "Game",
        help: &["Stored in deadsync.ini for compatibility; no runtime game-switch behavior yet."],
    },
    Item {
        name: "Theme",
        help: &[
            "Stored in deadsync.ini for compatibility; theme switching is not implemented yet.",
        ],
    },
    Item {
        name: "Language",
        help: &[
            "Stored in deadsync.ini for compatibility; runtime language switching is not implemented yet.",
        ],
    },
    Item {
        name: "Default NoteSkin",
        help: &["Choose the machine-wide default noteskin used by guests and new profiles."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

#[cfg(target_os = "windows")]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(not(target_os = "windows"))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
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
const SOFTWARE_THREADS_ROW_INDEX: usize = 1;
const DISPLAY_MODE_ROW_INDEX: usize = 2;
const DISPLAY_ASPECT_RATIO_ROW_INDEX: usize = 3;
const DISPLAY_RESOLUTION_ROW_INDEX: usize = 4;
const REFRESH_RATE_ROW_INDEX: usize = 5;
const FULLSCREEN_TYPE_ROW_INDEX: usize = 6;
const GRAPHICS_ROW_VIDEO_RENDERER: &str = "Video Renderer";
const GRAPHICS_ROW_SOFTWARE_THREADS: &str = "Software Renderer Threads";
const GRAPHICS_ROW_VALIDATION_LAYERS: &str = "Validation Layers";
const SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX: usize = 1;
const SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX: usize = 2;
const SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX: usize = 9;
const SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX: usize = 10;
const ADVANCED_BANNER_CACHE_ROW_INDEX: usize = 1;
const ADVANCED_BANNER_COLOR_DEPTH_ROW_INDEX: usize = 2;
const ADVANCED_BANNER_MIN_DIMENSION_ROW_INDEX: usize = 3;
const ADVANCED_BANNER_POW2_ROW_INDEX: usize = 4;
const ADVANCED_BANNER_SCALE_DIVISOR_ROW_INDEX: usize = 5;
const ADVANCED_SONG_PARSING_THREADS_ROW_INDEX: usize = 6;

const BG_BRIGHTNESS_CHOICES: [&str; 11] = [
    "0%", "10%", "20%", "30%", "40%", "50%", "60%", "70%", "80%", "90%", "100%",
];
const CENTERED_P1_NOTEFIELD_CHOICES: [&str; 2] = ["Off", "On"];
const ADVANCED_BANNER_COLOR_DEPTH_CHOICES: [&str; 3] = ["8", "16", "32"];
const ADVANCED_BANNER_MIN_DIMENSION_CHOICES: [&str; 6] = ["16", "32", "64", "128", "256", "512"];
const ADVANCED_BANNER_SCALE_DIVISOR_CHOICES: [&str; 8] =
    ["1", "2", "3", "4", "5", "6", "7", "8"];
const ADVANCED_BANNER_COLOR_DEPTH_VALUES: [u8; 3] = [8, 16, 32];
const ADVANCED_BANNER_MIN_DIMENSION_VALUES: [u16; 6] = [16, 32, 64, 128, 256, 512];
const ADVANCED_BANNER_SCALE_DIVISOR_VALUES: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
const MUSIC_WHEEL_SCROLL_SPEED_CHOICES: [&str; 7] = [
    "Slow",
    "Normal",
    "Fast",
    "Faster",
    "Ridiculous",
    "Ludicrous",
    "Plaid",
];
const MUSIC_WHEEL_SCROLL_SPEED_VALUES: [u8; 7] = [5, 10, 15, 25, 30, 45, 100];

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
        label: GRAPHICS_ROW_VIDEO_RENDERER,
        choices: VIDEO_RENDERER_LABELS,
        inline: false,
    },
    SubRow {
        label: GRAPHICS_ROW_SOFTWARE_THREADS,
        choices: &["Auto"],
        inline: false,
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
        inline: false,
    },
    SubRow {
        label: "Refresh Rate",
        choices: &[
            "Default", "60 Hz", "75 Hz", "120 Hz", "144 Hz", "165 Hz", "240 Hz", "360 Hz",
        ], // Replaced dynamically
        inline: false,
    },
    SubRow {
        label: "Fullscreen Type",
        choices: &["Exclusive", "Borderless"],
        inline: true,
    },
    SubRow {
        label: "Wait for VSync",
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: "Show Stats",
        choices: &["Off", "FPS", "FPS+Stutter"],
        inline: true,
    },
    SubRow {
        label: GRAPHICS_ROW_VALIDATION_LAYERS,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: "Visual Delay (ms)",
        choices: &["0 ms"],
        inline: false,
    },
];

pub const GRAPHICS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: GRAPHICS_ROW_VIDEO_RENDERER,
        help: &["Select the rendering backend."],
    },
    Item {
        name: GRAPHICS_ROW_SOFTWARE_THREADS,
        help: &[
            "Shown only when Video Renderer is Software.",
            "Set how many CPU threads software rendering can use.",
        ],
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
        name: "Wait for VSync",
        help: &["Enable vertical sync."],
    },
    Item {
        name: "Show Stats",
        help: &["Choose performance overlay mode: Off, FPS only, or FPS with stutter list."],
    },
    Item {
        name: GRAPHICS_ROW_VALIDATION_LAYERS,
        help: &[
            "Enable Vulkan/D3D/OpenGL validation layers for graphics debugging.",
            "Recommended: Off (FPS will drop by half but useful for debugging).",
        ],
    },
    Item {
        name: "Visual Delay (ms)",
        help: &["Apply a visual timing offset in 1 ms steps."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const INPUT_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: INPUT_ROW_CONFIGURE_MAPPINGS,
        choices: &["Open"],
        inline: false,
    },
    SubRow {
        label: INPUT_ROW_TEST,
        choices: &["Open"],
        inline: false,
    },
    SubRow {
        label: INPUT_ROW_OPTIONS,
        choices: &["Open"],
        inline: false,
    },
];

pub const INPUT_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: INPUT_ROW_CONFIGURE_MAPPINGS,
        help: &["Map keyboard keys, panels, menu buttons, etc. to game functions."],
    },
    Item {
        name: INPUT_ROW_TEST,
        help: &[
            "Test your dance pad/controller and menu buttons.\n\nIf one of your buttons is not mapped to a game function, it will appear here as \"not mapped\".",
        ],
    },
    Item {
        name: INPUT_ROW_OPTIONS,
        help: &[
            "Open additional input settings.",
            "Gamepad Backend",
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const INPUT_BACKEND_OPTIONS_ROWS: &[SubRow] = &[SubRow {
    label: INPUT_ROW_BACKEND,
    choices: INPUT_BACKEND_CHOICES,
    inline: INPUT_BACKEND_INLINE,
}];

pub const INPUT_BACKEND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: INPUT_ROW_BACKEND,
        help: &[
            "Choose gamepad input backend. On Windows this switches between WGI and W32 Raw Input.",
            "Changing backend requires a restart.",
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const MACHINE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: MACHINE_ROW_SELECT_PROFILE,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_SELECT_COLOR,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_SELECT_STYLE,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_SELECT_PLAY_MODE,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_EVAL_SUMMARY,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_NAME_ENTRY,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_GAMEOVER,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_MENU_MUSIC,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_KEYBOARD_FEATURES,
        choices: &["Off", "On"],
        inline: true,
    },
];

pub const MACHINE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: MACHINE_ROW_SELECT_PROFILE,
        help: &["Show or skip Select Profile during startup."],
    },
    Item {
        name: MACHINE_ROW_SELECT_COLOR,
        help: &["Show or skip Select Color during startup."],
    },
    Item {
        name: MACHINE_ROW_SELECT_STYLE,
        help: &["Show or skip Select Style during startup."],
    },
    Item {
        name: MACHINE_ROW_SELECT_PLAY_MODE,
        help: &["Show or skip Select Play Mode during startup."],
    },
    Item {
        name: MACHINE_ROW_EVAL_SUMMARY,
        help: &["Show or skip the Evaluation Summary flow after leaving song/course select."],
    },
    Item {
        name: MACHINE_ROW_NAME_ENTRY,
        help: &["Show or skip Name Entry after Evaluation Summary."],
    },
    Item {
        name: MACHINE_ROW_GAMEOVER,
        help: &["Show or skip the Gameover screen after Name Entry."],
    },
    Item {
        name: MACHINE_ROW_MENU_MUSIC,
        help: &["Play or mute the looping menu song on Select Color/Style/Play Mode."],
    },
    Item {
        name: MACHINE_ROW_KEYBOARD_FEATURES,
        help: &["Enable keyboard-only shortcuts like Ctrl+R restart in gameplay."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const GAMEPLAY_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: GAMEPLAY_ROW_BG_BRIGHTNESS,
        choices: &BG_BRIGHTNESS_CHOICES,
        inline: false,
    },
    SubRow {
        label: GAMEPLAY_ROW_CENTERED_P1,
        choices: &CENTERED_P1_NOTEFIELD_CHOICES,
        inline: true,
    },
    SubRow {
        label: GAMEPLAY_ROW_ZMOD_RATING_BOX,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: GAMEPLAY_ROW_BPM_DECIMAL,
        choices: &["Off", "On"],
        inline: true,
    },
];

pub const GAMEPLAY_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: GAMEPLAY_ROW_BG_BRIGHTNESS,
        help: &["Adjust the background brightness during gameplay."],
    },
    Item {
        name: GAMEPLAY_ROW_CENTERED_P1,
        help: &["Center the active single-player notefield during gameplay."],
    },
    Item {
        name: GAMEPLAY_ROW_ZMOD_RATING_BOX,
        help: &["Show the zmod-style difficulty text label with the rating box in gameplay/eval."],
    },
    Item {
        name: GAMEPLAY_ROW_BPM_DECIMAL,
        help: &["Show one decimal place for live gameplay BPM when BPM is non-integer."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const SOUND_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: "Master Volume",
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: "SFX Volume",
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: "Music Volume",
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: "Audio Sample Rate",
        choices: &["Auto", "44100 Hz", "48000 Hz"],
        inline: true,
    },
    SubRow {
        label: "Mine Sounds",
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: "Global Offset (ms)",
        choices: &["0 ms"],
        inline: false,
    },
    SubRow {
        label: "RateMod Preserves Pitch",
        choices: &["Off", "On"],
        inline: true,
    },
];

pub const SOUND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: "Master Volume",
        help: &["Set the overall volume for all audio."],
    },
    Item {
        name: "SFX Volume",
        help: &["Set the sound-effect volume before master volume is applied."],
    },
    Item {
        name: "Music Volume",
        help: &["Set the music volume before master volume is applied."],
    },
    Item {
        name: "Audio Sample Rate",
        help: &["Select an audio output sample rate."],
    },
    Item {
        name: "Mine Sounds",
        help: &["Play a sound when mines are hit."],
    },
    Item {
        name: "Global Offset (ms)",
        help: &["Apply a global audio timing offset in 1 ms steps."],
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

pub const SELECT_MUSIC_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: SELECT_MUSIC_ROW_SHOW_BANNERS,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_SHOW_BREAKDOWN,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_BREAKDOWN_STYLE,
        choices: &["SL", "SN"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_NATIVE_LANGUAGE,
        choices: &["Translit", "Native"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_WHEEL_SPEED,
        choices: &MUSIC_WHEEL_SCROLL_SPEED_CHOICES,
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_CDTITLES,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_WHEEL_GRADES,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_WHEEL_LAMPS,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_PATTERN_INFO,
        choices: &["Auto", "Tech", "Stamina"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_PREVIEWS,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_PREVIEW_LOOP,
        choices: &["Play Once", "Loop"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_GAMEPLAY_TIMER,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SELECT_MUSIC_ROW_SHOW_RIVALS,
        choices: &["No", "Yes"],
        inline: true,
    },
];

pub const SELECT_MUSIC_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: SELECT_MUSIC_ROW_SHOW_BANNERS,
        help: &["Show song/pack banners or force color fallback banners."],
    },
    Item {
        name: SELECT_MUSIC_ROW_SHOW_BREAKDOWN,
        help: &["Show or hide the stream breakdown panel in Select Music."],
    },
    Item {
        name: SELECT_MUSIC_ROW_BREAKDOWN_STYLE,
        help: &[
            "Choose which breakdown format to show in Select Music.",
            "SL uses Simply Love stream breakdown formatting.",
            "SN uses Stamina Nation stream breakdown formatting.",
        ],
    },
    Item {
        name: SELECT_MUSIC_ROW_NATIVE_LANGUAGE,
        help: &[
            "Choose how wheel titles are displayed.",
            "Translit uses transliterated tags when available; Native uses original tags.",
        ],
    },
    Item {
        name: SELECT_MUSIC_ROW_WHEEL_SPEED,
        help: &[
            "Set Select Music wheel hold-scroll speed.",
            "Parity mapping: Slow=5, Normal=10, Fast=15, Faster=25, Ridiculous=30, Ludicrous=45, Plaid=100.",
        ],
    },
    Item {
        name: SELECT_MUSIC_ROW_CDTITLES,
        help: &["Show or hide CDTitle sprites on Select Music."],
    },
    Item {
        name: SELECT_MUSIC_ROW_WHEEL_GRADES,
        help: &["Show or hide grade sprites on wheel rows."],
    },
    Item {
        name: SELECT_MUSIC_ROW_WHEEL_LAMPS,
        help: &["Show or hide lamp indicators on wheel rows."],
    },
    Item {
        name: SELECT_MUSIC_ROW_PATTERN_INFO,
        help: &[
            "Choose whether the lower chart info panel favors Tech, Stamina, or Auto detection.",
            "Recommended: Tech.",
        ],
    },
    Item {
        name: SELECT_MUSIC_ROW_PREVIEWS,
        help: &["Enable or disable Select Music audio previews."],
    },
    Item {
        name: SELECT_MUSIC_ROW_PREVIEW_LOOP,
        help: &["Choose whether previews loop or play once."],
    },
    Item {
        name: SELECT_MUSIC_ROW_GAMEPLAY_TIMER,
        help: &["Show the gameplay session timer on Select Music."],
    },
    Item {
        name: SELECT_MUSIC_ROW_SHOW_RIVALS,
        help: &[
            "Show rivals in pane/scorebox areas when available.",
            "When off, selected difficulty remains visible in the pane.",
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const ADVANCED_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: ADVANCED_ROW_DEFAULT_FAIL_TYPE,
        choices: &["Immediate", "ImmediateContinue"],
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_BANNER_CACHE,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_BANNER_COLOR_DEPTH,
        choices: &ADVANCED_BANNER_COLOR_DEPTH_CHOICES,
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_BANNER_MIN_DIMENSION,
        choices: &ADVANCED_BANNER_MIN_DIMENSION_CHOICES,
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_BANNER_POW2,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_BANNER_SCALE_DIVISOR,
        choices: &ADVANCED_BANNER_SCALE_DIVISOR_CHOICES,
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_SONG_PARSING_THREADS,
        choices: &["Auto"],
        inline: false,
    },
    SubRow {
        label: ADVANCED_ROW_CACHE_SONGS,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: ADVANCED_ROW_FAST_LOAD,
        choices: &["Off", "On"],
        inline: true,
    },
];

pub const ADVANCED_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: ADVANCED_ROW_DEFAULT_FAIL_TYPE,
        help: &[
            "Choose the machine fail behavior used when gameplay life reaches zero.",
            "Immediate: cuts to Evaluation as soon as all joined players fail.",
            "ImmediateContinue: keep playing to song end after failing.",
            "Default: ImmediateContinue (recommended).",
        ],
    },
    Item {
        name: ADVANCED_ROW_BANNER_CACHE,
        help: &[
            "Enable or disable the wheel banner cache on disk.",
            "Default: On (BannerCache=1).",
            "When Off, banner cache sub-options are hidden.",
        ],
    },
    Item {
        name: ADVANCED_ROW_BANNER_COLOR_DEPTH,
        help: &[
            "Set cached banner color depth in bits (8/16/32).",
            "Default: 16 (BannerCacheColorDepth=16).",
        ],
    },
    Item {
        name: ADVANCED_ROW_BANNER_MIN_DIMENSION,
        help: &[
            "Set the minimum cached banner dimension in pixels.",
            "Default: 32 (BannerCacheMinDimension=32).",
        ],
    },
    Item {
        name: ADVANCED_ROW_BANNER_POW2,
        help: &[
            "Round cached banner dimensions to power-of-two sizes.",
            "Default: On (BannerCachePow2=1).",
        ],
    },
    Item {
        name: ADVANCED_ROW_BANNER_SCALE_DIVISOR,
        help: &[
            "Set banner downscale divisor used by cache generation.",
            "Default: 2 (BannerCacheScaleDivisor=2).",
        ],
    },
    Item {
        name: ADVANCED_ROW_SONG_PARSING_THREADS,
        help: &[
            "Set worker threads for simfile parsing at startup.",
            "Default: Auto (SongParsingThreads=0).",
        ],
    },
    Item {
        name: ADVANCED_ROW_CACHE_SONGS,
        help: &[
            "Enable or disable writing/using cached song metadata.",
            "Default: On (CacheSongs=1).",
        ],
    },
    Item {
        name: ADVANCED_ROW_FAST_LOAD,
        help: &[
            "Enable startup shortcuts that reduce blocking load work.",
            "Default: On (FastLoad=1).",
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const GROOVESTATS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: GS_ROW_ENABLE,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: GS_ROW_AUTO_POPULATE,
        choices: &["No", "Yes"],
        inline: true,
    },
];

pub const SCORE_IMPORT_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: SCORE_IMPORT_ROW_ENDPOINT,
        choices: &["GrooveStats", "BoogieStats", "ArrowCloud"],
        inline: true,
    },
    SubRow {
        label: SCORE_IMPORT_ROW_PROFILE,
        choices: &["No eligible profiles"],
        inline: false,
    },
    SubRow {
        label: SCORE_IMPORT_ROW_PACK,
        choices: &[SCORE_IMPORT_ALL_PACKS],
        inline: false,
    },
    SubRow {
        label: SCORE_IMPORT_ROW_ONLY_MISSING,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: SCORE_IMPORT_ROW_START,
        choices: &["Start"],
        inline: false,
    },
];

pub const GROOVESTATS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: GS_ROW_ENABLE,
        help: &["Enable connection to GrooveStats services."],
    },
    Item {
        name: GS_ROW_AUTO_POPULATE,
        help: &["Import GS grade/lamp/score when scorebox leaderboard requests complete."],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const SCORE_IMPORT_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: SCORE_IMPORT_ROW_ENDPOINT,
        help: &[
            "Choose the source endpoint to import scores from.",
            "GrooveStats, BoogieStats, or ArrowCloud.",
        ],
    },
    Item {
        name: SCORE_IMPORT_ROW_PROFILE,
        help: &[
            "Select a local profile that has credentials configured for this endpoint.",
            "GS/BS require API key + username in groovestats.ini.",
            "AC requires API key in arrowcloud.ini.",
        ],
    },
    Item {
        name: SCORE_IMPORT_ROW_PACK,
        help: &[
            "Choose which installed pack to include in score import.",
            "Use All to import across every installed pack.",
        ],
    },
    Item {
        name: SCORE_IMPORT_ROW_ONLY_MISSING,
        help: &[
            "When Yes, import only charts with no cached GS score yet.",
            "When No, request every selected chart hash.",
        ],
    },
    Item {
        name: SCORE_IMPORT_ROW_START,
        help: &[
            "Bulk-imports this profile's scores for the selected endpoint and pack filter.",
            "Hard-limited to 3 requests/sec to avoid API spam.",
            "For many charts, this can take more than one hour.",
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

const fn submenu_rows(kind: SubmenuKind) -> &'static [SubRow<'static>] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ROWS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ROWS,
        SubmenuKind::Input => INPUT_OPTIONS_ROWS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ROWS,
        SubmenuKind::Machine => MACHINE_OPTIONS_ROWS,
        SubmenuKind::Advanced => ADVANCED_OPTIONS_ROWS,
        SubmenuKind::Gameplay => GAMEPLAY_OPTIONS_ROWS,
        SubmenuKind::Sound => SOUND_OPTIONS_ROWS,
        SubmenuKind::SelectMusic => SELECT_MUSIC_OPTIONS_ROWS,
        SubmenuKind::GrooveStats => GROOVESTATS_OPTIONS_ROWS,
        SubmenuKind::ScoreImport => SCORE_IMPORT_OPTIONS_ROWS,
    }
}

const fn submenu_items(kind: SubmenuKind) -> &'static [Item<'static>] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ITEMS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ITEMS,
        SubmenuKind::Input => INPUT_OPTIONS_ITEMS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ITEMS,
        SubmenuKind::Machine => MACHINE_OPTIONS_ITEMS,
        SubmenuKind::Advanced => ADVANCED_OPTIONS_ITEMS,
        SubmenuKind::Gameplay => GAMEPLAY_OPTIONS_ITEMS,
        SubmenuKind::Sound => SOUND_OPTIONS_ITEMS,
        SubmenuKind::SelectMusic => SELECT_MUSIC_OPTIONS_ITEMS,
        SubmenuKind::GrooveStats => GROOVESTATS_OPTIONS_ITEMS,
        SubmenuKind::ScoreImport => SCORE_IMPORT_OPTIONS_ITEMS,
    }
}

const fn submenu_title(kind: SubmenuKind) -> &'static str {
    match kind {
        SubmenuKind::System => "SYSTEM OPTIONS",
        SubmenuKind::Graphics => "GRAPHICS OPTIONS",
        SubmenuKind::Input => "INPUT OPTIONS",
        SubmenuKind::InputBackend => "INPUT OPTIONS",
        SubmenuKind::Machine => "MACHINE OPTIONS",
        SubmenuKind::Advanced => "ADVANCED OPTIONS",
        SubmenuKind::Gameplay => "GAMEPLAY OPTIONS",
        SubmenuKind::Sound => "SOUND OPTIONS",
        SubmenuKind::SelectMusic => "SELECT MUSIC OPTIONS",
        SubmenuKind::GrooveStats => "GROOVESTATS OPTIONS",
        SubmenuKind::ScoreImport => "SCORE IMPORT",
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
        .map_or_else(|| VIDEO_RENDERER_OPTIONS[0].0, |(backend, _)| *backend)
}

fn selected_video_renderer(state: &State) -> BackendType {
    let choice_idx = state
        .sub_choice_indices_graphics
        .get(VIDEO_RENDERER_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    renderer_choice_index_to_backend(choice_idx)
}

fn build_software_thread_choices() -> Vec<u8> {
    let max_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .clamp(2, 32);
    let mut out = Vec::with_capacity(max_threads + 1);
    out.push(0); // Auto
    for n in 1..=max_threads {
        out.push(n as u8);
    }
    out
}

fn software_thread_choice_labels(values: &[u8]) -> Vec<String> {
    values
        .iter()
        .map(|v| {
            if *v == 0 {
                "Auto".to_string()
            } else {
                v.to_string()
            }
        })
        .collect()
}

fn software_thread_choice_index(values: &[u8], thread_count: u8) -> usize {
    values
        .iter()
        .position(|&v| v == thread_count)
        .unwrap_or_else(|| {
            values
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.abs_diff(thread_count))
                .map_or(0, |(idx, _)| idx)
        })
}

fn software_thread_from_choice(values: &[u8], idx: usize) -> u8 {
    values.get(idx).copied().unwrap_or(0)
}

#[inline(always)]
fn graphics_show_software_threads(state: &State) -> bool {
    selected_video_renderer(state) == BackendType::Software
}

fn submenu_visible_row_indices(state: &State, kind: SubmenuKind, rows: &[SubRow<'_>]) -> Vec<usize> {
    match kind {
        SubmenuKind::Graphics => {
            let show_sw = graphics_show_software_threads(state);
            rows.iter()
                .enumerate()
                .filter_map(|(idx, row)| {
                    if row.label == GRAPHICS_ROW_SOFTWARE_THREADS && !show_sw {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        SubmenuKind::Advanced => {
            let show_banner_cache_rows = state
                .sub_choice_indices_advanced
                .get(ADVANCED_BANNER_CACHE_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 1;
            rows.iter()
                .enumerate()
                .filter_map(|(idx, _)| {
                    if !show_banner_cache_rows
                        && matches!(
                            idx,
                            ADVANCED_BANNER_COLOR_DEPTH_ROW_INDEX
                                | ADVANCED_BANNER_MIN_DIMENSION_ROW_INDEX
                                | ADVANCED_BANNER_POW2_ROW_INDEX
                                | ADVANCED_BANNER_SCALE_DIVISOR_ROW_INDEX
                        )
                    {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        SubmenuKind::SelectMusic => {
            let show_breakdown = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 1;
            let show_previews = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 1;
            rows.iter()
                .enumerate()
                .filter_map(|(idx, _)| {
                    if idx == SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX && !show_breakdown {
                        None
                    } else if idx == SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX && !show_previews {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        _ => (0..rows.len()).collect(),
    }
}

fn submenu_total_rows(state: &State, kind: SubmenuKind) -> usize {
    let rows = submenu_rows(kind);
    submenu_visible_row_indices(state, kind, rows).len() + 1
}

fn submenu_visible_row_to_actual(
    state: &State,
    kind: SubmenuKind,
    visible_row_idx: usize,
) -> Option<usize> {
    let rows = submenu_rows(kind);
    let visible_rows = submenu_visible_row_indices(state, kind, rows);
    visible_rows.get(visible_row_idx).copied()
}

#[cfg(target_os = "windows")]
const fn windows_backend_choice_index(backend: WindowsPadBackend) -> usize {
    match backend {
        WindowsPadBackend::RawInput => 0,
        WindowsPadBackend::Auto | WindowsPadBackend::Wgi => 1,
    }
}

#[cfg(target_os = "windows")]
const fn windows_backend_from_choice(idx: usize) -> WindowsPadBackend {
    match idx {
        0 => WindowsPadBackend::RawInput,
        _ => WindowsPadBackend::Wgi,
    }
}

const fn fullscreen_type_to_choice_index(fullscreen_type: FullscreenType) -> usize {
    match fullscreen_type {
        FullscreenType::Exclusive => 0,
        FullscreenType::Borderless => 1,
    }
}

const fn choice_index_to_fullscreen_type(idx: usize) -> FullscreenType {
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
        .map_or(FullscreenType::Exclusive, choice_index_to_fullscreen_type)
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
    let windowed_idx = state.display_mode_choices.len().saturating_sub(1);
    if windowed_idx == 0 || display_choice >= windowed_idx {
        0
    } else {
        display_choice.min(windowed_idx.saturating_sub(1))
    }
}

fn ensure_display_mode_choices(state: &mut State) {
    state.display_mode_choices = build_display_mode_choices(&state.monitor_specs);
    // If current selection is out of bounds, reset it.
    if let Some(idx) = state
        .sub_choice_indices_graphics
        .get_mut(DISPLAY_MODE_ROW_INDEX)
        && *idx >= state.display_mode_choices.len()
    {
        *idx = 0;
    }
    if let Some(choice_idx) = state
        .sub_choice_indices_graphics
        .get(DISPLAY_MODE_ROW_INDEX)
        .copied()
        && let Some(cursor_idx) = state
            .sub_cursor_indices_graphics
            .get_mut(DISPLAY_MODE_ROW_INDEX)
    {
        *cursor_idx = choice_idx;
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
    if let Some(slot) = state
        .sub_cursor_indices_graphics
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
        "4:3" => vec![
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 960),
            (1600, 1200),
        ],
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
        .or_else(|| state.resolution_choices.first().copied())
        .unwrap_or((state.display_width_at_load, state.display_height_at_load))
}

fn rebuild_refresh_rate_choices(state: &mut State) {
    if matches!(selected_display_mode(state), DisplayMode::Windowed) {
        state.refresh_rate_choices = vec![0];
        if let Some(slot) = state
            .sub_choice_indices_graphics
            .get_mut(REFRESH_RATE_ROW_INDEX)
        {
            *slot = 0;
        }
        if let Some(slot) = state
            .sub_cursor_indices_graphics
            .get_mut(REFRESH_RATE_ROW_INDEX)
        {
            *slot = 0;
        }
        return;
    }

    let (width, height) = selected_resolution(state);
    let mon_idx = selected_display_monitor(state);
    let mut rates = Vec::new();

    // Default choice is always available (0).
    rates.push(0);

    let supported_rates =
        display::supported_refresh_rates(state.monitor_specs.get(mon_idx), width, height);
    rates.extend(supported_rates);

    // Add common fallback rates if list is empty (besides Default)
    if rates.len() == 1 {
        rates.extend_from_slice(&[60000, 75000, 120000, 144000, 165000, 240000]);
    }

    // Preserve current selection if possible, else default to "Default".
    let current_rate = if let Some(idx) = state
        .sub_choice_indices_graphics
        .get(REFRESH_RATE_ROW_INDEX)
    {
        state.refresh_rate_choices.get(*idx).copied().unwrap_or(0)
    } else {
        0
    };

    state.refresh_rate_choices = rates;

    let next_idx = state
        .refresh_rate_choices
        .iter()
        .position(|&r| r == current_rate)
        .unwrap_or(0);
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(REFRESH_RATE_ROW_INDEX)
    {
        *slot = next_idx;
    }
    if let Some(slot) = state
        .sub_cursor_indices_graphics
        .get_mut(REFRESH_RATE_ROW_INDEX)
    {
        *slot = next_idx;
    }
}

fn rebuild_resolution_choices(state: &mut State, width: u32, height: u32) {
    let aspect_label = selected_aspect_label(state);
    let mon_idx = selected_display_monitor(state);

    let mut list: Vec<(u32, u32)> =
        display::supported_resolutions(state.monitor_specs.get(mon_idx))
            .into_iter()
            .filter(|(w, h)| aspect_matches(*w, *h, aspect_label))
            .collect();

    // 2. If list is empty (e.g. no monitor data or Aspect filter too strict), use presets.
    if list.is_empty() {
        list = preset_resolutions_for_aspect(aspect_label);
    }

    // 3. Keep the current resolution only if it matches the selected aspect.
    if aspect_matches(width, height, aspect_label) {
        push_unique_resolution(&mut list, width, height);
    }

    // Sort descending by width then height (typical UI preference).
    list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    state.resolution_choices = list;
    let next_idx = state
        .resolution_choices
        .iter()
        .position(|&(w, h)| w == width && h == height)
        .unwrap_or(0);
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(DISPLAY_RESOLUTION_ROW_INDEX)
    {
        *slot = next_idx;
    }
    if let Some(slot) = state
        .sub_cursor_indices_graphics
        .get_mut(DISPLAY_RESOLUTION_ROW_INDEX)
    {
        *slot = next_idx;
    }

    // Rebuild refresh rates since available rates depend on resolution.
    rebuild_refresh_rate_choices(state);
}

#[inline(always)]
const fn score_import_endpoint_from_choice_index(idx: usize) -> scores::ScoreImportEndpoint {
    match idx {
        1 => scores::ScoreImportEndpoint::BoogieStats,
        2 => scores::ScoreImportEndpoint::ArrowCloud,
        _ => scores::ScoreImportEndpoint::GrooveStats,
    }
}

#[inline(always)]
fn score_import_selected_endpoint(state: &State) -> scores::ScoreImportEndpoint {
    let idx = state
        .sub_choice_indices_score_import
        .get(SCORE_IMPORT_ROW_ENDPOINT_INDEX)
        .copied()
        .unwrap_or(0);
    score_import_endpoint_from_choice_index(idx)
}

fn score_import_pack_options() -> (Vec<String>, Vec<Option<String>>) {
    let cache = crate::game::song::get_song_cache();
    let mut packs: Vec<(String, String)> = Vec::with_capacity(cache.len());
    let mut seen_groups: HashSet<String> = HashSet::with_capacity(cache.len());

    for pack in cache.iter() {
        let group_name = pack.group_name.trim();
        if group_name.is_empty() {
            continue;
        }
        let group_key = group_name.to_ascii_lowercase();
        if !seen_groups.insert(group_key) {
            continue;
        }
        let display_name = if pack.name.trim().is_empty() {
            group_name.to_string()
        } else {
            pack.name.trim().to_string()
        };
        packs.push((display_name, group_name.to_string()));
    }

    packs.sort_by(|a, b| {
        a.0.to_ascii_lowercase()
            .cmp(&b.0.to_ascii_lowercase())
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut choices = Vec::with_capacity(packs.len() + 1);
    let mut filters = Vec::with_capacity(packs.len() + 1);
    choices.push(SCORE_IMPORT_ALL_PACKS.to_string());
    filters.push(None);
    for (display_name, group_name) in packs {
        choices.push(display_name);
        filters.push(Some(group_name));
    }
    (choices, filters)
}

fn load_score_import_profiles() -> Vec<ScoreImportProfileConfig> {
    let mut profiles = Vec::new();
    for summary in profile::scan_local_profiles() {
        let profile_dir = PathBuf::from("save/profiles").join(summary.id.as_str());
        let mut gs = SimpleIni::new();
        let mut ac = SimpleIni::new();
        let gs_api_key = if gs.load(profile_dir.join("groovestats.ini")).is_ok() {
            gs.get("GrooveStats", "ApiKey")
                .map_or_else(String::new, |v| v.trim().to_string())
        } else {
            String::new()
        };
        let gs_username = if gs_api_key.is_empty() {
            String::new()
        } else {
            gs.get("GrooveStats", "Username")
                .map_or_else(String::new, |v| v.trim().to_string())
        };
        let ac_api_key = if ac.load(profile_dir.join("arrowcloud.ini")).is_ok() {
            ac.get("ArrowCloud", "ApiKey")
                .map_or_else(String::new, |v| v.trim().to_string())
        } else {
            String::new()
        };
        profiles.push(ScoreImportProfileConfig {
            id: summary.id,
            display_name: summary.display_name.trim().to_string(),
            gs_api_key,
            gs_username,
            ac_api_key,
        });
    }
    profiles.sort_by(|a, b| {
        let al = a.display_name.to_ascii_lowercase();
        let bl = b.display_name.to_ascii_lowercase();
        al.cmp(&bl).then_with(|| a.id.cmp(&b.id))
    });
    profiles
}

#[inline(always)]
fn score_import_profile_eligible(
    endpoint: scores::ScoreImportEndpoint,
    profile_cfg: &ScoreImportProfileConfig,
) -> bool {
    match endpoint {
        scores::ScoreImportEndpoint::GrooveStats | scores::ScoreImportEndpoint::BoogieStats => {
            !profile_cfg.gs_api_key.is_empty() && !profile_cfg.gs_username.is_empty()
        }
        scores::ScoreImportEndpoint::ArrowCloud => !profile_cfg.ac_api_key.is_empty(),
    }
}

fn refresh_score_import_profile_options(state: &mut State) {
    state.score_import_profile_choices.clear();
    state.score_import_profile_ids.clear();

    let endpoint = score_import_selected_endpoint(state);
    for profile_cfg in &state.score_import_profiles {
        if !score_import_profile_eligible(endpoint, profile_cfg) {
            continue;
        }
        let label = if profile_cfg.display_name.is_empty() {
            profile_cfg.id.clone()
        } else {
            format!("{} ({})", profile_cfg.display_name, profile_cfg.id)
        };
        state.score_import_profile_choices.push(label);
        state
            .score_import_profile_ids
            .push(Some(profile_cfg.id.clone()));
    }
    if state.score_import_profile_choices.is_empty() {
        state
            .score_import_profile_choices
            .push("No eligible profiles".to_string());
        state.score_import_profile_ids.push(None);
    }

    let max_idx = state.score_import_profile_choices.len().saturating_sub(1);
    if let Some(slot) = state
        .sub_choice_indices_score_import
        .get_mut(SCORE_IMPORT_ROW_PROFILE_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state
        .sub_cursor_indices_score_import
        .get_mut(SCORE_IMPORT_ROW_PROFILE_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
}

fn refresh_score_import_pack_options(state: &mut State) {
    let (choices, filters) = score_import_pack_options();
    state.score_import_pack_choices = choices;
    state.score_import_pack_filters = filters;
    let max_idx = state.score_import_pack_choices.len().saturating_sub(1);
    if let Some(slot) = state
        .sub_choice_indices_score_import
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state
        .sub_cursor_indices_score_import
        .get_mut(SCORE_IMPORT_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
}

fn refresh_score_import_options(state: &mut State) {
    state.score_import_profiles = load_score_import_profiles();
    refresh_score_import_profile_options(state);
    refresh_score_import_pack_options(state);
}

fn selected_score_import_pack_group(state: &State) -> Option<String> {
    let pack_idx = state
        .sub_choice_indices_score_import
        .get(SCORE_IMPORT_ROW_PACK_INDEX)
        .copied()
        .unwrap_or(0)
        .min(state.score_import_pack_filters.len().saturating_sub(1));
    state
        .score_import_pack_filters
        .get(pack_idx)
        .and_then(|opt| opt.clone())
}

fn selected_score_import_profile(state: &State) -> Option<ScoreImportProfileConfig> {
    let profile_idx = state
        .sub_choice_indices_score_import
        .get(SCORE_IMPORT_ROW_PROFILE_INDEX)
        .copied()
        .unwrap_or(0)
        .min(state.score_import_profile_ids.len().saturating_sub(1));
    let profile_id = state
        .score_import_profile_ids
        .get(profile_idx)
        .and_then(|id| id.clone())?;
    state
        .score_import_profiles
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
}

#[inline(always)]
fn score_import_only_missing_gs_scores(state: &State) -> bool {
    state
        .sub_choice_indices_score_import
        .get(SCORE_IMPORT_ROW_ONLY_MISSING_INDEX)
        .copied()
        .unwrap_or(0)
        == 1
}

fn selected_score_import_selection(state: &State) -> Option<ScoreImportSelection> {
    let endpoint = score_import_selected_endpoint(state);
    let profile_cfg = selected_score_import_profile(state)?;
    if !score_import_profile_eligible(endpoint, &profile_cfg) {
        return None;
    }
    let pack_group = selected_score_import_pack_group(state);
    let pack_label = pack_group
        .as_ref()
        .cloned()
        .unwrap_or_else(|| SCORE_IMPORT_ALL_PACKS.to_string());
    let only_missing_gs_scores = score_import_only_missing_gs_scores(state);
    Some(ScoreImportSelection {
        endpoint,
        profile: profile_cfg,
        pack_group,
        pack_label,
        only_missing_gs_scores,
    })
}

fn row_choices<'a>(
    state: &'a State,
    kind: SubmenuKind,
    rows: &'a [SubRow<'a>],
    row_idx: usize,
) -> Vec<Cow<'a, str>> {
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::System)
        && row.label == "Default NoteSkin"
    {
        return state
            .system_noteskin_choices
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Graphics)
    {
        if row.label == GRAPHICS_ROW_SOFTWARE_THREADS {
            return state
                .software_thread_labels
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
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
            return state
                .refresh_rate_choices
                .iter()
                .map(|&mhz| {
                    if mhz == 0 {
                        Cow::Borrowed("Default")
                    } else {
                        // Format nicely: 60000 -> "60 Hz", 59940 -> "59.94 Hz"
                        let hz = mhz as f32 / 1000.0;
                        if (hz.fract()).abs() < 0.01 {
                            Cow::Owned(format!("{hz:.0}Hz"))
                        } else {
                            Cow::Owned(format!("{hz:.2}Hz"))
                        }
                    }
                })
                .collect();
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Advanced)
        && row.label == ADVANCED_ROW_SONG_PARSING_THREADS
    {
        return state
            .software_thread_labels
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::ScoreImport)
    {
        if row.label == SCORE_IMPORT_ROW_PROFILE {
            return state
                .score_import_profile_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.label == SCORE_IMPORT_ROW_PACK {
            return state
                .score_import_pack_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
    }
    rows.get(row_idx)
        .map(|row| row.choices.iter().map(|c| Cow::Borrowed(*c)).collect())
        .unwrap_or_default()
}

fn submenu_inline_choice_centers(
    state: &State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    row_idx: usize,
) -> Vec<f32> {
    let rows = submenu_rows(kind);
    let Some(row) = rows.get(row_idx) else {
        return Vec::new();
    };
    if !row.inline {
        return Vec::new();
    }
    let mut choice_texts = row_choices(state, kind, rows, row_idx);
    if choice_texts.is_empty() {
        return Vec::new();
    }
    if row.label == "Global Offset (ms)" {
        choice_texts[0] = Cow::Owned(format_ms(state.global_offset_ms));
    } else if row.label == "Visual Delay (ms)" {
        choice_texts[0] = Cow::Owned(format_ms(state.visual_delay_ms));
    }
    let value_zoom = 0.835_f32;
    let mut centers: Vec<f32> = Vec::with_capacity(choice_texts.len());
    let mut x = 0.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            for text in &choice_texts {
                let mut w =
                    font::measure_line_width_logical(metrics_font, text.as_ref(), all_fonts) as f32;
                if !w.is_finite() || w <= 0.0 {
                    w = 1.0;
                }
                let draw_w = w * value_zoom;
                centers.push(draw_w.mul_add(0.5, x));
                x += draw_w + INLINE_SPACING;
            }
        });
    });
    centers
}

fn sync_submenu_inline_x_from_row(
    state: &mut State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    visible_row_idx: usize,
) {
    let Some(row_idx) = submenu_visible_row_to_actual(state, kind, visible_row_idx) else {
        return;
    };
    let centers = submenu_inline_choice_centers(state, asset_manager, kind, row_idx);
    if centers.is_empty() {
        return;
    }
    let choice_idx = submenu_choice_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(centers.len().saturating_sub(1));
    state.sub_inline_x = centers[choice_idx];
}

fn apply_submenu_inline_x_to_row(
    state: &mut State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    visible_row_idx: usize,
) {
    let Some(row_idx) = submenu_visible_row_to_actual(state, kind, visible_row_idx) else {
        return;
    };
    let centers = submenu_inline_choice_centers(state, asset_manager, kind, row_idx);
    if centers.is_empty() {
        return;
    }
    let choice_idx = submenu_choice_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(centers.len().saturating_sub(1));
    if let Some(slot) = submenu_cursor_indices_mut(state, kind).get_mut(row_idx) {
        *slot = choice_idx;
    }
    if let Some(&x) = centers.get(choice_idx) {
        state.sub_inline_x = x;
    }
}

fn move_submenu_selection_vertical(
    state: &mut State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    dir: NavDirection,
) {
    let total = submenu_total_rows(state, kind);
    if total == 0 {
        return;
    }
    let current_row = state.sub_selected.min(total.saturating_sub(1));
    if !state.sub_inline_x.is_finite() {
        sync_submenu_inline_x_from_row(state, asset_manager, kind, current_row);
    }
    state.sub_selected = match dir {
        NavDirection::Up => {
            if current_row == 0 {
                total - 1
            } else {
                current_row - 1
            }
        }
        NavDirection::Down => (current_row + 1) % total,
    };
    apply_submenu_inline_x_to_row(state, asset_manager, kind, state.sub_selected);
}

const SOUND_VOLUME_LEVELS: [u8; 6] = [0, 10, 25, 50, 75, 100];
const SAMPLE_RATE_OPTIONS: [Option<u32>; 3] = [None, Some(44100), Some(48000)];

fn set_choice_by_label(choice_indices: &mut Vec<usize>, rows: &[SubRow], label: &str, idx: usize) {
    if let Some(pos) = rows.iter().position(|r| r.label == label)
        && let Some(slot) = choice_indices.get_mut(pos)
    {
        let max_idx = rows[pos].choices.len().saturating_sub(1);
        *slot = idx.min(max_idx);
    }
}

fn master_volume_choice_index(volume: u8) -> usize {
    let mut best_idx = 0usize;
    let mut best_diff = u8::MAX;
    for (idx, level) in SOUND_VOLUME_LEVELS.iter().enumerate() {
        let diff = volume.abs_diff(*level);
        if diff < best_diff {
            best_diff = diff;
            best_idx = idx;
        }
    }
    best_idx
}

fn master_volume_from_choice(idx: usize) -> u8 {
    SOUND_VOLUME_LEVELS
        .get(idx)
        .copied()
        .unwrap_or_else(|| *SOUND_VOLUME_LEVELS.last().unwrap_or(&100))
}

fn sample_rate_choice_index(rate: Option<u32>) -> usize {
    SAMPLE_RATE_OPTIONS
        .iter()
        .position(|&r| r == rate)
        .unwrap_or(0)
}

fn sample_rate_from_choice(idx: usize) -> Option<u32> {
    SAMPLE_RATE_OPTIONS.get(idx).copied().flatten()
}

fn bg_brightness_choice_index(brightness: f32) -> usize {
    ((brightness.clamp(0.0, 1.0) * 10.0).round() as i32).clamp(0, 10) as usize
}

fn bg_brightness_from_choice(idx: usize) -> f32 {
    idx.min(10) as f32 / 10.0
}

fn music_wheel_scroll_speed_choice_index(speed: u8) -> usize {
    let mut best_idx = 0usize;
    let mut best_diff = u8::MAX;
    for (idx, value) in MUSIC_WHEEL_SCROLL_SPEED_VALUES.iter().enumerate() {
        let diff = speed.abs_diff(*value);
        if diff < best_diff {
            best_diff = diff;
            best_idx = idx;
        }
    }
    best_idx
}

fn music_wheel_scroll_speed_from_choice(idx: usize) -> u8 {
    MUSIC_WHEEL_SCROLL_SPEED_VALUES
        .get(idx)
        .copied()
        .unwrap_or(15)
}

const fn breakdown_style_choice_index(style: BreakdownStyle) -> usize {
    match style {
        BreakdownStyle::Sl => 0,
        BreakdownStyle::Sn => 1,
    }
}

const fn breakdown_style_from_choice(idx: usize) -> BreakdownStyle {
    match idx {
        1 => BreakdownStyle::Sn,
        _ => BreakdownStyle::Sl,
    }
}

const fn default_fail_type_choice_index(fail_type: DefaultFailType) -> usize {
    match fail_type {
        DefaultFailType::Immediate => 0,
        DefaultFailType::ImmediateContinue => 1,
    }
}

const fn default_fail_type_from_choice(idx: usize) -> DefaultFailType {
    match idx {
        0 => DefaultFailType::Immediate,
        _ => DefaultFailType::ImmediateContinue,
    }
}

fn banner_color_depth_choice_index(depth: u8) -> usize {
    ADVANCED_BANNER_COLOR_DEPTH_VALUES
        .iter()
        .enumerate()
        .min_by_key(|(_, value)| value.abs_diff(depth))
        .map_or(1, |(idx, _)| idx)
}

fn banner_color_depth_from_choice(idx: usize) -> u8 {
    ADVANCED_BANNER_COLOR_DEPTH_VALUES
        .get(idx)
        .copied()
        .unwrap_or(16)
}

fn banner_min_dimension_choice_index(min_dimension: u16) -> usize {
    ADVANCED_BANNER_MIN_DIMENSION_VALUES
        .iter()
        .enumerate()
        .min_by_key(|(_, value)| value.abs_diff(min_dimension))
        .map_or(1, |(idx, _)| idx)
}

fn banner_min_dimension_from_choice(idx: usize) -> u16 {
    ADVANCED_BANNER_MIN_DIMENSION_VALUES
        .get(idx)
        .copied()
        .unwrap_or(32)
}

fn banner_scale_divisor_choice_index(scale_divisor: u8) -> usize {
    ADVANCED_BANNER_SCALE_DIVISOR_VALUES
        .iter()
        .position(|value| *value == scale_divisor)
        .unwrap_or(1)
}

fn banner_scale_divisor_from_choice(idx: usize) -> u8 {
    ADVANCED_BANNER_SCALE_DIVISOR_VALUES
        .get(idx)
        .copied()
        .unwrap_or(2)
}

const fn translated_titles_choice_index(translated_titles: bool) -> usize {
    if translated_titles { 0 } else { 1 }
}

const fn translated_titles_from_choice(idx: usize) -> bool {
    idx == 0
}

const fn select_music_pattern_info_choice_index(mode: SelectMusicPatternInfoMode) -> usize {
    match mode {
        SelectMusicPatternInfoMode::Auto => 0,
        SelectMusicPatternInfoMode::Tech => 1,
        SelectMusicPatternInfoMode::Stamina => 2,
    }
}

const fn select_music_pattern_info_from_choice(idx: usize) -> SelectMusicPatternInfoMode {
    match idx {
        1 => SelectMusicPatternInfoMode::Tech,
        2 => SelectMusicPatternInfoMode::Stamina,
        _ => SelectMusicPatternInfoMode::Auto,
    }
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
    pending_submenu_parent_kind: Option<SubmenuKind>,
    submenu_parent_kind: Option<SubmenuKind>,
    submenu_fade_t: f32,
    content_alpha: f32,
    reload_ui: Option<ReloadUiState>,
    score_import_ui: Option<ScoreImportUiState>,
    score_import_confirm: Option<ScoreImportConfirmState>,
    // Submenu state
    sub_selected: usize,
    sub_prev_selected: usize,
    sub_inline_x: f32,
    sub_choice_indices_system: Vec<usize>,
    sub_choice_indices_graphics: Vec<usize>,
    sub_choice_indices_input: Vec<usize>,
    sub_choice_indices_input_backend: Vec<usize>,
    sub_choice_indices_machine: Vec<usize>,
    sub_choice_indices_advanced: Vec<usize>,
    sub_choice_indices_gameplay: Vec<usize>,
    sub_choice_indices_sound: Vec<usize>,
    sub_choice_indices_select_music: Vec<usize>,
    sub_choice_indices_groovestats: Vec<usize>,
    sub_choice_indices_score_import: Vec<usize>,
    system_noteskin_choices: Vec<String>,
    sub_cursor_indices_system: Vec<usize>,
    sub_cursor_indices_graphics: Vec<usize>,
    sub_cursor_indices_input: Vec<usize>,
    sub_cursor_indices_input_backend: Vec<usize>,
    sub_cursor_indices_machine: Vec<usize>,
    sub_cursor_indices_advanced: Vec<usize>,
    sub_cursor_indices_gameplay: Vec<usize>,
    sub_cursor_indices_sound: Vec<usize>,
    sub_cursor_indices_select_music: Vec<usize>,
    sub_cursor_indices_groovestats: Vec<usize>,
    sub_cursor_indices_score_import: Vec<usize>,
    score_import_profiles: Vec<ScoreImportProfileConfig>,
    score_import_profile_choices: Vec<String>,
    score_import_profile_ids: Vec<Option<String>>,
    score_import_pack_choices: Vec<String>,
    score_import_pack_filters: Vec<Option<String>>,
    master_volume_pct: i32,
    sfx_volume_pct: i32,
    music_volume_pct: i32,
    global_offset_ms: i32,
    visual_delay_ms: i32,
    video_renderer_at_load: BackendType,
    display_mode_at_load: DisplayMode,
    display_monitor_at_load: usize,
    display_width_at_load: u32,
    display_height_at_load: u32,
    display_mode_choices: Vec<String>,
    software_thread_choices: Vec<u8>,
    software_thread_labels: Vec<String>,
    resolution_choices: Vec<(u32, u32)>,
    refresh_rate_choices: Vec<u32>, // New: stored in millihertz
    // Hardware info
    pub monitor_specs: Vec<MonitorSpec>,
    // Cursor ring tween (StopTweening/BeginTweening parity with ITGmania ScreenOptions::TweenCursor).
    cursor_initialized: bool,
    cursor_from_x: f32,
    cursor_from_y: f32,
    cursor_from_w: f32,
    cursor_from_h: f32,
    cursor_to_x: f32,
    cursor_to_y: f32,
    cursor_to_w: f32,
    cursor_to_h: f32,
    cursor_t: f32,
    // Shared row tween state for the active view (main list or submenu list).
    row_tweens: Vec<RowTween>,
    graphics_prev_visible_rows: Vec<usize>,
    advanced_prev_visible_rows: Vec<usize>,
    select_music_prev_visible_rows: Vec<usize>,
}

pub fn init() -> State {
    let cfg = config::get();
    let system_noteskin_choices = discover_system_noteskin_choices();
    let software_thread_choices = build_software_thread_choices();
    let software_thread_labels = software_thread_choice_labels(&software_thread_choices);
    let machine_noteskin = profile::machine_default_noteskin();
    let machine_noteskin_idx = system_noteskin_choices
        .iter()
        .position(|name| name.eq_ignore_ascii_case(machine_noteskin.as_str()))
        .unwrap_or(0);
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
        pending_submenu_parent_kind: None,
        submenu_parent_kind: None,
        submenu_fade_t: 0.0,
        content_alpha: 1.0,
        reload_ui: None,
        score_import_ui: None,
        score_import_confirm: None,
        view: OptionsView::Main,
        sub_selected: 0,
        sub_prev_selected: 0,
        sub_inline_x: f32::NAN,
        sub_choice_indices_system: vec![0; SYSTEM_OPTIONS_ROWS.len()],
        sub_choice_indices_graphics: vec![0; GRAPHICS_OPTIONS_ROWS.len()],
        sub_choice_indices_input: vec![0; INPUT_OPTIONS_ROWS.len()],
        sub_choice_indices_input_backend: vec![0; INPUT_BACKEND_OPTIONS_ROWS.len()],
        sub_choice_indices_machine: vec![0; MACHINE_OPTIONS_ROWS.len()],
        sub_choice_indices_advanced: vec![0; ADVANCED_OPTIONS_ROWS.len()],
        sub_choice_indices_gameplay: vec![0; GAMEPLAY_OPTIONS_ROWS.len()],
        sub_choice_indices_sound: vec![0; SOUND_OPTIONS_ROWS.len()],
        sub_choice_indices_select_music: vec![0; SELECT_MUSIC_OPTIONS_ROWS.len()],
        sub_choice_indices_groovestats: vec![0; GROOVESTATS_OPTIONS_ROWS.len()],
        sub_choice_indices_score_import: vec![0; SCORE_IMPORT_OPTIONS_ROWS.len()],
        system_noteskin_choices,
        sub_cursor_indices_system: vec![0; SYSTEM_OPTIONS_ROWS.len()],
        sub_cursor_indices_graphics: vec![0; GRAPHICS_OPTIONS_ROWS.len()],
        sub_cursor_indices_input: vec![0; INPUT_OPTIONS_ROWS.len()],
        sub_cursor_indices_input_backend: vec![0; INPUT_BACKEND_OPTIONS_ROWS.len()],
        sub_cursor_indices_machine: vec![0; MACHINE_OPTIONS_ROWS.len()],
        sub_cursor_indices_advanced: vec![0; ADVANCED_OPTIONS_ROWS.len()],
        sub_cursor_indices_gameplay: vec![0; GAMEPLAY_OPTIONS_ROWS.len()],
        sub_cursor_indices_sound: vec![0; SOUND_OPTIONS_ROWS.len()],
        sub_cursor_indices_select_music: vec![0; SELECT_MUSIC_OPTIONS_ROWS.len()],
        sub_cursor_indices_groovestats: vec![0; GROOVESTATS_OPTIONS_ROWS.len()],
        sub_cursor_indices_score_import: vec![0; SCORE_IMPORT_OPTIONS_ROWS.len()],
        score_import_profiles: Vec::new(),
        score_import_profile_choices: vec!["No eligible profiles".to_string()],
        score_import_profile_ids: vec![None],
        score_import_pack_choices: vec![SCORE_IMPORT_ALL_PACKS.to_string()],
        score_import_pack_filters: vec![None],
        master_volume_pct: i32::from(cfg.master_volume.clamp(0, 100)),
        sfx_volume_pct: i32::from(cfg.sfx_volume.clamp(0, 100)),
        music_volume_pct: i32::from(cfg.music_volume.clamp(0, 100)),
        global_offset_ms: {
            let ms = (cfg.global_offset_seconds * 1000.0).round() as i32;
            ms.clamp(GLOBAL_OFFSET_MIN_MS, GLOBAL_OFFSET_MAX_MS)
        },
        visual_delay_ms: {
            let ms = (cfg.visual_delay_seconds * 1000.0).round() as i32;
            ms.clamp(VISUAL_DELAY_MIN_MS, VISUAL_DELAY_MAX_MS)
        },
        video_renderer_at_load: cfg.video_renderer,
        display_mode_at_load: cfg.display_mode(),
        display_monitor_at_load: cfg.display_monitor,
        display_width_at_load: cfg.display_width,
        display_height_at_load: cfg.display_height,
        display_mode_choices: build_display_mode_choices(&[]),
        software_thread_choices,
        software_thread_labels,
        resolution_choices: Vec::new(),
        refresh_rate_choices: Vec::new(),
        monitor_specs: Vec::new(),
        cursor_initialized: false,
        cursor_from_x: 0.0,
        cursor_from_y: 0.0,
        cursor_from_w: 0.0,
        cursor_from_h: 0.0,
        cursor_to_x: 0.0,
        cursor_to_y: 0.0,
        cursor_to_w: 0.0,
        cursor_to_h: 0.0,
        cursor_t: 1.0,
        row_tweens: Vec::new(),
        graphics_prev_visible_rows: Vec::new(),
        advanced_prev_visible_rows: Vec::new(),
        select_music_prev_visible_rows: Vec::new(),
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

    set_choice_by_label(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        "Game",
        0,
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        "Theme",
        0,
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        "Language",
        0,
    );
    if let Some(noteskin_row_idx) = SYSTEM_OPTIONS_ROWS
        .iter()
        .position(|row| row.label == "Default NoteSkin")
        && let Some(slot) = state.sub_choice_indices_system.get_mut(noteskin_row_idx)
    {
        *slot = machine_noteskin_idx;
    }

    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        "Wait for VSync",
        usize::from(cfg.vsync),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        "Show Stats",
        cfg.show_stats_mode.min(2) as usize,
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        GRAPHICS_ROW_VALIDATION_LAYERS,
        usize::from(cfg.gfx_debug),
    );
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(SOFTWARE_THREADS_ROW_INDEX)
    {
        *slot = software_thread_choice_index(
            &state.software_thread_choices,
            cfg.software_renderer_threads,
        );
    }
    #[cfg(target_os = "windows")]
    set_choice_by_label(
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        INPUT_ROW_BACKEND,
        windows_backend_choice_index(cfg.windows_gamepad_backend),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_SELECT_PROFILE,
        usize::from(cfg.machine_show_select_profile),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_SELECT_COLOR,
        usize::from(cfg.machine_show_select_color),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_SELECT_STYLE,
        usize::from(cfg.machine_show_select_style),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_SELECT_PLAY_MODE,
        usize::from(cfg.machine_show_select_play_mode),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_EVAL_SUMMARY,
        usize::from(cfg.machine_show_eval_summary),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_NAME_ENTRY,
        usize::from(cfg.machine_show_name_entry),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_GAMEOVER,
        usize::from(cfg.machine_show_gameover),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_MENU_MUSIC,
        usize::from(cfg.menu_music),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_KEYBOARD_FEATURES,
        usize::from(cfg.keyboard_features),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_DEFAULT_FAIL_TYPE,
        default_fail_type_choice_index(cfg.default_fail_type),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_BANNER_CACHE,
        usize::from(cfg.banner_cache),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_BANNER_COLOR_DEPTH,
        banner_color_depth_choice_index(cfg.banner_cache_color_depth),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_BANNER_MIN_DIMENSION,
        banner_min_dimension_choice_index(cfg.banner_cache_min_dimension),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_BANNER_POW2,
        usize::from(cfg.banner_cache_pow2),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_BANNER_SCALE_DIVISOR,
        banner_scale_divisor_choice_index(cfg.banner_cache_scale_divisor),
    );
    if let Some(slot) = state
        .sub_choice_indices_advanced
        .get_mut(ADVANCED_SONG_PARSING_THREADS_ROW_INDEX)
    {
        *slot = software_thread_choice_index(&state.software_thread_choices, cfg.song_parsing_threads);
    }
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_CACHE_SONGS,
        usize::from(cfg.cachesongs),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_FAST_LOAD,
        usize::from(cfg.fastload),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        GAMEPLAY_ROW_BG_BRIGHTNESS,
        bg_brightness_choice_index(cfg.bg_brightness),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        GAMEPLAY_ROW_CENTERED_P1,
        usize::from(cfg.center_1player_notefield),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        GAMEPLAY_ROW_ZMOD_RATING_BOX,
        usize::from(cfg.zmod_rating_box_text),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        GAMEPLAY_ROW_BPM_DECIMAL,
        usize::from(cfg.show_bpm_decimal),
    );

    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        "Master Volume",
        master_volume_choice_index(cfg.master_volume),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        "SFX Volume",
        master_volume_choice_index(cfg.sfx_volume),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        "Music Volume",
        master_volume_choice_index(cfg.music_volume),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        "Audio Sample Rate",
        sample_rate_choice_index(cfg.audio_sample_rate_hz),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        "Mine Sounds",
        usize::from(cfg.mine_hit_sound),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        "RateMod Preserves Pitch",
        usize::from(cfg.rate_mod_preserves_pitch),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_BANNERS,
        usize::from(cfg.show_select_music_banners),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_BREAKDOWN,
        usize::from(cfg.show_select_music_breakdown),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_BREAKDOWN_STYLE,
        breakdown_style_choice_index(cfg.select_music_breakdown_style),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_NATIVE_LANGUAGE,
        translated_titles_choice_index(cfg.translated_titles),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_WHEEL_SPEED,
        music_wheel_scroll_speed_choice_index(cfg.music_wheel_switch_speed),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_CDTITLES,
        usize::from(cfg.show_select_music_cdtitles),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_WHEEL_GRADES,
        usize::from(cfg.show_music_wheel_grades),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_WHEEL_LAMPS,
        usize::from(cfg.show_music_wheel_lamps),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_PATTERN_INFO,
        select_music_pattern_info_choice_index(cfg.select_music_pattern_info_mode),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_PREVIEWS,
        usize::from(cfg.show_select_music_previews),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_PREVIEW_LOOP,
        usize::from(cfg.select_music_preview_loop),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_GAMEPLAY_TIMER,
        usize::from(cfg.show_select_music_gameplay_timer),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_RIVALS,
        usize::from(cfg.show_select_music_scorebox),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        GS_ROW_ENABLE,
        usize::from(cfg.enable_groovestats),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        GS_ROW_AUTO_POPULATE,
        usize::from(cfg.auto_populate_gs_scores),
    );
    refresh_score_import_options(&mut state);
    sync_submenu_cursor_indices(&mut state);
    state
}

fn submenu_choice_indices(state: &State, kind: SubmenuKind) -> &[usize] {
    match kind {
        SubmenuKind::System => &state.sub_choice_indices_system,
        SubmenuKind::Graphics => &state.sub_choice_indices_graphics,
        SubmenuKind::Input => &state.sub_choice_indices_input,
        SubmenuKind::InputBackend => &state.sub_choice_indices_input_backend,
        SubmenuKind::Machine => &state.sub_choice_indices_machine,
        SubmenuKind::Advanced => &state.sub_choice_indices_advanced,
        SubmenuKind::Gameplay => &state.sub_choice_indices_gameplay,
        SubmenuKind::Sound => &state.sub_choice_indices_sound,
        SubmenuKind::SelectMusic => &state.sub_choice_indices_select_music,
        SubmenuKind::GrooveStats => &state.sub_choice_indices_groovestats,
        SubmenuKind::ScoreImport => &state.sub_choice_indices_score_import,
    }
}

const fn submenu_choice_indices_mut(state: &mut State, kind: SubmenuKind) -> &mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_choice_indices_system,
        SubmenuKind::Graphics => &mut state.sub_choice_indices_graphics,
        SubmenuKind::Input => &mut state.sub_choice_indices_input,
        SubmenuKind::InputBackend => &mut state.sub_choice_indices_input_backend,
        SubmenuKind::Machine => &mut state.sub_choice_indices_machine,
        SubmenuKind::Advanced => &mut state.sub_choice_indices_advanced,
        SubmenuKind::Gameplay => &mut state.sub_choice_indices_gameplay,
        SubmenuKind::Sound => &mut state.sub_choice_indices_sound,
        SubmenuKind::SelectMusic => &mut state.sub_choice_indices_select_music,
        SubmenuKind::GrooveStats => &mut state.sub_choice_indices_groovestats,
        SubmenuKind::ScoreImport => &mut state.sub_choice_indices_score_import,
    }
}

fn submenu_cursor_indices(state: &State, kind: SubmenuKind) -> &[usize] {
    match kind {
        SubmenuKind::System => &state.sub_cursor_indices_system,
        SubmenuKind::Graphics => &state.sub_cursor_indices_graphics,
        SubmenuKind::Input => &state.sub_cursor_indices_input,
        SubmenuKind::InputBackend => &state.sub_cursor_indices_input_backend,
        SubmenuKind::Machine => &state.sub_cursor_indices_machine,
        SubmenuKind::Advanced => &state.sub_cursor_indices_advanced,
        SubmenuKind::Gameplay => &state.sub_cursor_indices_gameplay,
        SubmenuKind::Sound => &state.sub_cursor_indices_sound,
        SubmenuKind::SelectMusic => &state.sub_cursor_indices_select_music,
        SubmenuKind::GrooveStats => &state.sub_cursor_indices_groovestats,
        SubmenuKind::ScoreImport => &state.sub_cursor_indices_score_import,
    }
}

const fn submenu_cursor_indices_mut(state: &mut State, kind: SubmenuKind) -> &mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_cursor_indices_system,
        SubmenuKind::Graphics => &mut state.sub_cursor_indices_graphics,
        SubmenuKind::Input => &mut state.sub_cursor_indices_input,
        SubmenuKind::InputBackend => &mut state.sub_cursor_indices_input_backend,
        SubmenuKind::Machine => &mut state.sub_cursor_indices_machine,
        SubmenuKind::Advanced => &mut state.sub_cursor_indices_advanced,
        SubmenuKind::Gameplay => &mut state.sub_cursor_indices_gameplay,
        SubmenuKind::Sound => &mut state.sub_cursor_indices_sound,
        SubmenuKind::SelectMusic => &mut state.sub_cursor_indices_select_music,
        SubmenuKind::GrooveStats => &mut state.sub_cursor_indices_groovestats,
        SubmenuKind::ScoreImport => &mut state.sub_cursor_indices_score_import,
    }
}

fn sync_submenu_cursor_indices(state: &mut State) {
    state.sub_cursor_indices_system = state.sub_choice_indices_system.clone();
    state.sub_cursor_indices_graphics = state.sub_choice_indices_graphics.clone();
    state.sub_cursor_indices_input = state.sub_choice_indices_input.clone();
    state.sub_cursor_indices_input_backend = state.sub_choice_indices_input_backend.clone();
    state.sub_cursor_indices_machine = state.sub_choice_indices_machine.clone();
    state.sub_cursor_indices_advanced = state.sub_choice_indices_advanced.clone();
    state.sub_cursor_indices_gameplay = state.sub_choice_indices_gameplay.clone();
    state.sub_cursor_indices_sound = state.sub_choice_indices_sound.clone();
    state.sub_cursor_indices_select_music = state.sub_choice_indices_select_music.clone();
    state.sub_cursor_indices_groovestats = state.sub_choice_indices_groovestats.clone();
    state.sub_cursor_indices_score_import = state.sub_choice_indices_score_import.clone();
}

pub fn sync_video_renderer(state: &mut State, renderer: BackendType) {
    state.video_renderer_at_load = renderer;
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(VIDEO_RENDERER_ROW_INDEX)
    {
        *slot = backend_to_renderer_choice_index(renderer);
    }
    sync_submenu_cursor_indices(state);
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
    sync_submenu_cursor_indices(state);
}

pub fn sync_display_resolution(state: &mut State, width: u32, height: u32) {
    rebuild_resolution_choices(state, width, height);
    state.display_width_at_load = width;
    state.display_height_at_load = height;
    sync_submenu_cursor_indices(state);
}

pub fn sync_show_stats_mode(state: &mut State, mode: u8) {
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        "Show Stats",
        mode.min(2) as usize,
    );
    sync_submenu_cursor_indices(state);
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

fn clear_navigation_holds(state: &mut State) {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.nav_key_last_scrolled_at = None;
    state.nav_lr_held_direction = None;
    state.nav_lr_held_since = None;
    state.nav_lr_last_adjusted_at = None;
}

fn start_reload_songs_and_courses(state: &mut State) {
    if state.reload_ui.is_some() {
        return;
    }

    // Clear navigation holds so the menu can't "run away" after reload finishes.
    clear_navigation_holds(state);

    let (tx, rx) = std::sync::mpsc::channel::<ReloadMsg>();
    state.reload_ui = Some(ReloadUiState::new(rx));

    std::thread::spawn(move || {
        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Songs));

        let interval = Duration::from_millis(50);
        let mut last_sent = Instant::now() - interval;
        let mut on_song = |pack: &str, song: &str| {
            let now = Instant::now();
            if now.duration_since(last_sent) < interval {
                return;
            }
            last_sent = now;
            let _ = tx.send(ReloadMsg::Song {
                pack: pack.to_owned(),
                song: song.to_owned(),
            });
        };
        song_loading::scan_and_load_songs_with_progress("songs", &mut on_song);

        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Courses));

        let mut last_sent = Instant::now() - interval;
        let mut on_course = |group: &str, course: &str| {
            let now = Instant::now();
            if now.duration_since(last_sent) < interval {
                return;
            }
            last_sent = now;
            let _ = tx.send(ReloadMsg::Course {
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        song_loading::scan_and_load_courses_with_progress("courses", "songs", &mut on_course);

        let _ = tx.send(ReloadMsg::Done);
    });
}

fn begin_score_import(state: &mut State, selection: ScoreImportSelection) {
    if state.score_import_ui.is_some() {
        return;
    }
    clear_navigation_holds(state);
    let mut profile_cfg = profile::Profile::default();
    profile_cfg.display_name = selection.profile.display_name.clone();
    profile_cfg.groovestats_api_key = selection.profile.gs_api_key.clone();
    profile_cfg.groovestats_username = selection.profile.gs_username.clone();
    profile_cfg.arrowcloud_api_key = selection.profile.ac_api_key.clone();

    let endpoint = selection.endpoint;
    let profile_id = selection.profile.id.clone();
    let profile_name = if selection.profile.display_name.is_empty() {
        selection.profile.id.clone()
    } else {
        selection.profile.display_name.clone()
    };
    let pack_group = selection.pack_group.clone();
    let pack_label = selection.pack_label.clone();
    let only_missing_gs_scores = selection.only_missing_gs_scores;

    log::warn!(
        "{} score import starting for '{}' (pack: {}, only_missing_gs={}). Hard-limited to 3 requests/sec. For many charts this can take more than one hour.",
        endpoint.display_name(),
        profile_name,
        pack_label,
        if only_missing_gs_scores { "yes" } else { "no" }
    );

    let cancel_requested = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = Arc::clone(&cancel_requested);
    let (tx, rx) = std::sync::mpsc::channel::<ScoreImportMsg>();
    state.score_import_ui = Some(ScoreImportUiState::new(
        endpoint,
        profile_name.clone(),
        pack_label,
        cancel_requested,
        rx,
    ));

    std::thread::spawn(move || {
        let result = scores::import_scores_for_profile(
            endpoint,
            profile_id,
            profile_cfg,
            pack_group,
            only_missing_gs_scores,
            |progress| {
                let _ = tx.send(ScoreImportMsg::Progress(progress));
            },
            || cancel_for_thread.load(Ordering::Relaxed),
        );
        let done_msg = result.map_err(|e| e.to_string());
        let _ = tx.send(ScoreImportMsg::Done(done_msg));
    });
}

fn begin_score_import_from_confirm(state: &mut State) {
    let Some(confirm) = state.score_import_confirm.take() else {
        return;
    };
    begin_score_import(state, confirm.selection);
}

fn poll_reload_ui(reload: &mut ReloadUiState) {
    while let Ok(msg) = reload.rx.try_recv() {
        match msg {
            ReloadMsg::Phase(phase) => {
                reload.phase = phase;
                reload.line2.clear();
                reload.line3.clear();
            }
            ReloadMsg::Song { pack, song } => {
                reload.phase = ReloadPhase::Songs;
                reload.line2 = pack;
                reload.line3 = song;
            }
            ReloadMsg::Course { group, course } => {
                reload.phase = ReloadPhase::Courses;
                reload.line2 = group;
                reload.line3 = course;
            }
            ReloadMsg::Done => {
                reload.done = true;
            }
        }
    }
}

fn poll_score_import_ui(score_import: &mut ScoreImportUiState) {
    while let Ok(msg) = score_import.rx.try_recv() {
        match msg {
            ScoreImportMsg::Progress(progress) => {
                score_import.total_charts = progress.total_charts;
                score_import.processed_charts = progress.processed_charts;
                score_import.imported_scores = progress.imported_scores;
                score_import.missing_scores = progress.missing_scores;
                score_import.failed_requests = progress.failed_requests;
                score_import.detail_line = progress.detail;
            }
            ScoreImportMsg::Done(result) => {
                score_import.done = true;
                score_import.done_since = Some(Instant::now());
                score_import.done_message = match result {
                    Ok(summary) => {
                        if summary.canceled {
                            format!(
                                "Canceled: requested={}, imported={}, missing={}, failed={} (elapsed {:.1}s)",
                                summary.requested_charts,
                                summary.imported_scores,
                                summary.missing_scores,
                                summary.failed_requests,
                                summary.elapsed_seconds
                            )
                        } else {
                            format!(
                                "Complete: requested={}, imported={}, missing={}, failed={}, rate={} req/s (elapsed {:.1}s)",
                                summary.requested_charts,
                                summary.imported_scores,
                                summary.missing_scores,
                                summary.failed_requests,
                                summary.rate_limit_per_second,
                                summary.elapsed_seconds
                            )
                        }
                    }
                    Err(e) => format!("Import failed: {e}"),
                };
            }
        }
    }
}

pub fn update(state: &mut State, dt: f32, asset_manager: &AssetManager) -> Option<ScreenAction> {
    if state.reload_ui.is_some() {
        let done = {
            let reload = state.reload_ui.as_mut().unwrap();
            poll_reload_ui(reload);
            reload.done
        };
        if done {
            state.reload_ui = None;
            refresh_score_import_pack_options(state);
        }
        return None;
    }
    if let Some(score_import) = state.score_import_ui.as_mut() {
        poll_score_import_ui(score_import);
        if score_import.done
            && score_import
                .done_since
                .is_some_and(|at| at.elapsed().as_secs_f32() >= SCORE_IMPORT_DONE_OVERLAY_SECONDS)
        {
            state.score_import_ui = None;
        }
        return None;
    }

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
                state.submenu_parent_kind = state.pending_submenu_parent_kind.take();
                state.sub_selected = 0;
                state.sub_prev_selected = 0;
                state.sub_inline_x = f32::NAN;
                sync_submenu_cursor_indices(state);
                state.cursor_initialized = false;
                state.cursor_t = 1.0;
                state.row_tweens.clear();
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
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
                matches!(state.view, OptionsView::Submenu(SubmenuKind::Graphics));
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
                state.pending_submenu_parent_kind = None;
                state.submenu_parent_kind = None;
                state.cursor_initialized = false;
                state.cursor_t = 1.0;
                state.row_tweens.clear();
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
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

                if let Some(renderer) = desired_renderer
                    && renderer != state.video_renderer_at_load
                {
                    renderer_change = Some(renderer);
                }
                if let Some(display_mode) = desired_display_mode
                    && display_mode != state.display_mode_at_load
                {
                    display_mode_change = Some(display_mode);
                }
                if let Some(monitor) = desired_monitor
                    && monitor != state.display_monitor_at_load
                {
                    monitor_change = Some(monitor);
                }
                if let Some((w, h)) = desired_resolution
                    && (w != state.display_width_at_load || h != state.display_height_at_load)
                {
                    resolution_change = Some((w, h));
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

    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
        state.nav_key_held_direction,
        state.nav_key_held_since,
        state.nav_key_last_scrolled_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL
        {
            match state.view {
                OptionsView::Main => {
                    let total = ITEMS.len();
                    if total > 0 {
                        match direction {
                            NavDirection::Up => {
                                state.selected = if state.selected == 0 {
                                    total - 1
                                } else {
                                    state.selected - 1
                                };
                            }
                            NavDirection::Down => {
                                state.selected = (state.selected + 1) % total;
                            }
                        }
                        state.nav_key_last_scrolled_at = Some(now);
                    }
                }
                OptionsView::Submenu(kind) => {
                    move_submenu_selection_vertical(state, asset_manager, kind, direction);
                    state.nav_key_last_scrolled_at = Some(now);
                }
            }
        }
    }

    if let (Some(delta_lr), Some(held_since), Some(last_adjusted)) = (
        state.nav_lr_held_direction,
        state.nav_lr_held_since,
        state.nav_lr_last_adjusted_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_adjusted) >= NAV_REPEAT_SCROLL_INTERVAL
            && matches!(state.view, OptionsView::Submenu(_))
        {
            if pending_action.is_none() {
                pending_action = apply_submenu_choice_delta(state, asset_manager, delta_lr);
            } else {
                apply_submenu_choice_delta(state, asset_manager, delta_lr);
            }
            state.nav_lr_last_adjusted_at = Some(now);
        }
    }

    match state.view {
        OptionsView::Main => {
            if state.selected != state.prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");
                state.prev_selected = state.selected;
            }
        }
        OptionsView::Submenu(_) => {
            if state.sub_selected != state.sub_prev_selected {
                audio::play_sfx("assets/sounds/change.ogg");
                state.sub_prev_selected = state.sub_selected;
            }
        }
    }

    let (s, list_x, list_y) = scaled_block_origin_with_margins();
    match state.view {
        OptionsView::Main => {
            update_row_tweens(
                &mut state.row_tweens,
                ITEMS.len(),
                state.selected,
                s,
                list_y,
                dt,
            );
            state.cursor_initialized = false;
            state.graphics_prev_visible_rows.clear();
            state.advanced_prev_visible_rows.clear();
            state.select_music_prev_visible_rows.clear();
        }
        OptionsView::Submenu(kind) => {
            if matches!(kind, SubmenuKind::Graphics) {
                update_graphics_row_tweens(state, s, list_y, dt);
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
            } else if matches!(kind, SubmenuKind::Advanced) {
                update_advanced_row_tweens(state, s, list_y, dt);
                state.graphics_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
            } else if matches!(kind, SubmenuKind::SelectMusic) {
                update_select_music_row_tweens(state, s, list_y, dt);
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
            } else {
                let total_rows = submenu_total_rows(state, kind);
                update_row_tweens(
                    &mut state.row_tweens,
                    total_rows,
                    state.sub_selected,
                    s,
                    list_y,
                    dt,
                );
                state.graphics_prev_visible_rows.clear();
                state.advanced_prev_visible_rows.clear();
                state.select_music_prev_visible_rows.clear();
            }
            let list_w = LIST_W * s;
            if let Some((to_x, to_y, to_w, to_h)) =
                submenu_cursor_dest(state, asset_manager, kind, s, list_x, list_y, list_w)
            {
                if !state.cursor_initialized {
                    state.cursor_initialized = true;
                    state.cursor_from_x = to_x;
                    state.cursor_from_y = to_y;
                    state.cursor_from_w = to_w;
                    state.cursor_from_h = to_h;
                    state.cursor_to_x = to_x;
                    state.cursor_to_y = to_y;
                    state.cursor_to_w = to_w;
                    state.cursor_to_h = to_h;
                    state.cursor_t = 1.0;
                } else {
                    let dx = (to_x - state.cursor_to_x).abs();
                    let dy = (to_y - state.cursor_to_y).abs();
                    let dw = (to_w - state.cursor_to_w).abs();
                    let dh = (to_h - state.cursor_to_h).abs();
                    if dx > 0.01 || dy > 0.01 || dw > 0.01 || dh > 0.01 {
                        let t = state.cursor_t.clamp(0.0, 1.0);
                        let cur_x = (state.cursor_to_x - state.cursor_from_x)
                            .mul_add(t, state.cursor_from_x);
                        let cur_y = (state.cursor_to_y - state.cursor_from_y)
                            .mul_add(t, state.cursor_from_y);
                        let cur_w = (state.cursor_to_w - state.cursor_from_w)
                            .mul_add(t, state.cursor_from_w);
                        let cur_h = (state.cursor_to_h - state.cursor_from_h)
                            .mul_add(t, state.cursor_from_h);
                        state.cursor_from_x = cur_x;
                        state.cursor_from_y = cur_y;
                        state.cursor_from_w = cur_w;
                        state.cursor_from_h = cur_h;
                        state.cursor_to_x = to_x;
                        state.cursor_to_y = to_y;
                        state.cursor_to_w = to_w;
                        state.cursor_to_h = to_h;
                        state.cursor_t = 0.0;
                    }
                }
            } else {
                state.cursor_initialized = false;
            }
        }
    }

    if state.cursor_t < 1.0 {
        if CURSOR_TWEEN_SECONDS > 0.0 {
            state.cursor_t = (state.cursor_t + dt / CURSOR_TWEEN_SECONDS).min(1.0);
        } else {
            state.cursor_t = 1.0;
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

fn apply_submenu_choice_delta(
    state: &mut State,
    asset_manager: &AssetManager,
    delta: isize,
) -> Option<ScreenAction> {
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return None;
    }
    let kind = match state.view {
        OptionsView::Submenu(k) => k,
        _ => return None,
    };
    let rows = submenu_rows(kind);
    if rows.is_empty() {
        return None;
    }
    let Some(row_index) = submenu_visible_row_to_actual(state, kind, state.sub_selected) else {
        // Exit row – no choices to change.
        return None;
    };

    if let Some(row) = rows.get(row_index) {
        if matches!(kind, SubmenuKind::Sound) {
            match row.label {
                "Master Volume" => {
                    if adjust_ms_value(
                        &mut state.master_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_master_volume(state.master_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                    }
                    return None;
                }
                "SFX Volume" => {
                    if adjust_ms_value(
                        &mut state.sfx_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_sfx_volume(state.sfx_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                    }
                    return None;
                }
                "Music Volume" => {
                    if adjust_ms_value(
                        &mut state.music_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_music_volume(state.music_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                    }
                    return None;
                }
                _ => {}
            }
        }
        if matches!(kind, SubmenuKind::Sound) && row.label == "Global Offset (ms)" {
            if adjust_ms_value(
                &mut state.global_offset_ms,
                delta,
                GLOBAL_OFFSET_MIN_MS,
                GLOBAL_OFFSET_MAX_MS,
            ) {
                config::update_global_offset(state.global_offset_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
            return None;
        }
        if matches!(kind, SubmenuKind::Graphics) && row.label == "Visual Delay (ms)" {
            if adjust_ms_value(
                &mut state.visual_delay_ms,
                delta,
                VISUAL_DELAY_MIN_MS,
                VISUAL_DELAY_MAX_MS,
            ) {
                config::update_visual_delay_seconds(state.visual_delay_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
            }
            return None;
        }
    }

    let choices = row_choices(state, kind, rows, row_index);
    let num_choices = choices.len();
    if num_choices == 0 {
        return None;
    }
    let mut action: Option<ScreenAction> = None;
    if row_index >= submenu_choice_indices(state, kind).len()
        || row_index >= submenu_cursor_indices(state, kind).len()
    {
        return None;
    }
    let choice_index =
        submenu_cursor_indices(state, kind)[row_index].min(num_choices.saturating_sub(1));
    let cur = choice_index as isize;
    let n = num_choices as isize;
    let mut new_index = ((cur + delta).rem_euclid(n)) as usize;
    if new_index >= num_choices {
        new_index = num_choices.saturating_sub(1);
    }
    if new_index == choice_index {
        return None;
    }
    let selected_choice = choices
        .get(new_index)
        .map(|choice| choice.as_ref().to_string());
    drop(choices);

    submenu_choice_indices_mut(state, kind)[row_index] = new_index;
    submenu_cursor_indices_mut(state, kind)[row_index] = new_index;
    if rows.get(row_index).is_some_and(|row| row.inline) {
        let centers = submenu_inline_choice_centers(state, asset_manager, kind, row_index);
        if let Some(&x) = centers.get(new_index) {
            state.sub_inline_x = x;
        }
    }
    audio::play_sfx("assets/sounds/change_value.ogg");

    if matches!(kind, SubmenuKind::System) {
        let row = &rows[row_index];
        match row.label {
            "Game" => config::update_game_flag(config::GameFlag::Dance),
            "Theme" => config::update_theme_flag(config::ThemeFlag::SimplyLove),
            "Language" => config::update_language_flag(config::LanguageFlag::English),
            "Default NoteSkin" => {
                if let Some(skin_name) = selected_choice.as_deref() {
                    profile::update_machine_default_noteskin(profile::NoteSkin::new(skin_name));
                }
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Graphics) {
        let row = &rows[row_index];
        if row.label == "Display Aspect Ratio" {
            let (cur_w, cur_h) = selected_resolution(state);
            rebuild_resolution_choices(state, cur_w, cur_h);
        }
        if row.label == "Display Resolution" {
            rebuild_refresh_rate_choices(state);
        }
        if row.label == "Display Mode" {
            let (cur_w, cur_h) = selected_resolution(state);
            rebuild_resolution_choices(state, cur_w, cur_h);
        }
        if row.label == "Wait for VSync" {
            config::update_vsync(new_index == 1);
        }
        if row.label == "Show Stats" {
            let mode = new_index.min(2) as u8;
            action = Some(ScreenAction::UpdateShowOverlay(mode));
        }
        if row.label == GRAPHICS_ROW_VALIDATION_LAYERS {
            config::update_gfx_debug(new_index == 1);
        }
        if row.label == GRAPHICS_ROW_SOFTWARE_THREADS {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_software_renderer_threads(threads);
        }
    } else if matches!(kind, SubmenuKind::InputBackend) {
        let row = &rows[row_index];
        if row.label == INPUT_ROW_BACKEND {
            #[cfg(target_os = "windows")]
            {
                config::update_windows_gamepad_backend(windows_backend_from_choice(new_index));
            }
        }
    } else if matches!(kind, SubmenuKind::Machine) {
        let row = &rows[row_index];
        let enabled = new_index == 1;
        match row.label {
            MACHINE_ROW_SELECT_PROFILE => config::update_machine_show_select_profile(enabled),
            MACHINE_ROW_SELECT_COLOR => config::update_machine_show_select_color(enabled),
            MACHINE_ROW_SELECT_STYLE => config::update_machine_show_select_style(enabled),
            MACHINE_ROW_SELECT_PLAY_MODE => config::update_machine_show_select_play_mode(enabled),
            MACHINE_ROW_EVAL_SUMMARY => config::update_machine_show_eval_summary(enabled),
            MACHINE_ROW_NAME_ENTRY => config::update_machine_show_name_entry(enabled),
            MACHINE_ROW_GAMEOVER => config::update_machine_show_gameover(enabled),
            MACHINE_ROW_MENU_MUSIC => config::update_menu_music(enabled),
            MACHINE_ROW_KEYBOARD_FEATURES => config::update_keyboard_features(enabled),
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Advanced) {
        let row = &rows[row_index];
        if row.label == ADVANCED_ROW_DEFAULT_FAIL_TYPE {
            config::update_default_fail_type(default_fail_type_from_choice(new_index));
        } else if row.label == ADVANCED_ROW_BANNER_CACHE {
            config::update_banner_cache(new_index == 1);
        } else if row.label == ADVANCED_ROW_BANNER_COLOR_DEPTH {
            config::update_banner_cache_color_depth(banner_color_depth_from_choice(new_index));
        } else if row.label == ADVANCED_ROW_BANNER_MIN_DIMENSION {
            config::update_banner_cache_min_dimension(banner_min_dimension_from_choice(new_index));
        } else if row.label == ADVANCED_ROW_BANNER_POW2 {
            config::update_banner_cache_pow2(new_index == 1);
        } else if row.label == ADVANCED_ROW_BANNER_SCALE_DIVISOR {
            config::update_banner_cache_scale_divisor(banner_scale_divisor_from_choice(new_index));
        } else if row.label == ADVANCED_ROW_SONG_PARSING_THREADS {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_song_parsing_threads(threads);
        } else if row.label == ADVANCED_ROW_CACHE_SONGS {
            config::update_cache_songs(new_index == 1);
        } else if row.label == ADVANCED_ROW_FAST_LOAD {
            config::update_fastload(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Gameplay) {
        let row = &rows[row_index];
        if row.label == GAMEPLAY_ROW_BG_BRIGHTNESS {
            config::update_bg_brightness(bg_brightness_from_choice(new_index));
        } else if row.label == GAMEPLAY_ROW_CENTERED_P1 {
            config::update_center_1player_notefield(new_index == 1);
        } else if row.label == GAMEPLAY_ROW_ZMOD_RATING_BOX {
            config::update_zmod_rating_box_text(new_index == 1);
        } else if row.label == GAMEPLAY_ROW_BPM_DECIMAL {
            config::update_show_bpm_decimal(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Sound) {
        let row = &rows[row_index];
        match row.label {
            "Master Volume" => {
                let vol = master_volume_from_choice(new_index);
                config::update_master_volume(vol);
            }
            "SFX Volume" => {
                let vol = master_volume_from_choice(new_index);
                config::update_sfx_volume(vol);
            }
            "Music Volume" => {
                let vol = master_volume_from_choice(new_index);
                config::update_music_volume(vol);
            }
            "Audio Sample Rate" => {
                let rate = sample_rate_from_choice(new_index);
                config::update_audio_sample_rate(rate);
            }
            "Mine Sounds" => {
                config::update_mine_hit_sound(new_index == 1);
            }
            "RateMod Preserves Pitch" => {
                config::update_rate_mod_preserves_pitch(new_index == 1);
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::SelectMusic) {
        let row = &rows[row_index];
        if row.label == SELECT_MUSIC_ROW_SHOW_BANNERS {
            config::update_show_select_music_banners(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_SHOW_BREAKDOWN {
            config::update_show_select_music_breakdown(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_BREAKDOWN_STYLE {
            config::update_select_music_breakdown_style(breakdown_style_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_NATIVE_LANGUAGE {
            config::update_translated_titles(translated_titles_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_WHEEL_SPEED {
            config::update_music_wheel_switch_speed(music_wheel_scroll_speed_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_CDTITLES {
            config::update_show_select_music_cdtitles(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_WHEEL_GRADES {
            config::update_show_music_wheel_grades(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_WHEEL_LAMPS {
            config::update_show_music_wheel_lamps(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_PATTERN_INFO {
            config::update_select_music_pattern_info_mode(select_music_pattern_info_from_choice(
                new_index,
            ));
        } else if row.label == SELECT_MUSIC_ROW_PREVIEWS {
            config::update_show_select_music_previews(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_PREVIEW_LOOP {
            config::update_select_music_preview_loop(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_GAMEPLAY_TIMER {
            config::update_show_select_music_gameplay_timer(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_SHOW_RIVALS {
            config::update_show_select_music_scorebox(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::GrooveStats) {
        let row = &rows[row_index];
        if row.label == GS_ROW_ENABLE {
            let enabled = new_index == 1;
            config::update_enable_groovestats(enabled);
            // Re-run connectivity logic so toggling this option applies immediately.
            crate::core::network::init();
        } else if row.label == GS_ROW_AUTO_POPULATE {
            config::update_auto_populate_gs_scores(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::ScoreImport) {
        let row = &rows[row_index];
        if row.label == SCORE_IMPORT_ROW_ENDPOINT {
            refresh_score_import_profile_options(state);
        }
    }
    action
}

pub fn handle_input(
    state: &mut State,
    asset_manager: &AssetManager,
    ev: &InputEvent,
) -> ScreenAction {
    if state.reload_ui.is_some() {
        return ScreenAction::None;
    }
    if let Some(score_import) = state.score_import_ui.as_ref() {
        if ev.pressed && matches!(ev.action, VirtualAction::p1_back) {
            score_import.cancel_requested.store(true, Ordering::Relaxed);
            clear_navigation_holds(state);
            state.score_import_ui = None;
            audio::play_sfx("assets/sounds/change.ogg");
            log::warn!("Score import cancel requested by user.");
        }
        return ScreenAction::None;
    }
    if let Some(confirm) = state.score_import_confirm.as_mut() {
        if !ev.pressed {
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start | VirtualAction::p1_select => {
                let should_start = confirm.active_choice == 0;
                audio::play_sfx("assets/sounds/start.ogg");
                if should_start {
                    clear_navigation_holds(state);
                    begin_score_import_from_confirm(state);
                } else {
                    clear_navigation_holds(state);
                    state.score_import_confirm = None;
                }
            }
            VirtualAction::p1_back => {
                clear_navigation_holds(state);
                state.score_import_confirm = None;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    // Ignore new navigation while a local submenu fade is in progress.
    if !matches!(state.submenu_transition, SubmenuTransition::None) {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back if ev.pressed => {
            match state.view {
                OptionsView::Main => return ScreenAction::Navigate(Screen::Menu),
                OptionsView::Submenu(_) => {
                    if let Some(parent_kind) = state.submenu_parent_kind {
                        state.pending_submenu_kind = Some(parent_kind);
                        state.pending_submenu_parent_kind = None;
                        state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    } else {
                        // Fade back to the main Options list.
                        state.submenu_transition = SubmenuTransition::FadeOutToMain;
                    }
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
                            state.selected = if state.selected == 0 {
                                total - 1
                            } else {
                                state.selected - 1
                            };
                        }
                    }
                    OptionsView::Submenu(kind) => {
                        move_submenu_selection_vertical(
                            state,
                            asset_manager,
                            kind,
                            NavDirection::Up,
                        );
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
                        move_submenu_selection_vertical(
                            state,
                            asset_manager,
                            kind,
                            NavDirection::Down,
                        );
                    }
                }
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if ev.pressed {
                if let Some(action) = apply_submenu_choice_delta(state, asset_manager, -1) {
                    on_lr_press(state, -1);
                    return action;
                }
                on_lr_press(state, -1);
            } else {
                on_lr_release(state, -1);
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if ev.pressed {
                if let Some(action) = apply_submenu_choice_delta(state, asset_manager, 1) {
                    on_lr_press(state, 1);
                    return action;
                }
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
                    state.pending_submenu_parent_kind = None;

                    // Route based on the selected row label.
                    match item.name {
                        // Enter System Options submenu.
                        "System Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::System);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        // Enter Graphics Options submenu.
                        "Graphics Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Graphics);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Input Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Input);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Machine Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Machine);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Advanced Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Advanced);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Gameplay Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Gameplay);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        // Enter Sound Options submenu.
                        "Sound Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Sound);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Select Music Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::SelectMusic);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "GrooveStats Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::GrooveStats);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Score Import" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            refresh_score_import_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::ScoreImport);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                        }
                        "Manage Local Profiles" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::ManageLocalProfiles);
                        }
                        "Reload Songs/Courses" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            start_reload_songs_and_courses(state);
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
                    let total = submenu_total_rows(state, kind);
                    if total == 0 {
                        return ScreenAction::None;
                    }
                    let selected_row = state.sub_selected.min(total.saturating_sub(1));
                    // Exit row in the submenu: back to the main Options list.
                    if selected_row == total - 1 {
                        audio::play_sfx("assets/sounds/start.ogg");
                        if let Some(parent_kind) = state.submenu_parent_kind {
                            state.pending_submenu_kind = Some(parent_kind);
                            state.pending_submenu_parent_kind = None;
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                        } else {
                            state.submenu_transition = SubmenuTransition::FadeOutToMain;
                        }
                        state.submenu_fade_t = 0.0;
                    } else if matches!(kind, SubmenuKind::Input) {
                        let rows = submenu_rows(kind);
                        let Some(row_idx) =
                            submenu_visible_row_to_actual(state, kind, selected_row)
                        else {
                            return ScreenAction::None;
                        };
                        if let Some(row) = rows.get(row_idx) {
                            match row.label {
                                INPUT_ROW_CONFIGURE_MAPPINGS => {
                                    audio::play_sfx("assets/sounds/start.ogg");
                                    return ScreenAction::Navigate(Screen::Mappings);
                                }
                                INPUT_ROW_TEST => {
                                    audio::play_sfx("assets/sounds/start.ogg");
                                    return ScreenAction::Navigate(Screen::Input);
                                }
                                INPUT_ROW_OPTIONS => {
                                    audio::play_sfx("assets/sounds/start.ogg");
                                    state.pending_submenu_kind = Some(SubmenuKind::InputBackend);
                                    state.pending_submenu_parent_kind = Some(SubmenuKind::Input);
                                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                                    state.submenu_fade_t = 0.0;
                                }
                                _ => {}
                            }
                        }
                    } else if matches!(kind, SubmenuKind::ScoreImport) {
                        let rows = submenu_rows(kind);
                        let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row)
                        else {
                            return ScreenAction::None;
                        };
                        if let Some(row) = rows.get(row_idx)
                            && row.label == SCORE_IMPORT_ROW_START
                        {
                            audio::play_sfx("assets/sounds/start.ogg");
                            if let Some(selection) = selected_score_import_selection(state) {
                                if selection.pack_group.is_none() {
                                    clear_navigation_holds(state);
                                    state.score_import_confirm = Some(ScoreImportConfirmState {
                                        selection,
                                        active_choice: 1,
                                    });
                                } else {
                                    begin_score_import(state, selection);
                                }
                            } else {
                                log::warn!(
                                    "Score import start requested, but no eligible profile is selected."
                                );
                            }
                        }
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
/// Returns (scale, `origin_x`, `origin_y`).
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
    let s_w = if total_w > 0.0 {
        avail_w / total_w
    } else {
        1.0
    };
    let s_h = if total_h > 0.0 {
        avail_h / total_h
    } else {
        1.0
    };
    let s = s_w.min(s_h).max(0.0);

    // X origin:
    // Right-align inside [LEFT..(sw-RIGHT)] so the description box ends exactly
    // RIGHT_MARGIN_PX from the screen edge.
    let ox = LEFT_MARGIN_PX + total_w.mul_add(-s, avail_w).max(0.0);

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

#[inline(always)]
fn row_dest_for_index(
    total_rows: usize,
    selected: usize,
    row_idx: usize,
    s: f32,
    list_y: f32,
) -> (f32, f32) {
    if total_rows == 0 {
        return (list_y, 0.0);
    }
    let offset = scroll_offset(selected.min(total_rows - 1), total_rows);
    let row_step = (ROW_H + ROW_GAP) * s;
    let first_row_mid_y = (0.5 * ROW_H).mul_add(s, list_y);
    let top_hidden_mid_y = first_row_mid_y - 0.5 * row_step;
    let bottom_hidden_mid_y = ((VISIBLE_ROWS as f32) - 0.5).mul_add(row_step, first_row_mid_y);
    if row_idx < offset {
        (top_hidden_mid_y, 0.0)
    } else if row_idx >= offset + VISIBLE_ROWS {
        (bottom_hidden_mid_y, 0.0)
    } else {
        let vis = row_idx - offset;
        ((vis as f32).mul_add(row_step, first_row_mid_y), 1.0)
    }
}

fn init_row_tweens(total_rows: usize, selected: usize, s: f32, list_y: f32) -> Vec<RowTween> {
    let mut out: Vec<RowTween> = Vec::with_capacity(total_rows);
    for row_idx in 0..total_rows {
        let (y, a) = row_dest_for_index(total_rows, selected, row_idx, s, list_y);
        out.push(RowTween {
            from_y: y,
            to_y: y,
            from_a: a,
            to_a: a,
            t: 1.0,
        });
    }
    out
}

fn update_row_tweens(
    row_tweens: &mut Vec<RowTween>,
    total_rows: usize,
    selected: usize,
    s: f32,
    list_y: f32,
    dt: f32,
) {
    if total_rows == 0 {
        row_tweens.clear();
        return;
    }
    if row_tweens.len() != total_rows {
        *row_tweens = init_row_tweens(total_rows, selected, s, list_y);
        return;
    }
    for row_idx in 0..total_rows {
        let (to_y, to_a) = row_dest_for_index(total_rows, selected, row_idx, s, list_y);
        let tw = &mut row_tweens[row_idx];
        let cur_y = tw.y();
        let cur_a = tw.a();
        if (to_y - tw.to_y).abs() > 0.01 || (to_a - tw.to_a).abs() > 0.001 {
            tw.from_y = cur_y;
            tw.to_y = to_y;
            tw.from_a = cur_a;
            tw.to_a = to_a;
            tw.t = 0.0;
        }
        if tw.t < 1.0 {
            if ROW_TWEEN_SECONDS > 0.0 {
                tw.t = (tw.t + dt / ROW_TWEEN_SECONDS).min(1.0);
            } else {
                tw.t = 1.0;
            }
        }
    }
}

fn update_graphics_row_tweens(state: &mut State, s: f32, list_y: f32, dt: f32) {
    let rows = submenu_rows(SubmenuKind::Graphics);
    let visible_rows = submenu_visible_row_indices(state, SubmenuKind::Graphics, rows);
    let total_rows = visible_rows.len() + 1;
    if total_rows == 0 {
        state.row_tweens.clear();
        state.graphics_prev_visible_rows.clear();
        return;
    }

    let selected = state.sub_selected.min(total_rows.saturating_sub(1));
    let visibility_changed = state.graphics_prev_visible_rows != visible_rows;
    if state.row_tweens.is_empty() {
        state.row_tweens = init_row_tweens(total_rows, selected, s, list_y);
    } else if state.row_tweens.len() != total_rows || visibility_changed {
        let old_tweens = std::mem::take(&mut state.row_tweens);
        let old_visible_rows = state.graphics_prev_visible_rows.clone();
        let old_total_rows = old_visible_rows.len() + 1;

        let parent_from = old_visible_rows
            .iter()
            .position(|&idx| idx == VIDEO_RENDERER_ROW_INDEX)
            .and_then(|old_idx| old_tweens.get(old_idx))
            .map(|tw| (tw.y(), tw.a()))
            .unwrap_or_else(|| {
                row_dest_for_index(total_rows, selected, VIDEO_RENDERER_ROW_INDEX, s, list_y)
            });
        let old_exit_from = old_tweens
            .get(old_total_rows.saturating_sub(1))
            .map(|tw| (tw.y(), tw.a()));

        let mut mapped: Vec<RowTween> = Vec::with_capacity(total_rows);
        for (new_idx, actual_idx) in visible_rows.iter().copied().enumerate() {
            let (to_y, to_a) = row_dest_for_index(total_rows, selected, new_idx, s, list_y);
            let (from_y, from_a) = old_visible_rows
                .iter()
                .position(|&old_actual| old_actual == actual_idx)
                .and_then(|old_idx| old_tweens.get(old_idx).map(|tw| (tw.y(), tw.a())))
                .or_else(|| {
                    if actual_idx == SOFTWARE_THREADS_ROW_INDEX {
                        Some((parent_from.0, 0.0))
                    } else {
                        None
                    }
                })
                .unwrap_or((to_y, to_a));
            let t = if (to_y - from_y).abs() <= 0.01 && (to_a - from_a).abs() <= 0.001 {
                1.0
            } else {
                0.0
            };
            mapped.push(RowTween {
                from_y,
                to_y,
                from_a,
                to_a,
                t,
            });
        }

        let exit_idx = total_rows.saturating_sub(1);
        let (to_y, to_a) = row_dest_for_index(total_rows, selected, exit_idx, s, list_y);
        let (from_y, from_a) = old_exit_from.unwrap_or((to_y, to_a));
        let t = if (to_y - from_y).abs() <= 0.01 && (to_a - from_a).abs() <= 0.001 {
            1.0
        } else {
            0.0
        };
        mapped.push(RowTween {
            from_y,
            to_y,
            from_a,
            to_a,
            t,
        });
        state.row_tweens = mapped;
    }

    state.graphics_prev_visible_rows = visible_rows;
    update_row_tweens(
        &mut state.row_tweens,
        total_rows,
        selected,
        s,
        list_y,
        dt,
    );
}

const fn advanced_parent_row(actual_idx: usize) -> Option<usize> {
    match actual_idx {
        ADVANCED_BANNER_COLOR_DEPTH_ROW_INDEX
        | ADVANCED_BANNER_MIN_DIMENSION_ROW_INDEX
        | ADVANCED_BANNER_POW2_ROW_INDEX
        | ADVANCED_BANNER_SCALE_DIVISOR_ROW_INDEX => Some(ADVANCED_BANNER_CACHE_ROW_INDEX),
        _ => None,
    }
}

fn update_advanced_row_tweens(state: &mut State, s: f32, list_y: f32, dt: f32) {
    let rows = submenu_rows(SubmenuKind::Advanced);
    let visible_rows = submenu_visible_row_indices(state, SubmenuKind::Advanced, rows);
    let total_rows = visible_rows.len() + 1;
    if total_rows == 0 {
        state.row_tweens.clear();
        state.advanced_prev_visible_rows.clear();
        return;
    }

    let selected = state.sub_selected.min(total_rows.saturating_sub(1));
    let visibility_changed = state.advanced_prev_visible_rows != visible_rows;
    if state.row_tweens.is_empty() {
        state.row_tweens = init_row_tweens(total_rows, selected, s, list_y);
    } else if state.row_tweens.len() != total_rows || visibility_changed {
        let old_tweens = std::mem::take(&mut state.row_tweens);
        let old_visible_rows = state.advanced_prev_visible_rows.clone();
        let old_total_rows = old_visible_rows.len() + 1;
        let old_exit_from = old_tweens
            .get(old_total_rows.saturating_sub(1))
            .map(|tw| (tw.y(), tw.a()));

        let mut mapped: Vec<RowTween> = Vec::with_capacity(total_rows);
        for (new_idx, actual_idx) in visible_rows.iter().copied().enumerate() {
            let (to_y, to_a) = row_dest_for_index(total_rows, selected, new_idx, s, list_y);
            let parent_from = advanced_parent_row(actual_idx).and_then(|parent_actual_idx| {
                old_visible_rows
                    .iter()
                    .position(|&idx| idx == parent_actual_idx)
                    .and_then(|old_idx| old_tweens.get(old_idx))
                    .map(|tw| (tw.y(), 0.0))
            });
            let (from_y, from_a) = old_visible_rows
                .iter()
                .position(|&old_actual| old_actual == actual_idx)
                .and_then(|old_idx| old_tweens.get(old_idx).map(|tw| (tw.y(), tw.a())))
                .or(parent_from)
                .unwrap_or((to_y, to_a));
            let t = if (to_y - from_y).abs() <= 0.01 && (to_a - from_a).abs() <= 0.001 {
                1.0
            } else {
                0.0
            };
            mapped.push(RowTween {
                from_y,
                to_y,
                from_a,
                to_a,
                t,
            });
        }

        let exit_idx = total_rows.saturating_sub(1);
        let (to_y, to_a) = row_dest_for_index(total_rows, selected, exit_idx, s, list_y);
        let (from_y, from_a) = old_exit_from.unwrap_or((to_y, to_a));
        let t = if (to_y - from_y).abs() <= 0.01 && (to_a - from_a).abs() <= 0.001 {
            1.0
        } else {
            0.0
        };
        mapped.push(RowTween {
            from_y,
            to_y,
            from_a,
            to_a,
            t,
        });
        state.row_tweens = mapped;
    }

    state.advanced_prev_visible_rows = visible_rows;
    update_row_tweens(
        &mut state.row_tweens,
        total_rows,
        selected,
        s,
        list_y,
        dt,
    );
}

const fn select_music_parent_row(actual_idx: usize) -> Option<usize> {
    match actual_idx {
        SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX => Some(SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX),
        SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX => Some(SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX),
        _ => None,
    }
}

fn update_select_music_row_tweens(state: &mut State, s: f32, list_y: f32, dt: f32) {
    let rows = submenu_rows(SubmenuKind::SelectMusic);
    let visible_rows = submenu_visible_row_indices(state, SubmenuKind::SelectMusic, rows);
    let total_rows = visible_rows.len() + 1;
    if total_rows == 0 {
        state.row_tweens.clear();
        state.select_music_prev_visible_rows.clear();
        return;
    }

    let selected = state.sub_selected.min(total_rows.saturating_sub(1));
    let visibility_changed = state.select_music_prev_visible_rows != visible_rows;
    if state.row_tweens.is_empty() {
        state.row_tweens = init_row_tweens(total_rows, selected, s, list_y);
    } else if state.row_tweens.len() != total_rows || visibility_changed {
        let old_tweens = std::mem::take(&mut state.row_tweens);
        let old_visible_rows = state.select_music_prev_visible_rows.clone();
        let old_total_rows = old_visible_rows.len() + 1;
        let old_exit_from = old_tweens
            .get(old_total_rows.saturating_sub(1))
            .map(|tw| (tw.y(), tw.a()));

        let mut mapped: Vec<RowTween> = Vec::with_capacity(total_rows);
        for (new_idx, actual_idx) in visible_rows.iter().copied().enumerate() {
            let (to_y, to_a) = row_dest_for_index(total_rows, selected, new_idx, s, list_y);
            let parent_from = select_music_parent_row(actual_idx).and_then(|parent_actual_idx| {
                old_visible_rows
                    .iter()
                    .position(|&idx| idx == parent_actual_idx)
                    .and_then(|old_idx| old_tweens.get(old_idx))
                    .map(|tw| (tw.y(), 0.0))
            });
            let (from_y, from_a) = old_visible_rows
                .iter()
                .position(|&old_actual| old_actual == actual_idx)
                .and_then(|old_idx| old_tweens.get(old_idx).map(|tw| (tw.y(), tw.a())))
                .or(parent_from)
                .unwrap_or((to_y, to_a));
            let t = if (to_y - from_y).abs() <= 0.01 && (to_a - from_a).abs() <= 0.001 {
                1.0
            } else {
                0.0
            };
            mapped.push(RowTween {
                from_y,
                to_y,
                from_a,
                to_a,
                t,
            });
        }

        let exit_idx = total_rows.saturating_sub(1);
        let (to_y, to_a) = row_dest_for_index(total_rows, selected, exit_idx, s, list_y);
        let (from_y, from_a) = old_exit_from.unwrap_or((to_y, to_a));
        let t = if (to_y - from_y).abs() <= 0.01 && (to_a - from_a).abs() <= 0.001 {
            1.0
        } else {
            0.0
        };
        mapped.push(RowTween {
            from_y,
            to_y,
            from_a,
            to_a,
            t,
        });
        state.row_tweens = mapped;
    }

    state.select_music_prev_visible_rows = visible_rows;
    update_row_tweens(
        &mut state.row_tweens,
        total_rows,
        selected,
        s,
        list_y,
        dt,
    );
}

#[inline(always)]
fn measure_text_box(asset_manager: &AssetManager, text: &str, zoom: f32) -> (f32, f32) {
    let mut out_w = 1.0_f32;
    let mut out_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            out_h = (metrics_font.height as f32).max(1.0) * zoom;
            let mut w = font::measure_line_width_logical(metrics_font, text, all_fonts) as f32;
            if !w.is_finite() || w <= 0.0 {
                w = 1.0;
            }
            out_w = w * zoom;
        });
    });
    (out_w, out_h)
}

#[inline(always)]
fn ring_size_for_text(draw_w: f32, text_h: f32) -> (f32, f32) {
    let pad_y = widescale(6.0, 8.0);
    let min_pad_x = widescale(2.0, 3.0);
    let max_pad_x = widescale(22.0, 28.0);
    let width_ref = widescale(180.0, 220.0);
    let border_w = widescale(2.0, 2.5);
    let mut size_t = draw_w / width_ref;
    if !size_t.is_finite() {
        size_t = 0.0;
    }
    size_t = size_t.clamp(0.0, 1.0);
    let mut pad_x = (max_pad_x - min_pad_x).mul_add(size_t, min_pad_x);
    let max_pad_by_spacing = (INLINE_SPACING - border_w).max(min_pad_x);
    if pad_x > max_pad_by_spacing {
        pad_x = max_pad_by_spacing;
    }
    (draw_w + pad_x * 2.0, text_h + pad_y * 2.0)
}

#[inline(always)]
fn row_mid_y_for_cursor(
    state: &State,
    row_idx: usize,
    total_rows: usize,
    selected: usize,
    s: f32,
    list_y: f32,
) -> f32 {
    state
        .row_tweens
        .get(row_idx)
        .map(|tw| tw.to_y)
        .unwrap_or_else(|| row_dest_for_index(total_rows, selected, row_idx, s, list_y).0)
}

fn submenu_cursor_dest(
    state: &State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    s: f32,
    list_x: f32,
    list_y: f32,
    list_w: f32,
) -> Option<(f32, f32, f32, f32)> {
    if matches!(kind, SubmenuKind::Input) {
        return None;
    }
    let rows = submenu_rows(kind);
    let total_rows = submenu_total_rows(state, kind);
    if total_rows == 0 {
        return None;
    }
    let selected_row = state.sub_selected.min(total_rows - 1);
    let row_mid_y =
        row_mid_y_for_cursor(state, selected_row, total_rows, selected_row, s, list_y);
    let value_zoom = 0.835_f32;
    let label_bg_w = SUB_LABEL_COL_W * s;
    let item_col_left = list_x + label_bg_w;
    let item_col_w = list_w - label_bg_w;
    let single_center_x =
        item_col_w.mul_add(0.5, item_col_left) + SUB_SINGLE_VALUE_CENTER_OFFSET * s;

    if selected_row == total_rows - 1 {
        let (draw_w, text_h) = measure_text_box(asset_manager, "Exit", value_zoom);
        let (ring_w, ring_h) = ring_size_for_text(draw_w, text_h);
        return Some((single_center_x, row_mid_y, ring_w, ring_h));
    }
    let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
        return None;
    };

    let row = &rows[row_idx];
    let mut choice_texts = row_choices(state, kind, rows, row_idx);
    if choice_texts.is_empty() {
        return None;
    }
    if row.label == "Global Offset (ms)" {
        choice_texts[0] = Cow::Owned(format_ms(state.global_offset_ms));
    } else if row.label == "Master Volume" {
        choice_texts[0] = Cow::Owned(format_percent(state.master_volume_pct));
    } else if row.label == "SFX Volume" {
        choice_texts[0] = Cow::Owned(format_percent(state.sfx_volume_pct));
    } else if row.label == "Music Volume" {
        choice_texts[0] = Cow::Owned(format_percent(state.music_volume_pct));
    } else if row.label == "Visual Delay (ms)" {
        choice_texts[0] = Cow::Owned(format_ms(state.visual_delay_ms));
    }

    let selected_choice = submenu_cursor_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(choice_texts.len().saturating_sub(1));

    let mut widths: Vec<f32> = Vec::with_capacity(choice_texts.len());
    let mut text_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
            for text in &choice_texts {
                let mut w =
                    font::measure_line_width_logical(metrics_font, text.as_ref(), all_fonts) as f32;
                if !w.is_finite() || w <= 0.0 {
                    w = 1.0;
                }
                widths.push(w * value_zoom);
            }
        });
    });
    if widths.is_empty() {
        return None;
    }

    let draw_w = widths[selected_choice.min(widths.len().saturating_sub(1))];
    let center_x = if row.inline {
        let choice_inner_left = SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);
        let mut x = choice_inner_left;
        for width in widths.iter().take(selected_choice) {
            x += *width + INLINE_SPACING;
        }
        x + draw_w * 0.5
    } else {
        single_center_x
    };
    let (ring_w, ring_h) = ring_size_for_text(draw_w, text_h);
    Some((center_x, row_mid_y, ring_w, ring_h))
}

/* -------------------------------- drawing -------------------------------- */

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Mesh { vertices, .. } => {
            let mut out: Vec<crate::core::gfx::MeshVertex> = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                let mut c = v.color;
                c[3] *= alpha;
                out.push(crate::core::gfx::MeshVertex {
                    pos: v.pos,
                    color: c,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::TexturedMesh { vertices, .. } => {
            let mut out: Vec<crate::core::gfx::TexturedMeshVertex> =
                Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                let mut c = v.color;
                c[3] *= alpha;
                out.push(crate::core::gfx::TexturedMeshVertex {
                    pos: v.pos,
                    uv: v.uv,
                    tex_matrix_scale: v.tex_matrix_scale,
                    color: c,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::Frame {
            background,
            children,
            ..
        } => {
            if let Some(actors::Background::Color(c)) = background {
                c[3] *= alpha;
            }
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Camera { children, .. } => {
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

    if let Some(reload) = &state.reload_ui {
        let header = match reload.phase {
            ReloadPhase::Songs => "Loading songs...",
            ReloadPhase::Courses => "Loading courses...",
        };
        let text = if reload.line2.is_empty() && reload.line3.is_empty() {
            header.to_string()
        } else if reload.line2.is_empty() {
            format!("{header}\n{}", reload.line3)
        } else if reload.line3.is_empty() {
            format!("{header}\n{}", reload.line2)
        } else {
            format!("{header}\n{}\n{}", reload.line2, reload.line3)
        };

        let mut ui_actors: Vec<Actor> = Vec::with_capacity(2);
        ui_actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.65):
            z(300)
        ));
        ui_actors.push(act!(text:
            align(0.5, 0.5):
            xy(screen_width() * 0.5, screen_height() * 0.5):
            zoom(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"):
            settext(text):
            horizalign(center):
            z(301)
        ));
        for actor in &mut ui_actors {
            apply_alpha_to_actor(actor, alpha_multiplier);
        }
        actors.extend(ui_actors);
        return actors;
    }
    if let Some(score_import) = &state.score_import_ui {
        let header = if score_import.done {
            "Score import complete"
        } else {
            "Importing scores..."
        };
        let total = score_import.total_charts.max(score_import.processed_charts);
        let progress_line = format!(
            "Endpoint: {}   Profile: {}\nPack: {}\nProgress: {}/{} (found={}, missing={}, failed={})",
            score_import.endpoint.display_name(),
            score_import.profile_name,
            score_import.pack_label,
            score_import.processed_charts,
            total,
            score_import.imported_scores,
            score_import.missing_scores,
            score_import.failed_requests
        );
        let detail_line = if score_import.done {
            score_import.done_message.as_str()
        } else {
            score_import.detail_line.as_str()
        };
        let text = format!("{header}\n{progress_line}\n{detail_line}");

        let mut ui_actors: Vec<Actor> = Vec::with_capacity(2);
        ui_actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.7):
            z(300)
        ));
        ui_actors.push(act!(text:
            align(0.5, 0.5):
            xy(screen_width() * 0.5, screen_height() * 0.5):
            zoom(0.95):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"):
            settext(text):
            horizalign(center):
            z(301)
        ));
        for actor in &mut ui_actors {
            apply_alpha_to_actor(actor, alpha_multiplier);
        }
        actors.extend(ui_actors);
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
        right_avatar: None,
        fg_color: FG,
    }));

    /* --------------------------- MAIN CONTENT UI -------------------------- */

    // --- global colors ---
    let col_active_bg = color::rgba_hex("#333333"); // active bg for normal rows

    // inactive bg = #071016 @ 0.8 alpha
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];

    let col_white = [1.0, 1.0, 1.0, 1.0];
    let col_black = [0.0, 0.0, 0.0, 1.0];

    // Simply Love brand color (now uses the active theme color).
    let col_brand_bg = color::simply_love_rgba(state.active_color_index); // <-- CHANGED

    // --- scale & origin honoring fixed screen-space margins ---
    let (s, list_x, list_y) = scaled_block_origin_with_margins();

    // Geometry (scaled)
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
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
    let cursor_now = || -> Option<(f32, f32, f32, f32)> {
        if !state.cursor_initialized {
            return None;
        }
        let t = state.cursor_t.clamp(0.0, 1.0);
        let x = (state.cursor_to_x - state.cursor_from_x).mul_add(t, state.cursor_from_x);
        let y = (state.cursor_to_y - state.cursor_from_y).mul_add(t, state.cursor_from_y);
        let w = (state.cursor_to_w - state.cursor_from_w).mul_add(t, state.cursor_from_w);
        let h = (state.cursor_to_h - state.cursor_from_h).mul_add(t, state.cursor_from_h);
        Some((x, y, w, h))
    };

    match state.view {
        OptionsView::Main => {
            // Active text color (for normal rows) – Simply Love uses row index + global color index.
            let col_active_text =
                color::simply_love_rgba(state.active_color_index + state.selected as i32);

            let total_items = ITEMS.len();
            let row_h = ROW_H * s;
            for item_idx in 0..total_items {
                let (row_mid_y, row_alpha) = state
                    .row_tweens
                    .get(item_idx)
                    .map(|tw| (tw.y(), tw.a()))
                    .unwrap_or_else(|| {
                        row_dest_for_index(total_items, state.selected, item_idx, s, list_y)
                    });
                let row_alpha = row_alpha.clamp(0.0, 1.0);
                if row_alpha <= 0.001 {
                    continue;
                }
                let row_y = row_mid_y - 0.5 * row_h;
                let is_active = item_idx == state.selected;
                let is_exit = item_idx == total_items - 1;
                let row_w = if is_exit || !is_active {
                    list_w - sep_w
                } else {
                    list_w
                };
                let bg = if is_active {
                    if is_exit { col_brand_bg } else { col_active_bg }
                } else {
                    col_inactive_bg
                };

                ui_actors.push(act!(quad:
                    align(0.0, 0.0):
                    xy(list_x, row_y):
                    zoomto(row_w, row_h):
                    diffuse(bg[0], bg[1], bg[2], bg[3] * row_alpha)
                ));

                let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
                let text_x_base = TEXT_LEFT_PAD.mul_add(s, list_x);
                if !is_exit {
                    let mut heart_tint = if is_active {
                        col_active_text
                    } else {
                        col_white
                    };
                    heart_tint[3] *= row_alpha;
                    ui_actors.push(act!(sprite("heart.png"):
                        align(0.0, 0.5):
                        xy(heart_x, row_mid_y):
                        zoom(HEART_ZOOM):
                        diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
                    ));
                }

                let text_x = if is_exit { heart_x } else { text_x_base };
                let label = ITEMS[item_idx].name;
                let mut color_t = if is_exit {
                    if is_active { col_black } else { col_white }
                } else if is_active {
                    col_active_text
                } else {
                    col_white
                };
                color_t[3] *= row_alpha;
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
            let visible_rows = submenu_visible_row_indices(state, kind, rows);
            if matches!(kind, SubmenuKind::Input) {
                let col_active_text =
                    color::simply_love_rgba(state.active_color_index + state.sub_selected as i32);
                let total_rows = rows.len() + 1;
                let row_h = ROW_H * s;
                for row_idx in 0..total_rows {
                    let (row_mid_y, row_alpha) = state
                        .row_tweens
                        .get(row_idx)
                        .map(|tw| (tw.y(), tw.a()))
                        .unwrap_or_else(|| {
                            row_dest_for_index(total_rows, state.sub_selected, row_idx, s, list_y)
                        });
                    let row_alpha = row_alpha.clamp(0.0, 1.0);
                    if row_alpha <= 0.001 {
                        continue;
                    }
                    let row_y = row_mid_y - 0.5 * row_h;
                    let is_active = row_idx == state.sub_selected;
                    let is_exit = row_idx == total_rows - 1;
                    let row_w = if is_exit || !is_active {
                        list_w - sep_w
                    } else {
                        list_w
                    };
                    let bg = if is_active {
                        if is_exit { col_brand_bg } else { col_active_bg }
                    } else {
                        col_inactive_bg
                    };

                    ui_actors.push(act!(quad:
                        align(0.0, 0.0):
                        xy(list_x, row_y):
                        zoomto(row_w, row_h):
                        diffuse(bg[0], bg[1], bg[2], bg[3] * row_alpha)
                    ));

                    let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
                    let text_x_base = TEXT_LEFT_PAD.mul_add(s, list_x);
                    if !is_exit {
                        let mut heart_tint = if is_active {
                            col_active_text
                        } else {
                            col_white
                        };
                        heart_tint[3] *= row_alpha;
                        ui_actors.push(act!(sprite("heart.png"):
                            align(0.0, 0.5):
                            xy(heart_x, row_mid_y):
                            zoom(HEART_ZOOM):
                            diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
                        ));
                    }

                    let text_x = if is_exit { heart_x } else { text_x_base };
                    let label = if row_idx < rows.len() {
                        rows[row_idx].label
                    } else {
                        "Exit"
                    };
                    let mut text_color = if is_exit {
                        if is_active { col_black } else { col_white }
                    } else if is_active {
                        col_active_text
                    } else {
                        col_white
                    };
                    text_color[3] *= row_alpha;
                    ui_actors.push(act!(text:
                        align(0.0, 0.5):
                        xy(text_x, row_mid_y):
                        zoom(ITEM_TEXT_ZOOM):
                        diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                        font("miso"):
                        settext(label):
                        horizalign(left)
                    ));

                    if row_idx < rows.len() {
                        let row = &rows[row_idx];
                        if row.label == INPUT_ROW_BACKEND {
                            let choices = row_choices(state, kind, rows, row_idx);
                            if !choices.is_empty() {
                                let choice_idx = choice_indices
                                    .get(row_idx)
                                    .copied()
                                    .unwrap_or(0)
                                    .min(choices.len().saturating_sub(1));
                                let mut value_color = if is_active {
                                    col_active_text
                                } else {
                                    col_white
                                };
                                value_color[3] *= row_alpha;
                                let value_x = list_w.mul_add(1.0, list_x - TEXT_LEFT_PAD * s);
                                ui_actors.push(act!(text:
                                    align(1.0, 0.5):
                                    xy(value_x, row_mid_y):
                                    zoom(ITEM_TEXT_ZOOM):
                                    diffuse(value_color[0], value_color[1], value_color[2], value_color[3]):
                                    font("miso"):
                                    settext(choices[choice_idx].as_ref()):
                                    horizalign(right)
                                ));
                            }
                        }
                    }
                }

                let sel = state.sub_selected.min(total_rows.saturating_sub(1));
                let item = if sel < rows.len() {
                    &items[sel]
                } else {
                    &items[items.len().saturating_sub(1)]
                };
                selected_item = Some(item);
            } else {
                // Active text color for submenu rows.
                let col_active_text = color::simply_love_rgba(state.active_color_index);
                // Inactive option text color should be #808080 (alpha 1.0), match player options.
                let sl_gray = color::rgba_hex("#808080");

                let total_rows = visible_rows.len() + 1; // + Exit row

                let label_bg_w = SUB_LABEL_COL_W * s;
                let label_text_x = SUB_LABEL_TEXT_LEFT_PAD.mul_add(s, list_x);
                // Keep submenu header labels bounded to the left label column.
                let label_text_max_w = (label_bg_w - SUB_LABEL_TEXT_LEFT_PAD * s - 5.0).max(0.0);

                // Helper to compute the cursor center X for a given submenu row index.
                let calc_row_center_x = |row_idx: usize| -> f32 {
                    if row_idx >= total_rows {
                        return list_w.mul_add(0.5, list_x);
                    }
                    if row_idx == total_rows - 1 {
                        // Exit row: center within the items column (row width minus label column),
                        // matching how single-value rows like Music Rate are centered in player_options.rs.
                        let item_col_left = list_x + label_bg_w;
                        let item_col_w = list_w - label_bg_w;
                        return item_col_w.mul_add(0.5, item_col_left)
                            + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                    }
                    let Some(actual_row_idx) = visible_rows.get(row_idx).copied() else {
                        return list_w.mul_add(0.5, list_x);
                    };
                    let row = &rows[actual_row_idx];
                    // Non-inline rows behave as single-value rows: keep the cursor centered
                    // on the center of the available items column (row width minus label column).
                    if !row.inline {
                        let item_col_left = list_x + label_bg_w;
                        let item_col_w = list_w - label_bg_w;
                        return item_col_w.mul_add(0.5, item_col_left)
                            + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                    }
                    let choices = row_choices(state, kind, rows, actual_row_idx);
                    if choices.is_empty() {
                        return list_w.mul_add(0.5, list_x);
                    }
                    let value_zoom = 0.835_f32;
                    let choice_inner_left =
                        SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);
                    let mut widths: Vec<f32> = Vec::with_capacity(choices.len());
                    asset_manager.with_fonts(|all_fonts| {
                        asset_manager.with_font("miso", |metrics_font| {
                            for text in choices {
                                let mut w = font::measure_line_width_logical(
                                    metrics_font,
                                    text.as_ref(),
                                    all_fonts,
                                ) as f32;
                                if !w.is_finite() || w <= 0.0 {
                                    w = 1.0;
                                }
                                widths.push(w * value_zoom);
                            }
                        });
                    });
                    if widths.is_empty() {
                        return list_w.mul_add(0.5, list_x);
                    }
                    let mut x_positions: Vec<f32> = Vec::with_capacity(widths.len());
                    let mut x = choice_inner_left;
                    for w in &widths {
                        x_positions.push(x);
                        x += *w + INLINE_SPACING;
                    }
                    let sel_idx = choice_indices
                        .get(actual_row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(widths.len().saturating_sub(1));
                    widths[sel_idx].mul_add(0.5, x_positions[sel_idx])
                };

                let row_h = ROW_H * s;
                for row_idx in 0..total_rows {
                    let (row_mid_y, row_alpha) = state
                        .row_tweens
                        .get(row_idx)
                        .map(|tw| (tw.y(), tw.a()))
                        .unwrap_or_else(|| {
                            row_dest_for_index(total_rows, state.sub_selected, row_idx, s, list_y)
                        });
                    let row_alpha = row_alpha.clamp(0.0, 1.0);
                    if row_alpha <= 0.001 {
                        continue;
                    }
                    let row_y = row_mid_y - 0.5 * row_h;

                    let is_active = row_idx == state.sub_selected;
                    let is_exit = row_idx == total_rows - 1;

                    let row_w = if is_exit {
                        list_w - sep_w
                    } else if is_active {
                        list_w
                    } else {
                        list_w - sep_w
                    };

                    let bg = if is_active {
                        col_active_bg
                    } else {
                        col_inactive_bg
                    };

                    ui_actors.push(act!(quad:
                        align(0.0, 0.0):
                        xy(list_x, row_y):
                        zoomto(row_w, row_h):
                        diffuse(bg[0], bg[1], bg[2], bg[3] * row_alpha)
                    ));

                    if !is_exit {
                        let Some(actual_row_idx) = visible_rows.get(row_idx).copied() else {
                            continue;
                        };
                        // Left label background column (matches player options style).
                        ui_actors.push(act!(quad:
                            align(0.0, 0.0):
                            xy(list_x, row_y):
                            zoomto(label_bg_w, row_h):
                            diffuse(0.0, 0.0, 0.0, 0.25 * row_alpha)
                        ));

                        let row = &rows[actual_row_idx];
                        let inline_row = row.inline;
                        let label = row.label;
                        let title_color = if is_active {
                            let mut c = col_active_text;
                            c[3] = 1.0;
                            c
                        } else {
                            col_white
                        };
                        let mut title_color = title_color;
                        title_color[3] *= row_alpha;

                        ui_actors.push(act!(text:
                            align(0.0, 0.5):
                            xy(label_text_x, row_mid_y):
                            zoom(ITEM_TEXT_ZOOM):
                            diffuse(title_color[0], title_color[1], title_color[2], title_color[3]):
                            font("miso"):
                            settext(label):
                            maxwidth(label_text_max_w):
                            horizalign(left)
                        ));

                        // Inline Off/On options in the items column (or a single centered value if inline == false).
                        let mut choice_texts: Vec<Cow<'_, str>> =
                            row_choices(state, kind, rows, actual_row_idx);
                        if !choice_texts.is_empty() {
                            let value_zoom = 0.835_f32;
                            if row.label == "Global Offset (ms)" {
                                let formatted = Cow::Owned(format_ms(state.global_offset_ms));
                                choice_texts[0] = formatted;
                            } else if row.label == "Master Volume" {
                                let formatted = Cow::Owned(format_percent(state.master_volume_pct));
                                choice_texts[0] = formatted;
                            } else if row.label == "SFX Volume" {
                                let formatted = Cow::Owned(format_percent(state.sfx_volume_pct));
                                choice_texts[0] = formatted;
                            } else if row.label == "Music Volume" {
                                let formatted = Cow::Owned(format_percent(state.music_volume_pct));
                                choice_texts[0] = formatted;
                            } else if row.label == "Visual Delay (ms)" {
                                let formatted = Cow::Owned(format_ms(state.visual_delay_ms));
                                choice_texts[0] = formatted;
                            }

                            let mut widths: Vec<f32> = Vec::with_capacity(choice_texts.len());
                            asset_manager.with_fonts(|all_fonts| {
                                asset_manager.with_font("miso", |metrics_font| {
                                    for text in &choice_texts {
                                        let mut w = font::measure_line_width_logical(
                                            metrics_font,
                                            text.as_ref(),
                                            all_fonts,
                                        )
                                            as f32;
                                        if !w.is_finite() || w <= 0.0 {
                                            w = 1.0;
                                        }
                                        widths.push(w * value_zoom);
                                    }
                                });
                            });

                            let selected_choice = choice_indices
                                .get(actual_row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(choice_texts.len().saturating_sub(1));
                            let mut selected_left_x: Option<f32> = None;

                            let choice_inner_left =
                                SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);
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
                                    let x =
                                        x_positions.get(idx).copied().unwrap_or(choice_inner_left);
                                    let is_choice_selected = idx == selected_choice;
                                    if is_choice_selected {
                                        selected_left_x = Some(x);
                                    }

                                    let mut choice_color =
                                        if is_active { col_white } else { sl_gray };
                                    choice_color[3] *= row_alpha;
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
                                let mut choice_color = if is_active { col_white } else { sl_gray };
                                choice_color[3] *= row_alpha;
                                let choice_center_x = calc_row_center_x(row_idx);
                                let choice_text = choice_texts
                                    .get(selected_choice)
                                    .map_or("??", std::convert::AsRef::as_ref);
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
                                    line_color[3] *= row_alpha;
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
                            if is_active
                                && !is_fading_submenu
                                && let Some((center_x, center_y, ring_w, ring_h)) = cursor_now()
                            {
                                let border_w = widescale(2.0, 2.5);
                                let left = center_x - ring_w * 0.5;
                                let right = center_x + ring_w * 0.5;
                                let top = center_y - ring_h * 0.5;
                                let bottom = center_y + ring_h * 0.5;
                                let mut ring_color =
                                    color::decorative_rgba(state.active_color_index);
                                ring_color[3] *= row_alpha;
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
                        }
                    } else {
                        // Exit row: centered "Exit" text in the items column.
                        let label = "Exit";
                        let value_zoom = 0.835_f32;
                        let mut choice_color = if is_active { col_white } else { sl_gray };
                        choice_color[3] *= row_alpha;
                        let center_x = calc_row_center_x(row_idx);
                        let center_y = row_mid_y;

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
                        if is_active
                            && !is_fading_submenu
                            && let Some((ring_x, ring_y, ring_w, ring_h)) = cursor_now()
                        {
                            let border_w = widescale(2.0, 2.5);
                            let left = ring_x - ring_w * 0.5;
                            let right = ring_x + ring_w * 0.5;
                            let top = ring_y - ring_h * 0.5;
                            let bottom = ring_y + ring_h * 0.5;
                            let mut ring_color = color::decorative_rgba(state.active_color_index);
                            ring_color[3] *= row_alpha;

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
                        }
                    }
                }

                // Description items for the submenu
                let total_rows = visible_rows.len() + 1;
                let sel = state.sub_selected.min(total_rows.saturating_sub(1));
                let item = if sel < visible_rows.len() {
                    let actual_row_idx = visible_rows[sel];
                    &items[actual_row_idx]
                } else {
                    &items[items.len().saturating_sub(1)]
                };
                selected_item = Some(item);
            }
        }
    }

    // ------------------- Description content (selected) -------------------
    if let Some(item) = selected_item {
        // Match Simply Love's description box feel:
        // - explicit top/side padding for title and bullets so they can be tuned
        // - text zoom similar to other help text (player options, etc.)
        let mut cursor_y = DESC_TITLE_TOP_PAD_PX.mul_add(s, list_y);
        let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
        let title_step_px = 20.0 * s; // approximate vertical advance for title line

        // Title/explanation text:
        // - For any item with help lines, use the first help line as the long explanation,
        //   with remaining lines rendered as the bullet list (if any).
        // - Fallback to the item name if there is no help text.
        let help = item.help;
        let (raw_title_text, bullet_lines): (&str, &[&str]) = if help.is_empty() {
            (item.name, &[][..])
        } else {
            (help[0], &help[1..])
        };

        // Word-wrapping using actual font metrics so the title respects the
        // description box's inner width and padding exactly.
        let wrapped_title = asset_manager
            .with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |miso_font| {
                    let max_width_px = DESC_W.mul_add(s, -(2.0 * DESC_TITLE_SIDE_PAD_PX * s));
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
                                font::measure_line_width_logical(miso_font, &candidate, all_fonts)
                                    as f32;
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
            let bullet_x = DESC_BULLET_INDENT_PX.mul_add(s, desc_x + bullet_side_pad);
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
    if let Some(confirm) = &state.score_import_confirm {
        let w = screen_width();
        let h = screen_height();
        let cx = w * 0.5;
        let cy = h * 0.5;
        let answer_y = cy + 118.0;
        let yes_x = cx - 100.0;
        let no_x = cx + 100.0;
        let cursor_x = [yes_x, no_x][confirm.active_choice.min(1) as usize];
        let cursor_color = color::simply_love_rgba(state.active_color_index);
        let prompt_text = format!(
            "Import ALL packs for {} / {}?\nOnly missing GS scores: {}.\nRate limit is hard-capped at 3 requests per second.\nFor many charts this can take more than one hour.\nSpamming APIs can be problematic.\n\nStart now?",
            confirm.selection.endpoint.display_name(),
            if confirm.selection.profile.display_name.is_empty() {
                confirm.selection.profile.id.as_str()
            } else {
                confirm.selection.profile.display_name.as_str()
            },
            if confirm.selection.only_missing_gs_scores {
                "Yes"
            } else {
                "No"
            }
        );

        ui_actors.push(act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.9):
            z(700)
        ));
        ui_actors.push(act!(quad:
            align(0.5, 0.5):
            xy(cursor_x, answer_y):
            setsize(145.0, 40.0):
            diffuse(cursor_color[0], cursor_color[1], cursor_color[2], 1.0):
            z(701)
        ));
        ui_actors.push(act!(text:
            align(0.5, 0.5):
            xy(cx, cy - 65.0):
            font("miso"):
            zoom(0.95):
            maxwidth(w - 90.0):
            settext(prompt_text):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ));
        ui_actors.push(act!(text:
            align(0.5, 0.5):
            xy(yes_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext("YES"):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ));
        ui_actors.push(act!(text:
            align(0.5, 0.5):
            xy(no_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext("NO"):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ));
    }

    let combined_alpha = alpha_multiplier * state.content_alpha;
    for actor in &mut ui_actors {
        apply_alpha_to_actor(actor, combined_alpha);
    }
    actors.extend(ui_actors);

    actors
}
