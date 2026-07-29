use super::super::*;

pub(in crate::screens::options) use crate::config::{
    language_choice_index, language_flag_from_choice, translated_titles_choice_index,
    translated_titles_from_choice,
};
use deadsync_profile::{BackgroundFilter, NoteSkin, ScrollOption};
use deadsync_rules::scroll::ScrollSpeedSetting;

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
        id: SubRowId::DefaultScrollSpeed,
        label: lookup_key("OptionsSystem", "DefaultScrollSpeed"),
        choices: &[literal_choice("C600")],
        inline: false,
    },
    SubRow {
        id: SubRowId::DefaultScrollDirection,
        label: lookup_key("OptionsSystem", "DefaultScrollDirection"),
        choices: &[literal_choice("Normal")],
        inline: false,
    },
    SubRow {
        id: SubRowId::DefaultBackgroundFilter,
        label: lookup_key("OptionsSystem", "DefaultBackgroundFilter"),
        choices: &[literal_choice("95%")],
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
        id: ItemId::SysDefaultScrollSpeed,
        name: lookup_key("OptionsSystem", "DefaultScrollSpeed"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "DefaultScrollSpeedHelp",
        ))],
    },
    Item {
        id: ItemId::SysDefaultScrollDirection,
        name: lookup_key("OptionsSystem", "DefaultScrollDirection"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "DefaultScrollDirectionHelp",
        ))],
    },
    Item {
        id: ItemId::SysDefaultBackgroundFilter,
        name: lookup_key("OptionsSystem", "DefaultBackgroundFilter"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsSystemHelp",
            "DefaultBackgroundFilterHelp",
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

pub(in crate::screens::options) fn system_scroll_speed_values(
    current: ScrollSpeedSetting,
) -> Vec<ScrollSpeedSetting> {
    let mut values = Vec::with_capacity(108);
    for value in 2..=32 {
        values.push(ScrollSpeedSetting::XMod(value as f32 * 0.25));
    }
    for value in (100..=1000).step_by(25) {
        values.push(ScrollSpeedSetting::CMod(value as f32));
    }
    for value in (100..=1000).step_by(25) {
        values.push(ScrollSpeedSetting::MMod(value as f32));
    }
    if !values.contains(&current) {
        values.push(current);
    }
    values
}

pub(in crate::screens::options) fn system_scroll_direction_values(
    current: ScrollOption,
) -> Vec<ScrollOption> {
    let mut values = vec![ScrollOption::Normal, ScrollOption::Reverse];
    if !values.contains(&current) {
        values.push(current);
    }
    values
}

pub(in crate::screens::options) fn system_background_filter_values(
    current: BackgroundFilter,
) -> Vec<BackgroundFilter> {
    let mut values = (0..=100)
        .step_by(5)
        .map(BackgroundFilter::from_percent)
        .collect::<Vec<_>>();
    if !values.contains(&current) {
        values.push(current);
    }
    values
}
