use super::super::*;

pub(in crate::screens::options) const COIN_MODE_VALUES: [deadsync_config::prelude::CoinMode; 3] = [
    deadsync_config::prelude::CoinMode::Home,
    deadsync_config::prelude::CoinMode::Pay,
    deadsync_config::prelude::CoinMode::Free,
];
pub(in crate::screens::options) const PREMIUM_MINUTE_VALUES: [u8; 7] = [0, 10, 11, 12, 13, 14, 15];
pub(in crate::screens::options) const PREMIUM_GRACE_VALUES: [u16; 11] =
    [0, 60, 120, 180, 240, 300, 360, 420, 480, 540, 600];
pub(in crate::screens::options) const LONG_SONG_VALUES: [u16; 4] = [120, 150, 180, 210];
pub(in crate::screens::options) const MARATHON_SONG_VALUES: [u16; 7] =
    [240, 300, 360, 420, 480, 540, 600];

pub(in crate::screens::options) const COIN_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::CoinMode,
        label: lookup_key("OptionsCoin", "CoinMode"),
        choices: &[
            literal_choice("Home"),
            literal_choice("Pay"),
            literal_choice("Free"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::EventMode,
        label: lookup_key("OptionsCoin", "EventMode"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::CoinsPerCredit,
        label: lookup_key("OptionsCoin", "CoinsPerCredit"),
        choices: &[
            literal_choice("1"),
            literal_choice("2"),
            literal_choice("3"),
            literal_choice("4"),
            literal_choice("5"),
            literal_choice("6"),
            literal_choice("7"),
            literal_choice("8"),
            literal_choice("9"),
            literal_choice("10"),
            literal_choice("11"),
            literal_choice("12"),
            literal_choice("13"),
            literal_choice("14"),
            literal_choice("15"),
            literal_choice("16"),
            literal_choice("17"),
            literal_choice("18"),
            literal_choice("19"),
            literal_choice("20"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::SongsPerPlay,
        label: lookup_key("OptionsCoin", "SongsPerPlay"),
        choices: &[
            literal_choice("1"),
            literal_choice("2"),
            literal_choice("3"),
            literal_choice("4"),
            literal_choice("5"),
            literal_choice("6"),
            literal_choice("7"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PremiumFree,
        label: lookup_key("OptionsCoin", "PremiumFree"),
        choices: &[
            localized_choice("Common", "Off"),
            literal_choice("10 min"),
            literal_choice("11 min"),
            literal_choice("12 min"),
            literal_choice("13 min"),
            literal_choice("14 min"),
            literal_choice("15 min"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::PremiumGrace,
        label: lookup_key("OptionsCoin", "PremiumGrace"),
        choices: &[
            literal_choice("0 min"),
            literal_choice("1 min"),
            literal_choice("2 min"),
            literal_choice("3 min"),
            literal_choice("4 min"),
            literal_choice("5 min"),
            literal_choice("6 min"),
            literal_choice("7 min"),
            literal_choice("8 min"),
            literal_choice("9 min"),
            literal_choice("10 min"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::ContinueOnGiveUp,
        label: lookup_key("OptionsCoin", "ContinueOnGiveUp"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::LongSongTime,
        label: lookup_key("OptionsCoin", "LongSongTime"),
        choices: &[
            literal_choice("2:00"),
            literal_choice("2:30"),
            literal_choice("3:00"),
            literal_choice("3:30"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::MarathonSongTime,
        label: lookup_key("OptionsCoin", "MarathonSongTime"),
        choices: &[
            literal_choice("4:00"),
            literal_choice("5:00"),
            literal_choice("6:00"),
            literal_choice("7:00"),
            literal_choice("8:00"),
            literal_choice("9:00"),
            literal_choice("10:00"),
        ],
        inline: false,
    },
];

pub(in crate::screens::options) const COIN_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::CoinMode,
        name: lookup_key("OptionsCoin", "CoinMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "CoinMode",
        ))],
    },
    Item {
        id: ItemId::CoinEventMode,
        name: lookup_key("OptionsCoin", "EventMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "EventMode",
        ))],
    },
    Item {
        id: ItemId::CoinCoinsPerCredit,
        name: lookup_key("OptionsCoin", "CoinsPerCredit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "CoinsPerCredit",
        ))],
    },
    Item {
        id: ItemId::CoinSongsPerPlay,
        name: lookup_key("OptionsCoin", "SongsPerPlay"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "SongsPerPlay",
        ))],
    },
    Item {
        id: ItemId::CoinPremiumFree,
        name: lookup_key("OptionsCoin", "PremiumFree"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "PremiumFree",
        ))],
    },
    Item {
        id: ItemId::CoinPremiumGrace,
        name: lookup_key("OptionsCoin", "PremiumGrace"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "PremiumGrace",
        ))],
    },
    Item {
        id: ItemId::CoinContinueOnGiveUp,
        name: lookup_key("OptionsCoin", "ContinueOnGiveUp"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "ContinueOnGiveUp",
        ))],
    },
    Item {
        id: ItemId::CoinLongSongTime,
        name: lookup_key("OptionsCoin", "LongSongTime"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "LongSongTime",
        ))],
    },
    Item {
        id: ItemId::CoinMarathonSongTime,
        name: lookup_key("OptionsCoin", "MarathonSongTime"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsCoinHelp",
            "MarathonSongTime",
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

pub(in crate::screens::options) const BOOKKEEPING_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::CoinsInserted,
        label: lookup_key("OptionsBookkeeping", "CoinsInserted"),
        choices: &[literal_choice("0")],
        inline: false,
    },
    SubRow {
        id: SubRowId::CreditsSpent,
        label: lookup_key("OptionsBookkeeping", "CreditsSpent"),
        choices: &[literal_choice("0")],
        inline: false,
    },
    SubRow {
        id: SubRowId::PlaysStarted,
        label: lookup_key("OptionsBookkeeping", "PlaysStarted"),
        choices: &[literal_choice("0")],
        inline: false,
    },
    SubRow {
        id: SubRowId::StagesPlayed,
        label: lookup_key("OptionsBookkeeping", "StagesPlayed"),
        choices: &[literal_choice("0")],
        inline: false,
    },
];

pub(in crate::screens::options) const BOOKKEEPING_ITEMS: &[Item] = &[
    Item {
        id: ItemId::BookkeepingCoins,
        name: lookup_key("OptionsBookkeeping", "CoinsInserted"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsBookkeepingHelp",
            "CoinsInserted",
        ))],
    },
    Item {
        id: ItemId::BookkeepingCredits,
        name: lookup_key("OptionsBookkeeping", "CreditsSpent"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsBookkeepingHelp",
            "CreditsSpent",
        ))],
    },
    Item {
        id: ItemId::BookkeepingPlays,
        name: lookup_key("OptionsBookkeeping", "PlaysStarted"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsBookkeepingHelp",
            "PlaysStarted",
        ))],
    },
    Item {
        id: ItemId::BookkeepingStages,
        name: lookup_key("OptionsBookkeeping", "StagesPlayed"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsBookkeepingHelp",
            "StagesPlayed",
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
