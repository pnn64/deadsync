use super::super::*;

// USB polling choice list bounds: index N maps to 250 + N*50 microseconds
// (250us .. 1000us; 250us = 4000Hz, already overkill).
const USB_POLLING_MIN_US: u16 = 250;
const USB_POLLING_STEP_US: u16 = 50;
pub(in crate::screens::options) const USB_POLLING_CHOICE_COUNT: usize = 16;

/// Choice index for a polling value in microseconds (clamped to the list).
pub(in crate::screens::options) fn usb_polling_choice_index(value: u16) -> usize {
    let max = USB_POLLING_MIN_US + USB_POLLING_STEP_US * (USB_POLLING_CHOICE_COUNT as u16 - 1);
    let v = value.clamp(USB_POLLING_MIN_US, max);
    ((v - USB_POLLING_MIN_US) / USB_POLLING_STEP_US) as usize
}

/// Polling value in microseconds for a choice index (clamped to the list).
pub(in crate::screens::options) fn usb_polling_value(index: usize) -> u16 {
    USB_POLLING_MIN_US + (index.min(USB_POLLING_CHOICE_COUNT - 1) as u16) * USB_POLLING_STEP_US
}

/// Live help text for the Assign Pads row: which pad is currently P1 (blue) vs
/// P2 (red) by slot, plus a same-jumper warning when the pads are ambiguous and
/// not yet assigned.
fn smx_assignment_status() -> std::borrow::Cow<'static, str> {
    use crate::engine::smx;
    let label = |slot: usize| -> String {
        let info = smx::get_info(slot);
        if info.connected && !info.serial.is_empty() {
            format!("SMX[{}]", info.serial.chars().take(4).collect::<String>())
        } else {
            "—".to_owned()
        }
    };
    let mut s = format!("Now: P1 = {} (blue), P2 = {} (red)", label(0), label(1));
    if smx::conflict_warning_active() {
        s.push_str(
            "\n\nBoth pads share a jumper — assign them so the engine can tell them apart.",
        );
    }
    std::borrow::Cow::Owned(s)
}

pub(in crate::screens::options) const INPUT_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::ConfigureMappings,
        label: lookup_key("OptionsInput", "ConfigureMappings"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::TestInput,
        label: lookup_key("OptionsInput", "TestInput"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::ConfigurePads,
        label: lookup_key("OptionsInput", "ConfigurePads"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::InputOptions,
        label: lookup_key("OptionsInput", "InputOptions"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
];

pub(in crate::screens::options) const INPUT_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::InpConfigureMappings,
        name: lookup_key("OptionsInput", "ConfigureMappings"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "ConfigureMappingsHelp",
        ))],
    },
    Item {
        id: ItemId::InpTestInput,
        name: lookup_key("OptionsInput", "TestInput"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "TestInputHelp",
        ))],
    },
    Item {
        id: ItemId::InpConfigurePads,
        name: lookup_key("OptionsInput", "ConfigurePads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "ConfigurePadsHelp",
        ))],
    },
    Item {
        id: ItemId::InpInputOptions,
        name: lookup_key("OptionsInput", "InputOptions"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsInputHelp", "InputOptionsHelp")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "GamepadBackend")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "UseFSRs")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "SmxConfig")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "DebugFsrDump")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "MenuNavigation")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "OptionsNavigation")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "MenuButtons")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "Debounce")),
        ],
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

pub(in crate::screens::options) const INPUT_BACKEND_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::GamepadBackend,
        label: lookup_key("OptionsInput", "GamepadBackend"),
        choices: INPUT_BACKEND_CHOICES,
        inline: INPUT_BACKEND_INLINE,
    },
    SubRow {
        id: SubRowId::UseFsrs,
        label: lookup_key("OptionsInput", "UseFSRs"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SmxConfig,
        label: lookup_key("OptionsInput", "SmxConfig"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::DebugFsrDump,
        label: lookup_key("OptionsInput", "DebugFsrDump"),
        choices: &[localized_choice("Common", "Start")],
        inline: false,
    },
    SubRow {
        id: SubRowId::MenuNavigation,
        label: lookup_key("OptionsInput", "MenuNavigation"),
        choices: &[
            localized_choice("OptionsInput", "MenuNavigationFiveKey"),
            localized_choice("OptionsInput", "MenuNavigationThreeKey"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::OptionsNavigation,
        label: lookup_key("OptionsInput", "OptionsNavigation"),
        choices: &[
            localized_choice("OptionsInput", "OptionsNavigationStepMania"),
            localized_choice("OptionsInput", "OptionsNavigationArcade"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MenuButtons,
        label: lookup_key("OptionsInput", "MenuButtons"),
        choices: &[
            localized_choice("OptionsInput", "DedicatedMenuButtonsGameplay"),
            localized_choice("OptionsInput", "DedicatedMenuButtonsOnly"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::Debounce,
        label: lookup_key("OptionsInput", "Debounce"),
        choices: &[literal_choice("20ms")],
        inline: true,
    },
];

pub(in crate::screens::options) const INPUT_BACKEND_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::InpGamepadBackend,
        name: lookup_key("OptionsInput", "GamepadBackend"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "GamepadBackendHelp",
        ))],
    },
    Item {
        id: ItemId::InpUseFsrs,
        name: lookup_key("OptionsInput", "UseFSRs"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "UseFSRsHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxConfig,
        name: lookup_key("OptionsInput", "SmxConfig"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxConfigHelp",
        ))],
    },
    Item {
        id: ItemId::InpDebugFsrDump,
        name: lookup_key("OptionsInput", "DebugFsrDump"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "DebugFsrDumpHelp",
        ))],
    },
    Item {
        id: ItemId::InpMenuNavigation,
        name: lookup_key("OptionsInput", "MenuNavigation"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "MenuNavigationHelp",
        ))],
    },
    Item {
        id: ItemId::InpOptionsNavigation,
        name: lookup_key("OptionsInput", "OptionsNavigation"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "OptionsNavigationHelp",
        ))],
    },
    Item {
        id: ItemId::InpMenuButtons,
        name: lookup_key("OptionsInput", "MenuButtons"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "MenuButtonsHelp",
        ))],
    },
    Item {
        id: ItemId::InpDebounce,
        name: lookup_key("OptionsInput", "Debounce"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "DebounceHelp",
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

pub(in crate::screens::options) const SMX_CONFIG_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::SmxInput,
        label: lookup_key("OptionsInput", "SmxInput"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SmxManagesPadConfig,
        label: lookup_key("OptionsInput", "SmxManagesPadConfig"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SmxDefaultPadConfig,
        label: lookup_key("OptionsInput", "DefaultPadConfig"),
        choices: &[
            localized_choice("OptionsInput", "PresetLow"),
            localized_choice("OptionsInput", "PresetMedium"),
            localized_choice("OptionsInput", "PresetHigh"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SmxAssignPads,
        label: lookup_key("OptionsInput", "SmxAssignPads"),
        choices: &[localized_choice("Common", "Open")],
        inline: false,
    },
    SubRow {
        id: SubRowId::SmxSwapPads,
        label: lookup_key("OptionsInput", "SmxSwapPads"),
        choices: &[localized_choice("OptionsInput", "SmxSwapPadsAction")],
        inline: false,
    },
    SubRow {
        id: SubRowId::SmxUsbPolling,
        // 250-1000us in 50us steps; choice index N maps to 250 + N*50 us. The
        // "(NHz)" suffix is the rounded poll rate (1_000_000 / us); kept in lockstep
        // with the value table by `usb_polling_choice_labels_match_their_values`.
        label: lookup_key("OptionsInput", "UsbPolling"),
        choices: &[
            literal_choice("250us (4000Hz)"),
            literal_choice("300us (3333Hz)"),
            literal_choice("350us (2857Hz)"),
            literal_choice("400us (2500Hz)"),
            literal_choice("450us (2222Hz)"),
            literal_choice("500us (2000Hz)"),
            literal_choice("550us (1818Hz)"),
            literal_choice("600us (1667Hz)"),
            literal_choice("650us (1538Hz)"),
            literal_choice("700us (1429Hz)"),
            literal_choice("750us (1333Hz)"),
            literal_choice("800us (1250Hz)"),
            literal_choice("850us (1176Hz)"),
            literal_choice("900us (1111Hz)"),
            literal_choice("950us (1053Hz)"),
            literal_choice("1000us (1000Hz)"),
        ],
        inline: false,
    },
];

pub(in crate::screens::options) const SMX_CONFIG_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::InpSmxInput,
        name: lookup_key("OptionsInput", "SmxInput"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxInputHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxManagesPadConfig,
        name: lookup_key("OptionsInput", "SmxManagesPadConfig"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxManagesPadConfigHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxDefaultPadConfig,
        name: lookup_key("OptionsInput", "DefaultPadConfig"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "DefaultPadConfigHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxAssignPads,
        name: lookup_key("OptionsInput", "SmxAssignPads"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsInputHelp", "SmxAssignPadsHelp")),
            HelpEntry::Dynamic(smx_assignment_status),
        ],
    },
    Item {
        id: ItemId::InpSmxSwapPads,
        name: lookup_key("OptionsInput", "SmxSwapPads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxSwapPadsHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxUsbPolling,
        name: lookup_key("OptionsInput", "UsbPolling"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "UsbPollingHelp",
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
