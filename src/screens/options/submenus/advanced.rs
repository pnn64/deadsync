use super::super::*;

pub(in crate::screens::options) const ADVANCED_OPTIONS_ROWS: &[SubRow] = &[
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

pub(in crate::screens::options) const ADVANCED_OPTIONS_ITEMS: &[Item] = &[
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

pub(in crate::screens::options) const ADVANCED_SONG_PARSING_THREADS_ROW_INDEX: usize = 3;

pub(in crate::screens::options) const fn default_fail_type_choice_index(
    fail_type: DefaultFailType,
) -> usize {
    match fail_type {
        DefaultFailType::Immediate => 0,
        DefaultFailType::ImmediateContinue => 1,
    }
}

pub(in crate::screens::options) const fn default_fail_type_from_choice(
    idx: usize,
) -> DefaultFailType {
    match idx {
        0 => DefaultFailType::Immediate,
        _ => DefaultFailType::ImmediateContinue,
    }
}
