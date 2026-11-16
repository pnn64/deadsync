use super::super::*;

pub(in crate::screens::options) use deadsync_config::options::{
    GROOVESTATS_SCOREBOX_NUM_CHOICES, arrowcloud_regular_boards_shown,
    groovestats_scorebox_bit_from_choice, groovestats_scorebox_cursor_index,
    groovestats_scorebox_mask,
};

pub(in crate::screens::options) const GROOVESTATS_OPTIONS_ROWS: &[SubRow] = &[
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
        id: SubRowId::GrooveStatsScoreboxBoards,
        label: lookup_key("OptionsGrooveStats", "ScoreboxBoards"),
        choices: &[
            localized_choice("OptionsGrooveStats", "ScoreboxBoardsITG"),
            localized_choice("OptionsGrooveStats", "ScoreboxBoardsEX"),
            localized_choice("OptionsGrooveStats", "ScoreboxBoardsEvents"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ShowSrpgShop,
        label: lookup_key("OptionsGrooveStats", "ShowSRPGShop"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SrpgShopFolder,
        label: lookup_key("OptionsGrooveStats", "SRPGShopFolder"),
        choices: &[
            localized_choice("OptionsGrooveStats", "SRPGShopFolderUnlocks"),
            localized_choice("OptionsGrooveStats", "SRPGShopFolderShops"),
            localized_choice("OptionsGrooveStats", "SRPGShopFolderFaction"),
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
    SubRow {
        id: SubRowId::GrooveStatsQrLogin,
        label: lookup_key("OptionsGrooveStats", "GrooveStatsQRLogin"),
        choices: &[
            localized_choice("OptionsGrooveStats", "GrooveStatsQRLoginAlways"),
            localized_choice("OptionsGrooveStats", "GrooveStatsQRLoginSometimes"),
            localized_choice("OptionsGrooveStats", "GrooveStatsQRLoginDisabled"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const ARROWCLOUD_OPTIONS_ROWS: &[SubRow] = &[
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
        id: SubRowId::ArrowCloudRegularBoards,
        label: lookup_key("OptionsGrooveStats", "ArrowCloudRegularBoards"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ArrowCloudEventBoards,
        label: lookup_key("OptionsGrooveStats", "ArrowCloudEventBoards"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::EnableGolf,
        label: lookup_key("OptionsGrooveStats", "EnableGolf"),
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
    SubRow {
        id: SubRowId::ArrowCloudResultDialogs,
        label: lookup_key("OptionsGrooveStats", "ArrowCloudResultDialogs"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ArrowCloudQrLogin,
        label: lookup_key("OptionsGrooveStats", "ArrowCloudQRLogin"),
        choices: &[
            localized_choice("OptionsGrooveStats", "ArrowCloudQRLoginAlways"),
            localized_choice("OptionsGrooveStats", "ArrowCloudQRLoginSometimes"),
            localized_choice("OptionsGrooveStats", "ArrowCloudQRLoginDisabled"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const ONLINE_SCORING_OPTIONS_ROWS: &[SubRow] = &[
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
    SubRow {
        id: SubRowId::ShareRips,
        label: lookup_key("OptionsOnlineScoring", "ShareRips"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const GROOVESTATS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::GsEnable,
        name: lookup_key("OptionsGrooveStats", "EnableGrooveStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "EnableGrooveStatsHelp",
        ))],
    },
    Item {
        id: ItemId::GsScoreboxBoards,
        name: lookup_key("OptionsGrooveStats", "ScoreboxBoards"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "ScoreboxBoardsHelp",
        ))],
    },
    Item {
        id: ItemId::GsShowSrpgShop,
        name: lookup_key("OptionsGrooveStats", "ShowSRPGShop"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "ShowSRPGShopHelp",
        ))],
    },
    Item {
        id: ItemId::GsSrpgShopFolder,
        name: lookup_key("OptionsGrooveStats", "SRPGShopFolder"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "SRPGShopFolderHelp",
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
        id: ItemId::GsQrLogin,
        name: lookup_key("OptionsGrooveStats", "GrooveStatsQRLogin"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "GrooveStatsQRLoginHelp",
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

pub(in crate::screens::options) const ARROWCLOUD_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::AcEnable,
        name: lookup_key("OptionsGrooveStats", "EnableArrowCloud"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "EnableArrowCloudHelp",
        ))],
    },
    Item {
        id: ItemId::AcRegularBoards,
        name: lookup_key("OptionsGrooveStats", "ArrowCloudRegularBoards"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "ArrowCloudRegularBoardsHelp",
        ))],
    },
    Item {
        id: ItemId::AcEventBoards,
        name: lookup_key("OptionsGrooveStats", "ArrowCloudEventBoards"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsGrooveStatsHelp", "ArrowCloudEventBoardsHelp")),
            HelpEntry::Paragraph(lookup_key("OptionsGrooveStatsHelp", "GolfLeaderboardHelp")),
        ],
    },
    Item {
        id: ItemId::AcGolf,
        name: lookup_key("OptionsGrooveStats", "EnableGolf"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "EnableGolfHelp",
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
        id: ItemId::AcResultDialogs,
        name: lookup_key("OptionsGrooveStats", "ArrowCloudResultDialogs"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "ArrowCloudResultDialogsHelp",
        ))],
    },
    Item {
        id: ItemId::AcQrLogin,
        name: lookup_key("OptionsGrooveStats", "ArrowCloudQRLogin"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGrooveStatsHelp",
            "ArrowCloudQRLoginHelp",
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

pub(in crate::screens::options) const ONLINE_SCORING_OPTIONS_ITEMS: &[Item] = &[
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
        id: ItemId::OsShareRips,
        name: lookup_key("OptionsOnlineScoring", "ShareRips"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "ShareRipsHelp",
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
pub(in crate::screens::options) const fn groovestats_scorebox_mask_from_config(
    cfg: &config::app_config::Config,
) -> u8 {
    groovestats_scorebox_mask(
        cfg.groovestats_scorebox_itg,
        cfg.groovestats_scorebox_ex,
        cfg.groovestats_scorebox_events,
    )
}

pub(in crate::screens::options) fn toggle_groovestats_scorebox_board(
    state: &mut State,
    choice_idx: usize,
) -> ThemeEffect {
    let bit = groovestats_scorebox_bit_from_choice(choice_idx);
    if bit == 0 {
        return ThemeEffect::None;
    }
    let mask = state.groovestats_scorebox_mask ^ bit;
    state.groovestats_scorebox_mask = mask;

    let sub = &mut state.sub[SubmenuKind::GrooveStats];
    for indices in [&mut sub.choice_indices, &mut sub.cursor_indices] {
        if let Some(slot) = get_choice_by_id_mut(
            indices,
            GROOVESTATS_OPTIONS_ROWS,
            SubRowId::GrooveStatsScoreboxBoards,
        ) {
            *slot = choice_idx;
        }
    }
    queue_sfx(state, "assets/sounds/change_value.ogg");
    online_config_effect(crate::SimplyLoveOnlineConfigRequest::GrooveStatsScoreboxBoards(mask))
}

#[inline(always)]
pub(in crate::screens::options) const fn groovestats_scorebox_enabled_mask(state: &State) -> u8 {
    state.groovestats_scorebox_mask
}
