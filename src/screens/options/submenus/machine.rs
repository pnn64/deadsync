use super::super::*;

pub(in crate::screens::options) const MACHINE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::VisualStyle,
        label: lookup_key("OptionsMachine", "VisualStyle"),
        choices: VISUAL_STYLE_CHOICES,
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectProfile,
        label: lookup_key("OptionsMachine", "SelectProfile"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectColor,
        label: lookup_key("OptionsMachine", "SelectColor"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectStyle,
        label: lookup_key("OptionsMachine", "SelectStyle"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PreferredStyle,
        label: lookup_key("OptionsMachine", "PreferredStyle"),
        choices: &[
            localized_choice("OptionsMachine", "PreferredStyleSingle"),
            localized_choice("OptionsMachine", "PreferredStyleVersus"),
            localized_choice("OptionsMachine", "PreferredStyleDouble"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SelectPlayMode,
        label: lookup_key("OptionsMachine", "SelectPlayMode"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PreferredMode,
        label: lookup_key("OptionsMachine", "PreferredMode"),
        choices: &[
            localized_choice("OptionsMachine", "PreferredModeRegular"),
            localized_choice("OptionsMachine", "PreferredModeMarathon"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::Font,
        label: lookup_key("OptionsMachine", "MachineFont"),
        choices: &[
            localized_choice("OptionsMachine", "MachineFontCommon"),
            localized_choice("OptionsMachine", "MachineFontMega"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::EvalSummary,
        label: lookup_key("OptionsMachine", "EvalSummary"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::NameEntry,
        label: lookup_key("OptionsMachine", "NameEntry"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GameoverScreen,
        label: lookup_key("OptionsMachine", "GameoverScreen"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::WriteCurrentScreen,
        label: lookup_key("OptionsMachine", "WriteCurrentScreen"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MenuMusic,
        label: lookup_key("OptionsMachine", "MenuMusic"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::Replays,
        label: lookup_key("OptionsMachine", "Replays"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PerPlayerGlobalOffsets,
        label: lookup_key("OptionsMachine", "PerPlayerGlobalOffsets"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::KeyboardFeatures,
        label: lookup_key("OptionsMachine", "KeyboardFeatures"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::VideoBgs,
        label: lookup_key("OptionsMachine", "VideoBGs"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const MACHINE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::MchVisualStyle,
        name: lookup_key("OptionsMachine", "VisualStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "VisualStyleHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectProfile,
        name: lookup_key("OptionsMachine", "SelectProfile"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectProfileHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectColor,
        name: lookup_key("OptionsMachine", "SelectColor"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectColorHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectStyle,
        name: lookup_key("OptionsMachine", "SelectStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectStyleHelp",
        ))],
    },
    Item {
        id: ItemId::MchPreferredStyle,
        name: lookup_key("OptionsMachine", "PreferredStyle"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PreferredStyleHelp",
        ))],
    },
    Item {
        id: ItemId::MchSelectPlayMode,
        name: lookup_key("OptionsMachine", "SelectPlayMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "SelectPlayModeHelp",
        ))],
    },
    Item {
        id: ItemId::MchPreferredMode,
        name: lookup_key("OptionsMachine", "PreferredMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PreferredModeHelp",
        ))],
    },
    Item {
        id: ItemId::MchFont,
        name: lookup_key("OptionsMachine", "MachineFont"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "MachineFontHelp",
        ))],
    },
    Item {
        id: ItemId::MchEvalSummary,
        name: lookup_key("OptionsMachine", "EvalSummary"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "EvalSummaryHelp",
        ))],
    },
    Item {
        id: ItemId::MchNameEntry,
        name: lookup_key("OptionsMachine", "NameEntry"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "NameEntryHelp",
        ))],
    },
    Item {
        id: ItemId::MchGameoverScreen,
        name: lookup_key("OptionsMachine", "GameoverScreen"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "GameoverScreenHelp",
        ))],
    },
    Item {
        id: ItemId::MchWriteCurrentScreen,
        name: lookup_key("OptionsMachine", "WriteCurrentScreen"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "WriteCurrentScreenHelp",
        ))],
    },
    Item {
        id: ItemId::MchMenuMusic,
        name: lookup_key("OptionsMachine", "MenuMusic"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "MenuMusicHelp",
        ))],
    },
    Item {
        id: ItemId::MchReplays,
        name: lookup_key("OptionsMachine", "Replays"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "ReplaysHelp",
        ))],
    },
    Item {
        id: ItemId::MchPerPlayerGlobalOffsets,
        name: lookup_key("OptionsMachine", "PerPlayerGlobalOffsets"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PerPlayerGlobalOffsetsHelp",
        ))],
    },
    Item {
        id: ItemId::MchKeyboardFeatures,
        name: lookup_key("OptionsMachine", "KeyboardFeatures"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "KeyboardFeaturesHelp",
        ))],
    },
    Item {
        id: ItemId::MchVideoBgs,
        name: lookup_key("OptionsMachine", "VideoBGs"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "VideoBgsHelp",
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



pub(in crate::screens::options) const fn machine_preferred_style_choice_index(
    style: MachinePreferredPlayStyle,
) -> usize {
    match style {
        MachinePreferredPlayStyle::Single => 0,
        MachinePreferredPlayStyle::Versus => 1,
        MachinePreferredPlayStyle::Double => 2,
    }
}

pub(in crate::screens::options) const fn machine_preferred_style_from_choice(
    idx: usize,
) -> MachinePreferredPlayStyle {
    match idx {
        1 => MachinePreferredPlayStyle::Versus,
        2 => MachinePreferredPlayStyle::Double,
        _ => MachinePreferredPlayStyle::Single,
    }
}

pub(in crate::screens::options) const fn machine_preferred_mode_choice_index(
    mode: MachinePreferredPlayMode,
) -> usize {
    match mode {
        MachinePreferredPlayMode::Regular => 0,
        MachinePreferredPlayMode::Marathon => 1,
    }
}

pub(in crate::screens::options) const fn machine_preferred_mode_from_choice(
    idx: usize,
) -> MachinePreferredPlayMode {
    match idx {
        1 => MachinePreferredPlayMode::Marathon,
        _ => MachinePreferredPlayMode::Regular,
    }
}

pub(in crate::screens::options) const fn machine_font_choice_index(font: MachineFont) -> usize {
    match font {
        MachineFont::Common => 0,
        MachineFont::Mega => 1,
    }
}

pub(in crate::screens::options) const fn machine_font_from_choice(idx: usize) -> MachineFont {
    match idx {
        1 => MachineFont::Mega,
        _ => MachineFont::Common,
    }
}

pub(in crate::screens::options) const fn visual_style_choice_index(style: VisualStyle) -> usize {
    match style {
        VisualStyle::Hearts => 0,
        VisualStyle::Arrows => 1,
        VisualStyle::Bears => 2,
        VisualStyle::Ducks => 3,
        VisualStyle::Cats => 4,
        VisualStyle::Spooky => 5,
        VisualStyle::Gay => 6,
        VisualStyle::Stars => 7,
        VisualStyle::Thonk => 8,
        VisualStyle::Technique => 9,
        VisualStyle::Srpg9 => 10,
    }
}

pub(in crate::screens::options) const fn visual_style_from_choice(idx: usize) -> VisualStyle {
    match idx {
        1 => VisualStyle::Arrows,
        2 => VisualStyle::Bears,
        3 => VisualStyle::Ducks,
        4 => VisualStyle::Cats,
        5 => VisualStyle::Spooky,
        6 => VisualStyle::Gay,
        7 => VisualStyle::Stars,
        8 => VisualStyle::Thonk,
        9 => VisualStyle::Technique,
        10 => VisualStyle::Srpg9,
        _ => VisualStyle::Hearts,
    }
}

pub(in crate::screens::options) const VISUAL_STYLE_CHOICES: &[Choice] = &[
    literal_choice("❤"),
    literal_choice("↖"),
    literal_choice("🐻"),
    literal_choice("🦆"),
    literal_choice("😺"),
    literal_choice("🎃"),
    literal_choice("🌈"),
    literal_choice("⭐"),
    literal_choice("🤔"),
    literal_choice("🌀"),
    literal_choice("💪"),
];

pub(in crate::screens::options) const fn log_level_choice_index(level: LogLevel) -> usize {
    match level {
        LogLevel::Error => 0,
        LogLevel::Warn => 1,
        LogLevel::Info => 2,
        LogLevel::Debug => 3,
        LogLevel::Trace => 4,
    }
}

pub(in crate::screens::options) const fn log_level_from_choice(idx: usize) -> LogLevel {
    match idx {
        0 => LogLevel::Error,
        1 => LogLevel::Warn,
        2 => LogLevel::Info,
        3 => LogLevel::Debug,
        _ => LogLevel::Trace,
    }
}
