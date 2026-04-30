use super::*;

/* ---------------------------- transitions ---------------------------- */
pub(super) const TRANSITION_IN_DURATION: f32 = 0.4;
pub(super) const TRANSITION_OUT_DURATION: f32 = 0.4;
pub(super) const RELOAD_BAR_H: f32 = 30.0;

/* -------------------------- hold-to-scroll timing ------------------------- */
pub(super) const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
pub(super) const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);
pub(super) const MAX_FPS_HOLD_FAST_AFTER: Duration = Duration::from_millis(700);
pub(super) const MAX_FPS_HOLD_FASTER_AFTER: Duration = Duration::from_millis(1200);
pub(super) const MAX_FPS_HOLD_FASTEST_AFTER: Duration = Duration::from_millis(1800);

/* ----------------------------- cursor tweening ----------------------------- */
// Simply Love metrics.ini uses 0.1 for both [ScreenOptions] TweenSeconds and CursorTweenSeconds.
// ScreenOptionsService rows inherit OptionRow tween behavior, so keep both aligned at 0.1.
pub(super) const SL_OPTION_ROW_TWEEN_SECONDS: f32 = 0.1;
pub(super) const CURSOR_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;
pub(super) const ROW_TWEEN_SECONDS: f32 = SL_OPTION_ROW_TWEEN_SECONDS;
// Spacing between inline items in OptionRows (pixels at current zoom)
pub(super) const INLINE_SPACING: f32 = 15.75;

// Match Simply Love operator menu ranges (±1000 ms) for these calibrations.
pub(super) const GLOBAL_OFFSET_MIN_MS: i32 = -1000;
pub(super) const GLOBAL_OFFSET_MAX_MS: i32 = 1000;
pub(super) const VISUAL_DELAY_MIN_MS: i32 = -1000;
pub(super) const VISUAL_DELAY_MAX_MS: i32 = 1000;
pub(super) const VOLUME_MIN_PERCENT: i32 = 0;
pub(super) const VOLUME_MAX_PERCENT: i32 = 100;
pub(super) const INPUT_DEBOUNCE_MIN_MS: i32 = 0;
pub(super) const INPUT_DEBOUNCE_MAX_MS: i32 = 200;
pub(super) const NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS: i32 = 1;
pub(super) const NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS: i32 = 1000;
pub(super) const NULL_OR_DIE_MAGIC_OFFSET_MIN_TENTHS: i32 = -1000;
pub(super) const NULL_OR_DIE_MAGIC_OFFSET_MAX_TENTHS: i32 = 1000;

// Local fade timing when swapping between main options list and System Options submenu.
pub(super) const SUBMENU_FADE_DURATION: f32 = 0.2;

/// Bars in `screen_bar.rs` use 32.0 px height.
pub(super) const BAR_H: f32 = 32.0;

/// Screen-space margins (pixels, not scaled)
pub(super) const LEFT_MARGIN_PX: f32 = 33.0;
pub(super) const RIGHT_MARGIN_PX: f32 = 25.0;
pub(super) const FIRST_ROW_TOP_MARGIN_PX: f32 = 18.0;
pub(super) const BOTTOM_MARGIN_PX: f32 = 0.0;

/// Unscaled spec constants (we’ll uniformly scale).
pub(super) const VISIBLE_ROWS: usize = 10; // how many rows are shown at once
// Match player_options.rs row height.
pub(super) const ROW_H: f32 = 33.0;
pub(super) const ROW_GAP: f32 = 2.5;
pub(super) const SEP_W: f32 = 2.5; // gap/stripe between rows and description
// Match SL non-wide/wide block sizing used by ScreenPlayerOptions underlay.
pub(super) const OPTIONS_BLOCK_W_43: f32 = 614.0;
pub(super) const OPTIONS_BLOCK_W_169: f32 = 792.0;
pub(super) const DESC_W_43: f32 = 287.0; // ScreenOptionsService overlay.lua: WideScale(287,292)
pub(super) const DESC_W_169: f32 = 292.0;
// derive description height from visible rows so it never includes a trailing gap
pub(super) const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

/// Left margin for row labels (in content-space pixels).
pub(super) const TEXT_LEFT_PAD: f32 = 40.66;
/// Left margin for the heart icon (in content-space pixels).
pub(super) const HEART_LEFT_PAD: f32 = 13.0;
/// Label text zoom, matched to the left column titles in `player_options.rs`.
pub(super) const ITEM_TEXT_ZOOM: f32 = 0.88;
/// Width of the System Options submenu label column (content-space pixels).
pub(super) const SUB_LABEL_COL_W: f32 = 142.5;
/// Left padding for text inside the System Options submenu label column.
pub(super) const SUB_LABEL_TEXT_LEFT_PAD: f32 = 11.0;
/// Left padding for inline option values in the System Options submenu (content-space pixels).
pub(super) const SUB_INLINE_ITEMS_LEFT_PAD: f32 = 13.0;
/// Horizontal offset (content-space pixels) for single-value submenu items
/// (e.g. Language and Exit) within the items column.
pub(super) const SUB_SINGLE_VALUE_CENTER_OFFSET: f32 = -43.0;

/// Heart sprite zoom for the options list rows.
/// This is a StepMania-style "zoom" factor applied to the native heart.png size.
pub(super) const HEART_ZOOM: f32 = 0.026;

/// Description pane layout (mirrors Simply Love's `ScreenOptionsService` overlay).
/// Title and bullet list use separate top/side padding so they can be tuned independently.
pub(super) const DESC_TITLE_TOP_PAD_PX: f32 = 9.75;
pub(super) const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5;
pub(super) const DESC_BULLET_TOP_PAD_PX: f32 = 23.25;
pub(super) const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5;
pub(super) const DESC_BULLET_INDENT_PX: f32 = 10.0;
pub(super) const DESC_TITLE_ZOOM: f32 = 1.0;
pub(super) const DESC_BODY_ZOOM: f32 = 1.0;

pub(super) const LANGUAGE_CHOICES: &[Choice] = &[
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
pub(super) const INPUT_BACKEND_CHOICES: &[Choice] = &[
    literal_choice("W32 Raw Input"),
    literal_choice("WGI (compat)"),
];
#[cfg(target_os = "macos")]
pub(super) const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("macOS IOHID")];
#[cfg(target_os = "linux")]
pub(super) const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("Linux evdev")];
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(super) const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("Platform Default")];
#[cfg(not(any(target_os = "windows", unix)))]
pub(super) const INPUT_BACKEND_CHOICES: &[Choice] = &[literal_choice("Platform Default")];
#[cfg(target_os = "windows")]
pub(super) const INPUT_BACKEND_INLINE: bool = true;
#[cfg(not(target_os = "windows"))]
pub(super) const INPUT_BACKEND_INLINE: bool = false;

pub(super) const SCORE_IMPORT_DONE_OVERLAY_SECONDS: f32 = 1.5;
pub(super) const SCORE_IMPORT_ROW_ENDPOINT_INDEX: usize = 0;
pub(super) const SCORE_IMPORT_ROW_PROFILE_INDEX: usize = 1;
pub(super) const SCORE_IMPORT_ROW_PACK_INDEX: usize = 2;
pub(super) const SCORE_IMPORT_ROW_ONLY_MISSING_INDEX: usize = 3;
pub(super) const SYNC_PACK_ROW_PACK_INDEX: usize = 0;
