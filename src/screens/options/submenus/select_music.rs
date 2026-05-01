use super::super::*;

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

pub(in crate::screens::options) const SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES: usize = 4;
pub(in crate::screens::options) const SELECT_MUSIC_CHART_INFO_NUM_CHOICES: usize = 2;

pub(in crate::screens::options) const MUSIC_WHEEL_SCROLL_SPEED_VALUES: [u8; 7] =
    [5, 10, 15, 25, 30, 45, 100];

pub(in crate::screens::options) fn bg_brightness_choice_index(brightness: f32) -> usize {
    ((brightness.clamp(0.0, 1.0) * 10.0).round() as i32).clamp(0, 10) as usize
}

pub(in crate::screens::options) fn bg_brightness_from_choice(idx: usize) -> f32 {
    idx.min(10) as f32 / 10.0
}

pub(in crate::screens::options) fn music_wheel_scroll_speed_choice_index(speed: u8) -> usize {
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

pub(in crate::screens::options) fn music_wheel_scroll_speed_from_choice(idx: usize) -> u8 {
    MUSIC_WHEEL_SCROLL_SPEED_VALUES
        .get(idx)
        .copied()
        .unwrap_or(15)
}

#[inline(always)]
pub(in crate::screens::options) const fn scorebox_cycle_mask(
    itg: bool,
    ex: bool,
    hard_ex: bool,
    tournaments: bool,
) -> u8 {
    (itg as u8) | ((ex as u8) << 1) | ((hard_ex as u8) << 2) | ((tournaments as u8) << 3)
}

#[inline(always)]
pub(in crate::screens::options) const fn auto_screenshot_cursor_index(mask: u8) -> usize {
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
pub(in crate::screens::options) const fn scorebox_cycle_cursor_index(
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
pub(in crate::screens::options) const fn scorebox_cycle_bit_from_choice(idx: usize) -> u8 {
    if idx < SELECT_MUSIC_SCOREBOX_CYCLE_NUM_CHOICES {
        1u8 << (idx as u8)
    } else {
        0
    }
}

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

#[inline(always)]
pub(in crate::screens::options) fn apply_scorebox_cycle_mask(mask: u8) {
    config::update_select_music_scorebox_cycle_itg((mask & (1u8 << 0)) != 0);
    config::update_select_music_scorebox_cycle_ex((mask & (1u8 << 1)) != 0);
    config::update_select_music_scorebox_cycle_hard_ex((mask & (1u8 << 2)) != 0);
    config::update_select_music_scorebox_cycle_tournaments((mask & (1u8 << 3)) != 0);
}

pub(in crate::screens::options) fn toggle_select_music_scorebox_cycle_option(
    state: &mut State,
    choice_idx: usize,
) {
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
    audio::play_sfx("assets/sounds/change_value.ogg");
}

#[inline(always)]
pub(in crate::screens::options) fn select_music_scorebox_cycle_enabled_mask() -> u8 {
    scorebox_cycle_mask_from_config(&config::get())
}

#[inline(always)]
pub(in crate::screens::options) const fn auto_screenshot_bit_from_choice(idx: usize) -> u8 {
    config::auto_screenshot_bit(idx)
}

#[inline(always)]
pub(in crate::screens::options) fn auto_screenshot_enabled_mask() -> u8 {
    config::get().auto_screenshot_eval
}

pub(in crate::screens::options) fn toggle_auto_screenshot_option(
    state: &mut State,
    choice_idx: usize,
) {
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
    audio::play_sfx("assets/sounds/change_value.ogg");
}

impl ChoiceEnum for SelectMusicPatternInfoMode {
    const ALL: &'static [Self] = &[Self::Auto, Self::Tech, Self::Stamina];
    const DEFAULT: Self = Self::Auto;
}

#[inline(always)]
pub(in crate::screens::options) const fn select_music_chart_info_mask(
    peak_nps: bool,
    matrix_rating: bool,
) -> u8 {
    (peak_nps as u8) | ((matrix_rating as u8) << 1)
}

#[inline(always)]
pub(in crate::screens::options) const fn select_music_chart_info_cursor_index(
    peak_nps: bool,
    matrix_rating: bool,
) -> usize {
    if peak_nps {
        0
    } else if matrix_rating {
        1
    } else {
        0
    }
}

#[inline(always)]
pub(in crate::screens::options) const fn select_music_chart_info_bit_from_choice(idx: usize) -> u8 {
    if idx < SELECT_MUSIC_CHART_INFO_NUM_CHOICES {
        1u8 << (idx as u8)
    } else {
        0
    }
}

#[inline(always)]
pub(in crate::screens::options) const fn select_music_chart_info_mask_from_config(
    cfg: &config::Config,
) -> u8 {
    select_music_chart_info_mask(
        cfg.select_music_chart_info_peak_nps,
        cfg.select_music_chart_info_matrix_rating,
    )
}

#[inline(always)]
pub(in crate::screens::options) fn apply_select_music_chart_info_mask(mask: u8) {
    config::update_select_music_chart_info_peak_nps((mask & (1u8 << 0)) != 0);
    config::update_select_music_chart_info_matrix_rating((mask & (1u8 << 1)) != 0);
}

pub(in crate::screens::options) fn toggle_select_music_chart_info_option(
    state: &mut State,
    choice_idx: usize,
) {
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
    audio::play_sfx("assets/sounds/change_value.ogg");
}

#[inline(always)]
pub(in crate::screens::options) fn select_music_chart_info_enabled_mask() -> u8 {
    let mask = select_music_chart_info_mask_from_config(&config::get());
    if mask == 0 { 1 } else { mask }
}

impl ChoiceEnum for SelectMusicItlWheelMode {
    const ALL: &'static [Self] = &[Self::Off, Self::Score, Self::PointsAndScore];
    const DEFAULT: Self = Self::Off;
}

impl ChoiceEnum for SelectMusicItlRankMode {
    const ALL: &'static [Self] = &[Self::None, Self::Chart, Self::Overall];
    const DEFAULT: Self = Self::None;
}

impl ChoiceEnum for SelectMusicWheelStyle {
    const ALL: &'static [Self] = &[Self::Itg, Self::Iidx];
    const DEFAULT: Self = Self::Itg;
}

impl ChoiceEnum for NewPackMode {
    const ALL: &'static [Self] = &[Self::Disabled, Self::OpenPack, Self::HasScore];
    const DEFAULT: Self = Self::Disabled;
}

impl ChoiceEnum for SelectMusicScoreboxPlacement {
    const ALL: &'static [Self] = &[Self::Auto, Self::StepPane];
    const DEFAULT: Self = Self::Auto;
}
