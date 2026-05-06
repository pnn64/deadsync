use super::super::choice;
use super::super::row::index_binding;
use super::*;
use crate::game::profile as gp;

// =============================== Bindings ===============================

const PERSPECTIVE: ChoiceBinding<usize> = index_binding!(
    PERSPECTIVE_VARIANTS,
    gp::Perspective::Overhead,
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
    gp::ComboFont::Wendy,
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
        p.background_filter = gp::BackgroundFilter::from_i32(v);
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
    apply: fn(&mut State, usize, &str, bool, gp::PlayerSide),
) -> Outcome {
    let Some(new_index) =
        choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
    else {
        return Outcome::NONE;
    };
    let choice = state
        .pane()
        .row_map
        .get(row_id)
        .and_then(|r| r.choices.get(new_index))
        .cloned()
        .unwrap_or_default();
    let (should_persist, side) = choice::persist_ctx(player_idx);
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
                    gp::NoteSkin::DEFAULT_NAME.to_string()
                } else {
                    choice.to_string()
                };
                let setting = gp::NoteSkin::new(&name);
                state.player_profiles[player_idx].noteskin = setting.clone();
                if should_persist {
                    gp::update_noteskin_for_side(side, setting);
                }
                sync_noteskin_previews_for_player(
                    &mut state.noteskin,
                    &state.player_profiles[player_idx],
                    player_idx,
                );
            },
        )
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
                    Some(gp::NoteSkin::new(choice))
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
                    Some(gp::NoteSkin::new(choice))
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
                );
            },
        )
    },
};
const TAP_EXPLOSION_SKIN: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        apply_noteskin_delta(
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
                    Some(gp::NoteSkin::none_choice())
                } else {
                    Some(gp::NoteSkin::new(choice))
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
                );
            },
        )
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
        gp::set_session_music_rate(state.music_rate);
        crate::engine::audio::set_music_rate(state.music_rate);
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
        let Some(new_index) =
            choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let new_type = SpeedModType::from_choice_index(new_index);
        let reference_bpm = reference_bpm_for_song(
            &state.song,
            resolve_p1_chart(&state.song, &state.chart_steps_index),
        );
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let speed_mod = {
            let speed_mod = &mut state.speed_mod[player_idx];
            let old_type = speed_mod.mod_type;
            let old_value = speed_mod.value;
            let target_bpm: f32 = match old_type {
                SpeedModType::C | SpeedModType::M => old_value,
                SpeedModType::X => (reference_bpm * rate * old_value).round(),
            };
            let new_value = match new_type {
                SpeedModType::X => {
                    let denom = reference_bpm * rate;
                    let raw = if denom.is_finite() && denom > 0.0 {
                        target_bpm / denom
                    } else {
                        1.0
                    };
                    round_to_step(raw, 0.05).clamp(0.05, 20.0)
                }
                SpeedModType::C | SpeedModType::M => {
                    round_to_step(target_bpm, 5.0).clamp(5.0, 2000.0)
                }
            };
            speed_mod.mod_type = new_type;
            speed_mod.value = new_value;
            speed_mod.clone()
        };
        sync_profile_scroll_speed(&mut state.player_profiles[player_idx], &speed_mod);
        Outcome::persisted()
    },
};

const MINI: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) =
            choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        let (should_persist, side) = choice::persist_ctx(player_idx);
        if should_persist {
            gp::update_mini_percent_for_side(side, val);
        }
        Outcome::persisted()
    },
};

const JUDGMENT_FONT: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) =
            choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let setting = assets::judgment_texture_choices()
            .get(new_index)
            .map(|choice| gp::JudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].judgment_graphic = setting;
        let (should_persist, side) = choice::persist_ctx(player_idx);
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
        let Some(new_index) =
            choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
        else {
            return Outcome::NONE;
        };
        let setting = assets::hold_judgment_texture_choices()
            .get(new_index)
            .map(|choice| gp::HoldJudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].hold_judgment_graphic = setting;
        let (should_persist, side) = choice::persist_ctx(player_idx);
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

const STEPCHART: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta, wrap| {
        let Some(new_index) =
            choice::cycle_choice_index(state, player_idx, row_id, delta, wrap)
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
        if difficulty_idx < crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() {
            state.chart_difficulty_index[player_idx] = difficulty_idx;
        }
        Outcome::persisted()
    },
};

pub(super) fn build_main_rows(
    song: &SongData,
    speed_mod: &SpeedMod,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    session_music_rate: f32,
    noteskin_names: &[String],
    return_screen: Screen,
    fixed_stepchart: Option<&FixedStepchart>,
) -> RowMap {
    let speed_mod_value_str = speed_mod.display();
    let (stepchart_choices, stepchart_choice_indices, initial_stepchart_choice_index) =
        if let Some(fixed) = fixed_stepchart {
            let fixed_steps_idx = chart_steps_index[session_persisted_player_idx()];
            (
                vec![fixed.label.clone()],
                vec![fixed_steps_idx],
                [0; PLAYER_SLOTS],
            )
        } else {
            // Build Stepchart choices from the song's charts for the current play style, ordered
            // Beginner..Challenge, then Edit charts.
            let target_chart_type = crate::game::profile::get_session_play_style().chart_type();
            let mut stepchart_choices: Vec<String> = Vec::with_capacity(5);
            let mut stepchart_choice_indices: Vec<usize> = Vec::with_capacity(5);
            for (i, file_name) in crate::engine::present::color::FILE_DIFFICULTY_NAMES
                .iter()
                .enumerate()
            {
                if let Some(chart) = song.charts.iter().find(|c| {
                    c.chart_type.eq_ignore_ascii_case(target_chart_type)
                        && c.difficulty.eq_ignore_ascii_case(file_name)
                }) {
                    let display_name = difficulty_display_name(i);
                    stepchart_choices.push(format!("{} {}", display_name, chart.meter));
                    stepchart_choice_indices.push(i);
                }
            }
            for (i, chart) in
                crate::screens::select_music::edit_charts_sorted(song, target_chart_type)
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
                stepchart_choice_indices
                    .push(crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() + i);
            }
            // Fallback if none found (defensive; SelectMusic filters songs by play style).
            if stepchart_choices.is_empty() {
                stepchart_choices.push(tr("PlayerOptions", "CurrentStepchartLabel").to_string());
                let base_pref = preferred_difficulty_index[session_persisted_player_idx()].min(
                    crate::engine::present::color::FILE_DIFFICULTY_NAMES
                        .len()
                        .saturating_sub(1),
                );
                stepchart_choice_indices.push(base_pref);
            }
            let initial_stepchart_choice_index: [usize; PLAYER_SLOTS] =
                std::array::from_fn(|player_idx| {
                    let steps_idx = chart_steps_index[player_idx];
                    let pref_idx = preferred_difficulty_index[player_idx].min(
                        crate::engine::present::color::FILE_DIFFICULTY_NAMES
                            .len()
                            .saturating_sub(1),
                    );
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
    b.push(Row::custom(
        RowId::Mini,
        lookup_key("PlayerOptions", "Mini"),
        lookup_key("PlayerOptionsHelp", "MiniHelp"),
        MINI,
        (gp::MINI_PERCENT_MIN..=gp::MINI_PERCENT_MAX)
            .map(|v| format!("{v}%"))
            .collect(),
    ));
    b.push(Row::numeric(
        RowId::Spacing,
        lookup_key("PlayerOptions", "Spacing"),
        lookup_key("PlayerOptionsHelp", "SpacingHelp"),
        SPACING,
        (SPACING_PERCENT_MIN..=SPACING_PERCENT_MAX)
            .map(|v| format!("{v}%"))
            .collect(),
    ));
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
    b.push(Row::custom(
        RowId::NoteSkin,
        lookup_key("PlayerOptions", "NoteSkin"),
        lookup_key("PlayerOptionsHelp", "NoteSkinHelp"),
        NOTE_SKIN,
        if noteskin_names.is_empty() {
            vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()]
        } else {
            noteskin_names.to_vec()
        },
    ));
    b.push(Row::custom(
        RowId::MineSkin,
        lookup_key("PlayerOptions", "MineSkin"),
        lookup_key("PlayerOptionsHelp", "MineSkinHelp"),
        MINE_SKIN,
        build_noteskin_override_choices(noteskin_names),
    ));
    b.push(Row::custom(
        RowId::ReceptorSkin,
        lookup_key("PlayerOptions", "ReceptorSkin"),
        lookup_key("PlayerOptionsHelp", "ReceptorSkinHelp"),
        RECEPTOR_SKIN,
        build_noteskin_override_choices(noteskin_names),
    ));
    b.push(Row::custom(
        RowId::TapExplosionSkin,
        lookup_key("PlayerOptions", "TapExplosionSkin"),
        lookup_key("PlayerOptionsHelp", "TapExplosionSkinHelp"),
        TAP_EXPLOSION_SKIN,
        build_tap_explosion_noteskin_choices(noteskin_names),
    ));
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
    b.push(
        Row::numeric(
            RowId::BackgroundFilter,
            lookup_key("PlayerOptions", "BackgroundFilter"),
            lookup_key("PlayerOptionsHelp", "BackgroundFilterHelp"),
            BACKGROUND_FILTER,
            (0..=gp::BackgroundFilter::MAX_PERCENT)
                .map(|v| format!("{v}%"))
                .collect(),
        )
        .with_initial_choice_index(gp::BackgroundFilter::DEFAULT.percent() as usize),
    );
    b.push(Row::numeric(
        RowId::NoteFieldOffsetX,
        lookup_key("PlayerOptions", "NoteFieldOffsetX"),
        lookup_key("PlayerOptionsHelp", "NoteFieldOffsetXHelp"),
        NOTEFIELD_OFFSET_X,
        (gp::NOTE_FIELD_OFFSET_X_MIN..=gp::NOTE_FIELD_OFFSET_X_MAX)
            .map(|v| v.to_string())
            .collect(),
    ));
    b.push(Row::numeric(
        RowId::NoteFieldOffsetY,
        lookup_key("PlayerOptions", "NoteFieldOffsetY"),
        lookup_key("PlayerOptionsHelp", "NoteFieldOffsetYHelp"),
        NOTEFIELD_OFFSET_Y,
        (gp::NOTE_FIELD_OFFSET_Y_MIN..=gp::NOTE_FIELD_OFFSET_Y_MAX)
            .map(|v| v.to_string())
            .collect(),
    ));
    b.push(
        Row::numeric(
            RowId::VisualDelay,
            lookup_key("PlayerOptions", "VisualDelay"),
            lookup_key("PlayerOptionsHelp", "VisualDelayHelp"),
            VISUAL_DELAY,
            (gp::VISUAL_DELAY_MS_MIN..=gp::VISUAL_DELAY_MS_MAX)
                .map(|v| format!("{v}ms"))
                .collect(),
        )
        .with_initial_choice_index((-gp::VISUAL_DELAY_MS_MIN) as usize),
    );
    b.push(
        Row::numeric(
            RowId::GlobalOffsetShift,
            lookup_key("PlayerOptions", "GlobalOffsetShift"),
            lookup_key("PlayerOptionsHelp", "GlobalOffsetShiftHelp"),
            GLOBAL_OFFSET_SHIFT,
            (gp::VISUAL_DELAY_MS_MIN..=gp::VISUAL_DELAY_MS_MAX)
                .map(|v| format!("{v}ms"))
                .collect(),
        )
        .with_initial_choice_index((-gp::VISUAL_DELAY_MS_MIN) as usize),
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
