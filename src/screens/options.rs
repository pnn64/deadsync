use crate::act;
use crate::assets::{self, AssetManager};
use crate::engine::display::{self, MonitorSpec};
use crate::engine::gfx::{BackendType, PresentModePolicy};
use crate::engine::space::{is_wide, screen_height, screen_width, widescale};
// Screen navigation is handled in app via the dispatcher
use crate::config::{
    self, BreakdownStyle, DefaultFailType, DisplayMode, FullscreenType, LogLevel,
    MachinePreferredPlayMode, MachinePreferredPlayStyle, MenuBackgroundStyle, NewPackMode,
    SelectMusicItlRankMode, SelectMusicItlWheelMode, SelectMusicPatternInfoMode,
    SelectMusicScoreboxPlacement, SelectMusicWheelStyle, SimpleIni, SyncGraphMode, dirs,
};
use crate::engine::audio;
#[cfg(target_os = "windows")]
use crate::engine::input::WindowsPadBackend;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::game::parsing::{noteskin as noteskin_parser, simfile as song_loading};
use crate::game::{course, profile, scores};
use crate::screens::input as screen_input;
use crate::screens::pack_sync as shared_pack_sync;
use crate::screens::select_music;
use crate::screens::{Screen, ScreenAction};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::engine::present::actors;
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::present::font;
use crate::screens::components::shared::screen_bar::{ScreenBarPosition, ScreenBarTitlePlacement};
use crate::screens::components::shared::{heart_bg, screen_bar, transitions};
use null_or_die::{BiasKernel, KernelTarget};

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
const NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS: i32 = 1;
const NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS: i32 = 1000;
const NULL_OR_DIE_MAGIC_OFFSET_MIN_TENTHS: i32 = -1000;
const NULL_OR_DIE_MAGIC_OFFSET_MAX_TENTHS: i32 = 1000;

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
fn format_tenths_ms(value_tenths: i32) -> String {
    format!("{:.1} ms", value_tenths as f64 / 10.0)
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

#[inline(always)]
fn adjust_tenths_value(value: &mut i32, delta: isize, min: i32, max: i32) -> bool {
    let new_value = (*value + delta as i32).clamp(min, max);
    if new_value == *value {
        false
    } else {
        *value = new_value;
        true
    }
}

#[inline(always)]
fn tenths_from_f64(value: f64) -> i32 {
    let scaled = value * 10.0;
    let nudge = scaled.signum() * scaled.abs().max(1.0) * f64::EPSILON * 16.0;
    (scaled + nudge).round() as i32
}

#[inline(always)]
fn f64_from_tenths(value: i32) -> f64 {
    value as f64 / 10.0
}

// Keyboard input is handled centrally via the virtual dispatcher in app

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

/// Typed identifier for each top-level Options menu row and submenu item.
/// Used for dispatch so that item selection is string-free.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemId {
    // Top-level Options menu
    SystemOptions,
    GraphicsOptions,
    SoundOptions,
    InputOptions,
    MachineOptions,
    GameplayOptions,
    SelectMusicOptions,
    AdvancedOptions,
    CourseOptions,
    ManageLocalProfiles,
    OnlineScoreServices,
    NullOrDieOptions,
    ReloadSongsCourses,
    Credits,
    Exit,

    // System Options submenu
    SysGame,
    SysTheme,
    SysLanguage,
    SysLogLevel,
    SysLogFile,
    SysDefaultNoteSkin,

    // Graphics Options submenu
    GfxVideoRenderer,
    GfxSoftwareThreads,
    GfxDisplayMode,
    GfxDisplayAspectRatio,
    GfxDisplayResolution,
    GfxRefreshRate,
    GfxFullscreenType,
    GfxVSync,
    GfxPresentMode,
    GfxMaxFps,
    GfxMaxFpsValue,
    GfxShowStats,
    GfxValidationLayers,
    GfxVisualDelay,

    // Input Options submenu (launcher)
    InpConfigureMappings,
    InpTestInput,
    InpInputOptions,

    // Input Backend Options submenu
    InpGamepadBackend,
    InpUseFsrs,
    InpMenuButtons,
    InpOptionsNavigation,
    InpMenuNavigation,
    InpDebounce,

    // Machine Options submenu
    MchSelectProfile,
    MchSelectColor,
    MchSelectStyle,
    MchPreferredStyle,
    MchSelectPlayMode,
    MchPreferredMode,
    MchEvalSummary,
    MchNameEntry,
    MchGameoverScreen,
    MchWriteCurrentScreen,
    MchMenuMusic,
    MchMenuBackground,
    MchReplays,
    MchPerPlayerGlobalOffsets,
    MchKeyboardFeatures,
    MchVideoBgs,

    // Gameplay Options submenu
    GpBgBrightness,
    GpCenteredP1,
    GpZmodRatingBox,
    GpBpmDecimal,
    GpAutoScreenshot,

    // Sound Options submenu
    SndDevice,
    SndOutputMode,
    SndLinuxBackend,
    SndAlsaExclusive,
    SndSampleRate,
    SndMasterVolume,
    SndSfxVolume,
    SndAssistTickVolume,
    SndMusicVolume,
    SndMineSounds,
    SndGlobalOffset,
    SndRateModPitch,

    // Select Music Options submenu
    SmShowBanners,
    SmShowVideoBanners,
    SmShowBreakdown,
    SmBreakdownStyle,
    SmNativeLanguage,
    SmWheelSpeed,
    SmWheelStyle,
    SmCdTitles,
    SmWheelGrades,
    SmWheelLamps,
    SmWheelItlRank,
    SmWheelItl,
    SmNewPackBadge,
    SmPatternInfo,
    SmChartInfo,
    SmPreviews,
    SmPreviewMarker,
    SmPreviewLoop,
    SmGameplayTimer,
    SmShowRivals,
    SmScoreboxPlacement,
    SmScoreboxCycle,

    // Course Options submenu
    CrsShowRandom,
    CrsShowMostPlayed,
    CrsShowIndividualScores,
    CrsAutosubmitIndividual,

    // Advanced Options submenu
    AdvDefaultFailType,
    AdvBannerCache,
    AdvCdTitleCache,
    AdvSongParsingThreads,
    AdvCacheSongs,
    AdvFastLoad,

    // GrooveStats Options submenu
    GsEnable,
    GsEnableBoogie,
    GsSubmitFails,
    GsAutoPopulate,
    GsAutoDownloadUnlocks,
    GsSeparateUnlocks,

    // ArrowCloud Options submenu
    AcEnable,
    AcSubmitFails,

    // Online Scoring submenu (launcher)
    OsGsBsOptions,
    OsArrowCloudOptions,
    OsScoreImport,

    // Null-or-Die menu (launcher)
    NodOptions,
    NodSyncPacks,

    // Null-or-Die Settings submenu
    NodSyncGraph,
    NodSyncConfidence,
    NodPackSyncThreads,
    NodFingerprint,
    NodWindow,
    NodStep,
    NodMagicOffset,
    NodKernelTarget,
    NodKernelType,
    NodFullSpectrogram,

    // Sync Pack submenu
    SpPack,
    SpStart,

    // Score Import submenu
    SiEndpoint,
    SiProfile,
    SiPack,
    SiOnlyMissing,
    SiStart,
}

/// An entry in the help/description pane for an option item.
#[derive(Clone, Copy)]
pub enum HelpEntry {
    /// Description paragraph text.
    Paragraph(LookupKey),
    /// Bullet point item (rendered with "•" prefix).
    Bullet(LookupKey),
}

/// A simple item model with help text for the description box.
pub struct Item {
    pub id: ItemId,
    pub name: LookupKey,
    pub help: &'static [HelpEntry],
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
        id: ItemId::SystemOptions,
        name: lookup_key("Options", "SystemOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "SystemOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsSystem", "Game")),
            HelpEntry::Bullet(lookup_key("OptionsSystem", "Theme")),
            HelpEntry::Bullet(lookup_key("OptionsSystem", "Language")),
            HelpEntry::Bullet(lookup_key("OptionsSystem", "LogFile")),
            HelpEntry::Bullet(lookup_key("OptionsSystem", "DefaultNoteSkin")),
        ],
    },
    Item {
        id: ItemId::GraphicsOptions,
        name: lookup_key("Options", "GraphicsOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "GraphicsOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "VideoRenderer")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "DisplayMode")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "DisplayAspectRatio")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "DisplayResolution")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "RefreshRate")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "FullscreenType")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "VSync")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "PresentMode")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "MaxFps")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "ShowStats")),
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "VisualDelay")),
        ],
    },
    Item {
        id: ItemId::SoundOptions,
        name: lookup_key("Options", "SoundOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "SoundOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "SoundDevice")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "AudioSampleRate")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "MasterVolume")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "SFXVolume")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "AssistTickVolume")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "MusicVolume")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "MineSounds")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "GlobalOffset")),
            HelpEntry::Bullet(lookup_key("OptionsSound", "RateModPreservesPitch")),
        ],
    },
    Item {
        id: ItemId::InputOptions,
        name: lookup_key("Options", "InputOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "InputOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "ConfigureMappings")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "TestInput")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "InputOptions")),
        ],
    },
    Item {
        id: ItemId::MachineOptions,
        name: lookup_key("Options", "MachineOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "MachineOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectProfile")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectColor")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectStyle")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectPlayMode")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "EvalSummary")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "NameEntry")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "GameoverScreen")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "MenuMusic")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "MenuBackground")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "KeyboardFeatures")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "VideoBGs")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "WriteCurrentScreen")),
        ],
    },
    Item {
        id: ItemId::GameplayOptions,
        name: lookup_key("Options", "GameplayOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "GameplayOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsGameplay", "BGBrightness")),
            HelpEntry::Bullet(lookup_key("OptionsGameplay", "CenteredP1Notefield")),
            HelpEntry::Bullet(lookup_key("OptionsGameplay", "ZmodRatingBox")),
            HelpEntry::Bullet(lookup_key("OptionsGameplay", "BpmDecimal")),
        ],
    },
    Item {
        id: ItemId::SelectMusicOptions,
        name: lookup_key("Options", "SelectMusicOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "SelectMusicOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowBanners")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowVideoBanners")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowBreakdown")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowNativeLanguage")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "MusicWheelSpeed")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowCDTitles")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowWheelGrades")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowWheelLamps")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ITLRank")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "NewPackBadge")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowPatternInfo")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ChartInfo")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "MusicPreviews")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowGameplayTimer")),
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "ShowGSBox")),
        ],
    },
    Item {
        id: ItemId::AdvancedOptions,
        name: lookup_key("Options", "AdvancedOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "AdvancedOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsAdvanced", "DefaultFailType")),
            HelpEntry::Bullet(lookup_key("OptionsAdvanced", "BannerCache")),
            HelpEntry::Bullet(lookup_key("OptionsAdvanced", "CDTitleCache")),
            HelpEntry::Bullet(lookup_key("OptionsAdvanced", "SongParsingThreads")),
            HelpEntry::Bullet(lookup_key("OptionsAdvanced", "CacheSongs")),
            HelpEntry::Bullet(lookup_key("OptionsAdvanced", "FastLoad")),
        ],
    },
    Item {
        id: ItemId::CourseOptions,
        name: lookup_key("Options", "CourseOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "CourseOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsCourse", "ShowRandomCourses")),
            HelpEntry::Bullet(lookup_key("OptionsCourse", "ShowMostPlayed")),
            HelpEntry::Bullet(lookup_key("OptionsCourse", "ShowIndividualScores")),
            HelpEntry::Bullet(lookup_key("OptionsCourse", "AutosubmitIndividual")),
        ],
    },
    Item {
        id: ItemId::ManageLocalProfiles,
        name: lookup_key("Options", "ManageLocalProfiles"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ManageLocalProfilesHelp",
        ))],
    },
    Item {
        id: ItemId::OnlineScoreServices,
        name: lookup_key("Options", "OnlineScoreServices"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "OnlineScoreServicesHelp")),
            HelpEntry::Bullet(lookup_key("OptionsOnlineScoring", "GsBsOptions")),
            HelpEntry::Bullet(lookup_key("OptionsOnlineScoring", "ArrowCloudOptions")),
            HelpEntry::Bullet(lookup_key("OptionsOnlineScoring", "ScoreImport")),
        ],
    },
    Item {
        id: ItemId::NullOrDieOptions,
        name: lookup_key("Options", "NullOrDieOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsHelp", "NullOrDieOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsOnlineScoring", "NullOrDieOptions")),
            HelpEntry::Bullet(lookup_key("OptionsOnlineScoring", "SyncPacks")),
        ],
    },
    Item {
        id: ItemId::ReloadSongsCourses,
        name: lookup_key("Options", "ReloadSongsCourses"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ReloadSongsCoursesHelp",
        ))],
    },
    Item {
        id: ItemId::Credits,
        name: lookup_key("Options", "Credits"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "CreditsHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key("OptionsHelp", "ExitHelp"))],
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
    NullOrDie,
    NullOrDieOptions,
    SyncPacks,
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
    matches!(
        kind,
        SubmenuKind::Input | SubmenuKind::OnlineScoring | SubmenuKind::NullOrDie
    )
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

/// A pre-wrapped block of text in the description pane, ready for rendering.
#[derive(Clone, Debug)]
enum RenderedHelpBlock {
    Paragraph { text: Arc<str>, line_count: usize },
    Bullet { text: Arc<str>, line_count: usize },
}

#[derive(Clone, Debug)]
struct DescriptionLayout {
    key: DescriptionCacheKey,
    blocks: Vec<RenderedHelpBlock>,
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
            detail_line: tr("OptionsScoreImport", "PreparingImport").to_string(),
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
struct SyncPackSelection {
    pack_group: Option<String>,
    pack_label: String,
}

#[derive(Clone, Debug)]
struct SyncPackConfirmState {
    selection: SyncPackSelection,
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubRowId {
    // System Options
    Game,
    Theme,
    Language,
    LogLevel,
    LogFile,
    DefaultNoteSkin,
    // Graphics Options
    VideoRenderer,
    SoftwareRendererThreads,
    DisplayMode,
    DisplayAspectRatio,
    DisplayResolution,
    RefreshRate,
    FullscreenType,
    VSync,
    PresentMode,
    MaxFps,
    MaxFpsValue,
    ShowStats,
    ValidationLayers,
    VisualDelay,
    // Sound Options
    SoundDevice,
    AudioOutputMode,
    AudioSampleRate,
    MasterVolume,
    SfxVolume,
    AssistTickVolume,
    MusicVolume,
    MineSounds,
    GlobalOffset,
    RateModPreservesPitch,
    #[cfg(target_os = "linux")]
    LinuxAudioBackend,
    #[cfg(target_os = "linux")]
    AlsaExclusive,
    // Input Options (launcher)
    ConfigureMappings,
    TestInput,
    InputOptions,
    // Input Backend Options
    GamepadBackend,
    UseFsrs,
    MenuNavigation,
    OptionsNavigation,
    MenuButtons,
    Debounce,
    // Machine Options
    SelectProfile,
    SelectColor,
    SelectStyle,
    PreferredStyle,
    SelectPlayMode,
    PreferredMode,
    EvalSummary,
    NameEntry,
    GameoverScreen,
    WriteCurrentScreen,
    MenuMusic,
    MenuBackground,
    Replays,
    PerPlayerGlobalOffsets,
    KeyboardFeatures,
    VideoBgs,
    // Gameplay Options
    BgBrightness,
    CenteredP1Notefield,
    ZmodRatingBox,
    BpmDecimal,
    AutoScreenshot,
    // Select Music Options
    ShowBanners,
    ShowVideoBanners,
    ShowBreakdown,
    BreakdownStyle,
    ShowNativeLanguage,
    MusicWheelSpeed,
    MusicWheelStyle,
    ShowCdTitles,
    ShowWheelGrades,
    ShowWheelLamps,
    ItlRank,
    ItlWheelData,
    NewPackBadge,
    ShowPatternInfo,
    ChartInfo,
    MusicPreviews,
    PreviewMarker,
    LoopMusic,
    ShowGameplayTimer,
    ShowGsBox,
    GsBoxPlacement,
    GsBoxLeaderboards,
    // Course Options
    ShowRandomCourses,
    ShowMostPlayed,
    ShowIndividualScores,
    AutosubmitIndividual,
    // Advanced Options
    DefaultFailType,
    BannerCache,
    CdTitleCache,
    SongParsingThreads,
    CacheSongs,
    FastLoad,
    // GrooveStats Options
    EnableGrooveStats,
    EnableBoogieStats,
    GsSubmitFails,
    AutoPopulateScores,
    AutoDownloadUnlocks,
    SeparateUnlocksByPlayer,
    // ArrowCloud Options
    EnableArrowCloud,
    ArrowCloudSubmitFails,
    // Online Scoring (launcher)
    GsBsOptions,
    ArrowCloudOptions,
    ScoreImport,
    // Null-or-Die (launcher)
    NullOrDieOptions,
    SyncPacks,
    // Null-or-Die Settings
    SyncGraph,
    SyncConfidence,
    PackSyncThreads,
    Fingerprint,
    Window,
    Step,
    MagicOffset,
    KernelTarget,
    KernelType,
    FullSpectrogram,
    // Sync Pack
    SyncPackPack,
    SyncPackStart,
    // Score Import
    ScoreImportEndpoint,
    ScoreImportProfile,
    ScoreImportPack,
    ScoreImportOnlyMissing,
    ScoreImportStart,
}

pub struct SubRow {
    pub id: SubRowId,
    pub label: LookupKey,
    pub choices: &'static [Choice],
    pub inline: bool, // whether to lay out choices inline (vs single centered value)
}

/// Choice values — some are localizable, some are format-specific literals.
#[derive(Clone, Copy)]
pub enum Choice {
    /// Translatable text (e.g., "Windowed", "On", "Off").
    Localized(LookupKey),
    /// Format-specific literal that should never be translated (e.g., "16:9", "1920x1080").
    Literal(&'static str),
}

impl Choice {
    pub fn get(&self) -> Arc<str> {
        match self {
            Choice::Localized(lkey) => lkey.get(),
            Choice::Literal(s) => Arc::from(*s),
        }
    }

    pub fn as_str_static(&self) -> Option<&'static str> {
        match self {
            Choice::Literal(s) => Some(s),
            Choice::Localized(_) => None,
        }
    }
}

/// Shorthand for `Choice::Localized(lookup_key(section, key))` in const arrays.
#[allow(non_snake_case)]
const fn localized_choice(section: &'static str, key: &'static str) -> Choice {
    Choice::Localized(lookup_key(section, key))
}

/// Shorthand for `Choice::Literal(s)` in const arrays.
const fn literal_choice(s: &'static str) -> Choice {
    Choice::Literal(s)
}

const LANGUAGE_CHOICES: &[Choice] = &[
    localized_choice("OptionsSystem", "EnglishLanguage"),
    localized_choice("OptionsSystem", "GermanLanguage"),
    localized_choice("OptionsSystem", "SpanishLanguage"),
    localized_choice("OptionsSystem", "FrenchLanguage"),
    localized_choice("OptionsSystem", "ItalianLanguage"),
    localized_choice("OptionsSystem", "JapaneseLanguage"),
    localized_choice("OptionsSystem", "PolishLanguage"),
    localized_choice("OptionsSystem", "PortugueseBrazilLanguage"),
    localized_choice("OptionsSystem", "RussianLanguage"),
    localized_choice("OptionsSystem", "SwedishLanguage"),
    localized_choice("OptionsSystem", "PseudoLanguage"),
];

#[cfg(target_os = "windows")]
const INPUT_BACKEND_CHOICES: &[Choice] = &[
    literal_choice("W32 Raw Input"),
    literal_choice("WGI (compat)"),
];
#[cfg(target_os = "macos")]
const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("macOS IOHID")];
#[cfg(target_os = "linux")]
const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("Linux evdev")];
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("Platform Default")];
#[cfg(not(any(target_os = "windows", unix)))]
const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("Platform Default")];
#[cfg(target_os = "windows")]
const INPUT_BACKEND_INLINE: bool = true;
#[cfg(not(target_os = "windows"))]
const INPUT_BACKEND_INLINE: bool = false;
const SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES: usize = 4;
const SELECT_MUSIC_CHART_INFO_NUM_CHOICES: usize = 2;

const SCORE_IMPORT_DONE_OVERLAY_SECONDS: f32 = 1.5;
const SCORE_IMPORT_ROW_ENDPOINT_INDEX: usize = 0;
const SCORE_IMPORT_ROW_PROFILE_INDEX: usize = 1;
const SCORE_IMPORT_ROW_PACK_INDEX: usize = 2;
const SCORE_IMPORT_ROW_ONLY_MISSING_INDEX: usize = 3;
const SYNC_PACK_ROW_PACK_INDEX: usize = 0;

#[cfg(target_os = "linux")]
const SOUND_LINUX_BACKEND_CHOICES: &[Choice] = &[localized_choice("Common", "Auto")];

fn discover_system_noteskin_choices() -> Vec<String> {
    let mut names = noteskin_parser::discover_itg_skins("dance");
    if names.is_empty() {
        names.push(profile::NoteSkin::DEFAULT_NAME.to_string());
    }
    names
}

fn build_sound_device_options() -> Vec<SoundDeviceOption> {
    let discovered = if audio::is_initialized() {
        audio::startup_output_devices()
    } else {
        Vec::new()
    };
    let default_rates = discovered
        .iter()
        .find(|dev| dev.is_default)
        .map(|dev| dev.sample_rates_hz.clone())
        .unwrap_or_default();
    let mut options = Vec::with_capacity(discovered.len() + 1);
    options.push(SoundDeviceOption {
        label: tr("Common", "Auto").to_string(),
        config_index: None,
        sample_rates_hz: default_rates,
    });
    for (idx, dev) in discovered.into_iter().enumerate() {
        let mut label = dev.name.clone();
        if dev.is_default {
            label.push_str(&tr("OptionsSound", "DefaultSuffix"));
        }
        options.push(SoundDeviceOption {
            label,
            config_index: Some(idx as u16),
            sample_rates_hz: dev.sample_rates_hz,
        });
    }
    options
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn linux_backend_label(backend: config::LinuxAudioBackend) -> std::sync::Arc<str> {
    match backend {
        config::LinuxAudioBackend::Auto => tr("Common", "Auto"),
        config::LinuxAudioBackend::PipeWire => std::sync::Arc::from("PipeWire"),
        config::LinuxAudioBackend::PulseAudio => std::sync::Arc::from("PulseAudio"),
        config::LinuxAudioBackend::Jack => std::sync::Arc::from("JACK"),
        config::LinuxAudioBackend::Alsa => std::sync::Arc::from("ALSA"),
    }
}

#[cfg(target_os = "linux")]
fn build_linux_backend_choices() -> Vec<String> {
    audio::available_linux_backends()
        .into_iter()
        .map(|backend| linux_backend_label(backend).to_string())
        .collect()
}

pub const SYSTEM_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::Game,
        label: lookup_key("OptionsSystem", "Game"),
        choices: &[localized_choice("OptionsSystem", "DanceGame")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Theme,
        label: lookup_key("OptionsSystem", "Theme"),
        choices: &[localized_choice("OptionsSystem", "SimplyLoveTheme")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Language,
        label: lookup_key("OptionsSystem", "Language"),
        choices: LANGUAGE_CHOICES,
        inline: false,
    },
    SubRow {
        id: SubRowId::LogLevel,
        label: lookup_key("OptionsSystem", "LogLevel"),
        choices: &[
            localized_choice("OptionsSystem", "LogLevelError"),
            localized_choice("OptionsSystem", "LogLevelWarn"),
            localized_choice("OptionsSystem", "LogLevelInfo"),
            localized_choice("OptionsSystem", "LogLevelDebug"),
            localized_choice("OptionsSystem", "LogLevelTrace"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::LogFile,
        label: lookup_key("OptionsSystem", "LogFile"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::DefaultNoteSkin,
        label: lookup_key("OptionsSystem", "DefaultNoteSkin"),
        choices: &[literal_choice(profile::NoteSkin::DEFAULT_NAME)],
        inline: false,
    },
];

pub const SYSTEM_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SysGame,
        name: lookup_key("OptionsSystem", "Game"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "GameHelp",
        ))],
    },
    Item {
        id: ItemId::SysTheme,
        name: lookup_key("OptionsSystem", "Theme"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "ThemeHelp",
        ))],
    },
    Item {
        id: ItemId::SysLanguage,
        name: lookup_key("OptionsSystem", "Language"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "LanguageHelp",
        ))],
    },
    Item {
        id: ItemId::SysLogLevel,
        name: lookup_key("OptionsSystem", "LogLevel"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "LogLevelHelp",
        ))],
    },
    Item {
        id: ItemId::SysLogFile,
        name: lookup_key("OptionsSystem", "LogFile"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "LogFileHelp",
        ))],
    },
    Item {
        id: ItemId::SysDefaultNoteSkin,
        name: lookup_key("OptionsSystem", "DefaultNoteSkin"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "DefaultNoteSkinHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

#[cfg(all(target_os = "windows", not(target_pointer_width = "32")))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::Metal, "Metal (wgpu)"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(
    not(any(target_os = "windows", target_os = "macos")),
    not(target_pointer_width = "32")
))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(not(target_os = "windows"), target_pointer_width = "32"))]
const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];

#[cfg(all(target_os = "windows", not(target_pointer_width = "32")))]
const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererVulkan"),
    localized_choice("OptionsGraphics", "RendererDirectX"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererVulkanWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererDirectX"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererVulkan"),
    localized_choice("OptionsGraphics", "RendererMetal"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererVulkanWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(
    not(any(target_os = "windows", target_os = "macos")),
    not(target_pointer_width = "32")
))]
const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererVulkan"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererVulkanWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(not(target_os = "windows"), target_pointer_width = "32"))]
const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];

const DISPLAY_ASPECT_RATIO_CHOICES: &[Choice] = &[
    literal_choice("16:9"),
    literal_choice("16:10"),
    literal_choice("4:3"),
    literal_choice("1:1"),
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
const SELECT_MUSIC_SHOW_BANNERS_ROW_INDEX: usize = 0;
const SELECT_MUSIC_SHOW_VIDEO_BANNERS_ROW_INDEX: usize = 1;
const SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX: usize = 2;
const SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX: usize = 3;
const SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX: usize = 15;
const SELECT_MUSIC_CHART_INFO_ROW_INDEX: usize = 14;
const SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX: usize = 17;
const SELECT_MUSIC_SHOW_SCOREBOX_ROW_INDEX: usize = 19;
const SELECT_MUSIC_SCOREBOX_PLACEMENT_ROW_INDEX: usize = 20;
const SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX: usize = 21;
const MACHINE_SELECT_STYLE_ROW_INDEX: usize = 2;
const MACHINE_PREFERRED_STYLE_ROW_INDEX: usize = 3;
const MACHINE_SELECT_PLAY_MODE_ROW_INDEX: usize = 4;
const MACHINE_PREFERRED_MODE_ROW_INDEX: usize = 5;
const ADVANCED_SONG_PARSING_THREADS_ROW_INDEX: usize = 3;

const MAX_FPS_MIN: u16 = 5;
const MAX_FPS_MAX: u16 = 1000;
const MAX_FPS_STEP: u16 = 5;
const MAX_FPS_DEFAULT: u16 = 60;
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
        return vec![
            tr("OptionsGraphics", "Screen1Fallback").to_string(),
            tr("OptionsGraphics", "Windowed").to_string(),
        ];
    }
    let mut out = Vec::with_capacity(monitor_specs.len() + 1);
    for spec in monitor_specs {
        out.push(spec.name.clone());
    }
    out.push(tr("OptionsGraphics", "Windowed").to_string());
    out
}

pub const GRAPHICS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::VideoRenderer,
        label: lookup_key("OptionsGraphics", "VideoRenderer"),
        choices: VIDEO_RENDERER_LABELS,
        inline: false,
    },
    SubRow {
        id: SubRowId::SoftwareRendererThreads,
        label: lookup_key("OptionsGraphics", "SoftwareRendererThreads"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::DisplayMode,
        label: lookup_key("OptionsGraphics", "DisplayMode"),
        choices: &[
            localized_choice("OptionsGraphics", "Windowed"),
            localized_choice("OptionsGraphics", "Fullscreen"),
            localized_choice("OptionsGraphics", "Borderless"),
        ], // Replaced dynamically
        inline: true,
    },
    SubRow {
        id: SubRowId::DisplayAspectRatio,
        label: lookup_key("OptionsGraphics", "DisplayAspectRatio"),
        choices: DISPLAY_ASPECT_RATIO_CHOICES,
        inline: true,
    },
    SubRow {
        id: SubRowId::DisplayResolution,
        label: lookup_key("OptionsGraphics", "DisplayResolution"),
        choices: &[
            literal_choice("1920x1080"),
            literal_choice("1600x900"),
            literal_choice("1280x720"),
            literal_choice("1024x768"),
            literal_choice("800x600"),
        ], // Replaced dynamically
        inline: false,
    },
    SubRow {
        id: SubRowId::RefreshRate,
        label: lookup_key("OptionsGraphics", "RefreshRate"),
        choices: &[
            localized_choice("Common", "Default"),
            literal_choice("60 Hz"),
            literal_choice("75 Hz"),
            literal_choice("120 Hz"),
            literal_choice("144 Hz"),
            literal_choice("165 Hz"),
            literal_choice("240 Hz"),
            literal_choice("360 Hz"),
        ], // Replaced dynamically
        inline: false,
    },
    SubRow {
        id: SubRowId::FullscreenType,
        label: lookup_key("OptionsGraphics", "FullscreenType"),
        choices: &[
            localized_choice("OptionsGraphics", "FullscreenTypeExclusive"),
            localized_choice("OptionsGraphics", "FullscreenTypeBorderless"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::VSync,
        label: lookup_key("OptionsGraphics", "VSync"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PresentMode,
        label: lookup_key("OptionsGraphics", "PresentMode"),
        choices: &[literal_choice("Mailbox"), literal_choice("Immediate")],
        inline: true,
    },
    SubRow {
        id: SubRowId::MaxFps,
        label: lookup_key("OptionsGraphics", "MaxFps"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MaxFpsValue,
        label: lookup_key("OptionsGraphics", "MaxFpsValue"),
        choices: &[localized_choice("Common", "Off")], // Replaced dynamically
        inline: false,
    },
    SubRow {
        id: SubRowId::ShowStats,
        label: lookup_key("OptionsGraphics", "ShowStats"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("OptionsGraphics", "ShowStatsFPS"),
            localized_choice("OptionsGraphics", "ShowStatsFPSStutter"),
            localized_choice("OptionsGraphics", "ShowStatsFPSStutterTiming"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ValidationLayers,
        label: lookup_key("OptionsGraphics", "ValidationLayers"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::VisualDelay,
        label: lookup_key("OptionsGraphics", "VisualDelay"),
        choices: &[literal_choice("0 ms")],
        inline: false,
    },
];

pub const GRAPHICS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::GfxVideoRenderer,
        name: lookup_key("OptionsGraphics", "VideoRenderer"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "VideoRendererHelp",
        ))],
    },
    Item {
        id: ItemId::GfxSoftwareThreads,
        name: lookup_key("OptionsGraphics", "SoftwareRendererThreads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "SoftwareRendererThreadsHelp",
        ))],
    },
    Item {
        id: ItemId::GfxDisplayMode,
        name: lookup_key("OptionsGraphics", "DisplayMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "DisplayModeHelp",
        ))],
    },
    Item {
        id: ItemId::GfxDisplayAspectRatio,
        name: lookup_key("OptionsGraphics", "DisplayAspectRatio"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "DisplayAspectRatioHelp",
        ))],
    },
    Item {
        id: ItemId::GfxDisplayResolution,
        name: lookup_key("OptionsGraphics", "DisplayResolution"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "DisplayResolutionHelp",
        ))],
    },
    Item {
        id: ItemId::GfxRefreshRate,
        name: lookup_key("OptionsGraphics", "RefreshRate"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "RefreshRateHelp",
        ))],
    },
    Item {
        id: ItemId::GfxFullscreenType,
        name: lookup_key("OptionsGraphics", "FullscreenType"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "FullscreenTypeHelp",
        ))],
    },
    Item {
        id: ItemId::GfxVSync,
        name: lookup_key("OptionsGraphics", "VSync"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "VSyncHelp",
        ))],
    },
    Item {
        id: ItemId::GfxPresentMode,
        name: lookup_key("OptionsGraphics", "PresentMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "PresentModeHelp",
        ))],
    },
    Item {
        id: ItemId::GfxMaxFps,
        name: lookup_key("OptionsGraphics", "MaxFps"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "MaxFpsHelp",
        ))],
    },
    Item {
        id: ItemId::GfxMaxFpsValue,
        name: lookup_key("OptionsGraphics", "MaxFpsValue"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "MaxFpsValueHelp",
        ))],
    },
    Item {
        id: ItemId::GfxShowStats,
        name: lookup_key("OptionsGraphics", "ShowStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "ShowStatsHelp",
        ))],
    },
    Item {
        id: ItemId::GfxValidationLayers,
        name: lookup_key("OptionsGraphics", "ValidationLayers"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "ValidationLayersHelp",
        ))],
    },
    Item {
        id: ItemId::GfxVisualDelay,
        name: lookup_key("OptionsGraphics", "VisualDelay"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "VisualDelayHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const INPUT_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::ConfigureMappings,
        label: lookup_key("OptionsInput", "ConfigureMappings"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::TestInput,
        label: lookup_key("OptionsInput", "TestInput"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::InputOptions,
        label: lookup_key("OptionsInput", "InputOptions"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
];

pub const INPUT_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::InpConfigureMappings,
        name: lookup_key("OptionsInput", "ConfigureMappings"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "ConfigureMappingsHelp",
        ))],
    },
    Item {
        id: ItemId::InpTestInput,
        name: lookup_key("OptionsInput", "TestInput"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "TestInputHelp",
        ))],
    },
    Item {
        id: ItemId::InpInputOptions,
        name: lookup_key("OptionsInput", "InputOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsInputHelp", "InputOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "GamepadBackend")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "UseFSRs")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "MenuNavigation")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "OptionsNavigation")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "MenuButtons")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "Debounce")),
        ],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const INPUT_BACKEND_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::GamepadBackend,
        label: lookup_key("OptionsInput", "GamepadBackend"),
        choices: INPUT_BACKEND_CHOICES,
        inline: INPUT_BACKEND_INLINE,
    },
    SubRow {
        id: SubRowId::UseFsrs,
        label: lookup_key("OptionsInput", "UseFSRs"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MenuNavigation,
        label: lookup_key("OptionsInput", "MenuNavigation"),
        choices: &[
            localized_choice("OptionsInput", "MenuNavigationFiveKey"),
            localized_choice("OptionsInput", "MenuNavigationThreeKey"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::OptionsNavigation,
        label: lookup_key("OptionsInput", "OptionsNavigation"),
        choices: &[
            localized_choice("OptionsInput", "OptionsNavigationStepMania"),
            localized_choice("OptionsInput", "OptionsNavigationArcade"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MenuButtons,
        label: lookup_key("OptionsInput", "MenuButtons"),
        choices: &[
            localized_choice("OptionsInput", "DedicatedMenuButtonsGameplay"),
            localized_choice("OptionsInput", "DedicatedMenuButtonsOnly"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::Debounce,
        label: lookup_key("OptionsInput", "Debounce"),
        choices: &[literal_choice("20ms")],
        inline: true,
    },
];

pub const INPUT_BACKEND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::InpGamepadBackend,
        name: lookup_key("OptionsInput", "GamepadBackend"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "GamepadBackendHelp",
        ))],
    },
    Item {
        id: ItemId::InpUseFsrs,
        name: lookup_key("OptionsInput", "UseFSRs"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "UseFSRsHelp",
        ))],
    },
    Item {
        id: ItemId::InpMenuButtons,
        name: lookup_key("OptionsInput", "MenuButtons"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "MenuButtonsHelp",
        ))],
    },
    Item {
        id: ItemId::InpOptionsNavigation,
        name: lookup_key("OptionsInput", "OptionsNavigation"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "OptionsNavigationHelp",
        ))],
    },
    Item {
        id: ItemId::InpMenuNavigation,
        name: lookup_key("OptionsInput", "MenuNavigation"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "MenuNavigationHelp",
        ))],
    },
    Item {
        id: ItemId::InpDebounce,
        name: lookup_key("OptionsInput", "Debounce"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "DebounceHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const MACHINE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SelectProfile,
        label: lookup_key("OptionsMachine", "SelectProfile"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectColor,
        label: lookup_key("OptionsMachine", "SelectColor"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectStyle,
        label: lookup_key("OptionsMachine", "SelectStyle"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PreferredStyle,
        label: lookup_key("OptionsMachine", "PreferredStyle"),
        choices: &[
            localized_choice("OptionsMachine", "PreferredStyleSingle"),
            localized_choice("OptionsMachine", "PreferredStyleVersus"),
            localized_choice("OptionsMachine", "PreferredStyleDouble"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectPlayMode,
        label: lookup_key("OptionsMachine", "SelectPlayMode"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PreferredMode,
        label: lookup_key("OptionsMachine", "PreferredMode"),
        choices: &[
            localized_choice("OptionsMachine", "PreferredModeRegular"),
            localized_choice("OptionsMachine", "PreferredModeMarathon"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::EvalSummary,
        label: lookup_key("OptionsMachine", "EvalSummary"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::NameEntry,
        label: lookup_key("OptionsMachine", "NameEntry"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GameoverScreen,
        label: lookup_key("OptionsMachine", "GameoverScreen"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::WriteCurrentScreen,
        label: lookup_key("OptionsMachine", "WriteCurrentScreen"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MenuMusic,
        label: lookup_key("OptionsMachine", "MenuMusic"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MenuBackground,
        label: lookup_key("OptionsMachine", "MenuBackground"),
        choices: &[literal_choice("❤"), literal_choice("🌀")],
        inline: true,
    },
    SubRow {
        id: SubRowId::Replays,
        label: lookup_key("OptionsMachine", "Replays"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PerPlayerGlobalOffsets,
        label: lookup_key("OptionsMachine", "PerPlayerGlobalOffsets"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::KeyboardFeatures,
        label: lookup_key("OptionsMachine", "KeyboardFeatures"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::VideoBgs,
        label: lookup_key("OptionsMachine", "VideoBGs"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
];

pub const MACHINE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::MchSelectProfile,
        name: lookup_key("OptionsMachine", "SelectProfile"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectProfileHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectColor,
        name: lookup_key("OptionsMachine", "SelectColor"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectColorHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectStyle,
        name: lookup_key("OptionsMachine", "SelectStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectStyleHelp",
        ))],
    },
    Item {
        id: ItemId::MchPreferredStyle,
        name: lookup_key("OptionsMachine", "PreferredStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PreferredStyleHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectPlayMode,
        name: lookup_key("OptionsMachine", "SelectPlayMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectPlayModeHelp",
        ))],
    },
    Item {
        id: ItemId::MchPreferredMode,
        name: lookup_key("OptionsMachine", "PreferredMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PreferredModeHelp",
        ))],
    },
    Item {
        id: ItemId::MchEvalSummary,
        name: lookup_key("OptionsMachine", "EvalSummary"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "EvalSummaryHelp",
        ))],
    },
    Item {
        id: ItemId::MchNameEntry,
        name: lookup_key("OptionsMachine", "NameEntry"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "NameEntryHelp",
        ))],
    },
    Item {
        id: ItemId::MchGameoverScreen,
        name: lookup_key("OptionsMachine", "GameoverScreen"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "GameoverScreenHelp",
        ))],
    },
    Item {
        id: ItemId::MchWriteCurrentScreen,
        name: lookup_key("OptionsMachine", "WriteCurrentScreen"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "WriteCurrentScreenHelp",
        ))],
    },
    Item {
        id: ItemId::MchMenuMusic,
        name: lookup_key("OptionsMachine", "MenuMusic"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "MenuMusicHelp",
        ))],
    },
    Item {
        id: ItemId::MchMenuBackground,
        name: lookup_key("OptionsMachine", "MenuBackground"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "MenuBackgroundHelp",
        ))],
    },
    Item {
        id: ItemId::MchReplays,
        name: lookup_key("OptionsMachine", "Replays"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "ReplaysHelp",
        ))],
    },
    Item {
        id: ItemId::MchPerPlayerGlobalOffsets,
        name: lookup_key("OptionsMachine", "PerPlayerGlobalOffsets"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PerPlayerGlobalOffsetsHelp",
        ))],
    },
    Item {
        id: ItemId::MchKeyboardFeatures,
        name: lookup_key("OptionsMachine", "KeyboardFeatures"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "KeyboardFeaturesHelp",
        ))],
    },
    Item {
        id: ItemId::MchVideoBgs,
        name: lookup_key("OptionsMachine", "VideoBGs"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "VideoBgsHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const COURSE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::ShowRandomCourses,
        label: lookup_key("OptionsCourse", "ShowRandomCourses"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowMostPlayed,
        label: lookup_key("OptionsCourse", "ShowMostPlayed"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowIndividualScores,
        label: lookup_key("OptionsCourse", "ShowIndividualScores"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AutosubmitIndividual,
        label: lookup_key("OptionsCourse", "AutosubmitIndividual"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
];

pub const COURSE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::CrsShowRandom,
        name: lookup_key("OptionsCourse", "ShowRandomCourses"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "ShowRandomCoursesHelp",
        ))],
    },
    Item {
        id: ItemId::CrsShowMostPlayed,
        name: lookup_key("OptionsCourse", "ShowMostPlayed"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "ShowMostPlayedHelp",
        ))],
    },
    Item {
        id: ItemId::CrsShowIndividualScores,
        name: lookup_key("OptionsCourse", "ShowIndividualScores"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "ShowIndividualScoresHelp",
        ))],
    },
    Item {
        id: ItemId::CrsAutosubmitIndividual,
        name: lookup_key("OptionsCourse", "AutosubmitIndividual"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCourseHelp",
            "AutosubmitIndividualHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const GAMEPLAY_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::BgBrightness,
        label: lookup_key("OptionsGameplay", "BGBrightness"),
        choices: &[
            literal_choice("0%"),
            literal_choice("10%"),
            literal_choice("20%"),
            literal_choice("30%"),
            literal_choice("40%"),
            literal_choice("50%"),
            literal_choice("60%"),
            literal_choice("70%"),
            literal_choice("80%"),
            literal_choice("90%"),
            literal_choice("100%"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::CenteredP1Notefield,
        label: lookup_key("OptionsGameplay", "CenteredP1Notefield"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ZmodRatingBox,
        label: lookup_key("OptionsGameplay", "ZmodRatingBox"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::BpmDecimal,
        label: lookup_key("OptionsGameplay", "BpmDecimal"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AutoScreenshot,
        label: lookup_key("OptionsGameplay", "AutoScreenshot"),
        choices: &[
            literal_choice("PBs"),
            literal_choice("Fails"),
            literal_choice("Clears"),
            literal_choice("Quads"),
            literal_choice("Quints"),
        ],
        inline: true,
    },
];

pub const GAMEPLAY_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::GpBgBrightness,
        name: lookup_key("OptionsGameplay", "BGBrightness"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "BgBrightnessHelp",
        ))],
    },
    Item {
        id: ItemId::GpCenteredP1,
        name: lookup_key("OptionsGameplay", "CenteredP1Notefield"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "CenteredP1NotefieldHelp",
        ))],
    },
    Item {
        id: ItemId::GpZmodRatingBox,
        name: lookup_key("OptionsGameplay", "ZmodRatingBox"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "ZmodRatingBoxHelp",
        ))],
    },
    Item {
        id: ItemId::GpBpmDecimal,
        name: lookup_key("OptionsGameplay", "BpmDecimal"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "BpmDecimalHelp",
        ))],
    },
    Item {
        id: ItemId::GpAutoScreenshot,
        name: lookup_key("OptionsGameplay", "AutoScreenshot"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "AutoScreenshotHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const SOUND_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SoundDevice,
        label: lookup_key("OptionsSound", "SoundDevice"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::AudioOutputMode,
        label: lookup_key("OptionsSound", "AudioOutputMode"),
        choices: &[
            localized_choice("OptionsSound", "OutputModeAuto"),
            localized_choice("OptionsSound", "OutputModeShared"),
        ],
        inline: false,
    },
    #[cfg(target_os = "linux")]
    SubRow {
        id: SubRowId::LinuxAudioBackend,
        label: lookup_key("OptionsSound", "LinuxAudioBackend"),
        choices: SOUND_LINUX_BACKEND_CHOICES,
        inline: false,
    },
    #[cfg(target_os = "linux")]
    SubRow {
        id: SubRowId::AlsaExclusive,
        label: lookup_key("OptionsSound", "AlsaExclusive"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AudioSampleRate,
        label: lookup_key("OptionsSound", "AudioSampleRate"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MasterVolume,
        label: lookup_key("OptionsSound", "MasterVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::SfxVolume,
        label: lookup_key("OptionsSound", "SFXVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::AssistTickVolume,
        label: lookup_key("OptionsSound", "AssistTickVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MusicVolume,
        label: lookup_key("OptionsSound", "MusicVolume"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MineSounds,
        label: lookup_key("OptionsSound", "MineSounds"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GlobalOffset,
        label: lookup_key("OptionsSound", "GlobalOffset"),
        choices: &[literal_choice("0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::RateModPreservesPitch,
        label: lookup_key("OptionsSound", "RateModPreservesPitch"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
];

pub const SOUND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SndDevice,
        name: lookup_key("OptionsSound", "SoundDevice"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "SoundDeviceHelp",
        ))],
    },
    Item {
        id: ItemId::SndOutputMode,
        name: lookup_key("OptionsSound", "AudioOutputMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AudioOutputModeHelp",
        ))],
    },
    #[cfg(target_os = "linux")]
    Item {
        id: ItemId::SndLinuxBackend,
        name: lookup_key("OptionsSound", "LinuxAudioBackend"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "LinuxAudioBackendHelp",
        ))],
    },
    #[cfg(target_os = "linux")]
    Item {
        id: ItemId::SndAlsaExclusive,
        name: lookup_key("OptionsSound", "AlsaExclusive"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AlsaExclusiveHelp",
        ))],
    },
    Item {
        id: ItemId::SndSampleRate,
        name: lookup_key("OptionsSound", "AudioSampleRate"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AudioSampleRateHelp",
        ))],
    },
    Item {
        id: ItemId::SndMasterVolume,
        name: lookup_key("OptionsSound", "MasterVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "MasterVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndSfxVolume,
        name: lookup_key("OptionsSound", "SFXVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "SfxVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndAssistTickVolume,
        name: lookup_key("OptionsSound", "AssistTickVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "AssistTickVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndMusicVolume,
        name: lookup_key("OptionsSound", "MusicVolume"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "MusicVolumeHelp",
        ))],
    },
    Item {
        id: ItemId::SndMineSounds,
        name: lookup_key("OptionsSound", "MineSounds"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "MineSoundsHelp",
        ))],
    },
    Item {
        id: ItemId::SndGlobalOffset,
        name: lookup_key("OptionsSound", "GlobalOffset"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "GlobalOffsetHelp",
        ))],
    },
    Item {
        id: ItemId::SndRateModPitch,
        name: lookup_key("OptionsSound", "RateModPreservesPitch"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSoundHelp",
            "RateModPreservesPitchHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const SELECT_MUSIC_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::ShowBanners,
        label: lookup_key("OptionsSelectMusic", "ShowBanners"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowVideoBanners,
        label: lookup_key("OptionsSelectMusic", "ShowVideoBanners"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowBreakdown,
        label: lookup_key("OptionsSelectMusic", "ShowBreakdown"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::BreakdownStyle,
        label: lookup_key("OptionsSelectMusic", "BreakdownStyle"),
        choices: &[literal_choice("SL"), literal_choice("SN")],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowNativeLanguage,
        label: lookup_key("OptionsSelectMusic", "ShowNativeLanguage"),
        choices: &[
            localized_choice("OptionsSelectMusic", "NativeLanguageTranslit"),
            localized_choice("OptionsSelectMusic", "NativeLanguageNative"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MusicWheelSpeed,
        label: lookup_key("OptionsSelectMusic", "MusicWheelSpeed"),
        choices: &[
            localized_choice("OptionsSelectMusic", "WheelSpeedSlow"),
            localized_choice("OptionsSelectMusic", "WheelSpeedNormal"),
            localized_choice("OptionsSelectMusic", "WheelSpeedFast"),
            localized_choice("OptionsSelectMusic", "WheelSpeedFaster"),
            localized_choice("OptionsSelectMusic", "WheelSpeedRidiculous"),
            localized_choice("OptionsSelectMusic", "WheelSpeedLudicrous"),
            localized_choice("OptionsSelectMusic", "WheelSpeedPlaid"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MusicWheelStyle,
        label: lookup_key("OptionsSelectMusic", "MusicWheelStyle"),
        choices: &[literal_choice("ITG"), literal_choice("IIDX")],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowCdTitles,
        label: lookup_key("OptionsSelectMusic", "ShowCDTitles"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowWheelGrades,
        label: lookup_key("OptionsSelectMusic", "ShowWheelGrades"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowWheelLamps,
        label: lookup_key("OptionsSelectMusic", "ShowWheelLamps"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ItlRank,
        label: lookup_key("OptionsSelectMusic", "ITLRank"),
        choices: &[
            localized_choice("OptionsSelectMusic", "ItlRankNone"),
            localized_choice("OptionsSelectMusic", "ItlRankChart"),
            localized_choice("OptionsSelectMusic", "ItlRankOverall"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ItlWheelData,
        label: lookup_key("OptionsSelectMusic", "ITLWheelData"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("OptionsSelectMusic", "ItlWheelScore"),
            localized_choice("OptionsSelectMusic", "ItlWheelPointsScore"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::NewPackBadge,
        label: lookup_key("OptionsSelectMusic", "NewPackBadge"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("OptionsSelectMusic", "NewPackOpenPack"),
            localized_choice("OptionsSelectMusic", "NewPackHasScore"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowPatternInfo,
        label: lookup_key("OptionsSelectMusic", "ShowPatternInfo"),
        choices: &[
            localized_choice("Common", "Auto"),
            localized_choice("OptionsSelectMusic", "PatternInfoTech"),
            localized_choice("OptionsSelectMusic", "PatternInfoStamina"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ChartInfo,
        label: lookup_key("OptionsSelectMusic", "ChartInfo"),
        choices: &[
            localized_choice("OptionsSelectMusic", "ChartInfoPeakNPS"),
            localized_choice("OptionsSelectMusic", "ChartInfoMatrixRating"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MusicPreviews,
        label: lookup_key("OptionsSelectMusic", "MusicPreviews"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PreviewMarker,
        label: lookup_key("OptionsSelectMusic", "PreviewMarker"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::LoopMusic,
        label: lookup_key("OptionsSelectMusic", "LoopMusic"),
        choices: &[
            localized_choice("OptionsSelectMusic", "LoopMusicPlayOnce"),
            localized_choice("OptionsSelectMusic", "LoopMusicLoop"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowGameplayTimer,
        label: lookup_key("OptionsSelectMusic", "ShowGameplayTimer"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowGsBox,
        label: lookup_key("OptionsSelectMusic", "ShowGSBox"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GsBoxPlacement,
        label: lookup_key("OptionsSelectMusic", "GSBoxPlacement"),
        choices: &[
            localized_choice("Common", "Auto"),
            localized_choice("OptionsSelectMusic", "GsBoxStepPane"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GsBoxLeaderboards,
        label: lookup_key("OptionsSelectMusic", "GSBoxLeaderboards"),
        choices: &[
            localized_choice("OptionsSelectMusic", "ScoreboxCycleITG"),
            localized_choice("OptionsSelectMusic", "ScoreboxCycleEX"),
            localized_choice("OptionsSelectMusic", "ScoreboxCycleHEX"),
            localized_choice("OptionsSelectMusic", "ScoreboxCycleTournaments"),
        ],
        inline: true,
    },
];

pub const SELECT_MUSIC_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SmShowBanners,
        name: lookup_key("OptionsSelectMusic", "ShowBanners"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowBannersHelp",
        ))],
    },
    Item {
        id: ItemId::SmShowVideoBanners,
        name: lookup_key("OptionsSelectMusic", "ShowVideoBanners"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowVideoBannersHelp",
        ))],
    },
    Item {
        id: ItemId::SmShowBreakdown,
        name: lookup_key("OptionsSelectMusic", "ShowBreakdown"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowBreakdownHelp",
        ))],
    },
    Item {
        id: ItemId::SmBreakdownStyle,
        name: lookup_key("OptionsSelectMusic", "BreakdownStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "BreakdownStyleHelp",
        ))],
    },
    Item {
        id: ItemId::SmNativeLanguage,
        name: lookup_key("OptionsSelectMusic", "ShowNativeLanguage"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowNativeLanguageHelp",
        ))],
    },
    Item {
        id: ItemId::SmWheelSpeed,
        name: lookup_key("OptionsSelectMusic", "MusicWheelSpeed"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "MusicWheelSpeedHelp",
        ))],
    },
    Item {
        id: ItemId::SmWheelStyle,
        name: lookup_key("OptionsSelectMusic", "MusicWheelStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "MusicWheelStyleHelp",
        ))],
    },
    Item {
        id: ItemId::SmCdTitles,
        name: lookup_key("OptionsSelectMusic", "ShowCDTitles"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowCdTitlesHelp",
        ))],
    },
    Item {
        id: ItemId::SmWheelGrades,
        name: lookup_key("OptionsSelectMusic", "ShowWheelGrades"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowWheelGradesHelp",
        ))],
    },
    Item {
        id: ItemId::SmWheelLamps,
        name: lookup_key("OptionsSelectMusic", "ShowWheelLamps"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowWheelLampsHelp",
        ))],
    },
    Item {
        id: ItemId::SmWheelItlRank,
        name: lookup_key("OptionsSelectMusic", "ITLRank"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ITLRankHelp",
        ))],
    },
    Item {
        id: ItemId::SmWheelItl,
        name: lookup_key("OptionsSelectMusic", "ITLWheelData"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ItlWheelDataHelp",
        ))],
    },
    Item {
        id: ItemId::SmNewPackBadge,
        name: lookup_key("OptionsSelectMusic", "NewPackBadge"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "NewPackBadgeHelp",
        ))],
    },
    Item {
        id: ItemId::SmPatternInfo,
        name: lookup_key("OptionsSelectMusic", "ShowPatternInfo"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowPatternInfoHelp",
        ))],
    },
    Item {
        id: ItemId::SmChartInfo,
        name: lookup_key("OptionsSelectMusic", "ChartInfo"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ChartInfoHelp",
        ))],
    },
    Item {
        id: ItemId::SmPreviews,
        name: lookup_key("OptionsSelectMusic", "MusicPreviews"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "MusicPreviewsHelp",
        ))],
    },
    Item {
        id: ItemId::SmPreviewMarker,
        name: lookup_key("OptionsSelectMusic", "PreviewMarker"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "PreviewMarkerHelp",
        ))],
    },
    Item {
        id: ItemId::SmPreviewLoop,
        name: lookup_key("OptionsSelectMusic", "LoopMusic"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "LoopMusicHelp",
        ))],
    },
    Item {
        id: ItemId::SmGameplayTimer,
        name: lookup_key("OptionsSelectMusic", "ShowGameplayTimer"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowGameplayTimerHelp",
        ))],
    },
    Item {
        id: ItemId::SmShowRivals,
        name: lookup_key("OptionsSelectMusic", "ShowGSBox"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowGsBoxHelp",
        ))],
    },
    Item {
        id: ItemId::SmScoreboxPlacement,
        name: lookup_key("OptionsSelectMusic", "GSBoxPlacement"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "GsBoxPlacementHelp",
        ))],
    },
    Item {
        id: ItemId::SmScoreboxCycle,
        name: lookup_key("OptionsSelectMusic", "GSBoxLeaderboards"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "GsBoxLeaderboardsHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const ADVANCED_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::DefaultFailType,
        label: lookup_key("OptionsAdvanced", "DefaultFailType"),
        choices: &[
            localized_choice("OptionsAdvanced", "FailImmediate"),
            localized_choice("OptionsAdvanced", "FailImmediateContinue"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::BannerCache,
        label: lookup_key("OptionsAdvanced", "BannerCache"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::CdTitleCache,
        label: lookup_key("OptionsAdvanced", "CDTitleCache"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SongParsingThreads,
        label: lookup_key("OptionsAdvanced", "SongParsingThreads"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::CacheSongs,
        label: lookup_key("OptionsAdvanced", "CacheSongs"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::FastLoad,
        label: lookup_key("OptionsAdvanced", "FastLoad"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
];

pub const ADVANCED_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::AdvDefaultFailType,
        name: lookup_key("OptionsAdvanced", "DefaultFailType"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsAdvancedHelp",
            "DefaultFailTypeHelp",
        ))],
    },
    Item {
        id: ItemId::AdvBannerCache,
        name: lookup_key("OptionsAdvanced", "BannerCache"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsAdvancedHelp",
            "BannerCacheHelp",
        ))],
    },
    Item {
        id: ItemId::AdvCdTitleCache,
        name: lookup_key("OptionsAdvanced", "CDTitleCache"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsAdvancedHelp",
            "CdTitleCacheHelp",
        ))],
    },
    Item {
        id: ItemId::AdvSongParsingThreads,
        name: lookup_key("OptionsAdvanced", "SongParsingThreads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsAdvancedHelp",
            "SongParsingThreadsHelp",
        ))],
    },
    Item {
        id: ItemId::AdvCacheSongs,
        name: lookup_key("OptionsAdvanced", "CacheSongs"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsAdvancedHelp",
            "CacheSongsHelp",
        ))],
    },
    Item {
        id: ItemId::AdvFastLoad,
        name: lookup_key("OptionsAdvanced", "FastLoad"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsAdvancedHelp",
            "FastLoadHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const GROOVESTATS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::EnableGrooveStats,
        label: lookup_key("OptionsGrooveStats", "EnableGrooveStats"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::EnableBoogieStats,
        label: lookup_key("OptionsGrooveStats", "EnableBoogieStats"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GsSubmitFails,
        label: lookup_key("OptionsGrooveStats", "GsSubmitFails"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AutoPopulateScores,
        label: lookup_key("OptionsGrooveStats", "AutoPopulateScores"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AutoDownloadUnlocks,
        label: lookup_key("OptionsGrooveStats", "AutoDownloadUnlocks"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SeparateUnlocksByPlayer,
        label: lookup_key("OptionsGrooveStats", "SeparateUnlocksByPlayer"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
];

pub const ARROWCLOUD_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::EnableArrowCloud,
        label: lookup_key("OptionsGrooveStats", "EnableArrowCloud"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ArrowCloudSubmitFails,
        label: lookup_key("OptionsGrooveStats", "ArrowCloudSubmitFails"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
];

pub const ONLINE_SCORING_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::GsBsOptions,
        label: lookup_key("OptionsOnlineScoring", "GsBsOptions"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::ArrowCloudOptions,
        label: lookup_key("OptionsOnlineScoring", "ArrowCloudOptions"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::ScoreImport,
        label: lookup_key("OptionsOnlineScoring", "ScoreImport"),
        choices: &[],
        inline: false,
    },
];

pub const NULL_OR_DIE_MENU_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::NullOrDieOptions,
        label: lookup_key("OptionsOnlineScoring", "NullOrDieOptions"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::SyncPacks,
        label: lookup_key("OptionsOnlineScoring", "SyncPacks"),
        choices: &[],
        inline: false,
    },
];

pub const NULL_OR_DIE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SyncGraph,
        label: lookup_key("OptionsNullOrDie", "SyncGraph"),
        choices: &[
            localized_choice("OptionsNullOrDie", "SyncGraphFrequency"),
            localized_choice("OptionsNullOrDie", "SyncGraphBeatIndex"),
            localized_choice("OptionsNullOrDie", "SyncGraphPostKernelFingerprint"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::SyncConfidence,
        label: lookup_key("OptionsNullOrDie", "SyncConfidence"),
        choices: &[
            literal_choice("0%"),
            literal_choice("5%"),
            literal_choice("10%"),
            literal_choice("15%"),
            literal_choice("20%"),
            literal_choice("25%"),
            literal_choice("30%"),
            literal_choice("35%"),
            literal_choice("40%"),
            literal_choice("45%"),
            literal_choice("50%"),
            literal_choice("55%"),
            literal_choice("60%"),
            literal_choice("65%"),
            literal_choice("70%"),
            literal_choice("75%"),
            literal_choice("80%"),
            literal_choice("85%"),
            literal_choice("90%"),
            literal_choice("95%"),
            literal_choice("100%"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::PackSyncThreads,
        label: lookup_key("OptionsNullOrDie", "PackSyncThreads"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Fingerprint,
        label: lookup_key("OptionsNullOrDie", "Fingerprint"),
        choices: &[literal_choice("50.0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Window,
        label: lookup_key("OptionsNullOrDie", "Window"),
        choices: &[literal_choice("10.0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Step,
        label: lookup_key("OptionsNullOrDie", "Step"),
        choices: &[literal_choice("0.2 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MagicOffset,
        label: lookup_key("OptionsNullOrDie", "MagicOffset"),
        choices: &[literal_choice("0.0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::KernelTarget,
        label: lookup_key("OptionsNullOrDie", "KernelTarget"),
        choices: &[
            localized_choice("OptionsNullOrDie", "KernelTargetDigest"),
            localized_choice("OptionsNullOrDie", "KernelTargetAccumulator"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::KernelType,
        label: lookup_key("OptionsNullOrDie", "KernelType"),
        choices: &[
            localized_choice("OptionsNullOrDie", "KernelTypeRising"),
            localized_choice("OptionsNullOrDie", "KernelTypeLoudest"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::FullSpectrogram,
        label: lookup_key("OptionsNullOrDie", "FullSpectrogram"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: false,
    },
];

pub const SYNC_PACK_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SyncPackPack,
        label: lookup_key("OptionsSyncPack", "SyncPackPack"),
        choices: &[localized_choice("OptionsSyncPack", "AllPacks")],
        inline: false,
    },
    SubRow {
        id: SubRowId::SyncPackStart,
        label: lookup_key("OptionsSyncPack", "SyncPackStart"),
        choices: &[localized_choice("Common", "Start")],
        inline: false,
    },
];

pub const SCORE_IMPORT_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::ScoreImportEndpoint,
        label: lookup_key("OptionsScoreImport", "ScoreImportEndpoint"),
        choices: &[
            literal_choice("GrooveStats"),
            literal_choice("BoogieStats"),
            literal_choice("ArrowCloud"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ScoreImportProfile,
        label: lookup_key("OptionsScoreImport", "ScoreImportProfile"),
        choices: &[localized_choice("OptionsScoreImport", "NoEligibleProfiles")],
        inline: false,
    },
    SubRow {
        id: SubRowId::ScoreImportPack,
        label: lookup_key("OptionsScoreImport", "ScoreImportPack"),
        choices: &[localized_choice("OptionsScoreImport", "AllPacks")],
        inline: false,
    },
    SubRow {
        id: SubRowId::ScoreImportOnlyMissing,
        label: lookup_key("OptionsScoreImport", "ScoreImportOnlyMissing"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ScoreImportStart,
        label: lookup_key("OptionsScoreImport", "ScoreImportStart"),
        choices: &[localized_choice("Common", "Start")],
        inline: false,
    },
];

pub const GROOVESTATS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::GsEnable,
        name: lookup_key("OptionsGrooveStats", "EnableGrooveStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "EnableGrooveStatsHelp",
        ))],
    },
    Item {
        id: ItemId::GsEnableBoogie,
        name: lookup_key("OptionsGrooveStats", "EnableBoogieStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "EnableBoogieStatsHelp",
        ))],
    },
    Item {
        id: ItemId::GsSubmitFails,
        name: lookup_key("OptionsGrooveStats", "GsSubmitFails"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "GsSubmitFailsHelp",
        ))],
    },
    Item {
        id: ItemId::GsAutoPopulate,
        name: lookup_key("OptionsGrooveStats", "AutoPopulateScores"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "AutoPopulateScoresHelp",
        ))],
    },
    Item {
        id: ItemId::GsAutoDownloadUnlocks,
        name: lookup_key("OptionsGrooveStats", "AutoDownloadUnlocks"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "AutoDownloadUnlocksHelp",
        ))],
    },
    Item {
        id: ItemId::GsSeparateUnlocks,
        name: lookup_key("OptionsGrooveStats", "SeparateUnlocksByPlayer"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "SeparateUnlocksByPlayerHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const ARROWCLOUD_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::AcEnable,
        name: lookup_key("OptionsGrooveStats", "EnableArrowCloud"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "EnableArrowCloudHelp",
        ))],
    },
    Item {
        id: ItemId::AcSubmitFails,
        name: lookup_key("OptionsGrooveStats", "ArrowCloudSubmitFails"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "ArrowCloudSubmitFailsHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const ONLINE_SCORING_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::OsGsBsOptions,
        name: lookup_key("OptionsOnlineScoring", "GsBsOptions"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "GsBsOptionsHelp",
        ))],
    },
    Item {
        id: ItemId::OsArrowCloudOptions,
        name: lookup_key("OptionsOnlineScoring", "ArrowCloudOptions"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "ArrowCloudOptionsHelp",
        ))],
    },
    Item {
        id: ItemId::OsScoreImport,
        name: lookup_key("OptionsOnlineScoring", "ScoreImport"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "ScoreImportHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const NULL_OR_DIE_MENU_ITEMS: &[Item] = &[
    Item {
        id: ItemId::NodOptions,
        name: lookup_key("OptionsOnlineScoring", "NullOrDieOptions"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "NullOrDieOptionsHelp",
        ))],
    },
    Item {
        id: ItemId::NodSyncPacks,
        name: lookup_key("OptionsOnlineScoring", "SyncPacks"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "SyncPacksHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const NULL_OR_DIE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::NodSyncGraph,
        name: lookup_key("OptionsNullOrDie", "SyncGraph"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "SyncGraphHelp",
        ))],
    },
    Item {
        id: ItemId::NodSyncConfidence,
        name: lookup_key("OptionsNullOrDie", "SyncConfidence"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "SyncConfidenceHelp",
        ))],
    },
    Item {
        id: ItemId::NodPackSyncThreads,
        name: lookup_key("OptionsNullOrDie", "PackSyncThreads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "PackSyncThreadsHelp",
        ))],
    },
    Item {
        id: ItemId::NodFingerprint,
        name: lookup_key("OptionsNullOrDie", "Fingerprint"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "FingerprintHelp",
        ))],
    },
    Item {
        id: ItemId::NodWindow,
        name: lookup_key("OptionsNullOrDie", "Window"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "WindowHelp",
        ))],
    },
    Item {
        id: ItemId::NodStep,
        name: lookup_key("OptionsNullOrDie", "Step"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "StepHelp",
        ))],
    },
    Item {
        id: ItemId::NodMagicOffset,
        name: lookup_key("OptionsNullOrDie", "MagicOffset"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "MagicOffsetHelp",
        ))],
    },
    Item {
        id: ItemId::NodKernelTarget,
        name: lookup_key("OptionsNullOrDie", "KernelTarget"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "KernelTargetHelp",
        ))],
    },
    Item {
        id: ItemId::NodKernelType,
        name: lookup_key("OptionsNullOrDie", "KernelType"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "KernelTypeHelp",
        ))],
    },
    Item {
        id: ItemId::NodFullSpectrogram,
        name: lookup_key("OptionsNullOrDie", "FullSpectrogram"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "FullSpectrogramHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const SYNC_PACK_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SpPack,
        name: lookup_key("OptionsSyncPack", "SyncPackPack"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSyncPackHelp",
            "SyncPackPackHelp",
        ))],
    },
    Item {
        id: ItemId::SpStart,
        name: lookup_key("OptionsSyncPack", "SyncPackStart"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSyncPackHelp",
            "SyncPackStartHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

pub const SCORE_IMPORT_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::SiEndpoint,
        name: lookup_key("OptionsScoreImport", "ScoreImportEndpoint"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsScoreImportHelp",
            "ScoreImportEndpointHelp",
        ))],
    },
    Item {
        id: ItemId::SiProfile,
        name: lookup_key("OptionsScoreImport", "ScoreImportProfile"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsScoreImportHelp",
            "ScoreImportProfileHelp",
        ))],
    },
    Item {
        id: ItemId::SiPack,
        name: lookup_key("OptionsScoreImport", "ScoreImportPack"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsScoreImportHelp",
            "ScoreImportPackHelp",
        ))],
    },
    Item {
        id: ItemId::SiOnlyMissing,
        name: lookup_key("OptionsScoreImport", "ScoreImportOnlyMissing"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsScoreImportHelp",
            "ScoreImportOnlyMissingHelp",
        ))],
    },
    Item {
        id: ItemId::SiStart,
        name: lookup_key("OptionsScoreImport", "ScoreImportStart"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsScoreImportHelp",
            "ScoreImportStartHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

/// Returns `true` when the given submenu row should be treated as disabled
/// (non-interactive and visually dimmed). Add new cases here for any row
/// that should be conditionally locked based on runtime state.
fn is_submenu_row_disabled(kind: SubmenuKind, id: SubRowId) -> bool {
    match (kind, id) {
        (SubmenuKind::InputBackend, SubRowId::MenuButtons) => {
            !crate::engine::input::any_player_has_dedicated_menu_buttons_for_mode(
                config::get().three_key_navigation,
            )
        }
        _ => false,
    }
}

const fn submenu_rows(kind: SubmenuKind) -> &'static [SubRow] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ROWS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ROWS,
        SubmenuKind::Input => INPUT_OPTIONS_ROWS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ROWS,
        SubmenuKind::OnlineScoring => ONLINE_SCORING_OPTIONS_ROWS,
        SubmenuKind::NullOrDie => NULL_OR_DIE_MENU_ROWS,
        SubmenuKind::NullOrDieOptions => NULL_OR_DIE_OPTIONS_ROWS,
        SubmenuKind::SyncPacks => SYNC_PACK_OPTIONS_ROWS,
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

const fn submenu_items(kind: SubmenuKind) -> &'static [Item] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ITEMS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ITEMS,
        SubmenuKind::Input => INPUT_OPTIONS_ITEMS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ITEMS,
        SubmenuKind::OnlineScoring => ONLINE_SCORING_OPTIONS_ITEMS,
        SubmenuKind::NullOrDie => NULL_OR_DIE_MENU_ITEMS,
        SubmenuKind::NullOrDieOptions => NULL_OR_DIE_OPTIONS_ITEMS,
        SubmenuKind::SyncPacks => SYNC_PACK_OPTIONS_ITEMS,
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
        SubmenuKind::NullOrDie => "NULL-OR-DIE OPTIONS",
        SubmenuKind::NullOrDieOptions => "NULL-OR-DIE OPTIONS",
        SubmenuKind::SyncPacks => "SYNC PACKS",
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
        .map(std::num::NonZero::get)
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
                tr("Common", "Auto").to_string()
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

fn submenu_visible_row_indices(state: &State, kind: SubmenuKind, rows: &[SubRow]) -> Vec<usize> {
    match kind {
        SubmenuKind::Graphics => {
            let show_sw = graphics_show_software_threads(state);
            let show_present_mode = graphics_show_present_mode(state);
            let show_max_fps = graphics_show_max_fps(state);
            let show_max_fps_value = graphics_show_max_fps_value(state);
            rows.iter()
                .enumerate()
                .filter_map(|(idx, row)| {
                    if row.id == SubRowId::SoftwareRendererThreads && !show_sw {
                        None
                    } else if row.id == SubRowId::PresentMode && !show_present_mode {
                        None
                    } else if row.id == SubRowId::MaxFps && !show_max_fps {
                        None
                    } else if row.id == SubRowId::MaxFpsValue && !show_max_fps_value {
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
                    } else if idx == SELECT_MUSIC_SCOREBOX_PLACEMENT_ROW_INDEX && !show_scorebox {
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
        #[cfg(target_os = "linux")]
        SubmenuKind::Sound => rows
            .iter()
            .enumerate()
            .filter_map(|(idx, row)| {
                if row.id == SubRowId::AlsaExclusive && !sound_show_alsa_exclusive(state) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect(),
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
    DISPLAY_ASPECT_RATIO_CHOICES
        .get(idx)
        .or(Some(&DISPLAY_ASPECT_RATIO_CHOICES[0]))
        .and_then(|c| c.as_str_static())
        .unwrap_or("16:9")
}

fn inferred_aspect_choice(width: u32, height: u32) -> usize {
    if height == 0 {
        return 0;
    }

    if let Some(idx) = DISPLAY_ASPECT_RATIO_CHOICES.iter().position(|c| {
        c.as_str_static()
            .map_or(false, |label| aspect_matches(width, height, label))
    }) {
        return idx;
    }

    let ratio = width as f32 / height as f32;
    let mut best_idx = 0;
    let mut best_delta = f32::INFINITY;
    for (idx, choice) in DISPLAY_ASPECT_RATIO_CHOICES.iter().enumerate() {
        let Some(label) = choice.as_str_static() else {
            continue;
        };
        let target = match label {
            "16:9" => 16.0 / 9.0,
            "16:10" => 16.0 / 10.0,
            "4:3" => 4.0 / 3.0,
            "1:1" => 1.0,
            _ => continue,
        };
        let delta = (ratio - target).abs();
        if delta < best_delta {
            best_delta = delta;
            best_idx = idx;
        }
    }
    best_idx
}

fn sync_display_aspect_ratio(state: &mut State, width: u32, height: u32) {
    let idx = inferred_aspect_choice(width, height);
    if let Some(slot) = state
        .sub_choice_indices_graphics
        .get_mut(DISPLAY_ASPECT_RATIO_ROW_INDEX)
    {
        *slot = idx;
    }
    if let Some(slot) = state
        .sub_cursor_indices_graphics
        .get_mut(DISPLAY_ASPECT_RATIO_ROW_INDEX)
    {
        *slot = idx;
    }
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

fn installed_pack_options(all_label: &str) -> (Vec<String>, Vec<Option<String>>) {
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
    choices.push(all_label.to_string());
    filters.push(None);
    for (display_name, group_name) in packs {
        choices.push(display_name);
        filters.push(Some(group_name));
    }
    (choices, filters)
}

fn score_import_pack_options() -> (Vec<String>, Vec<Option<String>>) {
    installed_pack_options(&tr("OptionsScoreImport", "AllPacks"))
}

fn sync_pack_options() -> (Vec<String>, Vec<Option<String>>) {
    installed_pack_options(&tr("OptionsSyncPack", "AllPacks"))
}

fn load_score_import_profiles() -> Vec<ScoreImportProfileConfig> {
    let mut profiles = Vec::new();
    for summary in profile::scan_local_profiles() {
        let profile_dir = dirs::app_dirs().profiles_root().join(summary.id.as_str());
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
            .push(tr("OptionsScoreImport", "NoEligibleProfiles").to_string());
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

fn refresh_sync_pack_options(state: &mut State) {
    let (choices, filters) = sync_pack_options();
    state.sync_pack_choices = choices;
    state.sync_pack_filters = filters;
    let max_idx = state.sync_pack_choices.len().saturating_sub(1);
    if let Some(slot) = state
        .sub_choice_indices_sync_packs
        .get_mut(SYNC_PACK_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
    if let Some(slot) = state
        .sub_cursor_indices_sync_packs
        .get_mut(SYNC_PACK_ROW_PACK_INDEX)
    {
        *slot = (*slot).min(max_idx);
    }
}

fn refresh_score_import_options(state: &mut State) {
    state.score_import_profiles = load_score_import_profiles();
    refresh_score_import_profile_options(state);
    refresh_score_import_pack_options(state);
}

fn refresh_null_or_die_options(state: &mut State) {
    refresh_sync_pack_options(state);
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
        .cloned()
        .flatten()
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
        .cloned()
        .flatten()?;
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
        .unwrap_or_else(|| tr("OptionsScoreImport", "AllPacks").to_string());
    let only_missing_gs_scores = score_import_only_missing_gs_scores(state);
    Some(ScoreImportSelection {
        endpoint,
        profile: profile_cfg,
        pack_group,
        pack_label,
        only_missing_gs_scores,
    })
}

fn selected_sync_pack_selection(state: &State) -> SyncPackSelection {
    let pack_idx = state
        .sub_choice_indices_sync_packs
        .get(SYNC_PACK_ROW_PACK_INDEX)
        .copied()
        .unwrap_or(0)
        .min(state.sync_pack_filters.len().saturating_sub(1));
    let pack_group = state.sync_pack_filters.get(pack_idx).cloned().flatten();
    let pack_label = state
        .sync_pack_choices
        .get(pack_idx)
        .cloned()
        .unwrap_or_else(|| tr("OptionsSyncPack", "AllPacks").to_string());
    SyncPackSelection {
        pack_group,
        pack_label,
    }
}

fn row_choices(
    state: &State,
    kind: SubmenuKind,
    rows: &[SubRow],
    row_idx: usize,
) -> Vec<Cow<'static, str>> {
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::System)
        && row.id == SubRowId::DefaultNoteSkin
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
        if row.id == SubRowId::SoftwareRendererThreads {
            return state
                .software_thread_labels
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::MaxFpsValue {
            return state
                .max_fps_labels
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::DisplayMode {
            return state
                .display_mode_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::DisplayResolution {
            return state
                .resolution_choices
                .iter()
                .map(|&(w, h)| Cow::Owned(format!("{w}x{h}")))
                .collect();
        }
        if row.id == SubRowId::RefreshRate {
            return state
                .refresh_rate_choices
                .iter()
                .map(|&mhz| {
                    if mhz == 0 {
                        Cow::Owned(tr("Common", "Default").to_string())
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
        && row.id == SubRowId::SongParsingThreads
    {
        return state
            .software_thread_labels
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::NullOrDieOptions)
        && row.id == SubRowId::PackSyncThreads
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
        if row.id == SubRowId::SoundDevice {
            return state
                .sound_device_options
                .iter()
                .map(|opt| Cow::Owned(opt.label.clone()))
                .collect();
        }
        if row.id == SubRowId::AudioSampleRate {
            return sound_sample_rate_choices(state)
                .into_iter()
                .map(|rate| match rate {
                    None => Cow::Owned(tr("Common", "Auto").to_string()),
                    Some(hz) => Cow::Owned(format!("{hz} Hz")),
                })
                .collect();
        }
        #[cfg(target_os = "linux")]
        if row.id == SubRowId::LinuxAudioBackend {
            return state
                .linux_backend_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::ScoreImport)
    {
        if row.id == SubRowId::ScoreImportProfile {
            return state
                .score_import_profile_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::ScoreImportPack {
            return state
                .score_import_pack_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::SyncPacks)
        && row.id == SubRowId::SyncPackPack
    {
        return state
            .sync_pack_choices
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
    }
    rows.get(row_idx)
        .map(|row| {
            row.choices
                .iter()
                .map(|c| Cow::Owned(c.get().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn submenu_display_choice_texts(
    state: &State,
    kind: SubmenuKind,
    rows: &[SubRow],
    row_idx: usize,
) -> Vec<Cow<'static, str>> {
    let mut choice_texts = row_choices(state, kind, rows, row_idx);
    let Some(row) = rows.get(row_idx) else {
        return choice_texts;
    };
    if choice_texts.is_empty() {
        return choice_texts;
    }
    if row.id == SubRowId::GlobalOffset {
        choice_texts[0] = Cow::Owned(format_ms(state.global_offset_ms));
    } else if row.id == SubRowId::MasterVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.master_volume_pct));
    } else if row.id == SubRowId::SfxVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.sfx_volume_pct));
    } else if row.id == SubRowId::AssistTickVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.assist_tick_volume_pct));
    } else if row.id == SubRowId::MusicVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.music_volume_pct));
    } else if row.id == SubRowId::VisualDelay {
        choice_texts[0] = Cow::Owned(format_ms(state.visual_delay_ms));
    } else if row.id == SubRowId::Debounce {
        choice_texts[0] = Cow::Owned(format_ms(state.input_debounce_ms));
    } else if row.id == SubRowId::Fingerprint {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_fingerprint_tenths));
    } else if row.id == SubRowId::Window {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_window_tenths));
    } else if row.id == SubRowId::Step {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_step_tenths));
    } else if row.id == SubRowId::MagicOffset {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_magic_offset_tenths));
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
    let row = rows.get(row_idx)?;
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
    if widths.len() != texts.len() {
        widths.clear();
        widths.extend(
            texts
                .iter()
                .map(|text| (text.chars().count().max(1) as f32) * 8.0 * value_zoom),
        );
    }
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
    if let Some(layout) = cache.get(row_idx).cloned().flatten() {
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

fn set_choice_by_id(choice_indices: &mut Vec<usize>, rows: &[SubRow], id: SubRowId, idx: usize) {
    if let Some(pos) = rows.iter().position(|r| r.id == id)
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

fn sound_row_index(id: SubRowId) -> Option<usize> {
    SOUND_OPTIONS_ROWS.iter().position(|row| row.id == id)
}

fn selected_sound_device_choice(state: &State) -> usize {
    sound_row_index(SubRowId::SoundDevice)
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
        config::AudioOutputMode::Shared | config::AudioOutputMode::Exclusive => 1,
    }
}

fn audio_output_mode_from_choice(idx: usize) -> config::AudioOutputMode {
    match idx {
        1 => config::AudioOutputMode::Shared,
        _ => config::AudioOutputMode::Auto,
    }
}

#[cfg(target_os = "linux")]
#[inline(always)]
const fn alsa_exclusive_choice_index(mode: config::AudioOutputMode) -> usize {
    if matches!(mode, config::AudioOutputMode::Exclusive) {
        1
    } else {
        0
    }
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn selected_audio_output_mode(state: &State) -> config::AudioOutputMode {
    sound_row_index(SubRowId::AudioOutputMode)
        .and_then(|idx| state.sub_choice_indices_sound.get(idx).copied())
        .map(audio_output_mode_from_choice)
        .unwrap_or(config::AudioOutputMode::Auto)
}

#[cfg(target_os = "linux")]
fn linux_audio_backend_choice_index(state: &State, backend: config::LinuxAudioBackend) -> usize {
    let target = linux_backend_label(backend).to_string();
    state
        .linux_backend_choices
        .iter()
        .position(|choice| *choice == target)
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn linux_audio_backend_from_choice(state: &State, idx: usize) -> config::LinuxAudioBackend {
    match state
        .linux_backend_choices
        .get(idx)
        .map(String::as_str)
        .unwrap_or("Auto")
    {
        "PipeWire" => config::LinuxAudioBackend::PipeWire,
        "PulseAudio" => config::LinuxAudioBackend::PulseAudio,
        "JACK" => config::LinuxAudioBackend::Jack,
        "ALSA" => config::LinuxAudioBackend::Alsa,
        _ => config::LinuxAudioBackend::Auto,
    }
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn selected_linux_audio_backend(state: &State) -> config::LinuxAudioBackend {
    sound_row_index(SubRowId::LinuxAudioBackend)
        .and_then(|idx| state.sub_choice_indices_sound.get(idx).copied())
        .map(|idx| linux_audio_backend_from_choice(state, idx))
        .unwrap_or(config::LinuxAudioBackend::Auto)
}

#[cfg(target_os = "linux")]
#[inline(always)]
fn sound_show_alsa_exclusive(state: &State) -> bool {
    matches!(
        selected_linux_audio_backend(state),
        config::LinuxAudioBackend::Alsa
    )
}

#[cfg(target_os = "linux")]
fn sound_parent_row(actual_idx: usize) -> Option<usize> {
    let child_idx = sound_row_index(SubRowId::AlsaExclusive)?;
    if actual_idx != child_idx {
        return None;
    }
    sound_row_index(SubRowId::LinuxAudioBackend)
}

fn set_sound_choice_index(state: &mut State, id: SubRowId, idx: usize) {
    let Some(row_idx) = sound_row_index(id) else {
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
const fn auto_screenshot_cursor_index(mask: u8) -> usize {
    if (mask & config::AUTO_SS_PBS) != 0 {
        0
    } else if (mask & config::AUTO_SS_FAILS) != 0 {
        1
    } else if (mask & config::AUTO_SS_CLEARS) != 0 {
        2
    } else if (mask & config::AUTO_SS_QUADS) != 0 {
        3
    } else if (mask & config::AUTO_SS_QUINTS) != 0 {
        4
    } else {
        0
    }
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

#[inline(always)]
const fn auto_screenshot_bit_from_choice(idx: usize) -> u8 {
    config::auto_screenshot_bit(idx)
}

#[inline(always)]
fn auto_screenshot_enabled_mask() -> u8 {
    config::get().auto_screenshot_eval
}

fn toggle_auto_screenshot_option(state: &mut State, choice_idx: usize) {
    let bit = auto_screenshot_bit_from_choice(choice_idx);
    if bit == 0 {
        return;
    }
    let mut mask = config::get().auto_screenshot_eval;
    if (mask & bit) != 0 {
        mask &= !bit;
    } else {
        mask |= bit;
    }
    config::update_auto_screenshot_eval(mask);

    let clamped = choice_idx.min(config::AUTO_SS_NUM_FLAGS.saturating_sub(1));
    set_choice_by_id(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AutoScreenshot,
        clamped,
    );
    set_choice_by_id(
        &mut state.sub_cursor_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AutoScreenshot,
        clamped,
    );
    audio::play_sfx("assets/sounds/change_value.ogg");
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

const fn sync_confidence_choice_index(percent: u8) -> usize {
    let capped = if percent > 100 { 100 } else { percent };
    ((capped as usize) + 2) / 5
}

const fn sync_confidence_from_choice(idx: usize) -> u8 {
    let capped = if idx > 20 { 20 } else { idx };
    capped as u8 * 5
}

const fn null_or_die_kernel_target_choice_index(target: KernelTarget) -> usize {
    match target {
        KernelTarget::Digest => 0,
        KernelTarget::Accumulator => 1,
    }
}

const fn null_or_die_kernel_target_from_choice(idx: usize) -> KernelTarget {
    match idx {
        1 => KernelTarget::Accumulator,
        _ => KernelTarget::Digest,
    }
}

const fn null_or_die_kernel_type_choice_index(kind: BiasKernel) -> usize {
    match kind {
        BiasKernel::Rising => 0,
        BiasKernel::Loudest => 1,
    }
}

const fn null_or_die_kernel_type_from_choice(idx: usize) -> BiasKernel {
    match idx {
        1 => BiasKernel::Loudest,
        _ => BiasKernel::Rising,
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

const fn language_choice_index(flag: config::LanguageFlag) -> usize {
    match flag {
        config::LanguageFlag::Auto | config::LanguageFlag::English => 0,
        config::LanguageFlag::German => 1,
        config::LanguageFlag::Spanish => 2,
        config::LanguageFlag::French => 3,
        config::LanguageFlag::Italian => 4,
        config::LanguageFlag::Japanese => 5,
        config::LanguageFlag::Polish => 6,
        config::LanguageFlag::PortugueseBrazil => 7,
        config::LanguageFlag::Russian => 8,
        config::LanguageFlag::Swedish => 9,
        config::LanguageFlag::Pseudo => 10,
    }
}

const fn language_flag_from_choice(idx: usize) -> config::LanguageFlag {
    match idx {
        1 => config::LanguageFlag::German,
        2 => config::LanguageFlag::Spanish,
        3 => config::LanguageFlag::French,
        4 => config::LanguageFlag::Italian,
        5 => config::LanguageFlag::Japanese,
        6 => config::LanguageFlag::Polish,
        7 => config::LanguageFlag::PortugueseBrazil,
        8 => config::LanguageFlag::Russian,
        9 => config::LanguageFlag::Swedish,
        10 => config::LanguageFlag::Pseudo,
        _ => config::LanguageFlag::English,
    }
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

#[inline(always)]
const fn select_music_chart_info_mask(peak_nps: bool, matrix_rating: bool) -> u8 {
    (peak_nps as u8) | ((matrix_rating as u8) << 1)
}

#[inline(always)]
const fn select_music_chart_info_cursor_index(peak_nps: bool, matrix_rating: bool) -> usize {
    if peak_nps {
        0
    } else if matrix_rating {
        1
    } else {
        0
    }
}

#[inline(always)]
const fn select_music_chart_info_bit_from_choice(idx: usize) -> u8 {
    if idx < SELECT_MUSIC_CHART_INFO_NUM_CHOICES {
        1u8 << (idx as u8)
    } else {
        0
    }
}

#[inline(always)]
const fn select_music_chart_info_mask_from_config(cfg: &config::Config) -> u8 {
    select_music_chart_info_mask(
        cfg.select_music_chart_info_peak_nps,
        cfg.select_music_chart_info_matrix_rating,
    )
}

#[inline(always)]
fn apply_select_music_chart_info_mask(mask: u8) {
    config::update_select_music_chart_info_peak_nps((mask & (1u8 << 0)) != 0);
    config::update_select_music_chart_info_matrix_rating((mask & (1u8 << 1)) != 0);
}

fn toggle_select_music_chart_info_option(state: &mut State, choice_idx: usize) {
    let bit = select_music_chart_info_bit_from_choice(choice_idx);
    if bit == 0 {
        return;
    }
    let mut mask = select_music_chart_info_mask_from_config(&config::get());
    if (mask & bit) != 0 {
        if (mask & !bit) == 0 {
            return;
        }
        mask &= !bit;
    } else {
        mask |= bit;
    }
    apply_select_music_chart_info_mask(mask);

    let clamped = choice_idx.min(SELECT_MUSIC_CHART_INFO_NUM_CHOICES.saturating_sub(1));
    if let Some(slot) = state
        .sub_choice_indices_select_music
        .get_mut(SELECT_MUSIC_CHART_INFO_ROW_INDEX)
    {
        *slot = clamped;
    }
    if let Some(slot) = state
        .sub_cursor_indices_select_music
        .get_mut(SELECT_MUSIC_CHART_INFO_ROW_INDEX)
    {
        *slot = clamped;
    }
    audio::play_sfx("assets/sounds/change_value.ogg");
}

#[inline(always)]
fn select_music_chart_info_enabled_mask() -> u8 {
    let mask = select_music_chart_info_mask_from_config(&config::get());
    if mask == 0 { 1 } else { mask }
}

const fn select_music_itl_wheel_choice_index(mode: SelectMusicItlWheelMode) -> usize {
    match mode {
        SelectMusicItlWheelMode::Off => 0,
        SelectMusicItlWheelMode::Score => 1,
        SelectMusicItlWheelMode::PointsAndScore => 2,
    }
}

const fn select_music_itl_rank_choice_index(mode: SelectMusicItlRankMode) -> usize {
    match mode {
        SelectMusicItlRankMode::None => 0,
        SelectMusicItlRankMode::Chart => 1,
        SelectMusicItlRankMode::Overall => 2,
    }
}

const fn select_music_itl_rank_from_choice(idx: usize) -> SelectMusicItlRankMode {
    match idx {
        1 => SelectMusicItlRankMode::Chart,
        2 => SelectMusicItlRankMode::Overall,
        _ => SelectMusicItlRankMode::None,
    }
}

const fn select_music_itl_wheel_from_choice(idx: usize) -> SelectMusicItlWheelMode {
    match idx {
        1 => SelectMusicItlWheelMode::Score,
        2 => SelectMusicItlWheelMode::PointsAndScore,
        _ => SelectMusicItlWheelMode::Off,
    }
}

const fn select_music_wheel_style_choice_index(style: SelectMusicWheelStyle) -> usize {
    match style {
        SelectMusicWheelStyle::Itg => 0,
        SelectMusicWheelStyle::Iidx => 1,
    }
}

const fn select_music_wheel_style_from_choice(idx: usize) -> SelectMusicWheelStyle {
    match idx {
        1 => SelectMusicWheelStyle::Iidx,
        _ => SelectMusicWheelStyle::Itg,
    }
}

const fn new_pack_mode_choice_index(mode: NewPackMode) -> usize {
    match mode {
        NewPackMode::Disabled => 0,
        NewPackMode::OpenPack => 1,
        NewPackMode::HasScore => 2,
    }
}

const fn new_pack_mode_from_choice(idx: usize) -> NewPackMode {
    match idx {
        1 => NewPackMode::OpenPack,
        2 => NewPackMode::HasScore,
        _ => NewPackMode::Disabled,
    }
}

const fn select_music_scorebox_placement_choice_index(
    placement: SelectMusicScoreboxPlacement,
) -> usize {
    match placement {
        SelectMusicScoreboxPlacement::Auto => 0,
        SelectMusicScoreboxPlacement::StepPane => 1,
    }
}

const fn select_music_scorebox_placement_from_choice(idx: usize) -> SelectMusicScoreboxPlacement {
    match idx {
        1 => SelectMusicScoreboxPlacement::StepPane,
        _ => SelectMusicScoreboxPlacement::Auto,
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

const fn menu_background_style_choice_index(style: MenuBackgroundStyle) -> usize {
    match style {
        MenuBackgroundStyle::Hearts => 0,
        MenuBackgroundStyle::Technique => 1,
    }
}

const fn menu_background_style_from_choice(idx: usize) -> MenuBackgroundStyle {
    match idx {
        1 => MenuBackgroundStyle::Technique,
        _ => MenuBackgroundStyle::Hearts,
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
    pack_sync_overlay: shared_pack_sync::OverlayState,
    score_import_confirm: Option<ScoreImportConfirmState>,
    sync_pack_confirm: Option<SyncPackConfirmState>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: i8,
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
    sub_choice_indices_null_or_die: Vec<usize>,
    sub_choice_indices_null_or_die_options: Vec<usize>,
    sub_choice_indices_sync_packs: Vec<usize>,
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
    sub_cursor_indices_null_or_die: Vec<usize>,
    sub_cursor_indices_null_or_die_options: Vec<usize>,
    sub_cursor_indices_sync_packs: Vec<usize>,
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
    sync_pack_choices: Vec<String>,
    sync_pack_filters: Vec<Option<String>>,
    sound_device_options: Vec<SoundDeviceOption>,
    #[cfg(target_os = "linux")]
    linux_backend_choices: Vec<String>,
    master_volume_pct: i32,
    sfx_volume_pct: i32,
    assist_tick_volume_pct: i32,
    music_volume_pct: i32,
    global_offset_ms: i32,
    visual_delay_ms: i32,
    input_debounce_ms: i32,
    null_or_die_fingerprint_tenths: i32,
    null_or_die_window_tenths: i32,
    null_or_die_step_tenths: i32,
    null_or_die_magic_offset_tenths: i32,
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
    i18n_revision: u64,
}

pub fn init() -> State {
    let cfg = config::get();
    let system_noteskin_choices = discover_system_noteskin_choices();
    let software_thread_choices = build_software_thread_choices();
    let software_thread_labels = software_thread_choice_labels(&software_thread_choices);
    let max_fps_choices = build_max_fps_choices();
    let max_fps_labels = max_fps_choice_labels(&max_fps_choices);
    let sound_device_options = build_sound_device_options();
    #[cfg(target_os = "linux")]
    let linux_backend_choices = build_linux_backend_choices();
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
        pack_sync_overlay: shared_pack_sync::OverlayState::Hidden,
        score_import_confirm: None,
        sync_pack_confirm: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: 0,
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
        sub_choice_indices_null_or_die: vec![0; NULL_OR_DIE_MENU_ROWS.len()],
        sub_choice_indices_null_or_die_options: vec![0; NULL_OR_DIE_OPTIONS_ROWS.len()],
        sub_choice_indices_sync_packs: vec![0; SYNC_PACK_OPTIONS_ROWS.len()],
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
        sub_cursor_indices_null_or_die: vec![0; NULL_OR_DIE_MENU_ROWS.len()],
        sub_cursor_indices_null_or_die_options: vec![0; NULL_OR_DIE_OPTIONS_ROWS.len()],
        sub_cursor_indices_sync_packs: vec![0; SYNC_PACK_OPTIONS_ROWS.len()],
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
        score_import_profile_choices: vec![
            tr("OptionsScoreImport", "NoEligibleProfiles").to_string(),
        ],
        score_import_profile_ids: vec![None],
        score_import_pack_choices: vec![tr("OptionsScoreImport", "AllPacks").to_string()],
        score_import_pack_filters: vec![None],
        sync_pack_choices: vec![tr("OptionsSyncPack", "AllPacks").to_string()],
        sync_pack_filters: vec![None],
        sound_device_options,
        #[cfg(target_os = "linux")]
        linux_backend_choices,
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
        null_or_die_fingerprint_tenths: tenths_from_f64(cfg.null_or_die_fingerprint_ms).clamp(
            NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
            NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
        ),
        null_or_die_window_tenths: tenths_from_f64(cfg.null_or_die_window_ms).clamp(
            NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
            NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
        ),
        null_or_die_step_tenths: tenths_from_f64(cfg.null_or_die_step_ms).clamp(
            NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
            NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
        ),
        null_or_die_magic_offset_tenths: tenths_from_f64(cfg.null_or_die_magic_offset_ms).clamp(
            NULL_OR_DIE_MAGIC_OFFSET_MIN_TENTHS,
            NULL_OR_DIE_MAGIC_OFFSET_MAX_TENTHS,
        ),
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
        i18n_revision: crate::assets::i18n::revision(),
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

    set_choice_by_id(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::Game,
        0,
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::Theme,
        0,
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::Language,
        language_choice_index(cfg.language_flag),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::LogLevel,
        log_level_choice_index(cfg.log_level),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_system,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::LogFile,
        usize::from(cfg.log_to_file),
    );
    if let Some(noteskin_row_idx) = SYSTEM_OPTIONS_ROWS
        .iter()
        .position(|row| row.id == SubRowId::DefaultNoteSkin)
        && let Some(slot) = state.sub_choice_indices_system.get_mut(noteskin_row_idx)
    {
        *slot = machine_noteskin_idx;
    }

    set_choice_by_id(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::VSync,
        yes_no_choice_index(cfg.vsync),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::PresentMode,
        present_mode_choice_index(cfg.present_mode_policy),
    );
    sync_max_fps(&mut state, cfg.max_fps);
    set_choice_by_id(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::ShowStats,
        cfg.show_stats_mode.min(3) as usize,
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::ValidationLayers,
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
    set_choice_by_id(
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::GamepadBackend,
        windows_backend_choice_index(cfg.windows_gamepad_backend),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::UseFsrs,
        yes_no_choice_index(cfg.use_fsrs),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuNavigation,
        usize::from(cfg.three_key_navigation),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::OptionsNavigation,
        usize::from(cfg.arcade_options_navigation),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_input_backend,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuButtons,
        usize::from(cfg.only_dedicated_menu_buttons),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectProfile,
        usize::from(cfg.machine_show_select_profile),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectColor,
        usize::from(cfg.machine_show_select_color),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectStyle,
        usize::from(cfg.machine_show_select_style),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PreferredStyle,
        machine_preferred_style_choice_index(cfg.machine_preferred_style),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectPlayMode,
        usize::from(cfg.machine_show_select_play_mode),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PreferredMode,
        machine_preferred_mode_choice_index(cfg.machine_preferred_play_mode),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::EvalSummary,
        usize::from(cfg.machine_show_eval_summary),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::NameEntry,
        usize::from(cfg.machine_show_name_entry),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::GameoverScreen,
        usize::from(cfg.machine_show_gameover),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::MenuMusic,
        usize::from(cfg.menu_music),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::MenuBackground,
        menu_background_style_choice_index(cfg.menu_background_style),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::Replays,
        usize::from(cfg.machine_enable_replays),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PerPlayerGlobalOffsets,
        usize::from(cfg.machine_allow_per_player_global_offsets),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::KeyboardFeatures,
        usize::from(cfg.keyboard_features),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VideoBgs,
        usize::from(cfg.show_video_backgrounds),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_machine,
        MACHINE_OPTIONS_ROWS,
        SubRowId::WriteCurrentScreen,
        usize::from(cfg.write_current_screen),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::DefaultFailType,
        default_fail_type_choice_index(cfg.default_fail_type),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::BannerCache,
        usize::from(cfg.banner_cache),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::CdTitleCache,
        usize::from(cfg.cdtitle_cache),
    );
    if let Some(slot) = state
        .sub_choice_indices_advanced
        .get_mut(ADVANCED_SONG_PARSING_THREADS_ROW_INDEX)
    {
        *slot =
            software_thread_choice_index(&state.software_thread_choices, cfg.song_parsing_threads);
    }
    set_choice_by_id(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::CacheSongs,
        usize::from(cfg.cachesongs),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_advanced,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::FastLoad,
        usize::from(cfg.fastload),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_null_or_die_options,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::SyncGraph,
        sync_graph_mode_choice_index(cfg.null_or_die_sync_graph),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_null_or_die_options,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::SyncConfidence,
        sync_confidence_choice_index(cfg.null_or_die_confidence_percent),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_null_or_die_options,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::PackSyncThreads,
        software_thread_choice_index(
            &state.software_thread_choices,
            cfg.null_or_die_pack_sync_threads,
        ),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_null_or_die_options,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::KernelTarget,
        null_or_die_kernel_target_choice_index(cfg.null_or_die_kernel_target),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_null_or_die_options,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::KernelType,
        null_or_die_kernel_type_choice_index(cfg.null_or_die_kernel_type),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_null_or_die_options,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::FullSpectrogram,
        yes_no_choice_index(cfg.null_or_die_full_spectrogram),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        SubRowId::ShowRandomCourses,
        yes_no_choice_index(cfg.show_random_courses),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        SubRowId::ShowMostPlayed,
        yes_no_choice_index(cfg.show_most_played_courses),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        SubRowId::ShowIndividualScores,
        yes_no_choice_index(cfg.show_course_individual_scores),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_course,
        COURSE_OPTIONS_ROWS,
        SubRowId::AutosubmitIndividual,
        yes_no_choice_index(cfg.autosubmit_course_scores_individually),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::BgBrightness,
        bg_brightness_choice_index(cfg.bg_brightness),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::CenteredP1Notefield,
        usize::from(cfg.center_1player_notefield),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::ZmodRatingBox,
        usize::from(cfg.zmod_rating_box_text),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::BpmDecimal,
        usize::from(cfg.show_bpm_decimal),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_gameplay,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AutoScreenshot,
        auto_screenshot_cursor_index(cfg.auto_screenshot_eval),
    );

    set_choice_by_id(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SubRowId::MasterVolume,
        master_volume_choice_index(cfg.master_volume),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SubRowId::SfxVolume,
        master_volume_choice_index(cfg.sfx_volume),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SubRowId::AssistTickVolume,
        master_volume_choice_index(cfg.assist_tick_volume),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SubRowId::MusicVolume,
        master_volume_choice_index(cfg.music_volume),
    );
    let sound_device_idx =
        sound_device_choice_index(&state.sound_device_options, cfg.audio_output_device_index);
    set_sound_choice_index(&mut state, SubRowId::SoundDevice, sound_device_idx);
    set_sound_choice_index(
        &mut state,
        SubRowId::AudioOutputMode,
        audio_output_mode_choice_index(cfg.audio_output_mode),
    );
    #[cfg(target_os = "linux")]
    let linux_backend_idx = linux_audio_backend_choice_index(&state, cfg.linux_audio_backend);
    #[cfg(target_os = "linux")]
    set_sound_choice_index(&mut state, SubRowId::LinuxAudioBackend, linux_backend_idx);
    #[cfg(target_os = "linux")]
    set_sound_choice_index(
        &mut state,
        SubRowId::AlsaExclusive,
        alsa_exclusive_choice_index(cfg.audio_output_mode),
    );
    let sound_rate_idx = sample_rate_choice_index(&state, cfg.audio_sample_rate_hz);
    set_sound_choice_index(&mut state, SubRowId::AudioSampleRate, sound_rate_idx);
    set_choice_by_id(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SubRowId::MineSounds,
        usize::from(cfg.mine_hit_sound),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_sound,
        SOUND_OPTIONS_ROWS,
        SubRowId::RateModPreservesPitch,
        usize::from(cfg.rate_mod_preserves_pitch),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowBanners,
        yes_no_choice_index(cfg.show_select_music_banners),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowVideoBanners,
        yes_no_choice_index(cfg.show_select_music_video_banners),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowBreakdown,
        yes_no_choice_index(cfg.show_select_music_breakdown),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::BreakdownStyle,
        breakdown_style_choice_index(cfg.select_music_breakdown_style),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowNativeLanguage,
        translated_titles_choice_index(cfg.translated_titles),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::MusicWheelSpeed,
        music_wheel_scroll_speed_choice_index(cfg.music_wheel_switch_speed),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::MusicWheelStyle,
        select_music_wheel_style_choice_index(cfg.select_music_wheel_style),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowCdTitles,
        yes_no_choice_index(cfg.show_select_music_cdtitles),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowWheelGrades,
        yes_no_choice_index(cfg.show_music_wheel_grades),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowWheelLamps,
        yes_no_choice_index(cfg.show_music_wheel_lamps),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ItlRank,
        select_music_itl_rank_choice_index(cfg.select_music_itl_rank_mode),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ItlWheelData,
        select_music_itl_wheel_choice_index(cfg.select_music_itl_wheel_mode),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::NewPackBadge,
        new_pack_mode_choice_index(cfg.select_music_new_pack_mode),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowPatternInfo,
        select_music_pattern_info_choice_index(cfg.select_music_pattern_info_mode),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ChartInfo,
        select_music_chart_info_cursor_index(
            cfg.select_music_chart_info_peak_nps,
            cfg.select_music_chart_info_matrix_rating,
        ),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::MusicPreviews,
        yes_no_choice_index(cfg.show_select_music_previews),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::PreviewMarker,
        yes_no_choice_index(cfg.show_select_music_preview_marker),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::LoopMusic,
        usize::from(cfg.select_music_preview_loop),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowGameplayTimer,
        yes_no_choice_index(cfg.show_select_music_gameplay_timer),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowGsBox,
        yes_no_choice_index(cfg.show_select_music_scorebox),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::GsBoxPlacement,
        select_music_scorebox_placement_choice_index(cfg.select_music_scorebox_placement),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::GsBoxLeaderboards,
        scorebox_cycle_cursor_index(
            cfg.select_music_scorebox_cycle_itg,
            cfg.select_music_scorebox_cycle_ex,
            cfg.select_music_scorebox_cycle_hard_ex,
            cfg.select_music_scorebox_cycle_tournaments,
        ),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::EnableGrooveStats,
        yes_no_choice_index(cfg.enable_groovestats),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::EnableBoogieStats,
        yes_no_choice_index(cfg.enable_boogiestats),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::GsSubmitFails,
        yes_no_choice_index(cfg.submit_groovestats_fails),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::AutoPopulateScores,
        yes_no_choice_index(cfg.auto_populate_gs_scores),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::AutoDownloadUnlocks,
        yes_no_choice_index(cfg.auto_download_unlocks),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_groovestats,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::SeparateUnlocksByPlayer,
        yes_no_choice_index(cfg.separate_unlocks_by_player),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_arrowcloud,
        ARROWCLOUD_OPTIONS_ROWS,
        SubRowId::EnableArrowCloud,
        yes_no_choice_index(cfg.enable_arrowcloud),
    );
    set_choice_by_id(
        &mut state.sub_choice_indices_arrowcloud,
        ARROWCLOUD_OPTIONS_ROWS,
        SubRowId::ArrowCloudSubmitFails,
        yes_no_choice_index(cfg.submit_arrowcloud_fails),
    );
    refresh_score_import_options(&mut state);
    refresh_null_or_die_options(&mut state);
    set_choice_by_id(
        &mut state.sub_choice_indices_score_import,
        SCORE_IMPORT_OPTIONS_ROWS,
        SubRowId::ScoreImportOnlyMissing,
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
        SubmenuKind::NullOrDie => &state.sub_choice_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &state.sub_choice_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &state.sub_choice_indices_sync_packs,
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
        SubmenuKind::NullOrDie => &mut state.sub_choice_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &mut state.sub_choice_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &mut state.sub_choice_indices_sync_packs,
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
        SubmenuKind::NullOrDie => &state.sub_cursor_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &state.sub_cursor_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &state.sub_cursor_indices_sync_packs,
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
        SubmenuKind::NullOrDie => &mut state.sub_cursor_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &mut state.sub_cursor_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &mut state.sub_cursor_indices_sync_packs,
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
    state.sub_cursor_indices_null_or_die = state.sub_choice_indices_null_or_die.clone();
    state.sub_cursor_indices_null_or_die_options =
        state.sub_choice_indices_null_or_die_options.clone();
    state.sub_cursor_indices_sync_packs = state.sub_choice_indices_sync_packs.clone();
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
    sync_display_aspect_ratio(state, width, height);
    rebuild_resolution_choices(state, width, height);
    state.display_width_at_load = width;
    state.display_height_at_load = height;
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_show_stats_mode(state: &mut State, mode: u8) {
    set_choice_by_id(
        &mut state.sub_choice_indices_graphics,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::ShowStats,
        mode.min(3) as usize,
    );
    sync_submenu_cursor_indices(state);
    clear_render_cache(state);
}

pub fn sync_translated_titles(state: &mut State, enabled: bool) {
    set_choice_by_id(
        &mut state.sub_choice_indices_select_music,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowNativeLanguage,
        translated_titles_choice_index(enabled),
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
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

/* --------------------------------- input --------------------------------- */

// Keyboard input is handled centrally via the virtual dispatcher in app

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
        song_loading::scan_and_load_songs_with_progress_counts(
            &dirs::app_dirs().songs_dir(),
            &mut on_song,
        );

        let _ = tx.send(ReloadMsg::Phase(ReloadPhase::Courses));

        let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
            let _ = tx.send(ReloadMsg::Course {
                done,
                total,
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        let dirs = dirs::app_dirs();
        course::scan_and_load_courses_with_progress_counts(
            &dirs.courses_dir(),
            &dirs.songs_dir(),
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
    profile_cfg
        .display_name
        .clone_from(&selection.profile.display_name);
    profile_cfg
        .groovestats_api_key
        .clone_from(&selection.profile.gs_api_key);
    profile_cfg
        .groovestats_username
        .clone_from(&selection.profile.gs_username);
    profile_cfg
        .arrowcloud_api_key
        .clone_from(&selection.profile.ac_api_key);

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

#[inline(always)]
fn sync_pack_preferred_difficulty_index() -> usize {
    let profile_data = profile::get();
    let play_style = profile::get_session_play_style();
    let max_diff_index = color::FILE_DIFFICULTY_NAMES.len().saturating_sub(1);
    if max_diff_index == 0 {
        0
    } else {
        profile_data
            .last_played(play_style)
            .difficulty_index
            .min(max_diff_index)
    }
}

fn begin_pack_sync(state: &mut State, selection: SyncPackSelection) {
    if !matches!(
        state.pack_sync_overlay,
        shared_pack_sync::OverlayState::Hidden
    ) {
        return;
    }

    clear_navigation_holds(state);

    let target_chart_type = profile::get_session_play_style().chart_type();
    let preferred_difficulty_index = sync_pack_preferred_difficulty_index();
    let pack_group = selection.pack_group.as_deref();
    let song_cache = crate::game::song::get_song_cache();
    let mut targets = Vec::new();

    for pack in song_cache.iter() {
        if pack_group.is_some() && Some(pack.group_name.as_str()) != pack_group {
            continue;
        }
        for song in &pack.songs {
            let Some(steps_index) = select_music::best_steps_index(
                song.as_ref(),
                target_chart_type,
                preferred_difficulty_index,
            ) else {
                continue;
            };
            let Some(chart_ix) = select_music::selected_chart_ix_for_sync(
                song.as_ref(),
                target_chart_type,
                steps_index,
            ) else {
                continue;
            };
            let Some(chart) = song.charts.get(chart_ix) else {
                continue;
            };
            targets.push(shared_pack_sync::TargetSpec {
                simfile_path: song.simfile_path.clone(),
                song_title: song.display_full_title(false),
                chart_label: shared_pack_sync::chart_label(chart),
                chart_ix,
            });
        }
    }
    drop(song_cache);

    if !shared_pack_sync::begin(
        &mut state.pack_sync_overlay,
        selection.pack_label.clone(),
        targets,
    ) {
        log::warn!(
            "Failed to start pack sync for {:?}: no matching charts were found.",
            selection.pack_group
        );
    }
}

fn begin_pack_sync_from_confirm(state: &mut State) {
    let Some(confirm) = state.sync_pack_confirm.take() else {
        return;
    };
    begin_pack_sync(state, confirm.selection);
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

fn reload_detail_lines(reload: &ReloadUiState) -> (String, String) {
    (reload.line2.clone(), reload.line3.clone())
}

fn build_reload_overlay_actors(reload: &ReloadUiState, active_color_index: i32) -> Vec<Actor> {
    let (done, total, progress) = reload_progress(reload);
    let elapsed = reload.started_at.elapsed().as_secs_f32().max(0.0);
    let count_text = if total == 0 {
        String::new()
    } else {
        crate::screens::progress_count_text(done, total)
    };
    let show_speed_row = total > 0;
    let speed_text = if elapsed > 0.0 && show_speed_row {
        tr_fmt(
            "SelectMusic",
            "LoadingSpeed",
            &[("speed", &format!("{:.1}", done as f32 / elapsed))],
        )
        .to_string()
    } else if show_speed_row {
        tr_fmt("SelectMusic", "LoadingSpeed", &[("speed", "0.0")]).to_string()
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
    let phase_label = match reload.phase {
        ReloadPhase::Songs => tr("Init", "LoadingSongsText"),
        ReloadPhase::Courses => tr("Init", "LoadingCoursesText"),
    };
    out.push(act!(text:
        font("miso"):
        settext(if total == 0 { tr("Init", "InitializingText") } else { phase_label }):
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
                    Err(e) => tr_fmt(
                        "OptionsScoreImport",
                        "ImportFailed",
                        &[("error", &e.to_string())],
                    )
                    .to_string(),
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
    if shared_pack_sync::poll(&mut state.pack_sync_overlay) {
        return None;
    }

    sync_i18n_cache(state);

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
                if matches!(state.view, OptionsView::Submenu(SubmenuKind::InputBackend))
                    && let Some(enabled) = state.pending_dedicated_menu_buttons.take()
                {
                    config::update_only_dedicated_menu_buttons(enabled);
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
                    .is_none_or(yes_no_from_choice);
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
                let needs_cursor_init = !state.cursor_initialized;
                if needs_cursor_init {
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
        if is_submenu_row_disabled(kind, row.id) {
            return None;
        }
        if matches!(kind, SubmenuKind::Sound) {
            match row.id {
                SubRowId::MasterVolume => {
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
                SubRowId::SfxVolume => {
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
                SubRowId::AssistTickVolume => {
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
                SubRowId::MusicVolume => {
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
        if matches!(kind, SubmenuKind::Sound) && row.id == SubRowId::GlobalOffset {
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
        if matches!(kind, SubmenuKind::Graphics) && row.id == SubRowId::VisualDelay {
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
        if matches!(kind, SubmenuKind::InputBackend) && row.id == SubRowId::Debounce {
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
        if matches!(kind, SubmenuKind::NullOrDieOptions) {
            match row.id {
                SubRowId::Fingerprint => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_fingerprint_tenths,
                        delta,
                        NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
                        NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_fingerprint_ms(f64_from_tenths(
                            state.null_or_die_fingerprint_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::Window => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_window_tenths,
                        delta,
                        NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
                        NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_window_ms(f64_from_tenths(
                            state.null_or_die_window_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::Step => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_step_tenths,
                        delta,
                        NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
                        NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_step_ms(f64_from_tenths(
                            state.null_or_die_step_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                SubRowId::MagicOffset => {
                    if adjust_tenths_value(
                        &mut state.null_or_die_magic_offset_tenths,
                        delta,
                        NULL_OR_DIE_MAGIC_OFFSET_MIN_TENTHS,
                        NULL_OR_DIE_MAGIC_OFFSET_MAX_TENTHS,
                    ) {
                        config::update_null_or_die_magic_offset_ms(f64_from_tenths(
                            state.null_or_die_magic_offset_tenths,
                        ));
                        audio::play_sfx("assets/sounds/change_value.ogg");
                        clear_render_cache(state);
                    }
                    return None;
                }
                _ => {}
            }
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
        match row.id {
            SubRowId::Game => config::update_game_flag(config::GameFlag::Dance),
            SubRowId::Theme => config::update_theme_flag(config::ThemeFlag::SimplyLove),
            SubRowId::Language => {
                let flag = language_flag_from_choice(new_index);
                config::update_language_flag(flag);
                assets::i18n::set_locale(&assets::i18n::resolve_locale(flag));
            }
            SubRowId::LogLevel => config::update_log_level(log_level_from_choice(new_index)),
            SubRowId::LogFile => config::update_log_to_file(new_index == 1),
            SubRowId::DefaultNoteSkin => {
                if let Some(skin_name) = selected_choice.as_deref() {
                    profile::update_machine_default_noteskin(profile::NoteSkin::new(skin_name));
                }
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Graphics) {
        let row = &rows[row_index];
        if row.id == SubRowId::DisplayAspectRatio {
            let (cur_w, cur_h) = selected_resolution(state);
            rebuild_resolution_choices(state, cur_w, cur_h);
        }
        if row.id == SubRowId::DisplayResolution {
            rebuild_refresh_rate_choices(state);
        }
        if row.id == SubRowId::DisplayMode {
            let (cur_w, cur_h) = selected_resolution(state);
            rebuild_resolution_choices(state, cur_w, cur_h);
        }
        if row.id == SubRowId::RefreshRate && state.max_fps_at_load == 0 && !max_fps_enabled(state)
        {
            seed_max_fps_value_choice(state, 0);
        }
        if row.id == SubRowId::MaxFps && yes_no_from_choice(new_index) && state.max_fps_at_load == 0
        {
            seed_max_fps_value_choice(state, 0);
        }
        if row.id == SubRowId::ShowStats {
            let mode = new_index.min(3) as u8;
            action = Some(ScreenAction::UpdateShowOverlay(mode));
        }
        if row.id == SubRowId::ValidationLayers {
            config::update_gfx_debug(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::SoftwareRendererThreads {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_software_renderer_threads(threads);
        }
    } else if matches!(kind, SubmenuKind::InputBackend) {
        let row = &rows[row_index];
        if row.id == SubRowId::GamepadBackend {
            #[cfg(target_os = "windows")]
            {
                config::update_windows_gamepad_backend(windows_backend_from_choice(new_index));
            }
        }
        if row.id == SubRowId::UseFsrs {
            config::update_use_fsrs(yes_no_from_choice(new_index));
        }
        if row.id == SubRowId::MenuNavigation {
            config::update_three_key_navigation(new_index == 1);
        }
        if row.id == SubRowId::OptionsNavigation {
            config::update_arcade_options_navigation(new_index == 1);
        }
        if row.id == SubRowId::MenuButtons {
            state.pending_dedicated_menu_buttons = Some(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Machine) {
        let row = &rows[row_index];
        let enabled = new_index == 1;
        match row.id {
            SubRowId::SelectProfile => config::update_machine_show_select_profile(enabled),
            SubRowId::SelectColor => config::update_machine_show_select_color(enabled),
            SubRowId::SelectStyle => config::update_machine_show_select_style(enabled),
            SubRowId::PreferredStyle => config::update_machine_preferred_style(
                machine_preferred_style_from_choice(new_index),
            ),
            SubRowId::SelectPlayMode => config::update_machine_show_select_play_mode(enabled),
            SubRowId::PreferredMode => config::update_machine_preferred_play_mode(
                machine_preferred_mode_from_choice(new_index),
            ),
            SubRowId::EvalSummary => config::update_machine_show_eval_summary(enabled),
            SubRowId::NameEntry => config::update_machine_show_name_entry(enabled),
            SubRowId::GameoverScreen => config::update_machine_show_gameover(enabled),
            SubRowId::MenuMusic => config::update_menu_music(enabled),
            SubRowId::MenuBackground => {
                config::update_menu_background_style(menu_background_style_from_choice(new_index))
            }
            SubRowId::Replays => config::update_machine_enable_replays(enabled),
            SubRowId::PerPlayerGlobalOffsets => {
                config::update_machine_allow_per_player_global_offsets(enabled)
            }
            SubRowId::KeyboardFeatures => config::update_keyboard_features(enabled),
            SubRowId::VideoBgs => config::update_show_video_backgrounds(enabled),
            SubRowId::WriteCurrentScreen => config::update_write_current_screen(enabled),
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Advanced) {
        let row = &rows[row_index];
        if row.id == SubRowId::DefaultFailType {
            config::update_default_fail_type(default_fail_type_from_choice(new_index));
        } else if row.id == SubRowId::BannerCache {
            config::update_banner_cache(new_index == 1);
        } else if row.id == SubRowId::CdTitleCache {
            config::update_cdtitle_cache(new_index == 1);
        } else if row.id == SubRowId::SongParsingThreads {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_song_parsing_threads(threads);
        } else if row.id == SubRowId::CacheSongs {
            config::update_cache_songs(new_index == 1);
        } else if row.id == SubRowId::FastLoad {
            config::update_fastload(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::NullOrDieOptions) {
        let row = &rows[row_index];
        if row.id == SubRowId::SyncGraph {
            config::update_null_or_die_sync_graph(sync_graph_mode_from_choice(new_index));
        } else if row.id == SubRowId::SyncConfidence {
            config::update_null_or_die_confidence_percent(sync_confidence_from_choice(new_index));
        } else if row.id == SubRowId::PackSyncThreads {
            let threads = software_thread_from_choice(&state.software_thread_choices, new_index);
            config::update_null_or_die_pack_sync_threads(threads);
        } else if row.id == SubRowId::KernelTarget {
            config::update_null_or_die_kernel_target(null_or_die_kernel_target_from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::KernelType {
            config::update_null_or_die_kernel_type(null_or_die_kernel_type_from_choice(new_index));
        } else if row.id == SubRowId::FullSpectrogram {
            config::update_null_or_die_full_spectrogram(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::Course) {
        let row = &rows[row_index];
        let enabled = yes_no_from_choice(new_index);
        match row.id {
            SubRowId::ShowRandomCourses => config::update_show_random_courses(enabled),
            SubRowId::ShowMostPlayed => config::update_show_most_played_courses(enabled),
            SubRowId::ShowIndividualScores => config::update_show_course_individual_scores(enabled),
            SubRowId::AutosubmitIndividual => {
                config::update_autosubmit_course_scores_individually(enabled)
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::Gameplay) {
        let row = &rows[row_index];
        if row.id == SubRowId::BgBrightness {
            config::update_bg_brightness(bg_brightness_from_choice(new_index));
        } else if row.id == SubRowId::CenteredP1Notefield {
            config::update_center_1player_notefield(new_index == 1);
        } else if row.id == SubRowId::ZmodRatingBox {
            config::update_zmod_rating_box_text(new_index == 1);
        } else if row.id == SubRowId::BpmDecimal {
            config::update_show_bpm_decimal(new_index == 1);
        }
    } else if matches!(kind, SubmenuKind::Sound) {
        let row = &rows[row_index];
        match row.id {
            SubRowId::MasterVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_master_volume(vol);
            }
            SubRowId::SfxVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_sfx_volume(vol);
            }
            SubRowId::AssistTickVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_assist_tick_volume(vol);
            }
            SubRowId::MusicVolume => {
                let vol = master_volume_from_choice(new_index);
                config::update_music_volume(vol);
            }
            SubRowId::SoundDevice => {
                let device = sound_device_from_choice(state, new_index);
                config::update_audio_output_device(device);
                let current_rate = config::get().audio_sample_rate_hz;
                let rate_choice = sample_rate_choice_index(state, current_rate);
                if current_rate.is_some() && rate_choice == 0 {
                    config::update_audio_sample_rate(None);
                }
                set_sound_choice_index(state, SubRowId::AudioSampleRate, rate_choice);
            }
            SubRowId::AudioOutputMode => {
                config::update_audio_output_mode(audio_output_mode_from_choice(new_index));
                #[cfg(target_os = "linux")]
                set_sound_choice_index(state, SubRowId::AlsaExclusive, 0);
            }
            #[cfg(target_os = "linux")]
            SubRowId::LinuxAudioBackend => {
                let backend = linux_audio_backend_from_choice(state, new_index);
                config::update_linux_audio_backend(backend);
                if matches!(backend, config::LinuxAudioBackend::Alsa) {
                    set_sound_choice_index(
                        state,
                        SubRowId::AlsaExclusive,
                        alsa_exclusive_choice_index(config::get().audio_output_mode),
                    );
                } else {
                    if matches!(
                        config::get().audio_output_mode,
                        config::AudioOutputMode::Exclusive
                    ) {
                        config::update_audio_output_mode(selected_audio_output_mode(state));
                    }
                    set_sound_choice_index(state, SubRowId::AlsaExclusive, 0);
                }
            }
            #[cfg(target_os = "linux")]
            SubRowId::AlsaExclusive => {
                let mode = if new_index == 1 {
                    config::AudioOutputMode::Exclusive
                } else {
                    selected_audio_output_mode(state)
                };
                config::update_audio_output_mode(mode);
            }
            SubRowId::AudioSampleRate => {
                let rate = sample_rate_from_choice(state, new_index);
                config::update_audio_sample_rate(rate);
            }
            SubRowId::MineSounds => {
                config::update_mine_hit_sound(new_index == 1);
            }
            SubRowId::RateModPreservesPitch => {
                config::update_rate_mod_preserves_pitch(new_index == 1);
            }
            _ => {}
        }
    } else if matches!(kind, SubmenuKind::SelectMusic) {
        let row = &rows[row_index];
        if row.id == SubRowId::ShowBanners {
            config::update_show_select_music_banners(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowVideoBanners {
            config::update_show_select_music_video_banners(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowBreakdown {
            config::update_show_select_music_breakdown(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::BreakdownStyle {
            config::update_select_music_breakdown_style(breakdown_style_from_choice(new_index));
        } else if row.id == SubRowId::ShowNativeLanguage {
            config::update_translated_titles(translated_titles_from_choice(new_index));
        } else if row.id == SubRowId::MusicWheelSpeed {
            config::update_music_wheel_switch_speed(music_wheel_scroll_speed_from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::MusicWheelStyle {
            config::update_select_music_wheel_style(select_music_wheel_style_from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::ShowCdTitles {
            config::update_show_select_music_cdtitles(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowWheelGrades {
            config::update_show_music_wheel_grades(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowWheelLamps {
            config::update_show_music_wheel_lamps(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ItlRank {
            config::update_select_music_itl_rank_mode(select_music_itl_rank_from_choice(new_index));
        } else if row.id == SubRowId::ItlWheelData {
            config::update_select_music_itl_wheel_mode(select_music_itl_wheel_from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::NewPackBadge {
            config::update_select_music_new_pack_mode(new_pack_mode_from_choice(new_index));
        } else if row.id == SubRowId::ShowPatternInfo {
            config::update_select_music_pattern_info_mode(select_music_pattern_info_from_choice(
                new_index,
            ));
        } else if row.id == SubRowId::MusicPreviews {
            config::update_show_select_music_previews(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::PreviewMarker {
            config::update_show_select_music_preview_marker(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::LoopMusic {
            config::update_select_music_preview_loop(new_index == 1);
        } else if row.id == SubRowId::ShowGameplayTimer {
            config::update_show_select_music_gameplay_timer(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::ShowGsBox {
            config::update_show_select_music_scorebox(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::GsBoxPlacement {
            config::update_select_music_scorebox_placement(
                select_music_scorebox_placement_from_choice(new_index),
            );
        }
    } else if matches!(kind, SubmenuKind::GrooveStats) {
        let row = &rows[row_index];
        if row.id == SubRowId::EnableGrooveStats {
            let enabled = yes_no_from_choice(new_index);
            config::update_enable_groovestats(enabled);
            // Re-run connectivity logic so toggling this option applies immediately.
            crate::game::online::init();
        } else if row.id == SubRowId::EnableBoogieStats {
            config::update_enable_boogiestats(yes_no_from_choice(new_index));
            crate::game::online::init();
        } else if row.id == SubRowId::GsSubmitFails {
            config::update_submit_groovestats_fails(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::AutoPopulateScores {
            config::update_auto_populate_gs_scores(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::AutoDownloadUnlocks {
            config::update_auto_download_unlocks(yes_no_from_choice(new_index));
        } else if row.id == SubRowId::SeparateUnlocksByPlayer {
            config::update_separate_unlocks_by_player(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::ArrowCloud) {
        let row = &rows[row_index];
        if row.id == SubRowId::EnableArrowCloud {
            config::update_enable_arrowcloud(yes_no_from_choice(new_index));
            crate::game::online::init();
        } else if row.id == SubRowId::ArrowCloudSubmitFails {
            config::update_submit_arrowcloud_fails(yes_no_from_choice(new_index));
        }
    } else if matches!(kind, SubmenuKind::ScoreImport) {
        let row = &rows[row_index];
        if row.id == SubRowId::ScoreImportEndpoint {
            refresh_score_import_profile_options(state);
        }
    }
    clear_render_cache(state);
    action
}

fn cancel_current_view(state: &mut State) -> ScreenAction {
    match state.view {
        OptionsView::Main => ScreenAction::Navigate(Screen::Menu),
        OptionsView::Submenu(_) => {
            if let Some(parent_kind) = state.submenu_parent_kind {
                state.pending_submenu_kind = Some(parent_kind);
                state.pending_submenu_parent_kind = None;
                state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
            } else {
                state.submenu_transition = SubmenuTransition::FadeOutToMain;
            }
            state.submenu_fade_t = 0.0;
            ScreenAction::None
        }
    }
}

fn undo_three_key_selection(state: &mut State, asset_manager: &AssetManager) {
    match state.menu_lr_undo {
        1 => match state.view {
            OptionsView::Main => {
                let total = ITEMS.len();
                if total > 0 {
                    state.selected = (state.selected + 1) % total;
                }
            }
            OptionsView::Submenu(kind) => {
                move_submenu_selection_vertical(state, asset_manager, kind, NavDirection::Down);
            }
        },
        -1 => match state.view {
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
                move_submenu_selection_vertical(state, asset_manager, kind, NavDirection::Up);
            }
        },
        _ => {}
    }
}

fn activate_current_selection(state: &mut State, asset_manager: &AssetManager) -> ScreenAction {
    match state.view {
        OptionsView::Main => {
            let total = ITEMS.len();
            if total == 0 {
                return ScreenAction::None;
            }
            let sel = state.selected.min(total - 1);
            let item = &ITEMS[sel];
            state.pending_submenu_parent_kind = None;

            match item.id {
                ItemId::SystemOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::System);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::GraphicsOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Graphics);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::InputOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Input);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::MachineOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Machine);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::AdvancedOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Advanced);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::CourseOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Course);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::GameplayOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Gameplay);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::SoundOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::Sound);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::SelectMusicOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::SelectMusic);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::OnlineScoreServices => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    state.pending_submenu_kind = Some(SubmenuKind::OnlineScoring);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::NullOrDieOptions => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    refresh_null_or_die_options(state);
                    state.pending_submenu_kind = Some(SubmenuKind::NullOrDie);
                    state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                    state.submenu_fade_t = 0.0;
                }
                ItemId::ManageLocalProfiles => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::Navigate(Screen::ManageLocalProfiles);
                }
                ItemId::ReloadSongsCourses => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    start_reload_songs_and_courses(state);
                }
                ItemId::Credits => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::NavigateNoFade(Screen::Credits);
                }
                ItemId::Exit => {
                    audio::play_sfx("assets/sounds/start.ogg");
                    return ScreenAction::Navigate(Screen::Menu);
                }
                _ => {}
            }
            ScreenAction::None
        }
        OptionsView::Submenu(kind) => {
            let total = submenu_total_rows(state, kind);
            if total == 0 {
                return ScreenAction::None;
            }
            let selected_row = state.sub_selected.min(total.saturating_sub(1));
            if matches!(kind, SubmenuKind::SelectMusic)
                && let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row)
            {
                let rows = submenu_rows(kind);
                let row_id = rows.get(row_idx).map(|row| row.id);
                if row_id == Some(SubRowId::GsBoxLeaderboards) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES.saturating_sub(1));
                    toggle_select_music_scorebox_cycle_option(state, choice_idx);
                    return ScreenAction::None;
                } else if row_id == Some(SubRowId::ChartInfo) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(SELECT_MUSIC_CHART_INFO_NUM_CHOICES.saturating_sub(1));
                    toggle_select_music_chart_info_option(state, choice_idx);
                    return ScreenAction::None;
                }
            }
            if matches!(kind, SubmenuKind::Gameplay)
                && let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row)
            {
                let rows = submenu_rows(kind);
                if rows.get(row_idx).map(|row| row.id) == Some(SubRowId::AutoScreenshot) {
                    let choice_idx = submenu_cursor_indices(state, kind)
                        .get(row_idx)
                        .copied()
                        .unwrap_or(0)
                        .min(config::AUTO_SS_NUM_FLAGS.saturating_sub(1));
                    toggle_auto_screenshot_option(state, choice_idx);
                    return ScreenAction::None;
                }
            }
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
                return ScreenAction::None;
            }
            if matches!(kind, SubmenuKind::Input) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::ConfigureMappings => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Mappings);
                        }
                        SubRowId::TestInput => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            return ScreenAction::Navigate(Screen::Input);
                        }
                        SubRowId::InputOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::InputBackend);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::Input);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::OnlineScoring) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::GsBsOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::GrooveStats);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        SubRowId::ArrowCloudOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::ArrowCloud);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        SubRowId::ScoreImport => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            refresh_score_import_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::ScoreImport);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::OnlineScoring);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::NullOrDie) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx) {
                    match row.id {
                        SubRowId::NullOrDieOptions => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            state.pending_submenu_kind = Some(SubmenuKind::NullOrDieOptions);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::NullOrDie);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        SubRowId::SyncPacks => {
                            audio::play_sfx("assets/sounds/start.ogg");
                            refresh_sync_pack_options(state);
                            state.pending_submenu_kind = Some(SubmenuKind::SyncPacks);
                            state.pending_submenu_parent_kind = Some(SubmenuKind::NullOrDie);
                            state.submenu_transition = SubmenuTransition::FadeOutToSubmenu;
                            state.submenu_fade_t = 0.0;
                            return ScreenAction::None;
                        }
                        _ => {}
                    }
                }
            } else if matches!(kind, SubmenuKind::ScoreImport) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::ScoreImportStart
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
                    return ScreenAction::None;
                }
            } else if matches!(kind, SubmenuKind::SyncPacks) {
                let rows = submenu_rows(kind);
                let Some(row_idx) = submenu_visible_row_to_actual(state, kind, selected_row) else {
                    return ScreenAction::None;
                };
                if let Some(row) = rows.get(row_idx)
                    && row.id == SubRowId::SyncPackStart
                {
                    audio::play_sfx("assets/sounds/start.ogg");
                    let selection = selected_sync_pack_selection(state);
                    if selection.pack_group.is_none() {
                        clear_navigation_holds(state);
                        state.sync_pack_confirm = Some(SyncPackConfirmState {
                            selection,
                            active_choice: 1,
                        });
                    } else {
                        begin_pack_sync(state, selection);
                    }
                    return ScreenAction::None;
                }
            }
            if screen_input::dedicated_three_key_nav_enabled()
                && let Some(action) = apply_submenu_choice_delta(state, asset_manager, 1)
            {
                return action;
            }
            ScreenAction::None
        }
    }
}

pub fn handle_input(
    state: &mut State,
    asset_manager: &AssetManager,
    ev: &InputEvent,
) -> ScreenAction {
    if state.reload_ui.is_some() {
        return ScreenAction::None;
    }
    let three_key_action = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);
    if screen_input::dedicated_three_key_nav_enabled() {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Up);
                return ScreenAction::None;
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Down);
                return ScreenAction::None;
            }
            _ => {}
        }
    }
    if let Some(score_import) = state.score_import_ui.as_ref() {
        let cancel_requested = matches!(
            three_key_action,
            Some((_, screen_input::ThreeKeyMenuAction::Cancel))
        ) || (ev.pressed
            && matches!(ev.action, VirtualAction::p1_back | VirtualAction::p2_back));
        if cancel_requested {
            score_import.cancel_requested.store(true, Ordering::Relaxed);
            clear_navigation_holds(state);
            state.score_import_ui = None;
            audio::play_sfx("assets/sounds/change.ogg");
            log::warn!("Score import cancel requested by user.");
        }
        return ScreenAction::None;
    }
    if !matches!(
        state.pack_sync_overlay,
        shared_pack_sync::OverlayState::Hidden
    ) {
        return shared_pack_sync::handle_input(&mut state.pack_sync_overlay, ev);
    }
    if let Some(confirm) = state.score_import_confirm.as_mut() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    if confirm.active_choice > 0 {
                        confirm.active_choice -= 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if confirm.active_choice < 1 {
                        confirm.active_choice += 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
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
                screen_input::ThreeKeyMenuAction::Cancel => {
                    clear_navigation_holds(state);
                    state.score_import_confirm = None;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            return ScreenAction::None;
        }
        if !ev.pressed {
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start
            | VirtualAction::p1_select
            | VirtualAction::p2_start
            | VirtualAction::p2_select => {
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
            VirtualAction::p1_back | VirtualAction::p2_back => {
                clear_navigation_holds(state);
                state.score_import_confirm = None;
                audio::play_sfx("assets/sounds/change.ogg");
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    if let Some(confirm) = state.sync_pack_confirm.as_mut() {
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    if confirm.active_choice > 0 {
                        confirm.active_choice -= 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if confirm.active_choice < 1 {
                        confirm.active_choice += 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    let should_start = confirm.active_choice == 0;
                    audio::play_sfx("assets/sounds/start.ogg");
                    clear_navigation_holds(state);
                    if should_start {
                        begin_pack_sync_from_confirm(state);
                    } else {
                        state.sync_pack_confirm = None;
                    }
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    clear_navigation_holds(state);
                    state.sync_pack_confirm = None;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            return ScreenAction::None;
        }
        if !ev.pressed {
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left => {
                if confirm.active_choice > 0 {
                    confirm.active_choice -= 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right => {
                if confirm.active_choice < 1 {
                    confirm.active_choice += 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            }
            VirtualAction::p1_start
            | VirtualAction::p1_select
            | VirtualAction::p2_start
            | VirtualAction::p2_select => {
                let should_start = confirm.active_choice == 0;
                audio::play_sfx("assets/sounds/start.ogg");
                clear_navigation_holds(state);
                if should_start {
                    begin_pack_sync_from_confirm(state);
                } else {
                    state.sync_pack_confirm = None;
                }
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                clear_navigation_holds(state);
                state.sync_pack_confirm = None;
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
    if let Some((_, nav)) = three_key_action {
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
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
                state.menu_lr_undo = 1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
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
                state.menu_lr_undo = -1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Confirm => {
                state.menu_lr_undo = 0;
                clear_navigation_holds(state);
                activate_current_selection(state, asset_manager)
            }
            screen_input::ThreeKeyMenuAction::Cancel => {
                undo_three_key_selection(state, asset_manager);
                state.menu_lr_undo = 0;
                clear_navigation_holds(state);
                cancel_current_view(state)
            }
        };
    }

    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
            return cancel_current_view(state);
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
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
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
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
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
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
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
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
        VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
            return activate_current_selection(state, asset_manager);
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
    for (row_idx, tw) in row_tweens.iter_mut().enumerate().take(total_rows) {
        let (to_y, to_a) = row_dest_for_index(total_rows, selected, row_idx, s, list_y);
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
                .or({
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
        SELECT_MUSIC_SCOREBOX_PLACEMENT_ROW_INDEX => Some(SELECT_MUSIC_SHOW_SCOREBOX_ROW_INDEX),
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
    item: &Item,
    s: f32,
) -> DescriptionLayout {
    let title_side_pad = DESC_TITLE_SIDE_PAD_PX * s;
    let wrap_extra_pad = desc_wrap_extra_pad_unscaled() * s;
    let title_max_width_px =
        desc_w_unscaled().mul_add(s, -((2.0 * title_side_pad) + wrap_extra_pad));
    let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
    let bullet_max_width_px = desc_w_unscaled().mul_add(
        s,
        -((2.0 * bullet_side_pad) + (DESC_BULLET_INDENT_PX * s) + wrap_extra_pad),
    );

    let mut blocks = Vec::new();

    if item.help.is_empty() {
        // No help entries — show the item name as a paragraph fallback.
        let wrapped = wrap_miso_text(
            asset_manager,
            &item.name.get(),
            title_max_width_px,
            DESC_TITLE_ZOOM * s,
        );
        blocks.push(RenderedHelpBlock::Paragraph {
            line_count: wrapped.lines().count().max(1),
            text: Arc::from(wrapped),
        });
    } else {
        for entry in item.help {
            match entry {
                HelpEntry::Paragraph(lkey) => {
                    let raw = lkey.get();
                    let wrapped = wrap_miso_text(
                        asset_manager,
                        &raw,
                        title_max_width_px,
                        DESC_TITLE_ZOOM * s,
                    );
                    blocks.push(RenderedHelpBlock::Paragraph {
                        line_count: wrapped.lines().count().max(1),
                        text: Arc::from(wrapped),
                    });
                }
                HelpEntry::Bullet(lkey) => {
                    let resolved = lkey.get();
                    let trimmed = resolved.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let mut entry_str = String::with_capacity(trimmed.len() + 2);
                    entry_str.push('\u{2022}');
                    entry_str.push(' ');
                    entry_str.push_str(trimmed);
                    let wrapped = wrap_miso_text(
                        asset_manager,
                        &entry_str,
                        bullet_max_width_px,
                        DESC_BODY_ZOOM * s,
                    );
                    blocks.push(RenderedHelpBlock::Bullet {
                        line_count: wrapped.lines().count().max(1),
                        text: Arc::from(wrapped),
                    });
                }
            }
        }
    }

    DescriptionLayout { key, blocks }
}

fn description_layout(
    state: &State,
    asset_manager: &AssetManager,
    key: DescriptionCacheKey,
    item: &Item,
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

/// Refresh cached translated labels when the UI language changes.
fn sync_i18n_cache(state: &mut State) {
    let rev = crate::assets::i18n::revision();
    if state.i18n_revision == rev {
        return;
    }
    state.i18n_revision = rev;
    state.display_mode_choices = build_display_mode_choices(&state.monitor_specs);
    state.software_thread_labels = software_thread_choice_labels(&state.software_thread_choices);
    let (si_packs, si_filters) = score_import_pack_options();
    state.score_import_pack_choices = si_packs;
    state.score_import_pack_filters = si_filters;
    let (sp_packs, sp_filters) = sync_pack_options();
    state.sync_pack_choices = sp_packs;
    state.sync_pack_filters = sp_filters;
    #[cfg(target_os = "linux")]
    {
        state.linux_backend_choices = build_linux_backend_choices();
    }
    clear_render_cache(state);
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
    let row_idx = submenu_visible_row_to_actual(state, kind, selected_row)?;
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

fn build_yes_no_confirm_overlay(
    prompt_text: String,
    active_choice: u8,
    active_color_index: i32,
) -> Vec<Actor> {
    let w = screen_width();
    let h = screen_height();
    let cx = w * 0.5;
    let cy = h * 0.5;
    let answer_y = cy + 118.0;
    let yes_x = cx - 100.0;
    let no_x = cx + 100.0;
    let cursor_x = [yes_x, no_x][active_choice.min(1) as usize];
    let cursor_color = color::simply_love_rgba(active_color_index);

    vec![
        act!(quad:
            align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.9):
            z(700)
        ),
        act!(quad:
            align(0.5, 0.5):
            xy(cursor_x, answer_y):
            setsize(145.0, 40.0):
            diffuse(cursor_color[0], cursor_color[1], cursor_color[2], 1.0):
            z(701)
        ),
        act!(text:
            align(0.5, 0.5):
            xy(cx, cy - 65.0):
            font("miso"):
            zoom(0.95):
            maxwidth(w - 90.0):
            settext(prompt_text):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ),
        act!(text:
            align(0.5, 0.5):
            xy(yes_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext(tr("Common", "Yes")):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ),
        act!(text:
            align(0.5, 0.5):
            xy(no_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext(tr("Common", "No")):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(702):
            horizalign(center)
        ),
    ]
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
            actor.mul_alpha(alpha_multiplier);
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
            actor.mul_alpha(alpha_multiplier);
        }
        actors.extend(ui_actors);
        return actors;
    }
    if let Some(mut ui_actors) =
        shared_pack_sync::build_overlay(&state.pack_sync_overlay, state.active_color_index)
    {
        for actor in &mut ui_actors {
            actor.mul_alpha(alpha_multiplier);
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
            for (item_idx, _) in ITEMS.iter().enumerate() {
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
                let label = ITEMS[item_idx].name.get();
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
                    settext(&label):
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
                        rows[row_idx].label.get()
                    } else {
                        Arc::from("Exit")
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
                        settext(&label):
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
                                    settext(choices[choice_idx].clone().into_owned()):
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
                    let show_option_row = !is_exit;

                    if show_option_row {
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
                        let label = row.label.get();
                        let is_disabled = is_submenu_row_disabled(kind, row.id);
                        #[cfg(target_os = "linux")]
                        let child_label_indent = if matches!(kind, SubmenuKind::Sound)
                            && sound_parent_row(actual_row_idx).is_some()
                        {
                            12.0 * s
                        } else {
                            0.0
                        };
                        #[cfg(not(target_os = "linux"))]
                        let child_label_indent = 0.0;
                        let label_text_x = label_text_x + child_label_indent;
                        let label_text_max_w = (label_text_max_w - child_label_indent).max(0.0);
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
                            settext(&label):
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
                            let is_chart_info_row = matches!(kind, SubmenuKind::SelectMusic)
                                && row.id == SubRowId::ChartInfo;
                            let is_scorebox_cycle_row = matches!(kind, SubmenuKind::SelectMusic)
                                && row.id == SubRowId::GsBoxLeaderboards;
                            let is_auto_screenshot_row = matches!(kind, SubmenuKind::Gameplay)
                                && row.id == SubRowId::AutoScreenshot;
                            let is_multi_toggle_row = is_chart_info_row
                                || is_scorebox_cycle_row
                                || is_auto_screenshot_row;
                            let chart_info_enabled_mask = if is_chart_info_row {
                                select_music_chart_info_enabled_mask()
                            } else {
                                0
                            };
                            let scorebox_enabled_mask = if is_scorebox_cycle_row {
                                select_music_scorebox_cycle_enabled_mask()
                            } else {
                                0
                            };
                            let auto_screenshot_mask = if is_auto_screenshot_row {
                                auto_screenshot_enabled_mask()
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
                                    let is_choice_enabled = if is_chart_info_row {
                                        (chart_info_enabled_mask
                                            & select_music_chart_info_bit_from_choice(idx))
                                            != 0
                                    } else if is_scorebox_cycle_row {
                                        (scorebox_enabled_mask
                                            & scorebox_cycle_bit_from_choice(idx))
                                            != 0
                                    } else if is_auto_screenshot_row {
                                        (auto_screenshot_mask
                                            & auto_screenshot_bit_from_choice(idx))
                                            != 0
                                    } else {
                                        false
                                    };
                                    let mut choice_color = if is_disabled && !is_choice_selected {
                                        sl_gray
                                    } else if is_multi_toggle_row {
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
                            // For multi-toggle rows, underline each enabled option.
                            if layout.inline_row && is_multi_toggle_row {
                                let line_thickness = widescale(2.0, 2.5).round().max(1.0);
                                let offset = widescale(3.0, 4.0);
                                let underline_y = row_mid_y + layout.text_h * 0.5 + offset;
                                let mut line_color =
                                    color::decorative_rgba(state.active_color_index);
                                line_color[3] *= row_alpha;
                                for idx in 0..layout.texts.len() {
                                    let enabled = if is_chart_info_row {
                                        let bit = select_music_chart_info_bit_from_choice(idx);
                                        bit != 0 && (chart_info_enabled_mask & bit) != 0
                                    } else if is_scorebox_cycle_row {
                                        let bit = scorebox_cycle_bit_from_choice(idx);
                                        bit != 0 && (scorebox_enabled_mask & bit) != 0
                                    } else {
                                        let bit = auto_screenshot_bit_from_choice(idx);
                                        bit != 0 && (auto_screenshot_mask & bit) != 0
                                    };
                                    if !enabled {
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
                        let exit_label = tr("Common", "Exit");
                        let label = exit_label.clone();
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
        let title_step_px = 20.0 * s;
        let body_step_px = 18.0 * s;
        let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;

        for block in &desc_layout.blocks {
            match block {
                RenderedHelpBlock::Paragraph { text, line_count } => {
                    ui_actors.push(act!(text:
                        align(0.0, 0.0):
                        xy(desc_x + title_side_pad, cursor_y):
                        zoom(DESC_TITLE_ZOOM):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        font("miso"): settext(text):
                        horizalign(left)
                    ));
                    cursor_y += title_step_px * *line_count as f32 + DESC_BULLET_TOP_PAD_PX * s;
                }
                RenderedHelpBlock::Bullet { text, line_count } => {
                    let bullet_x = DESC_BULLET_INDENT_PX.mul_add(s, desc_x + bullet_side_pad);
                    ui_actors.push(act!(text:
                        align(0.0, 0.0):
                        xy(bullet_x, cursor_y):
                        zoom(DESC_BODY_ZOOM):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        font("miso"): settext(text):
                        horizalign(left)
                    ));
                    cursor_y += body_step_px * *line_count as f32;
                }
            }
        }
    }
    if let Some(confirm) = &state.score_import_confirm {
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
        ui_actors.extend(build_yes_no_confirm_overlay(
            prompt_text,
            confirm.active_choice,
            state.active_color_index,
        ));
    }
    if let Some(confirm) = &state.sync_pack_confirm {
        let prompt_text = format!(
            "Sync {}?\nThis will analyze every matching simfile here in Options.\nYou can review offsets and confidence before saving.\n\nStart now?",
            if confirm.selection.pack_group.is_none() {
                "ALL files"
            } else {
                confirm.selection.pack_label.as_str()
            }
        );
        ui_actors.extend(build_yes_no_confirm_overlay(
            prompt_text,
            confirm.active_choice,
            state.active_color_index,
        ));
    }

    let combined_alpha = alpha_multiplier * state.content_alpha;
    for actor in &mut ui_actors {
        actor.mul_alpha(combined_alpha);
    }
    actors.extend(ui_actors);

    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetManager;
    use crate::engine::input::{InputEvent, InputSource, VirtualAction};
    use std::time::Instant;

    fn press(
        state: &mut State,
        asset_manager: &AssetManager,
        action: VirtualAction,
    ) -> ScreenAction {
        let now = Instant::now();
        handle_input(
            state,
            asset_manager,
            &InputEvent {
                action,
                pressed: true,
                source: InputSource::Keyboard,
                timestamp: now,
                timestamp_host_nanos: 0,
                stored_at: now,
                emitted_at: now,
            },
        )
    }

    #[test]
    fn inferred_aspect_choice_maps_1024x768_to_4_3() {
        let idx = inferred_aspect_choice(1024, 768);
        assert_eq!(
            DISPLAY_ASPECT_RATIO_CHOICES[idx].as_str_static(),
            Some("4:3")
        );
    }

    #[test]
    fn sync_display_resolution_selects_loaded_4_3_mode() {
        let mut state = init();
        sync_display_resolution(&mut state, 1024, 768);

        assert_eq!(selected_aspect_label(&state), "4:3");
        assert_eq!(selected_resolution(&state), (1024, 768));
        assert!(state.resolution_choices.contains(&(1024, 768)));
    }

    #[test]
    fn p2_can_navigate_and_change_system_options() {
        let asset_manager = AssetManager::new();
        let mut state = init();

        assert_eq!(state.selected, 0);
        press(&mut state, &asset_manager, VirtualAction::p2_start);
        update(&mut state, 1.0, &asset_manager);
        update(&mut state, 1.0, &asset_manager);
        assert!(matches!(
            state.view,
            OptionsView::Submenu(SubmenuKind::System)
        ));

        press(&mut state, &asset_manager, VirtualAction::p2_down);
        press(&mut state, &asset_manager, VirtualAction::p2_down);
        press(&mut state, &asset_manager, VirtualAction::p2_down);
        assert_eq!(state.sub_selected, 3);

        let before = state.sub_cursor_indices_system[3];
        press(&mut state, &asset_manager, VirtualAction::p2_right);
        assert_eq!(state.sub_cursor_indices_system[3], before + 1);
    }
}
