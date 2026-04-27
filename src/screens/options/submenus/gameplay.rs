use super::super::*;

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

impl ChoiceEnum for BreakdownStyle {
    const ALL: &'static [Self] = &[Self::Sl, Self::Sn];
    const DEFAULT: Self = Self::Sl;
}

impl ChoiceEnum for SyncGraphMode {
    const ALL: &'static [Self] = &[Self::Frequency, Self::BeatIndex, Self::PostKernelFingerprint];
    const DEFAULT: Self = Self::PostKernelFingerprint;
}

pub(in crate::screens::options) const fn sync_confidence_choice_index(percent: u8) -> usize {
    let capped = if percent > 100 { 100 } else { percent };
    ((capped as usize) + 2) / 5
}

pub(in crate::screens::options) const fn sync_confidence_from_choice(idx: usize) -> u8 {
    let capped = if idx > 20 { 20 } else { idx };
    capped as u8 * 5
}
