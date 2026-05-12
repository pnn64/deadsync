use super::super::*;

pub(in crate::screens::options) const LIGHTS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::LightsDriver,
        label: lookup_key("OptionsLights", "Driver"),
        choices: &[
            literal_choice("None"),
            literal_choice("Snek"),
            literal_choice("Litboard"),
            literal_choice("Fusion"),
            literal_choice("GPB"),
            literal_choice("PacDrive"),
            literal_choice("HidBlueDot"),
            literal_choice("STAC2"),
            literal_choice("MinimaidHID"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::GameplayPadLights,
        label: lookup_key("OptionsLights", "GameplayPadLights"),
        choices: &[literal_choice("Input"), literal_choice("Chart")],
        inline: true,
    },
    SubRow {
        id: SubRowId::TestLights,
        label: lookup_key("OptionsLights", "TestLights"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
];

pub(in crate::screens::options) const LIGHTS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::LightsDriver,
        name: lookup_key("OptionsLights", "Driver"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsLightsHelp",
            "DriverHelp",
        ))],
    },
    Item {
        id: ItemId::LightsGameplayPadLights,
        name: lookup_key("OptionsLights", "GameplayPadLights"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsLightsHelp",
            "GameplayPadLightsHelp",
        ))],
    },
    Item {
        id: ItemId::LightsTest,
        name: lookup_key("OptionsLights", "TestLights"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsLightsHelp",
            "TestLightsHelp",
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

pub(in crate::screens::options) const fn lights_driver_choice_index(
    driver: LightsDriverKind,
) -> usize {
    match driver {
        LightsDriverKind::Off => 0,
        LightsDriverKind::Snek => 1,
        LightsDriverKind::Litboard => 2,
        LightsDriverKind::Fusion => 3,
        LightsDriverKind::Gpb => 4,
        LightsDriverKind::PacDrive => 5,
        LightsDriverKind::HidBlueDot => 6,
        LightsDriverKind::Stac2 => 7,
        LightsDriverKind::MinimaidHid => 8,
    }
}

pub(in crate::screens::options) const fn lights_driver_from_choice(idx: usize) -> LightsDriverKind {
    match idx {
        1 => LightsDriverKind::Snek,
        2 => LightsDriverKind::Litboard,
        3 => LightsDriverKind::Fusion,
        4 => LightsDriverKind::Gpb,
        5 => LightsDriverKind::PacDrive,
        6 => LightsDriverKind::HidBlueDot,
        7 => LightsDriverKind::Stac2,
        8 => LightsDriverKind::MinimaidHid,
        _ => LightsDriverKind::Off,
    }
}

pub(in crate::screens::options) const fn lights_gameplay_pad_choice_index(
    mode: LightsGameplayPadMode,
) -> usize {
    match mode {
        LightsGameplayPadMode::Input => 0,
        LightsGameplayPadMode::Chart => 1,
    }
}

pub(in crate::screens::options) const fn lights_gameplay_pad_from_choice(
    idx: usize,
) -> LightsGameplayPadMode {
    match idx {
        1 => LightsGameplayPadMode::Chart,
        _ => LightsGameplayPadMode::Input,
    }
}
