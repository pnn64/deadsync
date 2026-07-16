use super::super::choice;
use super::super::row::index_binding;
use super::*;
use deadsync_chart::{STANDARD_DIFFICULTY_COUNT, STANDARD_DIFFICULTY_NAMES};
use deadsync_profile::compat as gp;
use deadsync_profile::{
    BackgroundFilter, ComboFont, HeldMissGraphic, HoldJudgmentGraphic, JudgmentGraphic,
    MINI_PERCENT_MAX, MINI_PERCENT_MIN, NOTE_FIELD_OFFSET_X_MAX, NOTE_FIELD_OFFSET_X_MIN,
    NOTE_FIELD_OFFSET_Y_MAX, NOTE_FIELD_OFFSET_Y_MIN, NoCmodAlternative, NoteSkin, Perspective,
    PlayerSide, TapExplosionMask, VISUAL_DELAY_MS_MAX, VISUAL_DELAY_MS_MIN,
};

// =============================== Bindings ===============================

const NO_CMOD_ALTERNATIVE: ChoiceBinding<usize> = index_binding!(
    NO_CMOD_ALTERNATIVE_VARIANTS,
    NoCmodAlternative::None,
    no_cmod_alternative,
    gp::update_no_cmod_alternative_for_side,
    false,
    Some(CycleInit {
        from_profile: |p| {
            NO_CMOD_ALTERNATIVE_VARIANTS
                .iter()
                .position(|&v| v == p.no_cmod_alternative)
                .unwrap_or(0)
        }
    })
);
const PERSPECTIVE: ChoiceBinding<usize> = index_binding!(
    PERSPECTIVE_VARIANTS,
    Perspective::Overhead,
    perspective,
    gp::update_perspective_for_side,
    false,
    Some(CycleInit {
        from_profile: |p| {
            PERSPECTIVE_VARIANTS
                .iter()
                .position(|&v| v == p.perspective)
                .unwrap_or(0)
        }
    })
);
const COMBO_FONT: ChoiceBinding<usize> = index_binding!(
    COMBO_FONT_VARIANTS,
    ComboFont::Wendy,
    combo_font,
    gp::update_combo_font_for_side,
    true,
    Some(CycleInit {
        from_profile: |p| {
            COMBO_FONT_VARIANTS
                .iter()
                .position(|&v| v == p.combo_font)
                .unwrap_or(0)
        }
    })
);
const BACKGROUND_FILTER: NumericBinding = NumericBinding {
    parse: parse_i32_percent,
    apply: |p, v| {
        p.background_filter = BackgroundFilter::from_i32(v);
        Outcome::persisted()
    },
    persist_for_side: gp::update_background_filter_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.background_filter.percent() as i32,
        format: |v| format!("{v}%"),
    }),
};

const JUDGMENT_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.judgment_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_judgment_offset_x_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.judgment_offset_x.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        format: |v| format!("{v}"),
    }),
};
const JUDGMENT_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.judgment_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_judgment_offset_y_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.judgment_offset_y.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        format: |v| format!("{v}"),
    }),
};
const COMBO_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.combo_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_combo_offset_x_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.combo_offset_x.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        format: |v| format!("{v}"),
    }),
};
const COMBO_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.combo_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_combo_offset_y_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.combo_offset_y.clamp(HUD_OFFSET_MIN, HUD_OFFSET_MAX),
        format: |v| format!("{v}"),
    }),
};
const NOTEFIELD_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.note_field_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_notefield_offset_x_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.note_field_offset_x.clamp(0, 50),
        format: |v| format!("{v}"),
    }),
};
const NOTEFIELD_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.note_field_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_notefield_offset_y_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.note_field_offset_y.clamp(-50, 50),
        format: |v| format!("{v}"),
    }),
};
const VISUAL_DELAY: NumericBinding = NumericBinding {
    parse: parse_i32_ms,
    apply: |p, v| {
        p.visual_delay_ms = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_visual_delay_ms_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.visual_delay_ms.clamp(-100, 100),
        format: |v| format!("{v}ms"),
    }),
};
const GLOBAL_OFFSET_SHIFT: NumericBinding = NumericBinding {
    parse: parse_i32_ms,
    apply: |p, v| {
        p.global_offset_shift_ms = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_global_offset_shift_ms_for_side,
    init: Some(NumericInit {
        from_profile: |p| p.global_offset_shift_ms.clamp(-100, 100),
        format: |v| format!("{v}ms"),
    }),
};
const SPACING: NumericBinding = NumericBinding {
    parse: parse_i32_percent,
    apply: |p, v| {
        p.spacing_percent = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_spacing_percent_for_side,
    init: Some(NumericInit {
        from_profile: |p| {
            p.spacing_percent
                .clamp(SPACING_PERCENT_MIN, SPACING_PERCENT_MAX)
        },
        format: |v| format!("{v}%"),
    }),
};

/// Shared boilerplate for a noteskin-style cycle row implemented via
/// `CustomBinding`: advance the choice index, look up the chosen string, then
/// hand off to a row-specific `apply` closure that knows how to turn that
/// string into the right Profile/State write.
fn apply_noteskin_delta(
    state: &mut State,
    player_idx: usize,
    row_id: RowId,
    delta: isize,
    wrap: NavWrap,
    apply: fn(&mut State, usize, &str, bool, PlayerSide),
) -> Outcome {
    let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap) else {
        return Outcome::NONE;
    };
    let choice = state
        .pane()
        .row_map
        .get(row_id)
        .and_then(|r| r.choices.get(new_index))
        .cloned()
        .unwrap_or_default();
    let (should_persist, side) = choice::persist_ctx(state, player_idx);
    apply(state, player_idx, &choice, should_persist, side);
    Outcome::persisted()
}

const NOTE_SKIN: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
            wrap,
            |state, player_idx, choice, should_persist, side| {
                let name = if choice.is_empty() {
                    NoteSkin::DEFAULT_NAME.to_string()
                } else {
                    choice.to_string()
                };
                let setting = NoteSkin::new(&name);
                state.player_profiles[player_idx].noteskin = setting.clone();
                if should_persist {
                    gp::update_noteskin_for_side(side, setting);
                }
                sync_noteskin_previews_for_player(
                    &mut state.noteskin,
                    &state.player_profiles[player_idx],
                    player_idx,
                    state.cols_per_player,
                );
            },
        )
    },
};

const HEART_RATE_MONITOR: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(choice_idx) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let device_id = state
            .heart_rate_choice_ids
            .get(choice_idx)
            .cloned()
            .unwrap_or(None);
        state.player_profiles[player_idx].set_heart_rate_device_id(device_id.clone());
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        if should_persist {
            gp::update_heart_rate_device_id_for_side(side, device_id);
        }
        Outcome::persisted()
    },
};
const MINE_SKIN: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
            wrap,
            |state, player_idx, choice, should_persist, side| {
                let match_label = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
                let setting = if choice == match_label.as_ref() {
                    None
                } else {
                    Some(NoteSkin::new(choice))
                };
                state.player_profiles[player_idx]
                    .mine_noteskin
                    .clone_from(&setting);
                if should_persist {
                    gp::update_mine_noteskin_for_side(side, setting);
                }
                sync_noteskin_previews_for_player(
                    &mut state.noteskin,
                    &state.player_profiles[player_idx],
                    player_idx,
                    state.cols_per_player,
                );
            },
        )
    },
};
const RECEPTOR_SKIN: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
            wrap,
            |state, player_idx, choice, should_persist, side| {
                let match_label = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
                let setting = if choice == match_label.as_ref() {
                    None
                } else {
                    Some(NoteSkin::new(choice))
                };
                state.player_profiles[player_idx]
                    .receptor_noteskin
                    .clone_from(&setting);
                if should_persist {
                    gp::update_receptor_noteskin_for_side(side, setting);
                }
                sync_noteskin_previews_for_player(
                    &mut state.noteskin,
                    &state.player_profiles[player_idx],
                    player_idx,
                    state.cols_per_player,
                );
            },
        )
    },
};
const TAP_EXPLOSION_SKIN: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let outcome = apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
            wrap,
            |state, player_idx, choice, should_persist, side| {
                let match_label = tr("PlayerOptions", MATCH_NOTESKIN_LABEL);
                let no_tap_label = tr("PlayerOptions", NO_TAP_EXPLOSION_LABEL);
                let setting = if choice == match_label.as_ref() {
                    None
                } else if choice == no_tap_label.as_ref() {
                    Some(NoteSkin::none_choice())
                } else {
                    Some(NoteSkin::new(choice))
                };
                state.player_profiles[player_idx]
                    .tap_explosion_noteskin
                    .clone_from(&setting);
                if should_persist {
                    gp::update_tap_explosion_noteskin_for_side(side, setting);
                }
                sync_noteskin_previews_for_player(
                    &mut state.noteskin,
                    &state.player_profiles[player_idx],
                    player_idx,
                    state.cols_per_player,
                );
            },
        );
        if outcome.persisted {
            Outcome::persisted_with_visibility()
        } else {
            outcome
        }
    },
};

const TAP_EXPLOSION_OPTION_BITS: &[u32] = &[
    TapExplosionMask::FANTASTIC.bits() as u32,
    TapExplosionMask::EXCELLENT.bits() as u32,
    TapExplosionMask::GREAT.bits() as u32,
    TapExplosionMask::DECENT.bits() as u32,
    TapExplosionMask::WAY_OFF.bits() as u32,
    TapExplosionMask::MISS.bits() as u32,
    TapExplosionMask::HELD.bits() as u32,
    TapExplosionMask::HOLDING.bits() as u32,
];

const TAP_EXPLOSION_OPTIONS: BitmaskBinding = BitmaskBinding::Generic {
    init: BitmaskInit {
        from_profile: |p| p.tap_explosion_active_mask.bits() as u32,
        get_active: |m| m.tap_explosion.bits() as u32,
        set_active: |m, b| {
            debug_assert_eq!(
                b & !(u8::MAX as u32),
                0,
                "TapExplosionMask init bits exceed storage width",
            );
            m.tap_explosion = TapExplosionMask::from_bits_retain(b as u8);
        },
        cursor: CursorInit::FirstActiveBit,
    },
    writeback: BitmaskWriteback {
        project: |_m, p, b| {
            p.tap_explosion_active_mask = TapExplosionMask::from_bits_truncate(b as u8);
        },
        persist_for_side: |s, p| {
            gp::update_tap_explosion_mask_for_side(s, p.tap_explosion_active_mask);
        },
        bit_mapping: BitMapping::Explicit(TAP_EXPLOSION_OPTION_BITS),
        sync_visibility: false,
    },
};

const MUSIC_RATE: CustomBinding = CustomBinding {
    apply: |state, _player_idx, row_id, delta, _wrap| {
        let increment = 0.01f32;
        state.music_rate += delta as f32 * increment;
        state.music_rate = (state.music_rate / increment).round() * increment;
        state.music_rate = state.music_rate.clamp(0.05, 3.00);
        let formatted = fmt_music_rate(state.music_rate);
        if let Some(row) = state.pane_mut().row_map.get_mut(row_id) {
            row.choices[0] = formatted;
            for slot in 0..PLAYER_SLOTS {
                row.selected_choice_index[slot] = 0;
            }
        }
        let music_rate = state.music_rate;
        gp::set_session_music_rate(music_rate);
        super::super::queue_audio(
            state,
            deadsync_theme::AudioRequest::SetMusicRate(music_rate),
        );
        Outcome::persisted()
    },
};

const SPEED_MOD: CustomBinding = CustomBinding {
    apply: |state, player_idx, _row_id, delta, _wrap| {
        let speed_mod = {
            let speed_mod = &mut state.speed_mod[player_idx];
            let (upper, increment) = match speed_mod.mod_type {
                SpeedModType::X => (20.0, 0.05),
                SpeedModType::C | SpeedModType::M => (2000.0, 5.0),
            };
            speed_mod.value += delta as f32 * increment;
            speed_mod.value = (speed_mod.value / increment).round() * increment;
            speed_mod.value = speed_mod.value.clamp(increment, upper);
            speed_mod.clone()
        };
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
        Outcome::persisted()
    },
};

const TYPE_OF_SPEED_MOD: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let new_type = SpeedModType::from_choice_index(new_index);
        let reference_bpm = reference_bpm_for_song(
            &state.song,
            resolve_p1_chart(&state.song, &state.chart_steps_index, state.play_style),
        );
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let speed_mod = {
            let converted = convert_speed_mod_to_type(
                &state.speed_mod[player_idx],
                new_type,
                reference_bpm,
                rate,
            );
            state.speed_mod[player_idx] = converted.clone();
            converted
        };
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
        Outcome::persisted()
    },
};

const MINI: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let Some(choice) = state
            .pane()
            .row_map
            .get(row_id)
            .and_then(|r| r.choices.get(new_index))
            .cloned()
        else {
            return Outcome::NONE;
        };
        let Ok(val) = choice.trim_end_matches('%').parse::<i32>() else {
            return Outcome::persisted();
        };
        state.player_profiles[player_idx].mini_percent = val;
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        if should_persist {
            gp::update_mini_percent_for_side(side, val);
        }
        Outcome::persisted()
    },
};

const JUDGMENT_FONT: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let setting = assets::judgment_texture_choices()
            .get(new_index)
            .map(|choice| JudgmentGraphic::new(choice.key.as_ref()))
            .unwrap_or_default();
        state.player_profiles[player_idx].judgment_graphic = setting;
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        if should_persist {
            gp::update_judgment_graphic_for_side(
                side,
                state.player_profiles[player_idx].judgment_graphic.clone(),
            );
        }
        Outcome::persisted_with_visibility()
    },
};

const HOLD_JUDGMENT: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let setting = assets::hold_judgment_texture_choices()
            .get(new_index)
            .map(|choice| HoldJudgmentGraphic::new(choice.key.as_ref()))
            .unwrap_or_default();
        state.player_profiles[player_idx].hold_judgment_graphic = setting;
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        if should_persist {
            gp::update_hold_judgment_graphic_for_side(
                side,
                state.player_profiles[player_idx]
                    .hold_judgment_graphic
                    .clone(),
            );
        }
        Outcome::persisted()
    },
};

const HELD_GRAPHIC: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let setting = assets::held_miss_texture_choices()
            .get(new_index)
            .map(|choice| HeldMissGraphic::new(choice.key.as_ref()))
            .unwrap_or_default();
        state.player_profiles[player_idx].held_miss_graphic = setting;
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        if should_persist {
            gp::update_held_miss_graphic_for_side(
                side,
                state.player_profiles[player_idx].held_miss_graphic.clone(),
            );
        }
        Outcome::persisted()
    },
};

const STEPCHART: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let difficulty_idx = {
            let Some(row) = state.pane().row_map.get(row_id) else {
                return Outcome::persisted();
            };
            let Some(diff_indices) = &row.choice_difficulty_indices else {
                return Outcome::persisted();
            };
            let Some(&idx) = diff_indices.get(new_index) else {
                return Outcome::persisted();
            };
            idx
        };
        state.chart_steps_index[player_idx] = difficulty_idx;
        if difficulty_idx < STANDARD_DIFFICULTY_COUNT {
            state.chart_difficulty_index[player_idx] = difficulty_idx;
        }
        Outcome::persisted()
    },
};

fn push_mini_row(b: &mut RowBuilder) {
    b.push(Row::custom(
        RowId::Mini,
        lookup_key("PlayerOptions", "Mini"),
        lookup_key("PlayerOptionsHelp", "MiniHelp"),
        MINI,
        (MINI_PERCENT_MIN..=MINI_PERCENT_MAX)
            .map(|v| format!("{v}%"))
            .collect(),
    ));
}

fn push_spacing_row(b: &mut RowBuilder) {
    b.push(Row::numeric(
        RowId::Spacing,
        lookup_key("PlayerOptions", "Spacing"),
        lookup_key("PlayerOptionsHelp", "SpacingHelp"),
        SPACING,
        (SPACING_PERCENT_MIN..=SPACING_PERCENT_MAX)
            .map(|v| format!("{v}%"))
            .collect(),
    ));
}

fn push_perspective_row(b: &mut RowBuilder) {
    b.push(Row::cycle(
        RowId::Perspective,
        lookup_key("PlayerOptions", "Perspective"),
        lookup_key("PlayerOptionsHelp", "PerspectiveHelp"),
        CycleBinding::Index(PERSPECTIVE),
        vec![
            tr("PlayerOptions", "PerspectiveOverhead").to_string(),
            tr("PlayerOptions", "PerspectiveHallway").to_string(),
            tr("PlayerOptions", "PerspectiveDistant").to_string(),
            tr("PlayerOptions", "PerspectiveIncoming").to_string(),
            tr("PlayerOptions", "PerspectiveSpace").to_string(),
        ],
    ));
}

fn push_noteskin_row(b: &mut RowBuilder, noteskin_names: &[String]) {
    b.push(Row::custom(
        RowId::NoteSkin,
        lookup_key("PlayerOptions", "NoteSkin"),
        lookup_key("PlayerOptionsHelp", "NoteSkinHelp"),
        NOTE_SKIN,
        if noteskin_names.is_empty() {
            vec![NoteSkin::DEFAULT_NAME.to_string()]
        } else {
            noteskin_names.to_vec()
        },
    ));
}

fn push_mineskin_row(b: &mut RowBuilder, noteskin_names: &[String]) {
    b.push(Row::custom(
        RowId::MineSkin,
        lookup_key("PlayerOptions", "MineSkin"),
        lookup_key("PlayerOptionsHelp", "MineSkinHelp"),
        MINE_SKIN,
        build_noteskin_override_choices(noteskin_names),
    ));
}

fn push_receptorskin_row(b: &mut RowBuilder, noteskin_names: &[String]) {
    b.push(Row::custom(
        RowId::ReceptorSkin,
        lookup_key("PlayerOptions", "ReceptorSkin"),
        lookup_key("PlayerOptionsHelp", "ReceptorSkinHelp"),
        RECEPTOR_SKIN,
        build_noteskin_override_choices(noteskin_names),
    ));
}

fn push_tap_explosion_skin_row(b: &mut RowBuilder, noteskin_names: &[String]) {
    b.push(Row::custom(
        RowId::TapExplosionSkin,
        lookup_key("PlayerOptions", "TapExplosionSkin"),
        lookup_key("PlayerOptionsHelp", "TapExplosionSkinHelp"),
        TAP_EXPLOSION_SKIN,
        build_tap_explosion_noteskin_choices(noteskin_names),
    ));
}

fn push_tap_explosion_options_row(b: &mut RowBuilder) {
    b.push(Row::bitmask(
        RowId::TapExplosionOptions,
        lookup_key("PlayerOptions", "TapExplosionOptions"),
        lookup_key("PlayerOptionsHelp", "TapExplosionOptionsHelp"),
        TAP_EXPLOSION_OPTIONS,
        vec![
            tr("PlayerOptions", "TapExplosionOptionsFantastics").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsExcellents").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsGreats").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsDecents").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsWayOffs").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsMisses").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsHelds").to_string(),
            tr("PlayerOptions", "TapExplosionOptionsHolding").to_string(),
        ],
    ));
}

fn push_judgment_font_row(b: &mut RowBuilder) {
    b.push(Row::custom(
        RowId::JudgmentFont,
        lookup_key("PlayerOptions", "JudgmentFont"),
        lookup_key("PlayerOptionsHelp", "JudgmentFontHelp"),
        JUDGMENT_FONT,
        assets::judgment_texture_choices()
            .iter()
            .map(|choice| choice.label.clone())
            .collect(),
    ));
}

fn push_judgment_offset_rows(b: &mut RowBuilder) {
    b.push(
        Row::numeric(
            RowId::JudgmentOffsetX,
            lookup_key("PlayerOptions", "JudgmentOffsetX"),
            lookup_key("PlayerOptionsHelp", "JudgmentOffsetXHelp"),
            JUDGMENT_OFFSET_X,
            hud_offset_choices(),
        )
        .with_initial_choice_index(HUD_OFFSET_ZERO_INDEX),
    );
    b.push(
        Row::numeric(
            RowId::JudgmentOffsetY,
            lookup_key("PlayerOptions", "JudgmentOffsetY"),
            lookup_key("PlayerOptionsHelp", "JudgmentOffsetYHelp"),
            JUDGMENT_OFFSET_Y,
            hud_offset_choices(),
        )
        .with_initial_choice_index(HUD_OFFSET_ZERO_INDEX),
    );
}

fn push_combo_font_row(b: &mut RowBuilder) {
    b.push(Row::cycle(
        RowId::ComboFont,
        lookup_key("PlayerOptions", "ComboFont"),
        lookup_key("PlayerOptionsHelp", "ComboFontHelp"),
        CycleBinding::Index(COMBO_FONT),
        vec![
            tr("PlayerOptions", "ComboFontWendy").to_string(),
            tr("PlayerOptions", "ComboFontArialRounded").to_string(),
            tr("PlayerOptions", "ComboFontAsap").to_string(),
            tr("PlayerOptions", "ComboFontBebasNeue").to_string(),
            tr("PlayerOptions", "ComboFontSourceCode").to_string(),
            tr("PlayerOptions", "ComboFontWork").to_string(),
            tr("PlayerOptions", "ComboFontWendyCursed").to_string(),
            tr("PlayerOptions", "ComboFontMega").to_string(),
            tr("PlayerOptions", "ComboFontNone").to_string(),
        ],
    ));
}

fn push_combo_offset_rows(b: &mut RowBuilder) {
    b.push(
        Row::numeric(
            RowId::ComboOffsetX,
            lookup_key("PlayerOptions", "ComboOffsetX"),
            lookup_key("PlayerOptionsHelp", "ComboOffsetXHelp"),
            COMBO_OFFSET_X,
            hud_offset_choices(),
        )
        .with_initial_choice_index(HUD_OFFSET_ZERO_INDEX),
    );
    b.push(
        Row::numeric(
            RowId::ComboOffsetY,
            lookup_key("PlayerOptions", "ComboOffsetY"),
            lookup_key("PlayerOptionsHelp", "ComboOffsetYHelp"),
            COMBO_OFFSET_Y,
            hud_offset_choices(),
        )
        .with_initial_choice_index(HUD_OFFSET_ZERO_INDEX),
    );
}

fn push_hold_judgment_row(b: &mut RowBuilder) {
    b.push(Row::custom(
        RowId::HoldJudgment,
        lookup_key("PlayerOptions", "HoldJudgment"),
        lookup_key("PlayerOptionsHelp", "HoldJudgmentHelp"),
        HOLD_JUDGMENT,
        assets::hold_judgment_texture_choices()
            .iter()
            .map(|choice| choice.label.clone())
            .collect(),
    ));
}

fn push_held_graphic_row(b: &mut RowBuilder) {
    b.push(Row::custom(
        RowId::HeldGraphic,
        lookup_key("PlayerOptions", "HeldGraphic"),
        lookup_key("PlayerOptionsHelp", "HeldGraphicHelp"),
        HELD_GRAPHIC,
        assets::held_miss_texture_choices()
            .iter()
            .map(|choice| choice.label.clone())
            .collect(),
    ));
}

fn push_background_filter_row(b: &mut RowBuilder) {
    b.push(
        Row::numeric(
            RowId::BackgroundFilter,
            lookup_key("PlayerOptions", "BackgroundFilter"),
            lookup_key("PlayerOptionsHelp", "BackgroundFilterHelp"),
            BACKGROUND_FILTER,
            (0..=BackgroundFilter::MAX_PERCENT)
                .map(|v| format!("{v}%"))
                .collect(),
        )
        .with_initial_choice_index(BackgroundFilter::DEFAULT.percent() as usize),
    );
}

const PAD_LIGHT_BRIGHTNESS: NumericBinding = NumericBinding {
    parse: parse_i32_percent,
    apply: |p, v| {
        p.set_pad_light_brightness(v.clamp(0, 100) as u8);
        Outcome::persisted()
    },
    persist_for_side: gp::update_pad_light_brightness_for_side,
    init: Some(NumericInit {
        from_profile: |p| i32::from(p.pad_light_brightness),
        format: |v| format!("{v}%"),
    }),
};

fn push_pad_light_brightness_row(b: &mut RowBuilder) {
    // Always built; shown only when deadsync drives the SMX pad LEDs (see
    // `show_pad_light_brightness` in visibility.rs). Mirrors GlobalOffsetShift.
    b.push(
        Row::numeric(
            RowId::PadLightBrightness,
            lookup_key("PlayerOptions", "PadLightBrightness"),
            lookup_key("PlayerOptionsHelp", "PadLightBrightnessHelp"),
            PAD_LIGHT_BRIGHTNESS,
            (0..=100).map(|v| format!("{v}%")).collect(),
        )
        .with_initial_choice_index(deadsync_profile::PAD_LIGHT_BRIGHTNESS_DEFAULT as usize),
    );
}

fn push_notefield_offset_rows(b: &mut RowBuilder) {
    b.push(Row::numeric(
        RowId::NoteFieldOffsetX,
        lookup_key("PlayerOptions", "NoteFieldOffsetX"),
        lookup_key("PlayerOptionsHelp", "NoteFieldOffsetXHelp"),
        NOTEFIELD_OFFSET_X,
        (NOTE_FIELD_OFFSET_X_MIN..=NOTE_FIELD_OFFSET_X_MAX)
            .map(|v| v.to_string())
            .collect(),
    ));
    b.push(Row::numeric(
        RowId::NoteFieldOffsetY,
        lookup_key("PlayerOptions", "NoteFieldOffsetY"),
        lookup_key("PlayerOptionsHelp", "NoteFieldOffsetYHelp"),
        NOTEFIELD_OFFSET_Y,
        (NOTE_FIELD_OFFSET_Y_MIN..=NOTE_FIELD_OFFSET_Y_MAX)
            .map(|v| v.to_string())
            .collect(),
    ));
}

pub(super) fn build_smx_pack_choices(pack_names: &[String]) -> Vec<String> {
    let mut choices = Vec::with_capacity(pack_names.len() + 1);
    choices.push(tr("Common", "Default").to_string());
    choices.extend(pack_names.iter().cloned());
    choices
}

const SMX_BG_PACK: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let pack = if new_index == 0 {
            String::new()
        } else {
            state
                .pane()
                .row_map
                .get(row_id)
                .and_then(|r| r.choices.get(new_index))
                .cloned()
                .unwrap_or_default()
        };
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        state.player_profiles[player_idx].smx_bg_pack = if pack.is_empty() {
            None
        } else {
            Some(pack.clone())
        };
        if should_persist {
            gp::update_smx_bg_pack_for_side(side, &pack);
        }
        Outcome::persisted()
    },
};

const SMX_JUDGE_PACK: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) = choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let pack = if new_index == 0 {
            String::new()
        } else {
            state
                .pane()
                .row_map
                .get(row_id)
                .and_then(|r| r.choices.get(new_index))
                .cloned()
                .unwrap_or_default()
        };
        let (should_persist, side) = choice::persist_ctx(state, player_idx);
        state.player_profiles[player_idx].smx_judge_pack = if pack.is_empty() {
            None
        } else {
            Some(pack.clone())
        };
        if should_persist {
            gp::update_smx_judge_pack_for_side(side, &pack);
        }
        Outcome::persisted()
    },
};

fn push_smx_bg_pack_row(b: &mut RowBuilder, pack_names: &[String]) {
    b.push(Row::custom(
        RowId::SmxBgPack,
        lookup_key("PlayerOptions", "SmxBgPack"),
        lookup_key("PlayerOptionsHelp", "SmxBgPackHelp"),
        SMX_BG_PACK,
        build_smx_pack_choices(pack_names),
    ));
}

fn push_smx_judge_pack_row(b: &mut RowBuilder, pack_names: &[String]) {
    b.push(Row::custom(
        RowId::SmxJudgePack,
        lookup_key("PlayerOptions", "SmxJudgePack"),
        lookup_key("PlayerOptionsHelp", "SmxJudgePackHelp"),
        SMX_JUDGE_PACK,
        build_smx_pack_choices(pack_names),
    ));
}

pub(super) fn push_display_modifier_rows(
    b: &mut RowBuilder,
    noteskin_names: &[String],
    smx_bg_pack_names: &[String],
    smx_judge_pack_names: &[String],
) {
    push_mini_row(b);
    push_spacing_row(b);
    push_perspective_row(b);
    push_noteskin_row(b, noteskin_names);
    push_mineskin_row(b, noteskin_names);
    push_receptorskin_row(b, noteskin_names);
    push_tap_explosion_skin_row(b, noteskin_names);
    push_tap_explosion_options_row(b);
    push_judgment_font_row(b);
    push_judgment_offset_rows(b);
    push_combo_font_row(b);
    push_combo_offset_rows(b);
    push_hold_judgment_row(b);
    push_held_graphic_row(b);
    push_background_filter_row(b);
    push_pad_light_brightness_row(b);
    push_smx_bg_pack_row(b, smx_bg_pack_names);
    push_smx_judge_pack_row(b, smx_judge_pack_names);
    push_notefield_offset_rows(b);
}

pub(super) fn build_main_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    session_music_rate: f32,
    noteskin_names: &[String],
    heart_rate_choices: &[String],
    return_screen: Screen,
    fixed_stepchart: Option<&FixedStepchart>,
    play_style: profile_data::PlayStyle,
    persisted_player_idx: usize,
) -> RowMap {
    let speed_mod_value_str = speed_mod.display();
    let (stepchart_choices, stepchart_choice_indices, initial_stepchart_choice_index) =
        if let Some(fixed) = fixed_stepchart {
            let fixed_steps_idx = chart_steps_index[persisted_player_idx];
            (
                vec![fixed.label.clone()],
                vec![fixed_steps_idx],
                [0; PLAYER_SLOTS],
            )
        } else {
            // Build Stepchart choices from the song's charts for the current play style, ordered
            // Beginner..Challenge, then Edit charts.
            let target_chart_type = play_style.chart_type();
            let mut stepchart_choices: Vec<String> = Vec::with_capacity(STANDARD_DIFFICULTY_COUNT);
            let mut stepchart_choice_indices: Vec<usize> =
                Vec::with_capacity(STANDARD_DIFFICULTY_COUNT);
            for (i, file_name) in STANDARD_DIFFICULTY_NAMES.iter().enumerate() {
                if let Some(chart) = song.charts.iter().find(|c| {
                    c.chart_type.eq_ignore_ascii_case(target_chart_type)
                        && c.difficulty.eq_ignore_ascii_case(file_name)
                }) {
                    let display_name = difficulty_display_name(i);
                    stepchart_choices.push(format!("{} {}", display_name, chart.meter));
                    stepchart_choice_indices.push(i);
                }
            }
            for (i, chart) in song
                .edit_charts_sorted(target_chart_type)
                .into_iter()
                .enumerate()
            {
                let desc = chart.description.trim();
                if desc.is_empty() {
                    stepchart_choices.push(
                        tr_fmt(
                            "PlayerOptions",
                            "EditChartMeter",
                            &[("meter", &chart.meter.to_string())],
                        )
                        .to_string(),
                    );
                } else {
                    stepchart_choices.push(
                        tr_fmt(
                            "PlayerOptions",
                            "EditChartDescMeter",
                            &[("desc", desc), ("meter", &chart.meter.to_string())],
                        )
                        .to_string(),
                    );
                }
                stepchart_choice_indices.push(STANDARD_DIFFICULTY_COUNT + i);
            }
            // Fallback if none found (defensive; SelectMusic filters songs by play style).
            if stepchart_choices.is_empty() {
                stepchart_choices.push(tr("PlayerOptions", "CurrentStepchartLabel").to_string());
                let base_pref = preferred_difficulty_index[persisted_player_idx]
                    .min(STANDARD_DIFFICULTY_COUNT.saturating_sub(1));
                stepchart_choice_indices.push(base_pref);
            }
            let initial_stepchart_choice_index: [usize; PLAYER_SLOTS] =
                std::array::from_fn(|player_idx| {
                    let steps_idx = chart_steps_index[player_idx];
                    let pref_idx = preferred_difficulty_index[player_idx]
                        .min(STANDARD_DIFFICULTY_COUNT.saturating_sub(1));
                    stepchart_choice_indices
                        .iter()
                        .position(|&idx| idx == steps_idx)
                        .or_else(|| {
                            stepchart_choice_indices
                                .iter()
                                .position(|&idx| idx == pref_idx)
                        })
                        .unwrap_or(0)
                });
            (
                stepchart_choices,
                stepchart_choice_indices,
                initial_stepchart_choice_index,
            )
        };
    let mut b = RowBuilder::new();
    b.push(
        Row::custom(
            RowId::TypeOfSpeedMod,
            lookup_key("PlayerOptions", "TypeOfSpeedMod"),
            lookup_key("PlayerOptionsHelp", "TypeOfSpeedModHelp"),
            TYPE_OF_SPEED_MOD,
            vec![
                tr("PlayerOptions", "SpeedModTypeX").to_string(),
                tr("PlayerOptions", "SpeedModTypeC").to_string(),
                tr("PlayerOptions", "SpeedModTypeM").to_string(),
            ],
        )
        .with_initial_choice_index(speed_mod.mod_type.choice_index()),
    );
    b.push(Row::custom(
        RowId::SpeedMod,
        lookup_key("PlayerOptions", "SpeedMod"),
        lookup_key("PlayerOptionsHelp", "SpeedModHelp"),
        SPEED_MOD,
        vec![speed_mod_value_str], // Display only the current value
    ));
    b.push(Row::cycle(
        RowId::NoCmodAlternative,
        lookup_key("PlayerOptions", "NoCmodAlternative"),
        lookup_key("PlayerOptionsHelp", "NoCmodAlternativeHelp"),
        CycleBinding::Index(NO_CMOD_ALTERNATIVE),
        vec![
            tr("PlayerOptions", "NoCmodAlternativeNone").to_string(),
            tr("PlayerOptions", "NoCmodAlternativeXMod").to_string(),
            tr("PlayerOptions", "NoCmodAlternativeMMod").to_string(),
        ],
    ));
    push_mini_row(&mut b);
    push_perspective_row(&mut b);
    push_noteskin_row(&mut b, noteskin_names);
    push_judgment_font_row(&mut b);
    push_combo_font_row(&mut b);
    push_hold_judgment_row(&mut b);
    push_held_graphic_row(&mut b);
    push_background_filter_row(&mut b);
    b.push(
        Row::numeric(
            RowId::VisualDelay,
            lookup_key("PlayerOptions", "VisualDelay"),
            lookup_key("PlayerOptionsHelp", "VisualDelayHelp"),
            VISUAL_DELAY,
            (VISUAL_DELAY_MS_MIN..=VISUAL_DELAY_MS_MAX)
                .map(|v| format!("{v}ms"))
                .collect(),
        )
        .with_initial_choice_index((-VISUAL_DELAY_MS_MIN) as usize),
    );
    b.push(
        Row::numeric(
            RowId::GlobalOffsetShift,
            lookup_key("PlayerOptions", "GlobalOffsetShift"),
            lookup_key("PlayerOptionsHelp", "GlobalOffsetShiftHelp"),
            GLOBAL_OFFSET_SHIFT,
            (VISUAL_DELAY_MS_MIN..=VISUAL_DELAY_MS_MAX)
                .map(|v| format!("{v}ms"))
                .collect(),
        )
        .with_initial_choice_index((-VISUAL_DELAY_MS_MIN) as usize),
    );
    b.push(Row::custom(
        RowId::MusicRate,
        lookup_key("PlayerOptions", "MusicRate"),
        lookup_key("PlayerOptionsHelp", "MusicRateHelp"),
        MUSIC_RATE,
        vec![fmt_music_rate(session_music_rate.clamp(0.5, 3.0))],
    ));
    b.push(
        Row::custom(
            RowId::Stepchart,
            lookup_key("PlayerOptions", "Stepchart"),
            lookup_key("PlayerOptionsHelp", "StepchartHelp"),
            STEPCHART,
            stepchart_choices,
        )
        .with_initial_choice_indices(initial_stepchart_choice_index)
        .with_choice_difficulty_indices(stepchart_choice_indices),
    );
    if !heart_rate_choices.is_empty() {
        b.push(Row::custom(
            RowId::HeartRateMonitor,
            lookup_key("PlayerOptions", "HeartRateMonitor"),
            lookup_key("PlayerOptionsHelp", "HeartRateMonitorHelp"),
            HEART_RATE_MONITOR,
            heart_rate_choices.to_vec(),
        ));
    }
    b.push(
        Row::custom(
            RowId::WhatComesNext,
            lookup_key("PlayerOptions", "WhatComesNext"),
            lookup_key("PlayerOptionsHelp", "WhatComesNextHelp"),
            super::WHAT_COMES_NEXT,
            what_comes_next_choices(OptionsPane::Main, return_screen),
        )
        .with_mirror_across_players(),
    );
    b.push(Row::exit());
    b.finish()
}
