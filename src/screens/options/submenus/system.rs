use super::super::*;

pub(in crate::screens::options) const SYSTEM_OPTIONS_ROWS: &[SubRow] = &[
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

pub(in crate::screens::options) const SYSTEM_OPTIONS_ITEMS: &[Item] = &[
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

pub(in crate::screens::options) fn discover_system_noteskin_choices() -> Vec<String> {
    let mut names = noteskin_parser::discover_itg_skins("dance");
    if names.is_empty() {
        names.push(profile::NoteSkin::DEFAULT_NAME.to_string());
    }
    names
}

pub(in crate::screens::options) const fn translated_titles_choice_index(
    translated_titles: bool,
) -> usize {
    if translated_titles { 0 } else { 1 }
}

pub(in crate::screens::options) const fn translated_titles_from_choice(idx: usize) -> bool {
    idx == 0
}

pub(in crate::screens::options) const fn language_choice_index(
    flag: config::LanguageFlag,
) -> usize {
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

pub(in crate::screens::options) const fn language_flag_from_choice(
    idx: usize,
) -> config::LanguageFlag {
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
