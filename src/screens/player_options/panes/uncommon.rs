use super::*;
use super::super::row::index_binding;
use crate::game::profile as gp;

const ATTACKS: ChoiceBinding<usize> = index_binding!(
    ATTACK_MODE_VARIANTS,
    gp::AttackMode::On,
    attack_mode,
    gp::update_attack_mode_for_side,
    false
);
const HIDE_LIGHT_TYPE: ChoiceBinding<usize> = index_binding!(
    HIDE_LIGHT_TYPE_VARIANTS,
    gp::HideLightType::NoHideLights,
    hide_light_type,
    gp::update_hide_light_type_for_side,
    false
);

const INSERT: BitmaskBinding = BitmaskBinding { toggle: super::super::choice::toggle_insert_row };
const REMOVE: BitmaskBinding = BitmaskBinding { toggle: super::super::choice::toggle_remove_row };
const HOLDS: BitmaskBinding = BitmaskBinding { toggle: super::super::choice::toggle_holds_row };
const ACCEL: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_accel_effects_row,
};
const EFFECT: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_visual_effects_row,
};
const APPEARANCE: BitmaskBinding = BitmaskBinding {
    toggle: super::super::choice::toggle_appearance_effects_row,
};

pub(super) fn build_uncommon_rows(return_screen: Screen) -> RowMap {
    let mut b = RowBuilder::new();
    b.push(Row {
        id: RowId::Insert,
        behavior: RowBehavior::Bitmask(INSERT),
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
        behavior: RowBehavior::Bitmask(REMOVE),
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
        behavior: RowBehavior::Bitmask(HOLDS),
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
        behavior: RowBehavior::Bitmask(ACCEL),
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
        behavior: RowBehavior::Bitmask(EFFECT),
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
        behavior: RowBehavior::Bitmask(APPEARANCE),
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
        behavior: RowBehavior::Cycle(CycleBinding::Index(ATTACKS)),
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
        behavior: RowBehavior::Cycle(CycleBinding::Index(HIDE_LIGHT_TYPE)),
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
        behavior: RowBehavior::Action(ActionRow::WhatComesNext),
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
        behavior: RowBehavior::Action(ActionRow::Exit),
        name: lookup_key("Common", "Exit"),
        choices: vec![tr("Common", "Exit").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![String::new()],
        choice_difficulty_indices: None,
    });
    b.finish()
}
