use super::super::*;

pub(in crate::screens::options) const MACHINE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::VisualStyle,
        label: lookup_key("OptionsMachine", "VisualStyle"),
        choices: VISUAL_STYLE_CHOICES,
        inline: true,
    },
    SubRow {
        id: SubRowId::Font,
        label: lookup_key("OptionsMachine", "MachineFont"),
        choices: &[
            localized_choice("OptionsMachine", "MachineFontWendy"),
            localized_choice("OptionsMachine", "MachineFontMega"),
        ],
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
        id: SubRowId::PreferredColor,
        label: lookup_key("OptionsMachine", "PreferredColor"),
        choices: COLOR_CHOICES,
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
        id: SubRowId::PackIniOffsets,
        label: lookup_key("OptionsMachine", "PackIniOffsets"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::DefaultSyncOffset,
        label: lookup_key("OptionsMachine", "DefaultSyncOffset"),
        choices: &[
            localized_choice("OptionsMachine", "DefaultSyncOffsetNull"),
            localized_choice("OptionsMachine", "DefaultSyncOffsetItg"),
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
        id: ItemId::MchFont,
        name: lookup_key("OptionsMachine", "MachineFont"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "MachineFontHelp",
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
        id: ItemId::MchPreferredColor,
        name: lookup_key("OptionsMachine", "PreferredColor"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PreferredColorHelp",
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
        id: ItemId::MchPackIniOffsets,
        name: lookup_key("OptionsMachine", "PackIniOffsets"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "PackIniOffsetsHelp",
        ))],
    },
    Item {
        id: ItemId::MchDefaultSyncOffset,
        name: lookup_key("OptionsMachine", "DefaultSyncOffset"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsMachineHelp",
            "DefaultSyncOffsetHelp",
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

impl ChoiceEnum for MachinePreferredPlayStyle {
    const ALL: &'static [Self] = &[Self::Single, Self::Versus, Self::Double];
    const DEFAULT: Self = Self::Single;
}

impl ChoiceEnum for MachinePreferredPlayMode {
    const ALL: &'static [Self] = &[Self::Regular, Self::Marathon];
    const DEFAULT: Self = Self::Regular;
}

impl ChoiceEnum for MachineFont {
    const ALL: &'static [Self] = &[Self::Wendy, Self::Mega];
    const DEFAULT: Self = Self::Wendy;
}

impl ChoiceEnum for DefaultSyncOffset {
    const ALL: &'static [Self] = &[Self::Null, Self::Itg];
    const DEFAULT: Self = Self::Null;
}

impl ChoiceEnum for VisualStyle {
    const ALL: &'static [Self] = &[
        Self::Hearts,
        Self::Arrows,
        Self::Bears,
        Self::Ducks,
        Self::Cats,
        Self::Spooky,
        Self::Gay,
        Self::Stars,
        Self::Thonk,
        Self::Technique,
        Self::Srpg9,
    ];
    const DEFAULT: Self = Self::Hearts;
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

pub(in crate::screens::options) const COLOR_CHOICES: &[Choice] = &[
    literal_choice("0"),
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
];

impl ChoiceEnum for LogLevel {
    const ALL: &'static [Self] = &[
        Self::Error,
        Self::Warn,
        Self::Info,
        Self::Debug,
        Self::Trace,
    ];
    const DEFAULT: Self = Self::Trace;
}
