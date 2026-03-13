use crate::act;
use crate::assets::AssetManager;
use crate::core::display::{self, MonitorSpec};
use crate::core::gfx::{BackendType, PresentModePolicy};
use crate::core::space::{is_wide, screen_height, screen_width, widescale};
// Screen navigation is handled in app.rs via the dispatcher
use crate::config::{
    self, BreakdownStyle, DefaultFailType, DisplayMode, FullscreenType, LogLevel,
    MachinePreferredPlayMode, MachinePreferredPlayStyle, SelectMusicPatternInfoMode, SimpleIni,
    SyncGraphMode,
};
use crate::core::audio;
#[cfg(target_os = "windows")]
use crate::core::input::WindowsPadBackend;
use crate::core::input::{InputEvent, VirtualAction};
use crate::game::parsing::{noteskin as noteskin_parser, simfile as song_loading};
use crate::game::{profile, scores};
use crate::screens::{Screen, ScreenAction};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
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
const RELOAD_BAR_H: f32 = 30.0;

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
const INPUT_DEBOUNCE_MIN_MS: i32 = 0;
const INPUT_DEBOUNCE_MAX_MS: i32 = 200;

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

#[derive(Clone, Debug)]
struct SubmenuRowLayout {
    texts: Arc<[Arc<str>]>,
    widths: Arc<[f32]>,
    x_positions: Arc<[f32]>,
    centers: Arc<[f32]>,
    text_h: f32,
    inline_row: bool,
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
const SEP_W: f32 = 2.5; // gap/stripe between rows and description
// Match SL non-wide/wide block sizing used by ScreenPlayerOptions underlay.
const OPTIONS_BLOCK_W_43: f32 = 614.0;
const OPTIONS_BLOCK_W_169: f32 = 792.0;
const DESC_W_43: f32 = 287.0; // ScreenOptionsService overlay.lua: WideScale(287,292)
const DESC_W_169: f32 = 292.0;
// derive description height from visible rows so it never includes a trailing gap
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

#[inline(always)]
fn desc_w_unscaled() -> f32 {
    widescale(DESC_W_43, DESC_W_169)
}

#[inline(always)]
fn list_w_unscaled() -> f32 {
    widescale(
        OPTIONS_BLOCK_W_43 - SEP_W - DESC_W_43,
        OPTIONS_BLOCK_W_169 - SEP_W - DESC_W_169,
    )
}

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
const DESC_NOTE_BOTTOM_PAD_PX: f32 = 18.0; // bottom padding for footer/note text
const DESC_TITLE_ZOOM: f32 = 1.0; // title text zoom (roughly header-sized)
const DESC_BODY_ZOOM: f32 = 1.0; // body/bullet text zoom (similar to help text)

#[inline(always)]
fn desc_wrap_extra_pad_unscaled() -> f32 {
    // Slightly tighter wrap in 4:3 to avoid edge clipping from font metric/render mismatch.
    widescale(6.0, 0.0)
}

#[inline(always)]
fn submenu_inline_widths_fit(widths: &[f32]) -> bool {
    if widths.is_empty() {
        return false;
    }
    if is_wide() {
        return true;
    }
    let total_w = widths.iter().copied().sum::<f32>()
        + INLINE_SPACING * (widths.len().saturating_sub(1) as f32);
    let item_col_w = (list_w_unscaled() - SUB_LABEL_COL_W).max(0.0);
    let inline_w = (item_col_w - SUB_INLINE_ITEMS_LEFT_PAD).max(0.0);
    total_w <= inline_w
}

pub const ITEMS: &[Item] = &[
    // Top-level ScreenOptionsService rows, ordered to match Simply Love's LineNames.
    Item {
        name: "System Options",
        help: &[
            "Adjust high-level settings like game type, theme, language, and more.",
            "Game",
            "Theme",
            "Language",
            "Log File",
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
            GRAPHICS_ROW_PRESENT_MODE,
            "Max FPS",
            "Show Stats",
            "Visual Delay",
        ],
    },
    Item {
        name: "Sound Options",
        help: &[
            "Adjust audio output settings and feedback sounds.",
            "Sound Device",
            "Audio Sample Rate",
            "Master Volume",
            "SFX Volume",
            "Assist Tick Volume",
            "Music Volume",
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
            "Video BGs",
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
            "Show Video Banners",
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
            "Adjust machine-level fail, cache/parsing, and null-or-die behavior.",
            "Default Fail Type",
            "Banner Cache",
            "CDTitle Cache",
            "Background Cache",
            "Song Parsing Threads",
            "Cache Songs",
            "Fast Load",
            "Sync Graph",
        ],
    },
    Item {
        name: "Course Options",
        help: &[
            "Adjust options related to course selection and course play behavior.",
            COURSE_ROW_SHOW_RANDOM,
            COURSE_ROW_SHOW_MOST_PLAYED,
            COURSE_ROW_SHOW_INDIVIDUAL_SCORES,
            COURSE_ROW_AUTOSUBMIT_INDIVIDUAL_SCORES,
        ],
    },
    Item {
        name: "Manage Local Profiles",
        help: &[
            "Create, edit, and manage player profiles that are stored on this computer.\n\nYou'll need a keyboard to use this screen.",
        ],
    },
    Item {
        name: "Online Score Services",
        help: &[
            "Configure online score services and import tools.",
            ONLINE_SCORING_ROW_GS_BS,
            ONLINE_SCORING_ROW_ARROWCLOUD,
            ONLINE_SCORING_ROW_SCORE_IMPORT,
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
    OnlineScoring,
    Machine,
    Advanced,
    Course,
    Gameplay,
    Sound,
    SelectMusic,
    GrooveStats,
    ArrowCloud,
    ScoreImport,
}

#[inline(always)]
const fn is_launcher_submenu(kind: SubmenuKind) -> bool {
    matches!(kind, SubmenuKind::Input | SubmenuKind::OnlineScoring)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsView {
    Main,
    Submenu(SubmenuKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DescriptionCacheKey {
    Main(usize),
    Submenu(SubmenuKind, usize),
}

#[derive(Clone, Debug)]
struct DescriptionLayout {
    key: DescriptionCacheKey,
    title: Arc<str>,
    title_lines: usize,
    bullet_text: Option<Arc<str>>,
    bullet_line_count: usize,
    note_text: Option<Arc<str>>,
    note_line_count: usize,
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
    Song {
        done: usize,
        total: usize,
        pack: String,
        song: String,
    },
    Course {
        done: usize,
        total: usize,
        group: String,
        course: String,
    },
    Done,
}

struct ReloadUiState {
    phase: ReloadPhase,
    line2: String,
    line3: String,
    songs_done: usize,
    songs_total: usize,
    courses_done: usize,
    courses_total: usize,
    done: bool,
    started_at: Instant,
    rx: std::sync::mpsc::Receiver<ReloadMsg>,
}

impl ReloadUiState {
    fn new(rx: std::sync::mpsc::Receiver<ReloadMsg>) -> Self {
        Self {
            phase: ReloadPhase::Songs,
            line2: String::new(),
            line3: String::new(),
            songs_done: 0,
            songs_total: 0,
            courses_done: 0,
            courses_total: 0,
            done: false,
            started_at: Instant::now(),
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

#[derive(Clone, Debug)]
struct SoundDeviceOption {
    label: String,
    config_index: Option<u16>,
    sample_rates_hz: Vec<u32>,
}

// Local fade timing when swapping between main options list and System Options submenu.
const SUBMENU_FADE_DURATION: f32 = 0.2;

pub struct SubRow<'a> {
    pub label: &'a str,
    pub choices: &'a [&'a str],
    pub inline: bool, // whether to lay out choices inline (vs single centered value)
}

const GS_ROW_ENABLE: &str = "Enable GrooveStats";
const GS_ROW_ENABLE_BOOGIE: &str = "Enable BoogieStats";
const GS_ROW_AUTO_POPULATE: &str = "Auto Populate GS Scores";
const SYSTEM_ROW_LOG_FILE: &str = "Log File";
const INPUT_ROW_CONFIGURE_MAPPINGS: &str = "Configure Keyboard/Pad Mappings";
const INPUT_ROW_TEST: &str = "Test Input";
const INPUT_ROW_OPTIONS: &str = "Input Options";
const INPUT_ROW_BACKEND: &str = "Gamepad Backend";
const INPUT_ROW_DEDICATED_MENU_BUTTONS: &str = "Menu Buttons";
const INPUT_ROW_DEBOUNCE: &str = "Debounce (ms)";
#[cfg(target_os = "windows")]
const INPUT_BACKEND_CHOICES: &[&str] = &["W32 Raw Input", "WGI (compat)"];
#[cfg(target_os = "macos")]
const INPUT_BACKEND_CHOICES: &[&str] = &["macOS IOHID"];
#[cfg(target_os = "linux")]
const INPUT_BACKEND_CHOICES: &[&str] = &["Linux evdev"];
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
const INPUT_BACKEND_CHOICES: &[&str] = &["Platform Default"];
#[cfg(not(any(target_os = "windows", unix)))]
const INPUT_BACKEND_CHOICES: &[&str] = &["Platform Default"];
#[cfg(target_os = "windows")]
const INPUT_BACKEND_INLINE: bool = true;
#[cfg(not(target_os = "windows"))]
const INPUT_BACKEND_INLINE: bool = false;
const SELECT_MUSIC_ROW_SHOW_BANNERS: &str = "Show Banners";
const SELECT_MUSIC_ROW_SHOW_VIDEO_BANNERS: &str = "Show Video Banners";
const SELECT_MUSIC_ROW_SHOW_BREAKDOWN: &str = "Show Breakdown";
const SELECT_MUSIC_ROW_BREAKDOWN_STYLE: &str = "Breakdown Style";
const SELECT_MUSIC_ROW_NATIVE_LANGUAGE: &str = "Show Native Language";
const SELECT_MUSIC_ROW_WHEEL_SPEED: &str = "Music Wheel Speed";
const SELECT_MUSIC_ROW_CDTITLES: &str = "Show CDTitles";
const SELECT_MUSIC_ROW_WHEEL_GRADES: &str = "Show Music Wheel Grades";
const SELECT_MUSIC_ROW_WHEEL_LAMPS: &str = "Show Music Wheel Lamps";
const SELECT_MUSIC_ROW_PATTERN_INFO: &str = "Show Pattern Info";
const SELECT_MUSIC_ROW_PREVIEWS: &str = "Music Previews";
const SELECT_MUSIC_ROW_PREVIEW_MARKER: &str = "Preview Marker";
const SELECT_MUSIC_ROW_PREVIEW_LOOP: &str = "Loop Music";
const SELECT_MUSIC_ROW_GAMEPLAY_TIMER: &str = "Show Gameplay Timer";
const SELECT_MUSIC_ROW_SHOW_RIVALS: &str = "Show GS Box";
const SELECT_MUSIC_ROW_SCOREBOX_CYCLE: &str = "GS Box Leaderboards";
const SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES: usize = 4;
const MACHINE_ROW_SELECT_PROFILE: &str = "Select Profile";
const MACHINE_ROW_SELECT_COLOR: &str = "Select Color";
const MACHINE_ROW_SELECT_STYLE: &str = "Select Style";
const MACHINE_ROW_PREFERRED_STYLE: &str = "Preferred Style";
const MACHINE_ROW_SELECT_PLAY_MODE: &str = "Select Play Mode";
const MACHINE_ROW_PREFERRED_MODE: &str = "Preferred Mode";
const MACHINE_ROW_EVAL_SUMMARY: &str = "Eval Summary";
const MACHINE_ROW_NAME_ENTRY: &str = "Name Entry";
const MACHINE_ROW_GAMEOVER: &str = "Gameover Screen";
const MACHINE_ROW_MENU_MUSIC: &str = "Menu Music";
const MACHINE_ROW_KEYBOARD_FEATURES: &str = "Keyboard Features";
const MACHINE_ROW_VIDEO_BGS: &str = "Video BGs";
const ADVANCED_ROW_DEFAULT_FAIL_TYPE: &str = "Default Fail Type";
const ADVANCED_ROW_BANNER_CACHE: &str = "Banner Cache";
const ADVANCED_ROW_CDTITLE_CACHE: &str = "CDTitle Cache";
const ADVANCED_ROW_SONG_PARSING_THREADS: &str = "Song Parsing Threads";
const ADVANCED_ROW_CACHE_SONGS: &str = "Cache Songs";
const ADVANCED_ROW_FAST_LOAD: &str = "Fast Load";
const ADVANCED_ROW_SYNC_GRAPH: &str = "Sync Graph";
const ADVANCED_SYNC_GRAPH_CHOICES: &[&str] =
    &["Frequency", "Beat index", "Post-kernel fingerprint"];
const SOUND_ROW_MASTER_VOLUME: &str = "Master Volume";
const SOUND_ROW_SFX_VOLUME: &str = "SFX Volume";
const SOUND_ROW_ASSIST_TICK_VOLUME: &str = "Assist Tick Volume";
const SOUND_ROW_MUSIC_VOLUME: &str = "Music Volume";
const SOUND_ROW_DEVICE: &str = "Sound Device";
const SOUND_ROW_OUTPUT_MODE: &str = "Audio Output Mode";
#[cfg(target_os = "linux")]
const SOUND_ROW_LINUX_BACKEND: &str = "Linux Audio Backend";
const SOUND_ROW_SAMPLE_RATE: &str = "Audio Sample Rate";
const SOUND_ROW_MINE_SOUNDS: &str = "Mine Sounds";
const SOUND_ROW_GLOBAL_OFFSET: &str = "Global Offset (ms)";
const SOUND_ROW_RATEMOD_PITCH: &str = "RateMod Preserves Pitch";
const COURSE_ROW_SHOW_RANDOM: &str = "Show Random Courses";
const COURSE_ROW_SHOW_MOST_PLAYED: &str = "Show Most Played";
const COURSE_ROW_SHOW_INDIVIDUAL_SCORES: &str = "Show Individual Scores for Course";
const COURSE_ROW_AUTOSUBMIT_INDIVIDUAL_SCORES: &str = "Autosubmit Scores in Courses Individually";
const ONLINE_SCORING_ROW_GS_BS: &str = "GrooveStats / BoogieStats Options";
const ONLINE_SCORING_ROW_ARROWCLOUD: &str = "ArrowCloud Options";
const ONLINE_SCORING_ROW_SCORE_IMPORT: &str = "Score Import";
const ARROWCLOUD_ROW_ENABLE: &str = "Enable ArrowCloud";
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

#[cfg(all(
    target_os = "linux",
    has_pipewire_audio,
    has_pulse_audio,
    has_jack_audio
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "PipeWire", "PulseAudio", "JACK", "ALSA"];
#[cfg(all(
    target_os = "linux",
    has_pipewire_audio,
    has_pulse_audio,
    not(has_jack_audio)
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "PipeWire", "PulseAudio", "ALSA"];
#[cfg(all(
    target_os = "linux",
    has_pipewire_audio,
    not(has_pulse_audio),
    has_jack_audio
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "PipeWire", "JACK", "ALSA"];
#[cfg(all(
    target_os = "linux",
    has_pipewire_audio,
    not(has_pulse_audio),
    not(has_jack_audio)
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "PipeWire", "ALSA"];
#[cfg(all(
    target_os = "linux",
    not(has_pipewire_audio),
    has_pulse_audio,
    has_jack_audio
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "PulseAudio", "JACK", "ALSA"];
#[cfg(all(
    target_os = "linux",
    not(has_pipewire_audio),
    has_pulse_audio,
    not(has_jack_audio)
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "PulseAudio", "ALSA"];
#[cfg(all(
    target_os = "linux",
    not(has_pipewire_audio),
    not(has_pulse_audio),
    has_jack_audio
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "JACK", "ALSA"];
#[cfg(all(
    target_os = "linux",
    not(has_pipewire_audio),
    not(has_pulse_audio),
    not(has_jack_audio)
))]
const SOUND_LINUX_BACKEND_CHOICES: &[&str] = &["Auto", "ALSA"];

fn discover_system_noteskin_choices() -> Vec<String> {
    let mut names = noteskin_parser::discover_itg_skins("dance");
    if names.is_empty() {
        names.push(profile::NoteSkin::DEFAULT_NAME.to_string());
    }
    names
}

fn build_sound_device_options() -> Vec<SoundDeviceOption> {
    let discovered = audio::startup_output_devices();
    let default_rates = discovered
        .iter()
        .find(|dev| dev.is_default)
        .map(|dev| dev.sample_rates_hz.clone())
        .unwrap_or_default();
    let mut options = Vec::with_capacity(discovered.len() + 1);
    options.push(SoundDeviceOption {
        label: "Auto".to_string(),
        config_index: None,
        sample_rates_hz: default_rates,
    });
    for (idx, dev) in discovered.into_iter().enumerate() {
        let mut label = dev.name.clone();
        if dev.is_default {
            label.push_str(" (Default)");
        }
        options.push(SoundDeviceOption {
            label,
            config_index: Some(idx as u16),
            sample_rates_hz: dev.sample_rates_hz,
        });
    }
    options
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
        label: "Log Level",
        choices: &["Error", "Warn", "Info", "Debug", "Trace"],
        inline: false,
    },
    SubRow {
        label: SYSTEM_ROW_LOG_FILE,
        choices: &["Off", "On"],
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
        name: "Log Level",
        help: &[
            "Set application log verbosity.",
            "Applies immediately and is saved to deadsync.ini.",
        ],
    },
    Item {
        name: SYSTEM_ROW_LOG_FILE,
        help: &[
            "Mirror application logs to deadsync.log in the game root folder.",
            "Off keeps logs in the command line only.",
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
const VSYNC_ROW_INDEX: usize = 7;
const PRESENT_MODE_ROW_INDEX: usize = 8;
const MAX_FPS_ENABLED_ROW_INDEX: usize = 9;
const MAX_FPS_VALUE_ROW_INDEX: usize = 10;
const GRAPHICS_ROW_VIDEO_RENDERER: &str = "Video Renderer";
const GRAPHICS_ROW_SOFTWARE_THREADS: &str = "Software Renderer Threads";
const GRAPHICS_ROW_PRESENT_MODE: &str = "Present Mode";
const GRAPHICS_ROW_MAX_FPS: &str = "Max FPS";
const GRAPHICS_ROW_MAX_FPS_VALUE: &str = "FPS Limit";
const GRAPHICS_ROW_VALIDATION_LAYERS: &str = "Validation Layers";
const SELECT_MUSIC_SHOW_BANNERS_ROW_INDEX: usize = 0;
const SELECT_MUSIC_SHOW_VIDEO_BANNERS_ROW_INDEX: usize = 1;
const SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX: usize = 2;
const SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX: usize = 3;
const SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX: usize = 10;
const SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX: usize = 11;
const SELECT_MUSIC_SHOW_SCOREBOX_ROW_INDEX: usize = 13;
const SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX: usize = 14;
const MACHINE_SELECT_STYLE_ROW_INDEX: usize = 2;
const MACHINE_PREFERRED_STYLE_ROW_INDEX: usize = 3;
const MACHINE_SELECT_PLAY_MODE_ROW_INDEX: usize = 4;
const MACHINE_PREFERRED_MODE_ROW_INDEX: usize = 5;
const ADVANCED_SONG_PARSING_THREADS_ROW_INDEX: usize = 3;

const BG_BRIGHTNESS_CHOICES: [&str; 11] = [
    "0%", "10%", "20%", "30%", "40%", "50%", "60%", "70%", "80%", "90%", "100%",
];
const MAX_FPS_MIN: u16 = 5;
const MAX_FPS_MAX: u16 = 1000;
const MAX_FPS_STEP: u16 = 5;
const MAX_FPS_DEFAULT: u16 = 60;
const PRESENT_MODE_CHOICES: [&str; 2] = ["Mailbox", "Immediate"];
const CENTERED_P1_NOTEFIELD_CHOICES: [&str; 2] = ["Off", "On"];
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
const SELECT_MUSIC_SCOREBOX_CYCLE_CHOICES: [&str; SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES] =
    ["ITG", "EX", "H.EX", "Tournaments"];

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
        label: GRAPHICS_ROW_PRESENT_MODE,
        choices: &PRESENT_MODE_CHOICES,
        inline: true,
    },
    SubRow {
        label: GRAPHICS_ROW_MAX_FPS,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: GRAPHICS_ROW_MAX_FPS_VALUE,
        choices: &["Off"], // Replaced dynamically
        inline: false,
    },
    SubRow {
        label: "Show Stats",
        choices: &["Off", "FPS", "FPS+Stutter", "FPS+Stutter+Timing"],
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
        name: GRAPHICS_ROW_PRESENT_MODE,
        help: &[
            "Choose the present mode policy used when VSync is off.",
            "Mailbox prefers tear-free low-latency presentation and keeps present back-pressure on.",
            "Immediate prefers the lowest-latency uncapped path and may tear.",
        ],
    },
    Item {
        name: GRAPHICS_ROW_MAX_FPS,
        help: &[
            "Enable an optional redraw cap used when VSync is off.",
            "No leaves redraw scheduling uncapped.",
        ],
    },
    Item {
        name: GRAPHICS_ROW_MAX_FPS_VALUE,
        help: &[
            "Choose the redraw cap used when Max FPS is enabled.",
            "Values adjust in 5 FPS steps.",
        ],
    },
    Item {
        name: "Show Stats",
        help: &[
            "Choose performance overlay mode: Off, FPS only, FPS with stutter list, or FPS+Stutter+Timing.",
            "The timing mode adds present prediction, fallback, queue-pressure, and clock-domain details.",
        ],
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
            INPUT_ROW_DEBOUNCE,
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const INPUT_BACKEND_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: INPUT_ROW_BACKEND,
        choices: INPUT_BACKEND_CHOICES,
        inline: INPUT_BACKEND_INLINE,
    },
    SubRow {
        label: INPUT_ROW_DEDICATED_MENU_BUTTONS,
        choices: &["Use Gameplay Buttons", "Only Dedicated Buttons"],
        inline: true,
    },
    SubRow {
        label: INPUT_ROW_DEBOUNCE,
        choices: &["20ms"],
        inline: true,
    },
];

pub const INPUT_BACKEND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: INPUT_ROW_BACKEND,
        help: &[
            "Choose gamepad input backend. On Windows Raw Input is the default path and WGI remains available as a compatibility fallback.",
            "Changing backend requires a restart.",
        ],
    },
    Item {
        name: INPUT_ROW_DEDICATED_MENU_BUTTONS,
        help: &[
            "Choose whether to allow using gameplay buttons (e.g. directional arrows) for menu navigation.",
            "Use Gameplay Buttons - Navigate through the game using your dance pad.",
            "Only Dedicated Buttons - Navigate through the game using dedicated menu buttons. Requires all four menu directions (MenuUp, MenuDown, MenuLeft, MenuRight) to be mapped for at least one player.",
        ],
    },
    Item {
        name: INPUT_ROW_DEBOUNCE,
        help: &[
            "Per-input debounce window used across keyboard and all gamepad drivers.",
            "ITGmania default is 20ms. 50ms was common on older arcade pads.",
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
        label: MACHINE_ROW_PREFERRED_STYLE,
        choices: &["1 Player", "2 Players", "Double"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_SELECT_PLAY_MODE,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: MACHINE_ROW_PREFERRED_MODE,
        choices: &["Regular", "Marathon"],
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
    SubRow {
        label: MACHINE_ROW_VIDEO_BGS,
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
        name: MACHINE_ROW_PREFERRED_STYLE,
        help: &["Applied when Select Style is Off."],
    },
    Item {
        name: MACHINE_ROW_SELECT_PLAY_MODE,
        help: &["Show or skip Select Play Mode during startup."],
    },
    Item {
        name: MACHINE_ROW_PREFERRED_MODE,
        help: &["Applied when Select Play Mode is Off."],
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
        name: MACHINE_ROW_VIDEO_BGS,
        help: &[
            "Animate gameplay background movies.",
            "When Off, video BGs use the first-frame poster instead.",
        ],
    },
    Item {
        name: "Exit",
        help: &["Return to the main Options list."],
    },
];

pub const COURSE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: COURSE_ROW_SHOW_RANDOM,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: COURSE_ROW_SHOW_MOST_PLAYED,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: COURSE_ROW_SHOW_INDIVIDUAL_SCORES,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: COURSE_ROW_AUTOSUBMIT_INDIVIDUAL_SCORES,
        choices: &["No", "Yes"],
        inline: true,
    },
];

pub const COURSE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: COURSE_ROW_SHOW_RANDOM,
        help: &["Show or hide courses that contain random stage entries (e.g. RANDOM/group/*)."],
    },
    Item {
        name: COURSE_ROW_SHOW_MOST_PLAYED,
        help: &["Show or hide courses that contain MostPlayed/BEST sort-pick entries."],
    },
    Item {
        name: COURSE_ROW_SHOW_INDIVIDUAL_SCORES,
        help: &["When No, course per-song score pages are hidden in Evaluation and end flow."],
    },
    Item {
        name: COURSE_ROW_AUTOSUBMIT_INDIVIDUAL_SCORES,
        help: &["Enable per-song course autosubmit behavior (stored for parity wiring)."],
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
        label: SOUND_ROW_DEVICE,
        choices: &["Auto"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_OUTPUT_MODE,
        choices: &["Auto", "Shared", "Exclusive"],
        inline: false,
    },
    #[cfg(target_os = "linux")]
    SubRow {
        label: SOUND_ROW_LINUX_BACKEND,
        choices: SOUND_LINUX_BACKEND_CHOICES,
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_SAMPLE_RATE,
        choices: &["Auto"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_MASTER_VOLUME,
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_SFX_VOLUME,
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_ASSIST_TICK_VOLUME,
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_MUSIC_VOLUME,
        choices: &["100%"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_MINE_SOUNDS,
        choices: &["Off", "On"],
        inline: true,
    },
    SubRow {
        label: SOUND_ROW_GLOBAL_OFFSET,
        choices: &["0 ms"],
        inline: false,
    },
    SubRow {
        label: SOUND_ROW_RATEMOD_PITCH,
        choices: &["Off", "On"],
        inline: true,
    },
];

pub const SOUND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: SOUND_ROW_DEVICE,
        help: &[
            "Select an output device detected at startup.",
            "Auto uses the host default output device.",
            "Windows playback prefers native WASAPI.",
            "macOS playback prefers native CoreAudio.",
            "FreeBSD playback prefers native PCM/OSS.",
            "Linux backend routing depends on Linux Audio Backend and Audio Output Mode.",
            "Changing this takes effect on next launch.",
        ],
    },
    Item {
        name: SOUND_ROW_OUTPUT_MODE,
        help: &[
            "Select whether audio output should use Auto, Shared, or Exclusive mode.",
            "Auto keeps the backend default policy.",
            "Shared forces shared-mode output where supported.",
            "Exclusive requests direct/exclusive output where supported and may fail if unavailable.",
            "CoreAudio currently supports Auto/Shared; Exclusive is not implemented yet.",
            "FreeBSD PCM currently supports Auto/Shared; Exclusive is not implemented yet.",
            "Changing this takes effect on next launch.",
        ],
    },
    #[cfg(target_os = "linux")]
    Item {
        name: SOUND_ROW_LINUX_BACKEND,
        help: &[
            "Select which Linux backend to prefer.",
            "Backends shown in this menu depend on what this build includes.",
            "Auto prefers PipeWire first when available, then PulseAudio, and falls back to ALSA or CPAL as needed.",
            "PipeWire and PulseAudio are shared-output backends and currently ignore explicit Sound Device selection.",
            "JACK is an explicit low-latency backend and currently ignores Sound Device selection.",
            "ALSA is the direct Linux backend and remains the exclusive/direct path.",
            "Changing this takes effect on next launch.",
        ],
    },
    Item {
        name: SOUND_ROW_SAMPLE_RATE,
        help: &["Select an audio output sample rate for the chosen Sound Device."],
    },
    Item {
        name: SOUND_ROW_MASTER_VOLUME,
        help: &["Set the overall volume for all audio."],
    },
    Item {
        name: SOUND_ROW_SFX_VOLUME,
        help: &["Set the sound-effect volume before master volume is applied."],
    },
    Item {
        name: SOUND_ROW_ASSIST_TICK_VOLUME,
        help: &["Set the gameplay Assist Tick volume before master volume is applied."],
    },
    Item {
        name: SOUND_ROW_MUSIC_VOLUME,
        help: &["Set the music volume before master volume is applied."],
    },
    Item {
        name: SOUND_ROW_MINE_SOUNDS,
        help: &["Play a sound when mines are hit."],
    },
    Item {
        name: SOUND_ROW_GLOBAL_OFFSET,
        help: &["Apply a global audio timing offset in 1 ms steps."],
    },
    Item {
        name: SOUND_ROW_RATEMOD_PITCH,
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
        label: SELECT_MUSIC_ROW_SHOW_VIDEO_BANNERS,
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
        label: SELECT_MUSIC_ROW_PREVIEW_MARKER,
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
    SubRow {
        label: SELECT_MUSIC_ROW_SCOREBOX_CYCLE,
        choices: &SELECT_MUSIC_SCOREBOX_CYCLE_CHOICES,
        inline: true,
    },
];

pub const SELECT_MUSIC_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: SELECT_MUSIC_ROW_SHOW_BANNERS,
        help: &["Show song/pack banners or force color fallback banners."],
    },
    Item {
        name: SELECT_MUSIC_ROW_SHOW_VIDEO_BANNERS,
        help: &[
            "Animate MP4 banner files when a selection is settled.",
            "When No, video banners use the cached poster frame only.",
        ],
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
        name: SELECT_MUSIC_ROW_PREVIEW_MARKER,
        help: &[
            "Show a white line over the density graph for the current preview position.",
            "Only appears while music previews are playing.",
        ],
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
            "Show GS box in Select Music pane/scorebox areas when available.",
            "GS box will not show unless GrooveStats/BoogieStats/ArrowCloud is enabled and connected.",
        ],
    },
    Item {
        name: SELECT_MUSIC_ROW_SCOREBOX_CYCLE,
        help: &[
            "Choose which leaderboards the GS box cycles through.",
            "Use Left/Right to select ITG/EX/H.EX/Tournaments, then Start to toggle each option.",
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
        label: ADVANCED_ROW_CDTITLE_CACHE,
        choices: &["Off", "On"],
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
    SubRow {
        label: ADVANCED_ROW_SYNC_GRAPH,
        choices: ADVANCED_SYNC_GRAPH_CHOICES,
        inline: false,
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
        ],
    },
    Item {
        name: ADVANCED_ROW_CDTITLE_CACHE,
        help: &[
            "Enable or disable CDTitle raw texture cache on disk.",
            "Default: On (CDTitleCache=1).",
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
        name: ADVANCED_ROW_SYNC_GRAPH,
        help: &[
            "Choose which null-or-die graph the Select Music sync overlay shows.",
            "Frequency: weighted spectral accumulator.",
            "Beat index: per-beat digest over time.",
            "Post-kernel fingerprint: convolution heatmap with the final kernel response.",
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
        label: GS_ROW_ENABLE_BOOGIE,
        choices: &["No", "Yes"],
        inline: true,
    },
    SubRow {
        label: GS_ROW_AUTO_POPULATE,
        choices: &["No", "Yes"],
        inline: true,
    },
];

pub const ARROWCLOUD_OPTIONS_ROWS: &[SubRow] = &[SubRow {
    label: ARROWCLOUD_ROW_ENABLE,
    choices: &["No", "Yes"],
    inline: true,
}];

pub const ONLINE_SCORING_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        label: ONLINE_SCORING_ROW_GS_BS,
        choices: &[],
        inline: false,
    },
    SubRow {
        label: ONLINE_SCORING_ROW_ARROWCLOUD,
        choices: &[],
        inline: false,
    },
    SubRow {
        label: ONLINE_SCORING_ROW_SCORE_IMPORT,
        choices: &[],
        inline: false,
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
        name: GS_ROW_ENABLE_BOOGIE,
        help: &[
            "Switch GrooveStats service URLs to BoogieStats endpoints.",
            "Requires Enable GrooveStats to be On.",
        ],
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

pub const ARROWCLOUD_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: ARROWCLOUD_ROW_ENABLE,
        help: &["Enable connection to ArrowCloud services."],
    },
    Item {
        name: "Exit",
        help: &["Return to the previous menu."],
    },
];

pub const ONLINE_SCORING_OPTIONS_ITEMS: &[Item] = &[
    Item {
        name: ONLINE_SCORING_ROW_GS_BS,
        help: &["Open GrooveStats / BoogieStats settings."],
    },
    Item {
        name: ONLINE_SCORING_ROW_ARROWCLOUD,
        help: &["Open ArrowCloud settings."],
    },
    Item {
        name: ONLINE_SCORING_ROW_SCORE_IMPORT,
        help: &["Open score import tools and endpoint/profile selection."],
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

/// Returns `true` when the given submenu row should be treated as disabled
/// (non-interactive and visually dimmed). Add new cases here for any row
/// that should be conditionally locked based on runtime state.
fn is_submenu_row_disabled(kind: SubmenuKind, label: &str) -> bool {
    match (kind, label) {
        (SubmenuKind::InputBackend, INPUT_ROW_DEDICATED_MENU_BUTTONS) => {
            !crate::core::input::any_player_has_dedicated_menu_buttons()
        }
        _ => false,
    }
}

const fn submenu_rows(kind: SubmenuKind) -> &'static [SubRow<'static>] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ROWS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ROWS,
        SubmenuKind::Input => INPUT_OPTIONS_ROWS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ROWS,
        SubmenuKind::OnlineScoring => ONLINE_SCORING_OPTIONS_ROWS,
        SubmenuKind::Machine => MACHINE_OPTIONS_ROWS,
        SubmenuKind::Advanced => ADVANCED_OPTIONS_ROWS,
        SubmenuKind::Course => COURSE_OPTIONS_ROWS,
        SubmenuKind::Gameplay => GAMEPLAY_OPTIONS_ROWS,
        SubmenuKind::Sound => SOUND_OPTIONS_ROWS,
        SubmenuKind::SelectMusic => SELECT_MUSIC_OPTIONS_ROWS,
        SubmenuKind::GrooveStats => GROOVESTATS_OPTIONS_ROWS,
        SubmenuKind::ArrowCloud => ARROWCLOUD_OPTIONS_ROWS,
        SubmenuKind::ScoreImport => SCORE_IMPORT_OPTIONS_ROWS,
    }
}

const fn submenu_items(kind: SubmenuKind) -> &'static [Item<'static>] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ITEMS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ITEMS,
        SubmenuKind::Input => INPUT_OPTIONS_ITEMS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ITEMS,
        SubmenuKind::OnlineScoring => ONLINE_SCORING_OPTIONS_ITEMS,
        SubmenuKind::Machine => MACHINE_OPTIONS_ITEMS,
        SubmenuKind::Advanced => ADVANCED_OPTIONS_ITEMS,
        SubmenuKind::Course => COURSE_OPTIONS_ITEMS,
        SubmenuKind::Gameplay => GAMEPLAY_OPTIONS_ITEMS,
        SubmenuKind::Sound => SOUND_OPTIONS_ITEMS,
        SubmenuKind::SelectMusic => SELECT_MUSIC_OPTIONS_ITEMS,
        SubmenuKind::GrooveStats => GROOVESTATS_OPTIONS_ITEMS,
        SubmenuKind::ArrowCloud => ARROWCLOUD_OPTIONS_ITEMS,
        SubmenuKind::ScoreImport => SCORE_IMPORT_OPTIONS_ITEMS,
    }
}

const fn submenu_title(kind: SubmenuKind) -> &'static str {
    match kind {
        SubmenuKind::System => "SYSTEM OPTIONS",
        SubmenuKind::Graphics => "GRAPHICS OPTIONS",
        SubmenuKind::Input => "INPUT OPTIONS",
        SubmenuKind::InputBackend => "INPUT OPTIONS",
        SubmenuKind::OnlineScoring => "ONLINE SCORE SERVICES",
        SubmenuKind::Machine => "MACHINE OPTIONS",
        SubmenuKind::Advanced => "ADVANCED OPTIONS",
        SubmenuKind::Course => "COURSE OPTIONS",
        SubmenuKind::Gameplay => "GAMEPLAY OPTIONS",
        SubmenuKind::Sound => "SOUND OPTIONS",
        SubmenuKind::SelectMusic => "SELECT MUSIC OPTIONS",
        SubmenuKind::GrooveStats => "GROOVESTATS OPTIONS",
        SubmenuKind::ArrowCloud => "ARROWCLOUD OPTIONS",
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

fn build_max_fps_choices() -> Vec<u16> {
    let mut out = Vec::with_capacity(
        1 + usize::from(MAX_FPS_MAX.saturating_sub(MAX_FPS_MIN)) / usize::from(MAX_FPS_STEP),
    );
    let mut fps = MAX_FPS_MIN;
    while fps <= MAX_FPS_MAX {
        out.push(fps);
        fps = fps.saturating_add(MAX_FPS_STEP);
    }
    out
}

fn max_fps_choice_labels(values: &[u16]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

#[inline(always)]
const fn clamped_max_fps(max_fps: u16) -> u16 {
    if max_fps < MAX_FPS_MIN {
        MAX_FPS_MIN
    } else if max_fps > MAX_FPS_MAX {
        MAX_FPS_MAX
    } else {
        max_fps
    }
}

fn max_fps_choice_index(values: &[u16], max_fps: u16) -> usize {
    let target = clamped_max_fps(max_fps);
    values.iter().position(|&v| v == target).unwrap_or_else(|| {
        values
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.abs_diff(target))
            .map_or(0, |(idx, _)| idx)
    })
}

fn max_fps_from_choice(values: &[u16], idx: usize) -> u16 {
    values.get(idx).copied().unwrap_or(MAX_FPS_DEFAULT)
}

#[inline(always)]
const fn present_mode_choice_index(mode: PresentModePolicy) -> usize {
    match mode {
        PresentModePolicy::Mailbox => 0,
        PresentModePolicy::Immediate => 1,
    }
}

#[inline(always)]
const fn present_mode_from_choice(idx: usize) -> PresentModePolicy {
    match idx {
        1 => PresentModePolicy::Immediate,
        _ => PresentModePolicy::Mailbox,
    }
}

fn selected_present_mode_policy(state: &State) -> PresentModePolicy {
    state
        .sub_choice_indices_graphics
        .get(PRESENT_MODE_ROW_INDEX)
        .copied()
        .map_or(state.present_mode_policy_at_load, present_mode_from_choice)
}

#[inline(always)]
fn set_max_fps_enabled_choice(state: &mut State, enabled: bool) {
    let idx = yes_no_choice_index(enabled);
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(MAX_FPS_ENABLED_ROW_INDEX)
    {
        *slot = idx;
    }
    if let Some(slot) = state
        .sub_cursor_indices_graphics
        .get_mut(MAX_FPS_ENABLED_ROW_INDEX)
    {
        *slot = idx;
    }
}

#[inline(always)]
fn set_max_fps_value_choice_index(state: &mut State, idx: usize) {
    let max_idx = state.max_fps_choices.len().saturating_sub(1);
    let clamped = idx.min(max_idx);
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(MAX_FPS_VALUE_ROW_INDEX)
    {
        *slot = clamped;
    }
    if let Some(slot) = state
        .sub_cursor_indices_graphics
        .get_mut(MAX_FPS_VALUE_ROW_INDEX)
    {
        *slot = clamped;
    }
}

#[inline(always)]
fn graphics_show_software_threads(state: &State) -> bool {
    selected_video_renderer(state) == BackendType::Software
}

#[inline(always)]
fn graphics_show_present_mode(state: &State) -> bool {
    state
        .sub_choice_indices_graphics
        .get(VSYNC_ROW_INDEX)
        .copied()
        .is_some_and(|idx| !yes_no_from_choice(idx))
}

#[inline(always)]
fn graphics_show_max_fps(state: &State) -> bool {
    graphics_show_present_mode(state)
}

#[inline(always)]
fn max_fps_enabled(state: &State) -> bool {
    state
        .sub_choice_indices_graphics
        .get(MAX_FPS_ENABLED_ROW_INDEX)
        .copied()
        .is_some_and(yes_no_from_choice)
}

#[inline(always)]
fn graphics_show_max_fps_value(state: &State) -> bool {
    graphics_show_max_fps(state) && max_fps_enabled(state)
}

fn submenu_visible_row_indices(
    state: &State,
    kind: SubmenuKind,
    rows: &[SubRow<'_>],
) -> Vec<usize> {
    match kind {
        SubmenuKind::Graphics => {
            let show_sw = graphics_show_software_threads(state);
            let show_present_mode = graphics_show_present_mode(state);
            let show_max_fps = graphics_show_max_fps(state);
            let show_max_fps_value = graphics_show_max_fps_value(state);
            rows.iter()
                .enumerate()
                .filter_map(|(idx, row)| {
                    if row.label == GRAPHICS_ROW_SOFTWARE_THREADS && !show_sw {
                        None
                    } else if row.label == GRAPHICS_ROW_PRESENT_MODE && !show_present_mode {
                        None
                    } else if row.label == GRAPHICS_ROW_MAX_FPS && !show_max_fps {
                        None
                    } else if row.label == GRAPHICS_ROW_MAX_FPS_VALUE && !show_max_fps_value {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        SubmenuKind::Advanced => rows.iter().enumerate().map(|(idx, _)| idx).collect(),
        SubmenuKind::SelectMusic => {
            let show_banners = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_BANNERS_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_banners = yes_no_from_choice(show_banners);
            let show_breakdown = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_breakdown = yes_no_from_choice(show_breakdown);
            let show_previews = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_previews = yes_no_from_choice(show_previews);
            let show_scorebox = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_SCOREBOX_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_scorebox = yes_no_from_choice(show_scorebox);
            rows.iter()
                .enumerate()
                .filter_map(|(idx, _)| {
                    if idx == SELECT_MUSIC_SHOW_VIDEO_BANNERS_ROW_INDEX && !show_banners {
                        None
                    } else if idx == SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX && !show_breakdown {
                        None
                    } else if idx == SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX && !show_previews {
                        None
                    } else if idx == SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX && !show_scorebox {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        SubmenuKind::Machine => {
            let show_preferred_style = state
                .sub_choice_indices_machine
                .get(MACHINE_SELECT_STYLE_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 0;
            let show_preferred_mode = state
                .sub_choice_indices_machine
                .get(MACHINE_SELECT_PLAY_MODE_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 0;
            rows.iter()
                .enumerate()
                .filter_map(|(idx, _)| {
                    if idx == MACHINE_PREFERRED_STYLE_ROW_INDEX && !show_preferred_style {
                        None
                    } else if idx == MACHINE_PREFERRED_MODE_ROW_INDEX && !show_preferred_mode {
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
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => 0,
        WindowsPadBackend::Wgi => 1,
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

fn selected_refresh_rate_millihertz(state: &State) -> u32 {
    let idx = state
        .sub_choice_indices_graphics
        .get(REFRESH_RATE_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    state.refresh_rate_choices.get(idx).copied().unwrap_or(0)
}

fn max_fps_seed_value(state: &State, max_fps: u16) -> u16 {
    if max_fps != 0 {
        return clamped_max_fps(max_fps);
    }

    let selected_refresh_mhz = selected_refresh_rate_millihertz(state);
    let refresh_mhz = if selected_refresh_mhz != 0 {
        selected_refresh_mhz
    } else if let Some(spec) = state.monitor_specs.get(selected_display_monitor(state)) {
        if matches!(selected_display_mode(state), DisplayMode::Fullscreen(_)) {
            let (width, height) = selected_resolution(state);
            display::supported_refresh_rates(Some(spec), width, height)
                .into_iter()
                .max()
                .or_else(|| {
                    spec.modes
                        .iter()
                        .map(|mode| mode.refresh_rate_millihertz)
                        .max()
                })
                .unwrap_or(60_000)
        } else {
            spec.modes
                .iter()
                .map(|mode| mode.refresh_rate_millihertz)
                .max()
                .unwrap_or(60_000)
        }
    } else {
        60_000
    };

    clamped_max_fps(((refresh_mhz + 500) / 1000) as u16)
}

fn seed_max_fps_value_choice(state: &mut State, max_fps: u16) {
    let seeded = max_fps_seed_value(state, max_fps);
    let idx = max_fps_choice_index(&state.max_fps_choices, seeded);
    set_max_fps_value_choice_index(state, idx);
}

fn selected_max_fps(state: &State) -> u16 {
    if !max_fps_enabled(state) {
        return 0;
    }
    let idx = state
        .sub_choice_indices_graphics
        .get(MAX_FPS_VALUE_ROW_INDEX)
        .copied()
        .unwrap_or(0);
    max_fps_from_choice(&state.max_fps_choices, idx)
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
    if state.max_fps_at_load == 0 && !max_fps_enabled(state) {
        seed_max_fps_value_choice(state, 0);
    }
    clear_render_cache(state);
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
    if state.max_fps_at_load == 0 && !max_fps_enabled(state) {
        seed_max_fps_value_choice(state, 0);
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
    yes_no_from_choice(
        state
            .sub_choice_indices_score_import
            .get(SCORE_IMPORT_ROW_ONLY_MISSING_INDEX)
            .copied()
            .unwrap_or_else(|| yes_no_choice_index(false)),
    )
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
        if row.label == GRAPHICS_ROW_MAX_FPS_VALUE {
            return state
                .max_fps_labels
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
        && matches!(kind, SubmenuKind::Sound)
    {
        if row.label == SOUND_ROW_DEVICE {
            return state
                .sound_device_options
                .iter()
                .map(|opt| Cow::Owned(opt.label.clone()))
                .collect();
        }
        if row.label == SOUND_ROW_SAMPLE_RATE {
            return sound_sample_rate_choices(state)
                .into_iter()
                .map(|rate| match rate {
                    None => Cow::Borrowed("Auto"),
                    Some(hz) => Cow::Owned(format!("{hz} Hz")),
                })
                .collect();
        }
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

fn submenu_display_choice_texts<'a>(
    state: &'a State,
    kind: SubmenuKind,
    rows: &'a [SubRow<'a>],
    row_idx: usize,
) -> Vec<Cow<'a, str>> {
    let mut choice_texts = row_choices(state, kind, rows, row_idx);
    let Some(row) = rows.get(row_idx) else {
        return choice_texts;
    };
    if choice_texts.is_empty() {
        return choice_texts;
    }
    if row.label == SOUND_ROW_GLOBAL_OFFSET {
        choice_texts[0] = Cow::Owned(format_ms(state.global_offset_ms));
    } else if row.label == SOUND_ROW_MASTER_VOLUME {
        choice_texts[0] = Cow::Owned(format_percent(state.master_volume_pct));
    } else if row.label == SOUND_ROW_SFX_VOLUME {
        choice_texts[0] = Cow::Owned(format_percent(state.sfx_volume_pct));
    } else if row.label == SOUND_ROW_ASSIST_TICK_VOLUME {
        choice_texts[0] = Cow::Owned(format_percent(state.assist_tick_volume_pct));
    } else if row.label == SOUND_ROW_MUSIC_VOLUME {
        choice_texts[0] = Cow::Owned(format_percent(state.music_volume_pct));
    } else if row.label == "Visual Delay (ms)" {
        choice_texts[0] = Cow::Owned(format_ms(state.visual_delay_ms));
    } else if row.label == INPUT_ROW_DEBOUNCE {
        choice_texts[0] = Cow::Owned(format_ms(state.input_debounce_ms));
    }
    choice_texts
}

fn build_submenu_row_layout(
    state: &State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    row_idx: usize,
) -> Option<SubmenuRowLayout> {
    let rows = submenu_rows(kind);
    let Some(row) = rows.get(row_idx) else {
        return None;
    };
    let choice_texts = submenu_display_choice_texts(state, kind, rows, row_idx);
    if choice_texts.is_empty() {
        return None;
    }
    let value_zoom = 0.835_f32;
    let texts: Vec<Arc<str>> = choice_texts
        .iter()
        .map(|text| Arc::<str>::from(text.as_ref()))
        .collect();
    let mut widths: Vec<f32> = Vec::with_capacity(choice_texts.len());
    let mut text_h = 16.0_f32;
    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |metrics_font| {
            text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
            for text in &texts {
                let mut w =
                    font::measure_line_width_logical(metrics_font, text.as_ref(), all_fonts) as f32;
                if !w.is_finite() || w <= 0.0 {
                    w = 1.0;
                }
                widths.push(w * value_zoom);
            }
        });
    });
    let inline_row = row.inline && submenu_inline_widths_fit(&widths);
    let mut x_positions: Vec<f32> = Vec::new();
    let mut centers: Vec<f32> = Vec::new();
    if inline_row {
        x_positions = Vec::with_capacity(widths.len());
        centers = Vec::with_capacity(widths.len());
        let mut x = 0.0_f32;
        for &draw_w in &widths {
            x_positions.push(x);
            centers.push(draw_w.mul_add(0.5, x));
            x += draw_w + INLINE_SPACING;
        }
    }
    Some(SubmenuRowLayout {
        texts: Arc::from(texts),
        widths: Arc::from(widths),
        x_positions: Arc::from(x_positions),
        centers: Arc::from(centers),
        text_h,
        inline_row,
    })
}

fn submenu_row_layout(
    state: &State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    row_idx: usize,
) -> Option<SubmenuRowLayout> {
    let rows = submenu_rows(kind);
    let mut cache = state.submenu_row_layout_cache.borrow_mut();
    if state.submenu_layout_cache_kind.get() != Some(kind) || cache.len() != rows.len() {
        state.submenu_layout_cache_kind.set(Some(kind));
        cache.clear();
        cache.resize(rows.len(), None);
    }
    if let Some(layout) = cache.get(row_idx).and_then(|entry| entry.clone()) {
        return Some(layout);
    }
    let layout = build_submenu_row_layout(state, asset_manager, kind, row_idx)?;
    if row_idx < cache.len() {
        cache[row_idx] = Some(layout.clone());
    }
    Some(layout)
}

pub fn clear_submenu_row_layout_cache(state: &State) {
    state.submenu_layout_cache_kind.set(None);
    let mut cache = state.submenu_row_layout_cache.borrow_mut();
    cache.clear();
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
    let Some(layout) = submenu_row_layout(state, asset_manager, kind, row_idx) else {
        return;
    };
    if !layout.inline_row || layout.centers.is_empty() {
        return;
    }
    let choice_idx = submenu_choice_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(layout.centers.len().saturating_sub(1));
    state.sub_inline_x = layout.centers[choice_idx];
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
    let Some(layout) = submenu_row_layout(state, asset_manager, kind, row_idx) else {
        return;
    };
    if !layout.inline_row || layout.centers.is_empty() {
        return;
    }
    let choice_idx = submenu_choice_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(layout.centers.len().saturating_sub(1));
    if let Some(slot) = submenu_cursor_indices_mut(state, kind).get_mut(row_idx) {
        *slot = choice_idx;
    }
    state.sub_inline_x = layout.centers[choice_idx];
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

fn sound_row_index(label: &str) -> Option<usize> {
    SOUND_OPTIONS_ROWS.iter().position(|row| row.label == label)
}

fn selected_sound_device_choice(state: &State) -> usize {
    sound_row_index(SOUND_ROW_DEVICE)
        .and_then(|idx| state.sub_choice_indices_sound.get(idx).copied())
        .unwrap_or(0)
}

fn sound_sample_rate_choices(state: &State) -> Vec<Option<u32>> {
    let mut choices = Vec::new();
    choices.push(None);
    let device_idx =
        selected_sound_device_choice(state).min(state.sound_device_options.len().saturating_sub(1));
    if let Some(option) = state.sound_device_options.get(device_idx) {
        for &hz in &option.sample_rates_hz {
            let rate = Some(hz);
            if !choices.contains(&rate) {
                choices.push(rate);
            }
        }
    }
    if choices.len() == 1 {
        choices.push(Some(44100));
        choices.push(Some(48000));
    }
    choices
}

fn sound_device_choice_index(options: &[SoundDeviceOption], config_index: Option<u16>) -> usize {
    let Some(target) = config_index else {
        return 0;
    };
    options
        .iter()
        .position(|opt| opt.config_index == Some(target))
        .unwrap_or(0)
}

fn sound_device_from_choice(state: &State, idx: usize) -> Option<u16> {
    state
        .sound_device_options
        .get(idx)
        .and_then(|opt| opt.config_index)
}

fn audio_output_mode_choice_index(mode: config::AudioOutputMode) -> usize {
    match mode {
        config::AudioOutputMode::Auto => 0,
        config::AudioOutputMode::Shared => 1,
        config::AudioOutputMode::Exclusive => 2,
    }
}

fn audio_output_mode_from_choice(idx: usize) -> config::AudioOutputMode {
    match idx {
        1 => config::AudioOutputMode::Shared,
        2 => config::AudioOutputMode::Exclusive,
        _ => config::AudioOutputMode::Auto,
    }
}

#[cfg(target_os = "linux")]
fn linux_audio_backend_choice_index(backend: config::LinuxAudioBackend) -> usize {
    let target = match backend {
        config::LinuxAudioBackend::Auto => "Auto",
        config::LinuxAudioBackend::PipeWire => "PipeWire",
        config::LinuxAudioBackend::PulseAudio => "PulseAudio",
        config::LinuxAudioBackend::Jack => "JACK",
        config::LinuxAudioBackend::Alsa => "ALSA",
    };
    SOUND_LINUX_BACKEND_CHOICES
        .iter()
        .position(|&choice| choice == target)
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn linux_audio_backend_from_choice(idx: usize) -> config::LinuxAudioBackend {
    match SOUND_LINUX_BACKEND_CHOICES
        .get(idx)
        .copied()
        .unwrap_or("Auto")
    {
        "PipeWire" => config::LinuxAudioBackend::PipeWire,
        "PulseAudio" => config::LinuxAudioBackend::PulseAudio,
        "JACK" => config::LinuxAudioBackend::Jack,
        "ALSA" => config::LinuxAudioBackend::Alsa,
        _ => config::LinuxAudioBackend::Auto,
    }
}

fn set_sound_choice_index(state: &mut State, label: &str, idx: usize) {
    let Some(row_idx) = sound_row_index(label) else {
        return;
    };
    if let Some(slot) = state.sub_choice_indices_sound.get_mut(row_idx) {
        *slot = idx;
    }
    if let Some(slot) = state.sub_cursor_indices_sound.get_mut(row_idx) {
        *slot = idx;
    }
}

fn sample_rate_choice_index(state: &State, rate: Option<u32>) -> usize {
    sound_sample_rate_choices(state)
        .iter()
        .position(|&r| r == rate)
        .unwrap_or(0)
}

fn sample_rate_from_choice(state: &State, idx: usize) -> Option<u32> {
    sound_sample_rate_choices(state).get(idx).copied().flatten()
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

#[inline(always)]
const fn scorebox_cycle_mask(itg: bool, ex: bool, hard_ex: bool, tournaments: bool) -> u8 {
    (itg as u8) | ((ex as u8) << 1) | ((hard_ex as u8) << 2) | ((tournaments as u8) << 3)
}

#[inline(always)]
const fn scorebox_cycle_cursor_index(
    itg: bool,
    ex: bool,
    hard_ex: bool,
    tournaments: bool,
) -> usize {
    if itg {
        0
    } else if ex {
        1
    } else if hard_ex {
        2
    } else if tournaments {
        3
    } else {
        0
    }
}

#[inline(always)]
const fn scorebox_cycle_bit_from_choice(idx: usize) -> u8 {
    if idx < SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES {
        1u8 << (idx as u8)
    } else {
        0
    }
}

#[inline(always)]
const fn scorebox_cycle_mask_from_config(cfg: &config::Config) -> u8 {
    scorebox_cycle_mask(
        cfg.select_music_scorebox_cycle_itg,
        cfg.select_music_scorebox_cycle_ex,
        cfg.select_music_scorebox_cycle_hard_ex,
        cfg.select_music_scorebox_cycle_tournaments,
    )
}

#[inline(always)]
fn apply_scorebox_cycle_mask(mask: u8) {
    config::update_select_music_scorebox_cycle_itg((mask & (1u8 << 0)) != 0);
    config::update_select_music_scorebox_cycle_ex((mask & (1u8 << 1)) != 0);
    config::update_select_music_scorebox_cycle_hard_ex((mask & (1u8 << 2)) != 0);
    config::update_select_music_scorebox_cycle_tournaments((mask & (1u8 << 3)) != 0);
}

fn toggle_select_music_scorebox_cycle_option(state: &mut State, choice_idx: usize) {
    let bit = scorebox_cycle_bit_from_choice(choice_idx);
    if bit == 0 {
        return;
    }
    let mut mask = scorebox_cycle_mask_from_config(&config::get());
    if (mask & bit) != 0 {
        mask &= !bit;
    } else {
        mask |= bit;
    }
    apply_scorebox_cycle_mask(mask);

    let clamped = choice_idx.min(SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES.saturating_sub(1));
    if let Some(slot) = state
        .sub_choice_indices_select_music
        .get_mut(SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX)
    {
        *slot = clamped;
    }
    if let Some(slot) = state
        .sub_cursor_indices_select_music
        .get_mut(SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX)
    {
        *slot = clamped;
    }
    audio::play_sfx("assets/sounds/change_value.ogg");
}

#[inline(always)]
fn select_music_scorebox_cycle_enabled_mask() -> u8 {
    scorebox_cycle_mask_from_config(&config::get())
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

const fn sync_graph_mode_choice_index(mode: SyncGraphMode) -> usize {
    match mode {
        SyncGraphMode::Frequency => 0,
        SyncGraphMode::BeatIndex => 1,
        SyncGraphMode::PostKernelFingerprint => 2,
    }
}

const fn sync_graph_mode_from_choice(idx: usize) -> SyncGraphMode {
    match idx {
        0 => SyncGraphMode::Frequency,
        1 => SyncGraphMode::BeatIndex,
        _ => SyncGraphMode::PostKernelFingerprint,
    }
}

const fn yes_no_choice_index(enabled: bool) -> usize {
    if enabled { 1 } else { 0 }
}

const fn yes_no_from_choice(idx: usize) -> bool {
    idx == 1
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

const fn machine_preferred_style_choice_index(style: MachinePreferredPlayStyle) -> usize {
    match style {
        MachinePreferredPlayStyle::Single => 0,
        MachinePreferredPlayStyle::Versus => 1,
        MachinePreferredPlayStyle::Double => 2,
    }
}

const fn machine_preferred_style_from_choice(idx: usize) -> MachinePreferredPlayStyle {
    match idx {
        1 => MachinePreferredPlayStyle::Versus,
        2 => MachinePreferredPlayStyle::Double,
        _ => MachinePreferredPlayStyle::Single,
    }
}

const fn machine_preferred_mode_choice_index(mode: MachinePreferredPlayMode) -> usize {
    match mode {
        MachinePreferredPlayMode::Regular => 0,
        MachinePreferredPlayMode::Marathon => 1,
    }
}

const fn machine_preferred_mode_from_choice(idx: usize) -> MachinePreferredPlayMode {
    match idx {
        1 => MachinePreferredPlayMode::Marathon,
        _ => MachinePreferredPlayMode::Regular,
    }
}

const fn log_level_choice_index(level: LogLevel) -> usize {
    match level {
        LogLevel::Error => 0,
        LogLevel::Warn => 1,
        LogLevel::Info => 2,
        LogLevel::Debug => 3,
        LogLevel::Trace => 4,
    }
}

const fn log_level_from_choice(idx: usize) -> LogLevel {
    match idx {
        0 => LogLevel::Error,
        1 => LogLevel::Warn,
        2 => LogLevel::Info,
        3 => LogLevel::Debug,
        _ => LogLevel::Trace,
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
    pending_dedicated_menu_buttons: Option<bool>,
    // Submenu state
    sub_selected: usize,
    sub_prev_selected: usize,
    sub_inline_x: f32,
    sub_choice_indices_system: Vec<usize>,
    sub_choice_indices_graphics: Vec<usize>,
    sub_choice_indices_input: Vec<usize>,
    sub_choice_indices_input_backend: Vec<usize>,
    sub_choice_indices_online_scoring: Vec<usize>,
    sub_choice_indices_machine: Vec<usize>,
    sub_choice_indices_advanced: Vec<usize>,
    sub_choice_indices_course: Vec<usize>,
    sub_choice_indices_gameplay: Vec<usize>,
    sub_choice_indices_sound: Vec<usize>,
    sub_choice_indices_select_music: Vec<usize>,
    sub_choice_indices_groovestats: Vec<usize>,
    sub_choice_indices_arrowcloud: Vec<usize>,
    sub_choice_indices_score_import: Vec<usize>,
    system_noteskin_choices: Vec<String>,
    sub_cursor_indices_system: Vec<usize>,
    sub_cursor_indices_graphics: Vec<usize>,
    sub_cursor_indices_input: Vec<usize>,
    sub_cursor_indices_input_backend: Vec<usize>,
    sub_cursor_indices_online_scoring: Vec<usize>,
    sub_cursor_indices_machine: Vec<usize>,
    sub_cursor_indices_advanced: Vec<usize>,
    sub_cursor_indices_course: Vec<usize>,
    sub_cursor_indices_gameplay: Vec<usize>,
    sub_cursor_indices_sound: Vec<usize>,
    sub_cursor_indices_select_music: Vec<usize>,
    sub_cursor_indices_groovestats: Vec<usize>,
    sub_cursor_indices_arrowcloud: Vec<usize>,
    sub_cursor_indices_score_import: Vec<usize>,
    score_import_profiles: Vec<ScoreImportProfileConfig>,
    score_import_profile_choices: Vec<String>,
    score_import_profile_ids: Vec<Option<String>>,
    score_import_pack_choices: Vec<String>,
    score_import_pack_filters: Vec<Option<String>>,
    sound_device_options: Vec<SoundDeviceOption>,
    master_volume_pct: i32,
    sfx_volume_pct: i32,
    assist_tick_volume_pct: i32,
    music_volume_pct: i32,
    global_offset_ms: i32,
    visual_delay_ms: i32,
    input_debounce_ms: i32,
    video_renderer_at_load: BackendType,
    display_mode_at_load: DisplayMode,
    display_monitor_at_load: usize,
    display_width_at_load: u32,
    display_height_at_load: u32,
    max_fps_at_load: u16,
    vsync_at_load: bool,
    present_mode_policy_at_load: PresentModePolicy,
    display_mode_choices: Vec<String>,
    software_thread_choices: Vec<u8>,
    software_thread_labels: Vec<String>,
    max_fps_choices: Vec<u16>,
    max_fps_labels: Vec<String>,
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
    submenu_layout_cache_kind: Cell<Option<SubmenuKind>>,
    submenu_row_layout_cache: RefCell<Vec<Option<SubmenuRowLayout>>>,
    description_layout_cache: RefCell<Option<DescriptionLayout>>,
    graphics_prev_visible_rows: Vec<usize>,
    advanced_prev_visible_rows: Vec<usize>,
    select_music_prev_visible_rows: Vec<usize>,
}

pub fn init() -> State {
    let cfg = config::get();
    let system_noteskin_choices = discover_system_noteskin_choices();
    let software_thread_choices = build_software_thread_choices();
    let software_thread_labels = software_thread_choice_labels(&software_thread_choices);
    let max_fps_choices = build_max_fps_choices();
    let max_fps_labels = max_fps_choice_labels(&max_fps_choices);
    let sound_device_options = build_sound_device_options();
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
        pending_dedicated_menu_buttons: None,
        view: OptionsView::Main,
        sub_selected: 0,
        sub_prev_selected: 0,
        sub_inline_x: f32::NAN,
        sub_choice_indices_system: vec![0; SYSTEM_OPTIONS_ROWS.len()],
        sub_choice_indices_graphics: vec![0; GRAPHICS_OPTIONS_ROWS.len()],
        sub_choice_indices_input: vec![0; INPUT_OPTIONS_ROWS.len()],
        sub_choice_indices_input_backend: vec![0; INPUT_BACKEND_OPTIONS_ROWS.len()],
        sub_choice_indices_online_scoring: vec![0; ONLINE_SCORING_OPTIONS_ROWS.len()],
        sub_choice_indices_machine: vec![0; MACHINE_OPTIONS_ROWS.len()],
        sub_choice_indices_advanced: vec![0; ADVANCED_OPTIONS_ROWS.len()],
        sub_choice_indices_course: vec![0; COURSE_OPTIONS_ROWS.len()],
        sub_choice_indices_gameplay: vec![0; GAMEPLAY_OPTIONS_ROWS.len()],
        sub_choice_indices_sound: vec![0; SOUND_OPTIONS_ROWS.len()],
        sub_choice_indices_select_music: vec![0; SELECT_MUSIC_OPTIONS_ROWS.len()],
        sub_choice_indices_groovestats: vec![0; GROOVESTATS_OPTIONS_ROWS.len()],
        sub_choice_indices_arrowcloud: vec![0; ARROWCLOUD_OPTIONS_ROWS.len()],
        sub_choice_indices_score_import: vec![0; SCORE_IMPORT_OPTIONS_ROWS.len()],
        system_noteskin_choices,
        sub_cursor_indices_system: vec![0; SYSTEM_OPTIONS_ROWS.len()],
        sub_cursor_indices_graphics: vec![0; GRAPHICS_OPTIONS_ROWS.len()],
        sub_cursor_indices_input: vec![0; INPUT_OPTIONS_ROWS.len()],
        sub_cursor_indices_input_backend: vec![0; INPUT_BACKEND_OPTIONS_ROWS.len()],
        sub_cursor_indices_online_scoring: vec![0; ONLINE_SCORING_OPTIONS_ROWS.len()],
        sub_cursor_indices_machine: vec![0; MACHINE_OPTIONS_ROWS.len()],
        sub_cursor_indices_advanced: vec![0; ADVANCED_OPTIONS_ROWS.len()],
        sub_cursor_indices_course: vec![0; COURSE_OPTIONS_ROWS.len()],
        sub_cursor_indices_gameplay: vec![0; GAMEPLAY_OPTIONS_ROWS.len()],
        sub_cursor_indices_sound: vec![0; SOUND_OPTIONS_ROWS.len()],
        sub_cursor_indices_select_music: vec![0; SELECT_MUSIC_OPTIONS_ROWS.len()],
        sub_cursor_indices_groovestats: vec![0; GROOVESTATS_OPTIONS_ROWS.len()],
        sub_cursor_indices_arrowcloud: vec![0; ARROWCLOUD_OPTIONS_ROWS.len()],
        sub_cursor_indices_score_import: vec![0; SCORE_IMPORT_OPTIONS_ROWS.len()],
        score_import_profiles: Vec::new(),
        score_import_profile_choices: vec!["No eligible profiles".to_string()],
        score_import_profile_ids: vec![None],
        score_import_pack_choices: vec![SCORE_IMPORT_ALL_PACKS.to_string()],
        score_import_pack_filters: vec![None],
        sound_device_options,
        master_volume_pct: i32::from(cfg.master_volume.clamp(0, 100)),
        sfx_volume_pct: i32::from(cfg.sfx_volume.clamp(0, 100)),
        assist_tick_volume_pct: i32::from(cfg.assist_tick_volume.clamp(0, 100)),
        music_volume_pct: i32::from(cfg.music_volume.clamp(0, 100)),
        global_offset_ms: {
            let ms = (cfg.global_offset_seconds * 1000.0).round() as i32;
            ms.clamp(GLOBAL_OFFSET_MIN_MS, GLOBAL_OFFSET_MAX_MS)
        },
        visual_delay_ms: {
            let ms = (cfg.visual_delay_seconds * 1000.0).round() as i32;
            ms.clamp(VISUAL_DELAY_MIN_MS, VISUAL_DELAY_MAX_MS)
        },
        input_debounce_ms: {
            let ms = (cfg.input_debounce_seconds * 1000.0).round() as i32;
            ms.clamp(INPUT_DEBOUNCE_MIN_MS, INPUT_DEBOUNCE_MAX_MS)
        },
        video_renderer_at_load: cfg.video_renderer,
        display_mode_at_load: cfg.display_mode(),
        display_monitor_at_load: cfg.display_monitor,
        display_width_at_load: cfg.display_width,
        display_height_at_load: cfg.display_height,
        max_fps_at_load: cfg.max_fps,
        vsync_at_load: cfg.vsync,
        present_mode_policy_at_load: cfg.present_mode_policy,
        display_mode_choices: build_display_mode_choices(&[]),
        software_thread_choices,
        software_thread_labels,
        max_fps_choices,
        max_fps_labels,
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
        submenu_layout_cache_kind: Cell::new(None),
        submenu_row_layout_cache: RefCell::new(Vec::new()),
        description_layout_cache: RefCell::new(None),
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
    set_choice_by_label(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        "Log Level",
        log_level_choice_index(cfg.log_level),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        SYSTEM_ROW_LOG_FILE,
        usize::from(cfg.log_to_file),
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
        yes_no_choice_index(cfg.vsync),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        GRAPHICS_ROW_PRESENT_MODE,
        present_mode_choice_index(cfg.present_mode_policy),
    );
    sync_max_fps(&mut state, cfg.max_fps);
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        "Show Stats",
        cfg.show_stats_mode.min(3) as usize,
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        GRAPHICS_ROW_VALIDATION_LAYERS,
        yes_no_choice_index(cfg.gfx_debug),
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
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        INPUT_ROW_DEDICATED_MENU_BUTTONS,
        usize::from(cfg.only_dedicated_menu_buttons),
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
        MACHINE_ROW_PREFERRED_STYLE,
        machine_preferred_style_choice_index(cfg.machine_preferred_style),
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
        MACHINE_ROW_PREFERRED_MODE,
        machine_preferred_mode_choice_index(cfg.machine_preferred_play_mode),
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
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        MACHINE_ROW_VIDEO_BGS,
        usize::from(cfg.show_video_backgrounds),
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
        ADVANCED_ROW_CDTITLE_CACHE,
        usize::from(cfg.cdtitle_cache),
    );
    if let Some(slot) = state
        .sub_choice_indices_advanced
        .get_mut(ADVANCED_SONG_PARSING_THREADS_ROW_INDEX)
    {
        *slot =
            software_thread_choice_index(&state.software_thread_choices, cfg.song_parsing_threads);
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
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        ADVANCED_ROW_SYNC_GRAPH,
        sync_graph_mode_choice_index(cfg.null_or_die_sync_graph),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        COURSE_ROW_SHOW_RANDOM,
        yes_no_choice_index(cfg.show_random_courses),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        COURSE_ROW_SHOW_MOST_PLAYED,
        yes_no_choice_index(cfg.show_most_played_courses),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        COURSE_ROW_SHOW_INDIVIDUAL_SCORES,
        yes_no_choice_index(cfg.show_course_individual_scores),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        COURSE_ROW_AUTOSUBMIT_INDIVIDUAL_SCORES,
        yes_no_choice_index(cfg.autosubmit_course_scores_individually),
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
        SOUND_ROW_MASTER_VOLUME,
        master_volume_choice_index(cfg.master_volume),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SOUND_ROW_SFX_VOLUME,
        master_volume_choice_index(cfg.sfx_volume),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SOUND_ROW_ASSIST_TICK_VOLUME,
        master_volume_choice_index(cfg.assist_tick_volume),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SOUND_ROW_MUSIC_VOLUME,
        master_volume_choice_index(cfg.music_volume),
    );
    let sound_device_idx =
        sound_device_choice_index(&state.sound_device_options, cfg.audio_output_device_index);
    set_sound_choice_index(&mut state, SOUND_ROW_DEVICE, sound_device_idx);
    set_sound_choice_index(
        &mut state,
        SOUND_ROW_OUTPUT_MODE,
        audio_output_mode_choice_index(cfg.audio_output_mode),
    );
    #[cfg(target_os = "linux")]
    set_sound_choice_index(
        &mut state,
        SOUND_ROW_LINUX_BACKEND,
        linux_audio_backend_choice_index(cfg.linux_audio_backend),
    );
    let sound_rate_idx = sample_rate_choice_index(&state, cfg.audio_sample_rate_hz);
    set_sound_choice_index(&mut state, SOUND_ROW_SAMPLE_RATE, sound_rate_idx);
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SOUND_ROW_MINE_SOUNDS,
        usize::from(cfg.mine_hit_sound),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SOUND_ROW_RATEMOD_PITCH,
        usize::from(cfg.rate_mod_preserves_pitch),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_BANNERS,
        yes_no_choice_index(cfg.show_select_music_banners),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_VIDEO_BANNERS,
        yes_no_choice_index(cfg.show_select_music_video_banners),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_BREAKDOWN,
        yes_no_choice_index(cfg.show_select_music_breakdown),
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
        yes_no_choice_index(cfg.show_select_music_cdtitles),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_WHEEL_GRADES,
        yes_no_choice_index(cfg.show_music_wheel_grades),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_WHEEL_LAMPS,
        yes_no_choice_index(cfg.show_music_wheel_lamps),
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
        yes_no_choice_index(cfg.show_select_music_previews),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_PREVIEW_MARKER,
        yes_no_choice_index(cfg.show_select_music_preview_marker),
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
        yes_no_choice_index(cfg.show_select_music_gameplay_timer),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SHOW_RIVALS,
        yes_no_choice_index(cfg.show_select_music_scorebox),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SELECT_MUSIC_ROW_SCOREBOX_CYCLE,
        scorebox_cycle_cursor_index(
            cfg.select_music_scorebox_cycle_itg,
            cfg.select_music_scorebox_cycle_ex,
            cfg.select_music_scorebox_cycle_hard_ex,
            cfg.select_music_scorebox_cycle_tournaments,
        ),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        GS_ROW_ENABLE,
        yes_no_choice_index(cfg.enable_groovestats),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        GS_ROW_ENABLE_BOOGIE,
        yes_no_choice_index(cfg.enable_boogiestats),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        GS_ROW_AUTO_POPULATE,
        yes_no_choice_index(cfg.auto_populate_gs_scores),
    );
    set_choice_by_label(
        &mut state.sub_choice_indices_arrowcloud,
        ARROWCLOUD_OPTIONS_ROWS,
        ARROWCLOUD_ROW_ENABLE,
        yes_no_choice_index(cfg.enable_arrowcloud),
    );
    refresh_score_import_options(&mut state);
    set_choice_by_label(
        &mut state.sub_choice_indices_score_import,
        SCORE_IMPORT_OPTIONS_ROWS,
        SCORE_IMPORT_ROW_ONLY_MISSING,
        yes_no_choice_index(false),
    );
    sync_submenu_cursor_indices(&mut state);
    state
}

pub fn open_input_submenu(state: &mut State) {
    state.view = OptionsView::Submenu(SubmenuKind::Input);
    state.pending_submenu_kind = None;
    state.pending_submenu_parent_kind = None;
    state.submenu_parent_kind = None;
    state.submenu_transition = SubmenuTransition::None;
    state.submenu_fade_t = 0.0;
    state.content_alpha = 1.0;
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
    clear_navigation_holds(state);
    clear_render_cache(state);
}

fn submenu_choice_indices(state: &State, kind: SubmenuKind) -> &[usize] {
    match kind {
        SubmenuKind::System => &state.sub_choice_indices_system,
        SubmenuKind::Graphics => &state.sub_choice_indices_graphics,
        SubmenuKind::Input => &state.sub_choice_indices_input,
        SubmenuKind::InputBackend => &state.sub_choice_indices_input_backend,
        SubmenuKind::OnlineScoring => &state.sub_choice_indices_online_scoring,
        SubmenuKind::Machine => &state.sub_choice_indices_machine,
        SubmenuKind::Advanced => &state.sub_choice_indices_advanced,
        SubmenuKind::Course => &state.sub_choice_indices_course,
        SubmenuKind::Gameplay => &state.sub_choice_indices_gameplay,
        SubmenuKind::Sound => &state.sub_choice_indices_sound,
        SubmenuKind::SelectMusic => &state.sub_choice_indices_select_music,
        SubmenuKind::GrooveStats => &state.sub_choice_indices_groovestats,
        SubmenuKind::ArrowCloud => &state.sub_choice_indices_arrowcloud,
        SubmenuKind::ScoreImport => &state.sub_choice_indices_score_import,
    }
}

const fn submenu_choice_indices_mut(state: &mut State, kind: SubmenuKind) -> &mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_choice_indices_system,
        SubmenuKind::Graphics => &mut state.sub_choice_indices_graphics,
        SubmenuKind::Input => &mut state.sub_choice_indices_input,
        SubmenuKind::InputBackend => &mut state.sub_choice_indices_input_backend,
        SubmenuKind::OnlineScoring => &mut state.sub_choice_indices_online_scoring,
        SubmenuKind::Machine => &mut state.sub_choice_indices_machine,
        SubmenuKind::Advanced => &mut state.sub_choice_indices_advanced,
        SubmenuKind::Course => &mut state.sub_choice_indices_course,
        SubmenuKind::Gameplay => &mut state.sub_choice_indices_gameplay,
        SubmenuKind::Sound => &mut state.sub_choice_indices_sound,
        SubmenuKind::SelectMusic => &mut state.sub_choice_indices_select_music,
        SubmenuKind::GrooveStats => &mut state.sub_choice_indices_groovestats,
        SubmenuKind::ArrowCloud => &mut state.sub_choice_indices_arrowcloud,
        SubmenuKind::ScoreImport => &mut state.sub_choice_indices_score_import,
    }
}

fn submenu_cursor_indices(state: &State, kind: SubmenuKind) -> &[usize] {
    match kind {
        SubmenuKind::System => &state.sub_cursor_indices_system,
        SubmenuKind::Graphics => &state.sub_cursor_indices_graphics,
        SubmenuKind::Input => &state.sub_cursor_indices_input,
        SubmenuKind::InputBackend => &state.sub_cursor_indices_input_backend,
        SubmenuKind::OnlineScoring => &state.sub_cursor_indices_online_scoring,
        SubmenuKind::Machine => &state.sub_cursor_indices_machine,
        SubmenuKind::Advanced => &state.sub_cursor_indices_advanced,
        SubmenuKind::Course => &state.sub_cursor_indices_course,
        SubmenuKind::Gameplay => &state.sub_cursor_indices_gameplay,
        SubmenuKind::Sound => &state.sub_cursor_indices_sound,
        SubmenuKind::SelectMusic => &state.sub_cursor_indices_select_music,
        SubmenuKind::GrooveStats => &state.sub_cursor_indices_groovestats,
        SubmenuKind::ArrowCloud => &state.sub_cursor_indices_arrowcloud,
        SubmenuKind::ScoreImport => &state.sub_cursor_indices_score_import,
    }
}

const fn submenu_cursor_indices_mut(state: &mut State, kind: SubmenuKind) -> &mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_cursor_indices_system,
        SubmenuKind::Graphics => &mut state.sub_cursor_indices_graphics,
        SubmenuKind::Input => &mut state.sub_cursor_indices_input,
        SubmenuKind::InputBackend => &mut state.sub_cursor_indices_input_backend,
        SubmenuKind::OnlineScoring => &mut state.sub_cursor_indices_online_scoring,
        SubmenuKind::Machine => &mut state.sub_cursor_indices_machine,
        SubmenuKind::Advanced => &mut state.sub_cursor_indices_advanced,
        SubmenuKind::Course => &mut state.sub_cursor_indices_course,
        SubmenuKind::Gameplay => &mut state.sub_cursor_indices_gameplay,
        SubmenuKind::Sound => &mut state.sub_cursor_indices_sound,
        SubmenuKind::SelectMusic => &mut state.sub_cursor_indices_select_music,
        SubmenuKind::GrooveStats => &mut state.sub_cursor_indices_groovestats,
        SubmenuKind::ArrowCloud => &mut state.sub_cursor_indices_arrowcloud,
        SubmenuKind::ScoreImport => &mut state.sub_cursor_indices_score_import,
    }
}

fn sync_submenu_cursor_indices(state: &mut State) {
    state.sub_cursor_indices_system = state.sub_choice_indices_system.clone();
    state.sub_cursor_indices_graphics = state.sub_choice_indices_graphics.clone();
    state.sub_cursor_indices_input = state.sub_choice_indices_input.clone();
    state.sub_cursor_indices_input_backend = state.sub_choice_indices_input_backend.clone();
    state.sub_cursor_indices_online_scoring = state.sub_choice_indices_online_scoring.clone();
    state.sub_cursor_indices_machine = state.sub_choice_indices_machine.clone();
    state.sub_cursor_indices_advanced = state.sub_choice_indices_advanced.clone();
    state.sub_cursor_indices_course = state.sub_choice_indices_course.clone();
    state.sub_cursor_indices_gameplay = state.sub_choice_indices_gameplay.clone();
    state.sub_cursor_indices_sound = state.sub_choice_indices_sound.clone();
    state.sub_cursor_indices_select_music = state.sub_choice_indices_select_music.clone();
    state.sub_cursor_indices_groovestats = state.sub_choice_indices_groovestats.clone();
    state.sub_cursor_indices_arrowcloud = state.sub_choice_indices_arrowcloud.clone();
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
    clear_render_cache(state);
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
    clear_render_cache(state);
}

pub fn sync_display_resolution(state: &mut State, width: u32, height: u32) {
    rebuild_resolution_choices(state, width, height);
    state.display_width_at_load = width;
    state.display_height_at_load = height;
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_show_stats_mode(state: &mut State, mode: u8) {
    set_choice_by_label(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        "Show Stats",
        mode.min(3) as usize,
    );
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_max_fps(state: &mut State, max_fps: u16) {
    let had_explicit_cap = state.max_fps_at_load != 0;
    state.max_fps_at_load = max_fps;
    set_max_fps_enabled_choice(state, max_fps != 0);
    if max_fps != 0 || !had_explicit_cap {
        seed_max_fps_value_choice(state, max_fps);
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_vsync(state: &mut State, enabled: bool) {
    state.vsync_at_load = enabled;
    if let Some(slot) = state.sub_choice_indices_graphics.get_mut(VSYNC_ROW_INDEX) {
        *slot = yes_no_choice_index(enabled);
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_present_mode_policy(state: &mut State, mode: PresentModePolicy) {
    state.present_mode_policy_at_load = mode;
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(PRESENT_MODE_ROW_INDEX)
    {
        *slot = present_mode_choice_index(mode);
    }
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
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

        let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
            let _ = tx.send(ReloadMsg::Song {
                done,
                total,
                pack: pack.to_owned(),
                song: song.to_owned(),
            });
        };
        song_loading::scan_and_load_songs_with_progress_counts("songs", &mut on_song);

        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Courses));

        let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
            let _ = tx.send(ReloadMsg::Course {
                done,
                total,
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        song_loading::scan_and_load_courses_with_progress_counts(
            "courses",
            "songs",
            &mut on_course,
        );

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
            ReloadMsg::Song {
                done,
                total,
                pack,
                song,
            } => {
                reload.phase = ReloadPhase::Songs;
                reload.songs_done = done;
                reload.songs_total = total;
                reload.line2 = pack;
                reload.line3 = song;
            }
            ReloadMsg::Course {
                done,
                total,
                group,
                course,
            } => {
                reload.phase = ReloadPhase::Courses;
                reload.courses_done = done;
                reload.courses_total = total;
                reload.line2 = group;
                reload.line3 = course;
            }
            ReloadMsg::Done => {
                reload.done = true;
            }
        }
    }
}

#[inline(always)]
fn reload_progress(reload: &ReloadUiState) -> (usize, usize, f32) {
    let done = reload.songs_done.saturating_add(reload.courses_done);
    let mut total = reload.songs_total.saturating_add(reload.courses_total);
    if total < done {
        total = done;
    }
    let mut progress = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !reload.done && total > 0 && progress >= 1.0 {
        progress = 0.999;
    }
    (done, total, progress)
}

#[inline(always)]
const fn reload_phase_label(phase: ReloadPhase) -> &'static str {
    match phase {
        ReloadPhase::Songs => "Loading songs...",
        ReloadPhase::Courses => "Loading courses...",
    }
}

fn reload_detail_lines(reload: &ReloadUiState) -> (String, String) {
    (reload.line2.clone(), reload.line3.clone())
}

fn build_reload_overlay_actors(reload: &ReloadUiState, active_color_index: i32) -> Vec<Actor> {
    let (done, total, progress) = reload_progress(reload);
    let elapsed = reload.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        let pct = 100.0 * progress;
        format!("{done}/{total} ({pct:.1}%)")
    };
    let show_speed_row = total > 0;
    let speed_text = if elapsed > 0.0 && show_speed_row {
        format!("Current speed: {:.1} items/s", done as f32 / elapsed)
    } else if show_speed_row {
        "Current speed: 0.0 items/s".to_string()
    } else {
        String::new()
    };
    let (line2, line3) = reload_detail_lines(reload);
    let fill = color::decorative_rgba(active_color_index);

    let bar_w = widescale(360.0, 520.0);
    let bar_h = RELOAD_BAR_H;
    let bar_cx = screen_width() * 0.5;
    let bar_cy = screen_height() * 0.5 + 34.0;
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    let mut out: Vec<Actor> = Vec::with_capacity(7);
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(300)
    ));
    out.push(act!(text:
        font("miso"):
        settext(if total == 0 { "Initializing..." } else { reload_phase_label(reload.phase) }):
        align(0.5, 0.5):
        xy(screen_width() * 0.5, bar_cy - 98.0):
        zoom(1.05):
        horizalign(center):
        z(301)
    ));
    if !line2.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(line2):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 74.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }
    if !line3.is_empty() {
        out.push(act!(text:
            font("miso"):
            settext(line3):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy - 50.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(301)
        ));
    }

    let mut bar_children = Vec::with_capacity(4);
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w, bar_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(0)
    ));
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w - 4.0, bar_h - 4.0):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1)
    ));
    if fill_w > 0.0 {
        bar_children.push(act!(quad:
            align(0.0, 0.5):
            xy(2.0, bar_h / 2.0):
            zoomto(fill_w, bar_h - 4.0):
            diffuse(fill[0], fill[1], fill[2], 1.0):
            z(2)
        ));
    }
    bar_children.push(act!(text:
        font("miso"):
        settext(count_text):
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoom(0.9):
        horizalign(center):
        z(3)
    ));
    out.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        size: [actors::SizeSpec::Px(bar_w), actors::SizeSpec::Px(bar_h)],
        background: None,
        z: 301,
        children: bar_children,
    });

    if show_speed_row {
        out.push(act!(text:
            font("miso"):
            settext(speed_text):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, bar_cy + 36.0):
            zoom(0.9):
            horizalign(center):
            z(301)
        ));
    }
    out
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
                // Apply deferred settings before leaving the submenu.
                if matches!(state.view, OptionsView::Submenu(SubmenuKind::InputBackend)) {
                    if let Some(enabled) = state.pending_dedicated_menu_buttons.take() {
                        config::update_only_dedicated_menu_buttons(enabled);
                    }
                }
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
            let (
                desired_renderer,
                desired_display_mode,
                desired_resolution,
                desired_monitor,
                desired_vsync,
                desired_present_mode_policy,
                desired_max_fps,
            ) = if leaving_graphics {
                let vsync = state
                    .sub_choice_indices_graphics
                    .get(VSYNC_ROW_INDEX)
                    .copied()
                    .map_or(true, |idx| yes_no_from_choice(idx));
                (
                    Some(selected_video_renderer(state)),
                    Some(selected_display_mode(state)),
                    Some(selected_resolution(state)),
                    Some(selected_display_monitor(state)),
                    Some(vsync),
                    Some(selected_present_mode_policy(state)),
                    Some(selected_max_fps(state)),
                )
            } else {
                (None, None, None, None, None, None, None)
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
                let mut vsync_change: Option<bool> = None;
                let mut present_mode_policy_change: Option<PresentModePolicy> = None;
                let mut max_fps_change: Option<u16> = None;

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
                if let Some(vsync) = desired_vsync
                    && vsync != state.vsync_at_load
                {
                    vsync_change = Some(vsync);
                }
                if let Some(policy) = desired_present_mode_policy
                    && policy != state.present_mode_policy_at_load
                {
                    present_mode_policy_change = Some(policy);
                }
                if let Some(max_fps) = desired_max_fps
                    && max_fps != state.max_fps_at_load
                {
                    max_fps_change = Some(max_fps);
                }

                if renderer_change.is_some()
                    || display_mode_change.is_some()
                    || monitor_change.is_some()
                    || resolution_change.is_some()
                    || vsync_change.is_some()
                    || present_mode_policy_change.is_some()
                    || max_fps_change.is_some()
                {
                    pending_action = Some(ScreenAction::ChangeGraphics {
                        renderer: renderer_change,
                        display_mode: display_mode_change,
                        monitor: monitor_change,
                        resolution: resolution_change,
                        vsync: vsync_change,
                        present_mode_policy: present_mode_policy_change,
                        max_fps: max_fps_change,
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
            let list_w = list_w_unscaled() * s;
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
        // Block cycling disabled rows (e.g. dedicated menu buttons when unmapped).
        if is_submenu_row_disabled(kind, row.label) {
            return None;
        }
        if matches!(kind, SubmenuKind::Sound) {
            match row.label {
                SOUND_ROW_MASTER_VOLUME => {
                    if adjust_ms_value(
                        &mut state.master_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_master_volume(state.master_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SOUND_ROW_SFX_VOLUME => {
                    if adjust_ms_value(
                        &mut state.sfx_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_sfx_volume(state.sfx_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SOUND_ROW_ASSIST_TICK_VOLUME => {
                    if adjust_ms_value(
                        &mut state.assist_tick_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_assist_tick_volume(state.assist_tick_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SOUND_ROW_MUSIC_VOLUME => {
                    if adjust_ms_value(
                        &mut state.music_volume_pct,
                        delta,
                        VOLUME_MIN_PERCENT,
                        VOLUME_MAX_PERCENT,
                    ) {
                        config::update_music_volume(state.music_volume_pct as u8);
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                _ => {}
            }
        }
        if matches!(kind, SubmenuKind::Sound) && row.label == SOUND_ROW_GLOBAL_OFFSET {
            if adjust_ms_value(
                &mut state.global_offset_ms,
                delta,
                GLOBAL_OFFSET_MIN_MS,
                GLOBAL_OFFSET_MAX_MS,
            ) {
                config::update_global_offset(state.global_offset_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
                clear_render_cache(state);
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
                clear_render_cache(state);
            }
            return None;
        }
        if matches!(kind, SubmenuKind::InputBackend) && row.label == INPUT_ROW_DEBOUNCE {
            if adjust_ms_value(
                &mut state.input_debounce_ms,
                delta,
                INPUT_DEBOUNCE_MIN_MS,
                INPUT_DEBOUNCE_MAX_MS,
            ) {
                config::update_input_debounce_seconds(state.input_debounce_ms as f32 / 1000.0);
                audio::play_sfx("assets/sounds/change_value.ogg");
                clear_render_cache(state);
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
    if let Some(layout) = submenu_row_layout(state, asset_manager, kind, row_index)
        && layout.inline_row
        && let Some(&x) = layout.centers.get(new_index)
    {
        state.sub_inline_x = x;
    }
    audio::play_sfx("assets/sounds/change_value.ogg");

    if matches!(kind, SubmenuKind::System) {
        let row = &rows[row_index];
        match row.label {
            "Game" => config::update_game_flag(config::GameFlag::Dance),
            "Theme" => config::update_theme_flag(config::ThemeFlag::SimplyLove),
            "Language" => config::update_language_flag(config::LanguageFlag::English),
            "Log Level" => config::update_log_level(log_level_from_choice(new_index)),
            SYSTEM_ROW_LOG_FILE => config::update_log_to_file(new_index == 1),
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
        if row.label == "Refresh Rate" && state.max_fps_at_load == 0 && !max_fps_enabled(state) {
            seed_max_fps_value_choice(state, 0);
        }
        if row.label == GRAPHICS_ROW_MAX_FPS
            && yes_no_from_choice(new_index)
            && state.max_fps_at_load == 0
        {
            seed_max_fps_value_choice(state, 0);
        }
        if row.label == "Show Stats" {
            let mode = new_index.min(3) as u8;
            action = Some(ScreenAction::UpdateShowOverlay(mode));
        }
        if row.label == GRAPHICS_ROW_VALIDATION_LAYERS {
            config::update_gfx_debug(yes_no_from_choice(new_index));
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
        if row.label == INPUT_ROW_DEDICATED_MENU_BUTTONS {
            state.pending_dedicated_menu_buttons = Some(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Machine) {
        let row = &rows[row_index];
        let enabled = new_index == 1;
        match row.label {
            MACHINE_ROW_SELECT_PROFILE => config::update_machine_show_select_profile(enabled),
            MACHINE_ROW_SELECT_COLOR => config::update_machine_show_select_color(enabled),
            MACHINE_ROW_SELECT_STYLE => config::update_machine_show_select_style(enabled),
            MACHINE_ROW_PREFERRED_STYLE => config::update_machine_preferred_style(
                machine_preferred_style_from_choice(new_index),
            ),
            MACHINE_ROW_SELECT_PLAY_MODE => config::update_machine_show_select_play_mode(enabled),
            MACHINE_ROW_PREFERRED_MODE => config::update_machine_preferred_play_mode(
                machine_preferred_mode_from_choice(new_index),
            ),
            MACHINE_ROW_EVAL_SUMMARY => config::update_machine_show_eval_summary(enabled),
            MACHINE_ROW_NAME_ENTRY => config::update_machine_show_name_entry(enabled),
            MACHINE_ROW_GAMEOVER => config::update_machine_show_gameover(enabled),
            MACHINE_ROW_MENU_MUSIC => config::update_menu_music(enabled),
            MACHINE_ROW_KEYBOARD_FEATURES => config::update_keyboard_features(enabled),
            MACHINE_ROW_VIDEO_BGS => config::update_show_video_backgrounds(enabled),
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Advanced) {
        let row = &rows[row_index];
        if row.label == ADVANCED_ROW_DEFAULT_FAIL_TYPE {
            config::update_default_fail_type(default_fail_type_from_choice(new_index));
        } else if row.label == ADVANCED_ROW_BANNER_CACHE {
            config::update_banner_cache(new_index == 1);
        } else if row.label == ADVANCED_ROW_CDTITLE_CACHE {
            config::update_cdtitle_cache(new_index == 1);
        } else if row.label == ADVANCED_ROW_SONG_PARSING_THREADS {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_song_parsing_threads(threads);
        } else if row.label == ADVANCED_ROW_CACHE_SONGS {
            config::update_cache_songs(new_index == 1);
        } else if row.label == ADVANCED_ROW_FAST_LOAD {
            config::update_fastload(new_index == 1);
        } else if row.label == ADVANCED_ROW_SYNC_GRAPH {
            config::update_null_or_die_sync_graph(sync_graph_mode_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::Course) {
        let row = &rows[row_index];
        let enabled = yes_no_from_choice(new_index);
        match row.label {
            COURSE_ROW_SHOW_RANDOM => config::update_show_random_courses(enabled),
            COURSE_ROW_SHOW_MOST_PLAYED => config::update_show_most_played_courses(enabled),
            COURSE_ROW_SHOW_INDIVIDUAL_SCORES => {
                config::update_show_course_individual_scores(enabled)
            }
            COURSE_ROW_AUTOSUBMIT_INDIVIDUAL_SCORES => {
                config::update_autosubmit_course_scores_individually(enabled)
            }
            _ => {}
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
            SOUND_ROW_MASTER_VOLUME => {
                let vol = master_volume_from_choice(new_index);
                config::update_master_volume(vol);
            }
            SOUND_ROW_SFX_VOLUME => {
                let vol = master_volume_from_choice(new_index);
                config::update_sfx_volume(vol);
            }
            SOUND_ROW_ASSIST_TICK_VOLUME => {
                let vol = master_volume_from_choice(new_index);
                config::update_assist_tick_volume(vol);
            }
            SOUND_ROW_MUSIC_VOLUME => {
                let vol = master_volume_from_choice(new_index);
                config::update_music_volume(vol);
            }
            SOUND_ROW_DEVICE => {
                let device = sound_device_from_choice(state, new_index);
                config::update_audio_output_device(device);
                let current_rate = config::get().audio_sample_rate_hz;
                let rate_choice = sample_rate_choice_index(state, current_rate);
                if current_rate.is_some() && rate_choice == 0 {
                    config::update_audio_sample_rate(None);
                }
                set_sound_choice_index(state, SOUND_ROW_SAMPLE_RATE, rate_choice);
            }
            SOUND_ROW_OUTPUT_MODE => {
                config::update_audio_output_mode(audio_output_mode_from_choice(new_index));
            }
            #[cfg(target_os = "linux")]
            SOUND_ROW_LINUX_BACKEND => {
                config::update_linux_audio_backend(linux_audio_backend_from_choice(new_index));
            }
            SOUND_ROW_SAMPLE_RATE => {
                let rate = sample_rate_from_choice(state, new_index);
                config::update_audio_sample_rate(rate);
            }
            SOUND_ROW_MINE_SOUNDS => {
                config::update_mine_hit_sound(new_index == 1);
            }
            SOUND_ROW_RATEMOD_PITCH => {
                config::update_rate_mod_preserves_pitch(new_index == 1);
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::SelectMusic) {
        let row = &rows[row_index];
        if row.label == SELECT_MUSIC_ROW_SHOW_BANNERS {
            config::update_show_select_music_banners(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_SHOW_VIDEO_BANNERS {
            config::update_show_select_music_video_banners(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_SHOW_BREAKDOWN {
            config::update_show_select_music_breakdown(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_BREAKDOWN_STYLE {
            config::update_select_music_breakdown_style(breakdown_style_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_NATIVE_LANGUAGE {
            config::update_translated_titles(translated_titles_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_WHEEL_SPEED {
            config::update_music_wheel_switch_speed(music_wheel_scroll_speed_from_choice(
                new_index,
            ));
        } else if row.label == SELECT_MUSIC_ROW_CDTITLES {
            config::update_show_select_music_cdtitles(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_WHEEL_GRADES {
            config::update_show_music_wheel_grades(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_WHEEL_LAMPS {
            config::update_show_music_wheel_lamps(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_PATTERN_INFO {
            config::update_select_music_pattern_info_mode(select_music_pattern_info_from_choice(
                new_index,
            ));
        } else if row.label == SELECT_MUSIC_ROW_PREVIEWS {
            config::update_show_select_music_previews(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_PREVIEW_MARKER {
            config::update_show_select_music_preview_marker(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_PREVIEW_LOOP {
            config::update_select_music_preview_loop(new_index == 1);
        } else if row.label == SELECT_MUSIC_ROW_GAMEPLAY_TIMER {
            config::update_show_select_music_gameplay_timer(yes_no_from_choice(new_index));
        } else if row.label == SELECT_MUSIC_ROW_SHOW_RIVALS {
            config::update_show_select_music_scorebox(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::GrooveStats) {
        let row = &rows[row_index];
        if row.label == GS_ROW_ENABLE {
            let enabled = yes_no_from_choice(new_index);
            config::update_enable_groovestats(enabled);
            // Re-run connectivity logic so toggling this option applies immediately.
            crate::core::network::init();
        } else if row.label == GS_ROW_ENABLE_BOOGIE {
            config::update_enable_boogiestats(yes_no_from_choice(new_index));
            crate::core::network::init();
        } else if row.label == GS_ROW_AUTO_POPULATE {
            config::update_auto_populate_gs_scores(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::ArrowCloud) {
        let row = &rows[row_index];
        if row.label == ARROWCLOUD_ROW_ENABLE {
            config::update_enable_arrowcloud(yes_no_from_choice(new_index));
            crate::core::network::init();
        }
    } else if matches!(kind, SubmenuKind::ScoreImport) {
        let row = &rows[row_index];
        if row.label == SCORE_IMPORT_ROW_ENDPOINT {
            refresh_score_import_profile_options(state);
        }
    }
    clear_render_cache(state);
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
                        "Course Options" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::Course);
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
                        "Online Score Services" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::OnlineScoring);
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
                        "Credits" => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::NavigateNoFade(Screen::Credits);
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
                    if matches!(kind, SubmenuKind::SelectMusic)
                        && let Some(row_idx) =
                            submenu_visible_row_to_actual(state, kind, selected_row)
                    {
                        let rows = submenu_rows(kind);
                        if rows.get(row_idx).map(|row| row.label)
                            == Some(SELECT_MUSIC_ROW_SCOREBOX_CYCLE)
                        {
                            let choice_idx = submenu_cursor_indices(state, kind)
                                .get(row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES.saturating_sub(1));
                            toggle_select_music_scorebox_cycle_option(state, choice_idx);
                            return ScreenAction::None;
                        }
                    }
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
                    } else if matches!(kind, SubmenuKind::OnlineScoring) {
                        let rows = submenu_rows(kind);
                        let Some(row_idx) =
                            submenu_visible_row_to_actual(state, kind, selected_row)
                        else {
                            return ScreenAction::None;
                        };
                        if let Some(row) = rows.get(row_idx) {
                            match row.label {
                                ONLINE_SCORING_ROW_GS_BS => {
                                    audio::play_sfx("assets/sounds/start.ogg");
                                    state.pending_submenu_kind = Some(SubmenuKind::GrooveStats);
                                    state.pending_submenu_parent_kind =
                                        Some(SubmenuKind::OnlineScoring);
                                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                                    state.submenu_fade_t = 0.0;
                                }
                                ONLINE_SCORING_ROW_ARROWCLOUD => {
                                    audio::play_sfx("assets/sounds/start.ogg");
                                    state.pending_submenu_kind = Some(SubmenuKind::ArrowCloud);
                                    state.pending_submenu_parent_kind =
                                        Some(SubmenuKind::OnlineScoring);
                                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                                    state.submenu_fade_t = 0.0;
                                }
                                ONLINE_SCORING_ROW_SCORE_IMPORT => {
                                    audio::play_sfx("assets/sounds/start.ogg");
                                    refresh_score_import_options(state);
                                    state.pending_submenu_kind = Some(SubmenuKind::ScoreImport);
                                    state.pending_submenu_parent_kind =
                                        Some(SubmenuKind::OnlineScoring);
                                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                                    state.submenu_fade_t = 0.0;
                                }
                                _ => {}
                            }
                        }
                    } else if matches!(kind, SubmenuKind::ScoreImport) {
                        let rows = submenu_rows(kind);
                        let Some(row_idx) =
                            submenu_visible_row_to_actual(state, kind, selected_row)
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
    let total_w = list_w_unscaled() + SEP_W + desc_w_unscaled();
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
    update_row_tweens(&mut state.row_tweens, total_rows, selected, s, list_y, dt);
}

const fn advanced_parent_row(actual_idx: usize) -> Option<usize> {
    let _ = actual_idx;
    None
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
    update_row_tweens(&mut state.row_tweens, total_rows, selected, s, list_y, dt);
}

const fn select_music_parent_row(actual_idx: usize) -> Option<usize> {
    match actual_idx {
        SELECT_MUSIC_SHOW_VIDEO_BANNERS_ROW_INDEX => Some(SELECT_MUSIC_SHOW_BANNERS_ROW_INDEX),
        SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX => Some(SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX),
        SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX => Some(SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX),
        SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX => Some(SELECT_MUSIC_SHOW_SCOREBOX_ROW_INDEX),
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
    update_row_tweens(&mut state.row_tweens, total_rows, selected, s, list_y, dt);
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

#[inline(always)]
fn wrap_miso_text(
    asset_manager: &AssetManager,
    raw_text: &str,
    max_width_px: f32,
    zoom: f32,
) -> String {
    asset_manager
        .with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |miso_font| {
                let mut out = String::new();
                let mut is_first_output_line = true;

                for segment in raw_text.split('\n') {
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
                            font::measure_line_width_logical(miso_font, &candidate, all_fonts)
                                as f32;
                        if !current_line.is_empty() && logical_w * zoom > max_width_px {
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
                    raw_text.to_string()
                } else {
                    out
                }
            })
        })
        .unwrap_or_else(|| raw_text.to_string())
}

fn build_description_layout(
    asset_manager: &AssetManager,
    key: DescriptionCacheKey,
    item: &Item<'_>,
    s: f32,
) -> DescriptionLayout {
    let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
    let wrap_extra_pad = desc_wrap_extra_pad_unscaled() * s;
    let help = item.help;
    let (raw_title_text, bullet_lines): (&str, &[&str]) = if help.is_empty() {
        (item.name, &[][..])
    } else {
        (help[0], &help[1..])
    };
    let title_max_width_px =
        desc_w_unscaled().mul_add(s, -((2.0 * title_side_pad) + wrap_extra_pad));
    let wrapped_title = wrap_miso_text(
        asset_manager,
        raw_title_text,
        title_max_width_px,
        DESC_TITLE_ZOOM * s,
    );
    let title_lines = wrapped_title.lines().count().max(1);
    let mut bullet_text = String::new();
    let mut bullet_line_count = 0usize;
    let mut note_text = String::new();
    if !bullet_lines.is_empty() {
        let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
        let bullet_max_width_px = desc_w_unscaled().mul_add(
            s,
            -((2.0 * bullet_side_pad) + (DESC_BULLET_INDENT_PX * s) + wrap_extra_pad),
        );
        let note_max_width_px =
            desc_w_unscaled().mul_add(s, -((2.0 * title_side_pad) + wrap_extra_pad));
        for line in bullet_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(note) = trimmed
                .strip_prefix("NOTE:")
                .or_else(|| trimmed.strip_prefix("Note:"))
            {
                let wrapped = wrap_miso_text(
                    asset_manager,
                    note.trim(),
                    note_max_width_px,
                    DESC_BODY_ZOOM * s,
                );
                if !note_text.is_empty() {
                    note_text.push('\n');
                }
                note_text.push_str(&wrapped);
                continue;
            }
            let entry = if trimmed == "..." {
                "...".to_string()
            } else {
                let mut v = String::with_capacity(trimmed.len() + 2);
                v.push('•');
                v.push(' ');
                v.push_str(trimmed);
                v
            };
            let wrapped = wrap_miso_text(
                asset_manager,
                &entry,
                bullet_max_width_px,
                DESC_BODY_ZOOM * s,
            );
            bullet_line_count += wrapped.lines().count();
            if !bullet_text.is_empty() {
                bullet_text.push('\n');
            }
            bullet_text.push_str(&wrapped);
        }
    }
    let note_line_count = note_text.lines().count().max(1);
    DescriptionLayout {
        key,
        title: Arc::from(wrapped_title),
        title_lines,
        bullet_text: (!bullet_text.is_empty()).then(|| Arc::from(bullet_text)),
        bullet_line_count,
        note_text: (!note_text.is_empty()).then(|| Arc::from(note_text)),
        note_line_count,
    }
}

fn description_layout(
    state: &State,
    asset_manager: &AssetManager,
    key: DescriptionCacheKey,
    item: &Item<'_>,
    s: f32,
) -> DescriptionLayout {
    if let Some(layout) = state.description_layout_cache.borrow().as_ref()
        && layout.key == key
    {
        return layout.clone();
    }
    let layout = build_description_layout(asset_manager, key, item, s);
    *state.description_layout_cache.borrow_mut() = Some(layout.clone());
    layout
}

pub fn clear_description_layout_cache(state: &State) {
    *state.description_layout_cache.borrow_mut() = None;
}

pub fn clear_render_cache(state: &State) {
    clear_submenu_row_layout_cache(state);
    clear_description_layout_cache(state);
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
    if is_launcher_submenu(kind) {
        return None;
    }
    let rows = submenu_rows(kind);
    let total_rows = submenu_total_rows(state, kind);
    if total_rows == 0 {
        return None;
    }
    let selected_row = state.sub_selected.min(total_rows - 1);
    let row_mid_y = row_mid_y_for_cursor(state, selected_row, total_rows, selected_row, s, list_y);
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
    let layout = submenu_row_layout(state, asset_manager, kind, row_idx)?;
    if layout.texts.is_empty() {
        return None;
    }
    let selected_choice = submenu_cursor_indices(state, kind)
        .get(row_idx)
        .copied()
        .unwrap_or(0)
        .min(layout.texts.len().saturating_sub(1));

    let draw_w = layout.widths[selected_choice];
    let center_x = if row.inline && layout.inline_row {
        let choice_inner_left = SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);
        choice_inner_left + layout.centers[selected_choice]
    } else {
        single_center_x
    };
    let (ring_w, ring_h) = ring_size_for_text(draw_w, layout.text_h);
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
        let mut ui_actors = build_reload_overlay_actors(reload, state.active_color_index);
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
    let list_w = list_w_unscaled() * s;
    let sep_w = SEP_W * s;
    let desc_w = desc_w_unscaled() * s;
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
    let selected_item: Option<(DescriptionCacheKey, &Item)>;
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
            selected_item = Some((DescriptionCacheKey::Main(sel), &ITEMS[sel]));
        }
        OptionsView::Submenu(kind) => {
            let rows = submenu_rows(kind);
            let choice_indices = submenu_choice_indices(state, kind);
            let items = submenu_items(kind);
            let visible_rows = submenu_visible_row_indices(state, kind, rows);
            if is_launcher_submenu(kind) {
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
                        if row.inline {
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
                let (item_idx, item) = if sel < rows.len() {
                    (sel, &items[sel])
                } else {
                    let idx = items.len().saturating_sub(1);
                    (idx, &items[idx])
                };
                selected_item = Some((DescriptionCacheKey::Submenu(kind, item_idx), item));
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
                    let item_col_left = list_x + label_bg_w;
                    let item_col_w = list_w - label_bg_w;
                    let single_center_x =
                        item_col_w.mul_add(0.5, item_col_left) + SUB_SINGLE_VALUE_CENTER_OFFSET * s;
                    // Non-inline rows behave as single-value rows: keep the cursor centered
                    // on the center of the available items column (row width minus label column).
                    if !row.inline {
                        return single_center_x;
                    }
                    let Some(layout) =
                        submenu_row_layout(state, asset_manager, kind, actual_row_idx)
                    else {
                        return list_w.mul_add(0.5, list_x);
                    };
                    if !layout.inline_row || layout.centers.is_empty() {
                        return single_center_x;
                    }
                    let sel_idx = choice_indices
                        .get(actual_row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(layout.centers.len().saturating_sub(1));
                    SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w)
                        + layout.centers[sel_idx]
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
                        let label = row.label;
                        let is_disabled = is_submenu_row_disabled(kind, row.label);
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
                        if let Some(layout) =
                            submenu_row_layout(state, asset_manager, kind, actual_row_idx)
                            && !layout.texts.is_empty()
                        {
                            let value_zoom = 0.835_f32;
                            let selected_choice = choice_indices
                                .get(actual_row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(layout.texts.len().saturating_sub(1));
                            let is_scorebox_cycle_row = matches!(kind, SubmenuKind::SelectMusic)
                                && row.label == SELECT_MUSIC_ROW_SCOREBOX_CYCLE;
                            let scorebox_enabled_mask = if is_scorebox_cycle_row {
                                select_music_scorebox_cycle_enabled_mask()
                            } else {
                                0
                            };
                            let mut selected_left_x: Option<f32> = None;
                            let choice_inner_left =
                                SUB_INLINE_ITEMS_LEFT_PAD.mul_add(s, list_x + label_bg_w);

                            if layout.inline_row {
                                for (idx, choice) in layout.texts.iter().enumerate() {
                                    let x = choice_inner_left
                                        + layout.x_positions.get(idx).copied().unwrap_or_default();
                                    let is_choice_selected = idx == selected_choice;
                                    if is_choice_selected {
                                        selected_left_x = Some(x);
                                    }
                                    let is_choice_enabled = is_scorebox_cycle_row
                                        && (scorebox_enabled_mask
                                            & scorebox_cycle_bit_from_choice(idx))
                                            != 0;
                                    let mut choice_color = if is_disabled && !is_choice_selected {
                                        sl_gray
                                    } else if is_scorebox_cycle_row {
                                        if is_choice_enabled {
                                            col_white
                                        } else {
                                            sl_gray
                                        }
                                    } else if is_active {
                                        col_white
                                    } else {
                                        sl_gray
                                    };
                                    choice_color[3] *= row_alpha;
                                    ui_actors.push(act!(text:
                                        align(0.0, 0.5):
                                        xy(x, row_mid_y):
                                        zoom(value_zoom):
                                        diffuse(choice_color[0], choice_color[1], choice_color[2], choice_color[3]):
                                        font("miso"):
                                        settext(choice):
                                        horizalign(left)
                                    ));
                                }
                            } else {
                                let mut choice_color = if is_active { col_white } else { sl_gray };
                                choice_color[3] *= row_alpha;
                                let choice_center_x = calc_row_center_x(row_idx);
                                let draw_w =
                                    layout.widths.get(selected_choice).copied().unwrap_or(40.0);
                                selected_left_x = Some(choice_center_x - draw_w * 0.5);
                                let choice_text = layout
                                    .texts
                                    .get(selected_choice)
                                    .cloned()
                                    .unwrap_or_else(|| Arc::<str>::from("??"));
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

                            // For normal rows, underline the selected option.
                            // For GS Box Leaderboards, underline each enabled option (multi-select).
                            if layout.inline_row && is_scorebox_cycle_row {
                                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                let offset = widescale(3.0, 4.0);
                                let underline_y = row_mid_y + layout.text_h * 0.5 + offset;
                                let mut line_color =
                                    color::decorative_rgba(state.active_color_index);
                                line_color[3] *= row_alpha;
                                for idx in 0..layout.texts.len() {
                                    let bit = scorebox_cycle_bit_from_choice(idx);
                                    if bit == 0 || (scorebox_enabled_mask & bit) == 0 {
                                        continue;
                                    }
                                    let underline_left_x = choice_inner_left
                                        + layout.x_positions.get(idx).copied().unwrap_or_default();
                                    let underline_w =
                                        layout.widths.get(idx).copied().unwrap_or(40.0).ceil();
                                    ui_actors.push(act!(quad:
                                        align(0.0, 0.5):
                                        xy(underline_left_x, underline_y):
                                        zoomto(underline_w, line_thickness):
                                        diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                        z(101)
                                    ));
                                }
                            } else if let Some(sel_left_x) = selected_left_x {
                                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                let underline_w = layout
                                    .widths
                                    .get(selected_choice)
                                    .copied()
                                    .unwrap_or(40.0)
                                    .ceil();
                                let offset = widescale(3.0, 4.0);
                                let underline_y = row_mid_y + layout.text_h * 0.5 + offset;
                                let mut line_color =
                                    color::decorative_rgba(state.active_color_index);
                                line_color[3] *= row_alpha;
                                ui_actors.push(act!(quad:
                                    align(0.0, 0.5):
                                    xy(sel_left_x, underline_y):
                                    zoomto(underline_w, line_thickness):
                                    diffuse(line_color[0], line_color[1], line_color[2], line_color[3]):
                                    z(101)
                                ));
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
                let (item_idx, item) = if sel < visible_rows.len() {
                    let actual_row_idx = visible_rows[sel];
                    (actual_row_idx, &items[actual_row_idx])
                } else {
                    let idx = items.len().saturating_sub(1);
                    (idx, &items[idx])
                };
                selected_item = Some((DescriptionCacheKey::Submenu(kind, item_idx), item));
            }
        }
    }

    // ------------------- Description content (selected) -------------------
    if let Some((desc_key, item)) = selected_item {
        // Match Simply Love's description box feel:
        // - explicit top/side padding for title and bullets so they can be tuned
        // - text zoom similar to other help text (player options, etc.)
        let mut cursor_y = DESC_TITLE_TOP_PAD_PX.mul_add(s, list_y);
        let desc_layout = description_layout(state, asset_manager, desc_key, item, s);
        let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
        let title_step_px = 20.0 * s; // approximate vertical advance for title line
        let body_step_px = 18.0 * s;

        // Draw the wrapped explanation/title text.
        ui_actors.push(act!(text:
            align(0.0, 0.0):
            xy(desc_x + title_side_pad, cursor_y):
            zoom(DESC_TITLE_ZOOM):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"): settext(&desc_layout.title):
            horizalign(left)
        ));
        cursor_y += title_step_px * desc_layout.title_lines as f32 + DESC_BULLET_TOP_PAD_PX * s;

        if let Some(bullet_text) = desc_layout.bullet_text.as_ref() {
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
            cursor_y += body_step_px * desc_layout.bullet_line_count as f32;
        }
        if let Some(note_text) = desc_layout.note_text.as_ref() {
            let note_min_y = cursor_y
                + if desc_layout.bullet_text.is_some() {
                    8.0 * s
                } else {
                    0.0
                };
            let note_bottom_y = (list_y + desc_h)
                - (DESC_NOTE_BOTTOM_PAD_PX * s)
                - (body_step_px * desc_layout.note_line_count as f32);
            let note_y = note_min_y.max(note_bottom_y);
            ui_actors.push(act!(text:
                align(0.0, 0.0):
                xy(desc_x + title_side_pad, note_y):
                zoom(DESC_BODY_ZOOM):
                diffuse(1.0, 1.0, 1.0, 1.0):
                font("miso"): settext(note_text):
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
