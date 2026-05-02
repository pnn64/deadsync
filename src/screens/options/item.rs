use super::*;

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
    GfxHighDpi,
    GfxVisualDelay,

    // Input Options submenu (launcher)
    InpConfigureMappings,
    InpTestInput,
    InpInputOptions,

    // Input Backend Options submenu
    InpGamepadBackend,
    InpUseFsrs,
    InpDebugFsrDump,
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
    MchFont,
    MchEvalSummary,
    MchNameEntry,
    MchGameoverScreen,
    MchWriteCurrentScreen,
    MchMenuMusic,
    MchVisualStyle,
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
    #[cfg(target_os = "linux")]
    SndLinuxBackend,
    #[cfg(target_os = "linux")]
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
    SmSwitchProfile,
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

#[inline(always)]
pub(super) fn desc_wrap_extra_pad_unscaled() -> f32 {
    // Slightly tighter wrap in 4:3 to avoid edge clipping from font metric/render mismatch.
    widescale(6.0, 0.0)
}

#[inline(always)]
pub(super) fn submenu_inline_widths_fit(widths: &[f32], spacing: f32) -> bool {
    if widths.is_empty() {
        return false;
    }
    if is_wide() {
        return true;
    }
    let total_w =
        widths.iter().copied().sum::<f32>() + spacing * (widths.len().saturating_sub(1) as f32);
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
            HelpEntry::Bullet(lookup_key("OptionsGraphics", "HighDPI")),
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
            HelpEntry::Bullet(lookup_key("OptionsMachine", "VisualStyle")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectProfile")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectColor")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectStyle")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "SelectPlayMode")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "EvalSummary")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "NameEntry")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "GameoverScreen")),
            HelpEntry::Bullet(lookup_key("OptionsMachine", "MenuMusic")),
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
            HelpEntry::Bullet(lookup_key("OptionsSelectMusic", "SwitchProfile")),
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
