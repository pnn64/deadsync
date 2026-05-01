use super::super::*;
use ::null_or_die::{BiasKernel, KernelTarget};

pub(in crate::screens::options) const NULL_OR_DIE_MENU_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::NullOrDieOptions,
        label: lookup_key("OptionsOnlineScoring", "NullOrDieOptions"),
        choices: &[],
        inline: false,
    },
    SubRow {
        id: SubRowId::SyncPacks,
        label: lookup_key("OptionsOnlineScoring", "SyncPacks"),
        choices: &[],
        inline: false,
    },
];

pub(in crate::screens::options) const NULL_OR_DIE_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SyncGraph,
        label: lookup_key("OptionsNullOrDie", "SyncGraph"),
        choices: &[
            localized_choice("OptionsNullOrDie", "SyncGraphFrequency"),
            localized_choice("OptionsNullOrDie", "SyncGraphBeatIndex"),
            localized_choice("OptionsNullOrDie", "SyncGraphPostKernelFingerprint"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::SyncConfidence,
        label: lookup_key("OptionsNullOrDie", "SyncConfidence"),
        choices: &[
            literal_choice("0%"),
            literal_choice("5%"),
            literal_choice("10%"),
            literal_choice("15%"),
            literal_choice("20%"),
            literal_choice("25%"),
            literal_choice("30%"),
            literal_choice("35%"),
            literal_choice("40%"),
            literal_choice("45%"),
            literal_choice("50%"),
            literal_choice("55%"),
            literal_choice("60%"),
            literal_choice("65%"),
            literal_choice("70%"),
            literal_choice("75%"),
            literal_choice("80%"),
            literal_choice("85%"),
            literal_choice("90%"),
            literal_choice("95%"),
            literal_choice("100%"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::PackSyncThreads,
        label: lookup_key("OptionsNullOrDie", "PackSyncThreads"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Fingerprint,
        label: lookup_key("OptionsNullOrDie", "Fingerprint"),
        choices: &[literal_choice("50.0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Window,
        label: lookup_key("OptionsNullOrDie", "Window"),
        choices: &[literal_choice("10.0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::Step,
        label: lookup_key("OptionsNullOrDie", "Step"),
        choices: &[literal_choice("0.2 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MagicOffset,
        label: lookup_key("OptionsNullOrDie", "MagicOffset"),
        choices: &[literal_choice("0.0 ms")],
        inline: false,
    },
    SubRow {
        id: SubRowId::KernelTarget,
        label: lookup_key("OptionsNullOrDie", "KernelTarget"),
        choices: &[
            localized_choice("OptionsNullOrDie", "KernelTargetDigest"),
            localized_choice("OptionsNullOrDie", "KernelTargetAccumulator"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::KernelType,
        label: lookup_key("OptionsNullOrDie", "KernelType"),
        choices: &[
            localized_choice("OptionsNullOrDie", "KernelTypeRising"),
            localized_choice("OptionsNullOrDie", "KernelTypeLoudest"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::FullSpectrogram,
        label: lookup_key("OptionsNullOrDie", "FullSpectrogram"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: false,
    },
];

pub(in crate::screens::options) const NULL_OR_DIE_MENU_ITEMS: &[Item] = &[
    Item {
        id: ItemId::NodOptions,
        name: lookup_key("OptionsOnlineScoring", "NullOrDieOptions"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "NullOrDieOptionsHelp",
        ))],
    },
    Item {
        id: ItemId::NodSyncPacks,
        name: lookup_key("OptionsOnlineScoring", "SyncPacks"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsOnlineScoringHelp",
            "SyncPacksHelp",
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

pub(in crate::screens::options) const NULL_OR_DIE_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::NodSyncGraph,
        name: lookup_key("OptionsNullOrDie", "SyncGraph"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "SyncGraphHelp",
        ))],
    },
    Item {
        id: ItemId::NodSyncConfidence,
        name: lookup_key("OptionsNullOrDie", "SyncConfidence"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "SyncConfidenceHelp",
        ))],
    },
    Item {
        id: ItemId::NodPackSyncThreads,
        name: lookup_key("OptionsNullOrDie", "PackSyncThreads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "PackSyncThreadsHelp",
        ))],
    },
    Item {
        id: ItemId::NodFingerprint,
        name: lookup_key("OptionsNullOrDie", "Fingerprint"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "FingerprintHelp",
        ))],
    },
    Item {
        id: ItemId::NodWindow,
        name: lookup_key("OptionsNullOrDie", "Window"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "WindowHelp",
        ))],
    },
    Item {
        id: ItemId::NodStep,
        name: lookup_key("OptionsNullOrDie", "Step"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "StepHelp",
        ))],
    },
    Item {
        id: ItemId::NodMagicOffset,
        name: lookup_key("OptionsNullOrDie", "MagicOffset"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "MagicOffsetHelp",
        ))],
    },
    Item {
        id: ItemId::NodKernelTarget,
        name: lookup_key("OptionsNullOrDie", "KernelTarget"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "KernelTargetHelp",
        ))],
    },
    Item {
        id: ItemId::NodKernelType,
        name: lookup_key("OptionsNullOrDie", "KernelType"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "KernelTypeHelp",
        ))],
    },
    Item {
        id: ItemId::NodFullSpectrogram,
        name: lookup_key("OptionsNullOrDie", "FullSpectrogram"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsNullOrDieHelp",
            "FullSpectrogramHelp",
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

impl ChoiceEnum for KernelTarget {
    const ALL: &'static [Self] = &[Self::Digest, Self::Accumulator];
    const DEFAULT: Self = Self::Digest;
}

impl ChoiceEnum for BiasKernel {
    const ALL: &'static [Self] = &[Self::Rising, Self::Loudest];
    const DEFAULT: Self = Self::Rising;
}
