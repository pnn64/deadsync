use super::*;

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
    HighDpi,
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
    Font,
    EvalSummary,
    NameEntry,
    GameoverScreen,
    WriteCurrentScreen,
    MenuMusic,
    VisualStyle,
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
pub(super) const fn localized_choice(section: &'static str, key: &'static str) -> Choice {
    Choice::Localized(lookup_key(section, key))
}

/// Shorthand for `Choice::Literal(s)` in const arrays.
pub(super) const fn literal_choice(s: &'static str) -> Choice {
    Choice::Literal(s)
}

pub(super) fn set_choice_by_id(
    choice_indices: &mut Vec<usize>,
    rows: &[SubRow],
    id: SubRowId,
    idx: usize,
) {
    if let Some(pos) = rows.iter().position(|r| r.id == id)
        && let Some(slot) = choice_indices.get_mut(pos)
    {
        let max_idx = rows[pos].choices.len().saturating_sub(1);
        *slot = idx.min(max_idx);
    }
}

pub(super) const fn yes_no_choice_index(enabled: bool) -> usize {
    if enabled { 1 } else { 0 }
}

pub(super) const fn yes_no_from_choice(idx: usize) -> bool {
    idx == 1
}
