use super::super::row::index_binding;
use super::super::row::{BitMapping, BitmaskInit, BitmaskWriteback, CursorInit};
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

const INSERT: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.insert_active_mask.bits() as u32,
        get_active: |m| m.insert.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "InsertMask init bits exceed u8 width"
            );
            m.insert = InsertMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project_to_profile: |p, b| {
            p.insert_active_mask = InsertMask::from_bits_truncate(b as u8);
        },
        persist_for_side: |s, b| {
            gp::update_insert_mask_for_side(s, InsertMask::from_bits_truncate(b as u8));
        },
        bit_mapping: BitMapping::Sequential { width: 7 },
    },
};
const REMOVE: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.remove_active_mask.bits() as u32,
        get_active: |m| m.remove.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "RemoveMask init bits exceed u8 width"
            );
            m.remove = RemoveMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project_to_profile: |p, b| {
            p.remove_active_mask = RemoveMask::from_bits_truncate(b as u8);
        },
        persist_for_side: |s, b| {
            gp::update_remove_mask_for_side(s, RemoveMask::from_bits_truncate(b as u8));
        },
        bit_mapping: BitMapping::Sequential { width: 8 },
    },
};
const HOLDS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.holds_active_mask.bits() as u32,
        get_active: |m| m.holds.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "HoldsMask init bits exceed u8 width"
            );
            m.holds = HoldsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project_to_profile: |p, b| {
            p.holds_active_mask = HoldsMask::from_bits_truncate(b as u8);
        },
        persist_for_side: |s, b| {
            gp::update_holds_mask_for_side(s, HoldsMask::from_bits_truncate(b as u8));
        },
        bit_mapping: BitMapping::Sequential { width: 5 },
    },
};
const ACCEL: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.accel_effects_active_mask.bits() as u32,
        get_active: |m| m.accel_effects.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "AccelEffectsMask init bits exceed u8 width",
            );
            m.accel_effects = AccelEffectsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project_to_profile: |p, b| {
            p.accel_effects_active_mask = AccelEffectsMask::from_bits_truncate(b as u8);
        },
        persist_for_side: |s, b| {
            gp::update_accel_effects_mask_for_side(
                s,
                AccelEffectsMask::from_bits_truncate(b as u8),
            );
        },
        bit_mapping: BitMapping::Sequential { width: 5 },
    },
};
const EFFECT: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.visual_effects_active_mask.bits() as u32,
        get_active: |m| m.visual_effects.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u16::MAX as u32),
                0,
                "VisualEffectsMask init bits exceed u16 width",
            );
            m.visual_effects = VisualEffectsMask::from_bits_retain(b as u16);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project_to_profile: |p, b| {
            p.visual_effects_active_mask = VisualEffectsMask::from_bits_truncate(b as u16);
        },
        persist_for_side: |s, b| {
            gp::update_visual_effects_mask_for_side(
                s,
                VisualEffectsMask::from_bits_truncate(b as u16),
            );
        },
        bit_mapping: BitMapping::Sequential { width: 10 },
    },
};
const APPEARANCE: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.appearance_effects_active_mask.bits() as u32,
        get_active: |m| m.appearance_effects.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "AppearanceEffectsMask init bits exceed u8 width",
            );
            m.appearance_effects = AppearanceEffectsMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project_to_profile: |p, b| {
            p.appearance_effects_active_mask = AppearanceEffectsMask::from_bits_truncate(b as u8);
        },
        persist_for_side: |s, b| {
            gp::update_appearance_effects_mask_for_side(
                s,
                AppearanceEffectsMask::from_bits_truncate(b as u8),
            );
        },
        bit_mapping: BitMapping::Sequential { width: 5 },
    },
};

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
