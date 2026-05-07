use super::super::row::{index_binding, simple_bitmask_binding};
use super::*;
use crate::game::profile as gp;
use crate::game::profile::{
    AccelEffectsMask, AppearanceEffectsMask, HoldsMask, InsertMask, RemoveMask, VisualEffectsMask,
};

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

const INSERT: BitmaskBinding = simple_bitmask_binding!(
    mask = InsertMask,
    bits = u8,
    state_field = insert,
    profile_field = insert_active_mask,
    persist = gp::update_insert_mask_for_side,
    width = 7,
);
const REMOVE: BitmaskBinding = simple_bitmask_binding!(
    mask = RemoveMask,
    bits = u8,
    state_field = remove,
    profile_field = remove_active_mask,
    persist = gp::update_remove_mask_for_side,
    width = 8,
);
const HOLDS: BitmaskBinding = simple_bitmask_binding!(
    mask = HoldsMask,
    bits = u8,
    state_field = holds,
    profile_field = holds_active_mask,
    persist = gp::update_holds_mask_for_side,
    width = 5,
);
const ACCEL: BitmaskBinding = simple_bitmask_binding!(
    mask = AccelEffectsMask,
    bits = u8,
    state_field = accel_effects,
    profile_field = accel_effects_active_mask,
    persist = gp::update_accel_effects_mask_for_side,
    width = 5,
);
const EFFECT: BitmaskBinding = simple_bitmask_binding!(
    mask = VisualEffectsMask,
    bits = u16,
    state_field = visual_effects,
    profile_field = visual_effects_active_mask,
    persist = gp::update_visual_effects_mask_for_side,
    width = 10,
);
const APPEARANCE: BitmaskBinding = simple_bitmask_binding!(
    mask = AppearanceEffectsMask,
    bits = u8,
    state_field = appearance_effects,
    profile_field = appearance_effects_active_mask,
    persist = gp::update_appearance_effects_mask_for_side,
    width = 5,
);

pub(super) fn build_uncommon_rows(return_screen: Screen) -> RowMap {
    let mut b = RowBuilder::new();
    b.push(Row::bitmask(
        RowId::Insert,
        lookup_key("PlayerOptions", "Insert"),
        lookup_key("PlayerOptionsHelp", "InsertHelp"),
        INSERT,
        vec![
            tr("PlayerOptions", "InsertWide").to_string(),
            tr("PlayerOptions", "InsertBig").to_string(),
            tr("PlayerOptions", "InsertQuick").to_string(),
            tr("PlayerOptions", "InsertBMRize").to_string(),
            tr("PlayerOptions", "InsertSkippy").to_string(),
            tr("PlayerOptions", "InsertEcho").to_string(),
            tr("PlayerOptions", "InsertStomp").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::Remove,
        lookup_key("PlayerOptions", "Remove"),
        lookup_key("PlayerOptionsHelp", "RemoveHelp"),
        REMOVE,
        vec![
            tr("PlayerOptions", "RemoveLittle").to_string(),
            tr("PlayerOptions", "RemoveNoMines").to_string(),
            tr("PlayerOptions", "RemoveNoHolds").to_string(),
            tr("PlayerOptions", "RemoveNoJumps").to_string(),
            tr("PlayerOptions", "RemoveNoHands").to_string(),
            tr("PlayerOptions", "RemoveNoQuads").to_string(),
            tr("PlayerOptions", "RemoveNoLifts").to_string(),
            tr("PlayerOptions", "RemoveNoFakes").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::Holds,
        lookup_key("PlayerOptions", "Holds"),
        lookup_key("PlayerOptionsHelp", "HoldsHelp"),
        HOLDS,
        vec![
            tr("PlayerOptions", "HoldsPlanted").to_string(),
            tr("PlayerOptions", "HoldsFloored").to_string(),
            tr("PlayerOptions", "HoldsTwister").to_string(),
            tr("PlayerOptions", "HoldsNoRolls").to_string(),
            tr("PlayerOptions", "HoldsToRolls").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::Accel,
        lookup_key("PlayerOptions", "Accel"),
        lookup_key("PlayerOptionsHelp", "AccelHelp"),
        ACCEL,
        vec![
            tr("PlayerOptions", "AccelBoost").to_string(),
            tr("PlayerOptions", "AccelBrake").to_string(),
            tr("PlayerOptions", "AccelWave").to_string(),
            tr("PlayerOptions", "AccelExpand").to_string(),
            tr("PlayerOptions", "AccelBoomerang").to_string(),
        ],
    ));
    b.push(Row::bitmask(
        RowId::Effect,
        lookup_key("PlayerOptions", "Effect"),
        lookup_key("PlayerOptionsHelp", "EffectHelp"),
        EFFECT,
        vec![
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
    ));
    b.push(Row::bitmask(
        RowId::Appearance,
        lookup_key("PlayerOptions", "Appearance"),
        lookup_key("PlayerOptionsHelp", "AppearanceHelp"),
        APPEARANCE,
        vec![
            tr("PlayerOptions", "AppearanceHidden").to_string(),
            tr("PlayerOptions", "AppearanceSudden").to_string(),
            tr("PlayerOptions", "AppearanceStealth").to_string(),
            tr("PlayerOptions", "AppearanceBlink").to_string(),
            tr("PlayerOptions", "AppearanceRVanish").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::Attacks,
        lookup_key("PlayerOptions", "Attacks"),
        lookup_key("PlayerOptionsHelp", "AttacksHelp"),
        CycleBinding::Index(ATTACKS),
        vec![
            tr("PlayerOptions", "AttacksOn").to_string(),
            tr("PlayerOptions", "AttacksRandomAttacks").to_string(),
            tr("PlayerOptions", "AttacksOff").to_string(),
        ],
    ));
    b.push(Row::cycle(
        RowId::HideLightType,
        lookup_key("PlayerOptions", "HideLightType"),
        lookup_key("PlayerOptionsHelp", "HideLightTypeHelp"),
        CycleBinding::Index(HIDE_LIGHT_TYPE),
        vec![
            tr("PlayerOptions", "HideLightTypeNoHideLights").to_string(),
            tr("PlayerOptions", "HideLightTypeHideAllLights").to_string(),
            tr("PlayerOptions", "HideLightTypeHideMarqueeLights").to_string(),
            tr("PlayerOptions", "HideLightTypeHideBassLights").to_string(),
        ],
    ));
    // `WhatComesNext` here uses two distinct lookup keys for its two help
    // lines (not a single `\n`-split key), so the standard `help: LookupKey`
    // constructor parameter cannot express it. Keep the help vec as a
    // struct-update literal.
    b.push(Row {
        help: vec![
            tr("PlayerOptionsHelp", "WhatComesNextHelp1").to_string(),
            tr("PlayerOptionsHelp", "WhatComesNextHelp2").to_string(),
        ],
        ..Row::custom(
            RowId::WhatComesNext,
            lookup_key("PlayerOptions", "WhatComesNext"),
            lookup_key("PlayerOptionsHelp", "WhatComesNextHelp1"),
            super::WHAT_COMES_NEXT,
            what_comes_next_choices(OptionsPane::Uncommon, return_screen),
        )
        .with_mirror_across_players()
    });
    b.push(Row::exit());
    b.finish()
}
