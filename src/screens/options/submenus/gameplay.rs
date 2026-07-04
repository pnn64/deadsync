use super::super::*;

pub(in crate::screens::options) use crate::config::{
    sync_confidence_choice_index, sync_confidence_from_choice,
};

pub(in crate::screens::options) const GAMEPLAY_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::BgBrightness,
        label: lookup_key("OptionsGameplay", "BGBrightness"),
        choices: &[
            literal_choice("0%"),
            literal_choice("10%"),
            literal_choice("20%"),
            literal_choice("30%"),
            literal_choice("40%"),
            literal_choice("50%"),
            literal_choice("60%"),
            literal_choice("70%"),
            literal_choice("80%"),
            literal_choice("90%"),
            literal_choice("100%"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::CenteredP1Notefield,
        label: lookup_key("OptionsGameplay", "CenteredP1Notefield"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ZmodRatingBox,
        label: lookup_key("OptionsGameplay", "ZmodRatingBox"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::BpmDecimal,
        label: lookup_key("OptionsGameplay", "BpmDecimal"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("Common", "On"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::DelayedBack,
        label: lookup_key("OptionsGameplay", "DelayedBack"),
        choices: &[
            localized_choice("OptionsGameplay", "DelayedBackInstant"),
            localized_choice("OptionsGameplay", "DelayedBackHold"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::AutoScreenshot,
        label: lookup_key("OptionsGameplay", "AutoScreenshot"),
        choices: &[
            literal_choice("PBs"),
            literal_choice("Fails"),
            literal_choice("Clears"),
            literal_choice("Quads"),
            literal_choice("Quints"),
        ],
        inline: true,
    },
];

pub(in crate::screens::options) const GAMEPLAY_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::GpBgBrightness,
        name: lookup_key("OptionsGameplay", "BGBrightness"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "BgBrightnessHelp",
        ))],
    },
    Item {
        id: ItemId::GpCenteredP1,
        name: lookup_key("OptionsGameplay", "CenteredP1Notefield"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "CenteredP1NotefieldHelp",
        ))],
    },
    Item {
        id: ItemId::GpZmodRatingBox,
        name: lookup_key("OptionsGameplay", "ZmodRatingBox"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "ZmodRatingBoxHelp",
        ))],
    },
    Item {
        id: ItemId::GpBpmDecimal,
        name: lookup_key("OptionsGameplay", "BpmDecimal"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "BpmDecimalHelp",
        ))],
    },
    Item {
        id: ItemId::GpDelayedBack,
        name: lookup_key("OptionsGameplay", "DelayedBack"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "DelayedBackHelp",
        ))],
    },
    Item {
        id: ItemId::GpAutoScreenshot,
        name: lookup_key("OptionsGameplay", "AutoScreenshot"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGameplayHelp",
            "AutoScreenshotHelp",
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
