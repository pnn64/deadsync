use super::super::*;

/// Live help text for the Assign Pads row: which pad is currently P1 (blue) vs
/// P2 (red) by slot, plus a same-jumper warning when the pads are ambiguous and
/// not yet assigned.
pub(in crate::screens::options) fn smx_assignment_status(
    view: &deadsync_theme::views::SmxAssignmentView,
) -> String {
    use crate::assets::i18n::{tr, tr_fmt};
    let label = |slot: usize| -> String {
        let pad = &view.pads[slot];
        if pad.connected && !pad.label.is_empty() {
            pad.label.clone()
        } else {
            tr("OptionsInput", "SmxAssignStatusNone").to_string()
        }
    };
    let mut s = tr_fmt(
        "OptionsInput",
        "SmxAssignStatusLine",
        &[("p1", &label(0)), ("p2", &label(1))],
    )
    .to_string();
    if view.conflict_warning {
        s.push_str("\n\n");
        s.push_str(&tr("OptionsInput", "SmxAssignStatusConflict"));
    }
    s
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
            HelpEntry::Bullet(lookup_key("OptionsInput", "SmxBgPack")),
            HelpEntry::Bullet(lookup_key("OptionsInput", "SmxJudgePack")),
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
        id: SubRowId::SmxPanelLights,
        label: lookup_key("OptionsInput", "SmxPanelLights"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::SmxUnderglowTheme,
        label: lookup_key("OptionsInput", "SmxUnderglowTheme"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        // Shown only while Theme Underglow is on (see options visibility). The
        // options page holds the strips on a red test colour, so a wrong order
        // is immediately visible (red shows green on GRB strips).
        id: SubRowId::SmxUnderglowGrb,
        label: lookup_key("OptionsInput", "SmxUnderglowGrb"),
        choices: &[literal_choice("RGB"), literal_choice("GRB")],
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
        // Shown only when a single pad is connected (the assign/swap rows replace
        // it when two are connected). Picks which player that lone pad is.
        id: SubRowId::SmxSinglePadPlayer,
        label: lookup_key("OptionsInput", "SmxSinglePadPlayer"),
        choices: &[
            localized_choice("OptionsInput", "SmxSinglePadPlayerP1"),
            localized_choice("OptionsInput", "SmxSinglePadPlayerP2"),
        ],
        inline: true,
    },
    SubRow {
        // Numeric placeholder: the "100%" text is replaced live by the current
        // machine-default brightness in the options layout (like the volume rows).
        id: SubRowId::SmxDefaultLightBrightness,
        label: lookup_key("OptionsInput", "SmxDefaultLightBrightness"),
        choices: &[literal_choice("100%")],
        inline: false,
    },
    SubRow {
        // Placeholder choice overridden dynamically from smx_bg_pack_choices.
        id: SubRowId::SmxBgPack,
        label: lookup_key("OptionsInput", "SmxBgPack"),
        choices: &[localized_choice("Common", "Default")],
        inline: true,
    },
    SubRow {
        // Placeholder choice overridden dynamically from smx_judge_pack_choices.
        id: SubRowId::SmxJudgePack,
        label: lookup_key("OptionsInput", "SmxJudgePack"),
        choices: &[localized_choice("Common", "Default")],
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
        id: ItemId::InpSmxPanelLights,
        name: lookup_key("OptionsInput", "SmxPanelLights"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxPanelLightsHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxUnderglowTheme,
        name: lookup_key("OptionsInput", "SmxUnderglowTheme"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxUnderglowThemeHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxUnderglowGrb,
        name: lookup_key("OptionsInput", "SmxUnderglowGrb"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxUnderglowGrbHelp",
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
        id: ItemId::InpSmxSinglePadPlayer,
        name: lookup_key("OptionsInput", "SmxSinglePadPlayer"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsInputHelp", "SmxSinglePadPlayerHelp")),
            HelpEntry::SmxAssignmentStatus,
        ],
    },
    Item {
        id: ItemId::InpSmxDefaultLightBrightness,
        name: lookup_key("OptionsInput", "SmxDefaultLightBrightness"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxDefaultLightBrightnessHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxBgPack,
        name: lookup_key("OptionsInput", "SmxBgPack"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxBgPackHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxJudgePack,
        name: lookup_key("OptionsInput", "SmxJudgePack"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsInputHelp",
            "SmxJudgePackHelp",
        ))],
    },
    Item {
        id: ItemId::InpSmxAssignPads,
        name: lookup_key("OptionsInput", "SmxAssignPads"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsInputHelp", "SmxAssignPadsHelp")),
            HelpEntry::SmxAssignmentStatus,
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
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];
