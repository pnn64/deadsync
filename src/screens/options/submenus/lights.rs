use super::super::*;

pub(in crate::screens::options) use deadsync_lights::{
    lights_driver_choice_index, lights_driver_from_choice, lights_gameplay_pad_choice_index,
    lights_gameplay_pad_from_choice,
};

pub(in crate::screens::options) const LIGHTS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::LightsDriver,
        label: lookup_key("OptionsLights", "Driver"),
        choices: &[
            literal_choice("None"),
            literal_choice("Snek"),
            literal_choice("Litboard"),
            literal_choice("Win32Serial"),
            literal_choice("Fusion"),
            literal_choice("GPB"),
            literal_choice("PacDrive"),
            literal_choice("PIUIO_Leds"),
            literal_choice("ITGIO"),
            literal_choice("HidBlueDot"),
            literal_choice("STAC2"),
            literal_choice("MinimaidHID"),
        ],
        inline: false,
    },
    SubRow {
        id: SubRowId::GameplayPadLights,
        label: lookup_key("OptionsLights", "GameplayPadLights"),
        choices: &[literal_choice("Input"), literal_choice("Chart")],
        inline: true,
    },
    SubRow {
        id: SubRowId::LightsSimplifyBass,
        label: lookup_key("OptionsLights", "SimplifyBass"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
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
        id: ItemId::LightsSimplifyBass,
        name: lookup_key("OptionsLights", "SimplifyBass"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsLightsHelp",
            "SimplifyBassHelp",
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
