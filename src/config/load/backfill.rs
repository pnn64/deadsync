use super::*;

const OPTION_KEYS: &[&str] = &[
    "AudioOutputDevice",
    "AudioOutputMode",
    "AudioSampleRateHz",
    "AdditionalSongFolders",
    "AutoDownloadUnlocks",
    "AutoPopulateGrooveStatsScores",
    "BGBrightness",
    "BannerCache",
    "CacheSongs",
    "CDTitleCache",
    "Center1Player",
    "CourseAutosubmitScoresIndividually",
    "CourseShowIndividualScores",
    "CourseShowMostPlayed",
    "CourseShowRandom",
    "DefaultFailType",
    "DefaultNoteSkin",
    "DisplayHeight",
    "DisplayWidth",
    "FastLoad",
    "EnableArrowCloud",
    "EnableBoogieStats",
    "EnableGrooveStats",
    "SubmitArrowCloudFails",
    "SubmitGrooveStatsFails",
    "FullscreenType",
    "Game",
    "GamepadBackend",
    "GfxDebug",
    "GlobalOffsetSeconds",
    "Language",
    "LogLevel",
    "LogToFile",
    "LinuxAudioBackend",
    "MaxFps",
    "MasterVolume",
    "MenuMusic",
    "MineHitSound",
    "MusicVolume",
    "MusicWheelSwitchSpeed",
    "PackSyncThreads",
    "SongParsingThreads",
    "RateModPreservesPitch",
    "SelectMusicBreakdown",
    "SelectMusicShowBanners",
    "SelectMusicShowVideoBanners",
    "SelectMusicShowBreakdown",
    "SelectMusicShowCDTitles",
    "SelectMusicWheelGrades",
    "SelectMusicWheelLamps",
    "SelectMusicPreviews",
    "SelectMusicPreviewLoop",
    "SelectMusicPatternInfo",
    "SelectMusicScorebox",
    "SelectMusicScoreboxCycleItg",
    "SelectMusicScoreboxCycleEx",
    "SelectMusicScoreboxCycleHardEx",
    "SelectMusicScoreboxCycleTournaments",
    "SeparateUnlocksByPlayer",
    "ShowStats",
    "ShowStatsMode",
    "SmoothHistogram",
    "InputDebounceTime",
    "ThreeKeyNavigation",
    "OnlyDedicatedMenuButtons",
    "AssistTickVolume",
    "SFXVolume",
    "SoftwareRendererThreads",
    "Theme",
    "TranslatedTitles",
    "VideoRenderer",
    "VisualDelaySeconds",
    "Vsync",
    "Windowed",
    "WriteCurrentScreen",
];

const THEME_KEYS: &[&str] = &[
    "SimplyLoveColor",
    "ShowSelectMusicGameplayTimer",
    "KeyboardFeatures",
    "MenuBackgroundStyle",
    "VideoBackgrounds",
    "MachineShowEvalSummary",
    "MachineShowGameOver",
    "MachineShowNameEntry",
    "MachineShowSelectColor",
    "MachineShowSelectPlayMode",
    "MachineShowSelectProfile",
    "MachineShowSelectStyle",
    "MachineEnableReplays",
    "MachinePreferredStyle",
    "MachinePreferredPlayMode",
    "ZmodRatingBoxText",
    "ShowBpmDecimal",
];

pub(super) fn write_missing_fields(conf: &SimpleIni) {
    if has_missing_fields(conf) {
        save_without_keymaps();
        info!(
            "'{}' updated with default values for any missing fields.",
            dirs::app_dirs().config_path().display()
        );
    } else {
        info!("Configuration OK; no write needed.");
    }
}

fn has_missing_fields(conf: &SimpleIni) -> bool {
    OPTION_KEYS
        .iter()
        .any(|key| conf.get("Options", key).is_none())
        || THEME_KEYS
            .iter()
            .any(|key| conf.get("Theme", key).is_none())
}
