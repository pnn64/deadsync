use super::super::*;

pub(in crate::screens::options) use crate::config::{
    SELECT_MUSIC_CHART_INFO_NUM_CHOICES, SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES,
    auto_screenshot_bit_from_choice, auto_screenshot_cursor_index, bg_brightness_choice_index,
    music_wheel_scroll_speed_choice_index, music_wheel_scroll_speed_from_choice,
    scorebox_cycle_bit_from_choice, scorebox_cycle_cursor_index, scorebox_cycle_mask,
    select_music_chart_info_bit_from_choice, select_music_chart_info_cursor_index,
    select_music_chart_info_mask,
};

pub(in crate::screens::options) const SELECT_MUSIC_OPTIONS_ROWS: &[SubRow] = &[
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
        id: SubRowId::SeriesSort,
        label: lookup_key("OptionsSelectMusic", "SeriesSort"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SongSelectBg,
        label: lookup_key("OptionsSelectMusic", "SongSelectBG"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("OptionsSelectMusic", "SongSelectBgBanner"),
            localized_choice("OptionsSelectMusic", "SongSelectBgBG"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SwitchProfile,
        label: lookup_key("OptionsSelectMusic", "SwitchProfile"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
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
        id: SubRowId::FolderStats,
        label: lookup_key("OptionsSelectMusic", "FolderStats"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
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
        id: SubRowId::StepArtistBox,
        label: lookup_key("OptionsSelectMusic", "StepArtistBox"),
        choices: &[
            localized_choice("OptionsSelectMusic", "StepArtistBoxDefault"),
            localized_choice("OptionsSelectMusic", "StepArtistBoxLegacy"),
            localized_choice("OptionsSelectMusic", "StepArtistBoxExpanded"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ChartInfo,
        label: lookup_key("OptionsSelectMusic", "ChartInfo"),
        choices: &[
            localized_choice("OptionsSelectMusic", "ChartInfoTogglePNPS"),
            localized_choice("OptionsSelectMusic", "ChartInfoToggleEBPM"),
            localized_choice("OptionsSelectMusic", "ChartInfoToggleMR"),
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
        id: SubRowId::PreviewStartsImmediately,
        label: lookup_key("OptionsSelectMusic", "PreviewStartsImmediately"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
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
        id: SubRowId::ShowStageDisplay,
        label: lookup_key("OptionsSelectMusic", "ShowStageDisplay"),
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

pub(in crate::screens::options) const SELECT_MUSIC_OPTIONS_ITEMS: &[Item] = &[
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
        id: ItemId::SmSeriesSort,
        name: lookup_key("OptionsSelectMusic", "SeriesSort"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "SeriesSortHelp",
        ))],
    },
    Item {
        id: ItemId::SmSongSelectBg,
        name: lookup_key("OptionsSelectMusic", "SongSelectBG"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "SongSelectBGHelp",
        ))],
    },
    Item {
        id: ItemId::SmSwitchProfile,
        name: lookup_key("OptionsSelectMusic", "SwitchProfile"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "SwitchProfileHelp",
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
        id: ItemId::SmFolderStats,
        name: lookup_key("OptionsSelectMusic", "FolderStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "FolderStatsHelp",
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
        id: ItemId::SmStepArtistBox,
        name: lookup_key("OptionsSelectMusic", "StepArtistBox"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "StepArtistBoxHelp",
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
        id: ItemId::SmPreviewStartsImmediately,
        name: lookup_key("OptionsSelectMusic", "PreviewStartsImmediately"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "PreviewStartsImmediatelyHelp",
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
        id: ItemId::SmStageDisplay,
        name: lookup_key("OptionsSelectMusic", "ShowStageDisplay"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSelectMusicHelp",
            "ShowStageDisplayHelp",
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

#[inline(always)]
pub(in crate::screens::options) const fn scorebox_cycle_mask_from_config(
    cfg: &config::Config,
) -> u8 {
    scorebox_cycle_mask(
        cfg.select_music_scorebox_cycle_itg,
        cfg.select_music_scorebox_cycle_ex,
        cfg.select_music_scorebox_cycle_hard_ex,
        cfg.select_music_scorebox_cycle_tournaments,
    )
}

pub(in crate::screens::options) fn toggle_select_music_scorebox_cycle_option(
    state: &mut State,
    choice_idx: usize,
) -> ThemeEffect {
    let bit = scorebox_cycle_bit_from_choice(choice_idx);
    if bit == 0 {
        return ThemeEffect::None;
    }
    let mut mask = state.scorebox_cycle_mask;
    if (mask & bit) != 0 {
        mask &= !bit;
    } else {
        mask |= bit;
    }
    state.scorebox_cycle_mask = mask;

    let clamped = choice_idx.min(SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES.saturating_sub(1));
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::GsBoxLeaderboards,
    ) {
        *slot = clamped;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::SelectMusic].cursor_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::GsBoxLeaderboards,
    ) {
        *slot = clamped;
    }
    queue_sfx(state, "assets/sounds/change_value.ogg");
    select_music_config_effect(crate::SimplyLoveSelectMusicConfigRequest::ScoreboxCycleMask(mask))
}

#[inline(always)]
pub(in crate::screens::options) const fn select_music_scorebox_cycle_enabled_mask(
    state: &State,
) -> u8 {
    state.scorebox_cycle_mask
}

#[inline(always)]
pub(in crate::screens::options) const fn auto_screenshot_enabled_mask(state: &State) -> u8 {
    state.auto_screenshot_mask
}

pub(in crate::screens::options) fn toggle_auto_screenshot_option(
    state: &mut State,
    choice_idx: usize,
) -> ThemeEffect {
    let bit = auto_screenshot_bit_from_choice(choice_idx);
    if bit == 0 {
        return ThemeEffect::None;
    }
    let mut mask = state.auto_screenshot_mask;
    if (mask & bit) != 0 {
        mask &= !bit;
    } else {
        mask |= bit;
    }
    state.auto_screenshot_mask = mask;
    let clamped = choice_idx.min(config::AUTO_SS_NUM_FLAGS.saturating_sub(1));
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AutoScreenshot,
        clamped,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].cursor_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AutoScreenshot,
        clamped,
    );
    queue_sfx(state, "assets/sounds/change_value.ogg");
    gameplay_config_effect(crate::SimplyLoveGameplayConfigRequest::AutoScreenshotMask(
        mask,
    ))
}

#[inline(always)]
pub(in crate::screens::options) const fn select_music_chart_info_mask_from_config(
    cfg: &config::Config,
) -> u8 {
    select_music_chart_info_mask(
        cfg.select_music_chart_info_peak_nps,
        cfg.select_music_chart_info_effective_bpm,
        cfg.select_music_chart_info_matrix_rating,
    )
}

pub(in crate::screens::options) fn toggle_select_music_chart_info_option(
    state: &mut State,
    choice_idx: usize,
) -> ThemeEffect {
    let bit = select_music_chart_info_bit_from_choice(choice_idx);
    if bit == 0 {
        return ThemeEffect::None;
    }
    let mut mask = state.chart_info_mask;
    if (mask & bit) != 0 {
        if (mask & !bit) == 0 {
            return ThemeEffect::None;
        }
        mask &= !bit;
    } else {
        mask |= bit;
    }
    state.chart_info_mask = mask;

    let clamped = choice_idx.min(SELECT_MUSIC_CHART_INFO_NUM_CHOICES.saturating_sub(1));
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ChartInfo,
    ) {
        *slot = clamped;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::SelectMusic].cursor_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ChartInfo,
    ) {
        *slot = clamped;
    }
    queue_sfx(state, "assets/sounds/change_value.ogg");
    select_music_config_effect(crate::SimplyLoveSelectMusicConfigRequest::ChartInfoMask(
        mask,
    ))
}

#[inline(always)]
pub(in crate::screens::options) fn select_music_chart_info_enabled_mask(state: &State) -> u8 {
    config::select_music_chart_info_enabled_mask(state.chart_info_mask)
}
