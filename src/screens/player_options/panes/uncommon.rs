use super::*;

pub(super) fn build_uncommon_rows(return_screen: Screen) -> RowMap {
    let mut b = RowBuilder::new();
    b.push(Row {
        id: RowId::Insert,
        name: lookup_key("PlayerOptions", "Insert"),
        choices: vec![
            tr("PlayerOptions", "InsertWide").to_string(),
            tr("PlayerOptions", "InsertBig").to_string(),
            tr("PlayerOptions", "InsertQuick").to_string(),
            tr("PlayerOptions", "InsertBMRize").to_string(),
            tr("PlayerOptions", "InsertSkippy").to_string(),
            tr("PlayerOptions", "InsertEcho").to_string(),
            tr("PlayerOptions", "InsertStomp").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "InsertHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Remove,
        name: lookup_key("PlayerOptions", "Remove"),
        choices: vec![
            tr("PlayerOptions", "RemoveLittle").to_string(),
            tr("PlayerOptions", "RemoveNoMines").to_string(),
            tr("PlayerOptions", "RemoveNoHolds").to_string(),
            tr("PlayerOptions", "RemoveNoJumps").to_string(),
            tr("PlayerOptions", "RemoveNoHands").to_string(),
            tr("PlayerOptions", "RemoveNoQuads").to_string(),
            tr("PlayerOptions", "RemoveNoLifts").to_string(),
            tr("PlayerOptions", "RemoveNoFakes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "RemoveHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Holds,
        name: lookup_key("PlayerOptions", "Holds"),
        choices: vec![
            tr("PlayerOptions", "HoldsPlanted").to_string(),
            tr("PlayerOptions", "HoldsFloored").to_string(),
            tr("PlayerOptions", "HoldsTwister").to_string(),
            tr("PlayerOptions", "HoldsNoRolls").to_string(),
            tr("PlayerOptions", "HoldsToRolls").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "HoldsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Accel,
        name: lookup_key("PlayerOptions", "Accel"),
        choices: vec![
            tr("PlayerOptions", "AccelBoost").to_string(),
            tr("PlayerOptions", "AccelBrake").to_string(),
            tr("PlayerOptions", "AccelWave").to_string(),
            tr("PlayerOptions", "AccelExpand").to_string(),
            tr("PlayerOptions", "AccelBoomerang").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "AccelHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Effect,
        name: lookup_key("PlayerOptions", "Effect"),
        choices: vec![
            tr("PlayerOptions", "EffectDrunk").to_string(),
            tr("PlayerOptions", "EffectDizzy").to_string(),
            tr("PlayerOptions", "EffectConfusion").to_string(),
            tr("PlayerOptions", "EffectBig").to_string(),
            tr("PlayerOptions", "EffectFlip").to_string(),
            tr("PlayerOptions", "EffectInvert").to_string(),
            tr("PlayerOptions", "EffectTornado").to_string(),
            tr("PlayerOptions", "EffectTipsy").to_string(),
            tr("PlayerOptions", "EffectBumpy").to_string(),
            tr("PlayerOptions", "EffectBeat").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "EffectHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Appearance,
        name: lookup_key("PlayerOptions", "Appearance"),
        choices: vec![
            tr("PlayerOptions", "AppearanceHidden").to_string(),
            tr("PlayerOptions", "AppearanceSudden").to_string(),
            tr("PlayerOptions", "AppearanceStealth").to_string(),
            tr("PlayerOptions", "AppearanceBlink").to_string(),
            tr("PlayerOptions", "AppearanceRVanish").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "AppearanceHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Attacks,
        name: lookup_key("PlayerOptions", "Attacks"),
        choices: vec![
            tr("PlayerOptions", "AttacksOn").to_string(),
            tr("PlayerOptions", "AttacksRandomAttacks").to_string(),
            tr("PlayerOptions", "AttacksOff").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "AttacksHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::HideLightType,
        name: lookup_key("PlayerOptions", "HideLightType"),
        choices: vec![
            tr("PlayerOptions", "HideLightTypeNoHideLights").to_string(),
            tr("PlayerOptions", "HideLightTypeHideAllLights").to_string(),
            tr("PlayerOptions", "HideLightTypeHideMarqueeLights").to_string(),
            tr("PlayerOptions", "HideLightTypeHideBassLights").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "HideLightTypeHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::WhatComesNext,
        name: lookup_key("PlayerOptions", "WhatComesNext"),
        choices: what_comes_next_choices(OptionsPane::Uncommon, return_screen),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![
            tr("PlayerOptionsHelp", "WhatComesNextHelp1").to_string(),
            tr("PlayerOptionsHelp", "WhatComesNextHelp2").to_string(),
        ],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Exit,
        name: lookup_key("Common", "Exit"),
        choices: vec![tr("Common", "Exit").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![String::new()],
        choice_difficulty_indices: None,
    });
    b.finish()
}
