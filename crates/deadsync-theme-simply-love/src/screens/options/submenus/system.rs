use super::super::*;

pub(in crate::screens::options) use crate::config::{
    language_choice_index, language_flag_from_choice, translated_titles_choice_index,
    translated_titles_from_choice,
};
use deadsync_profile::NoteSkin;

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
        choices: &[literal_choice(NoteSkin::DEFAULT_NAME)],
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
