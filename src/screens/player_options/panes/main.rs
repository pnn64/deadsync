use super::super::row::index_binding;
use super::*;
use crate::game::profile as gp;

// =============================== Bindings ===============================

const PERSPECTIVE: ChoiceBinding<usize> = index_binding!(
    PERSPECTIVE_VARIANTS,
    gp::Perspective::Overhead,
    perspective,
    gp::update_perspective_for_side,
    false
);
const COMBO_FONT: ChoiceBinding<usize> = index_binding!(
    COMBO_FONT_VARIANTS,
    gp::ComboFont::Wendy,
    combo_font,
    gp::update_combo_font_for_side,
    true
);
const BACKGROUND_FILTER: NumericBinding = NumericBinding {
    parse: parse_i32_percent,
    apply: |p, v| {
        p.background_filter = gp::BackgroundFilter::from_i32(v);
        Outcome::persisted()
    },
    persist_for_side: gp::update_background_filter_for_side,
};

const JUDGMENT_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.judgment_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_judgment_offset_x_for_side,
};
const JUDGMENT_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.judgment_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_judgment_offset_y_for_side,
};
const COMBO_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.combo_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_combo_offset_x_for_side,
};
const COMBO_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.combo_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_combo_offset_y_for_side,
};
const NOTEFIELD_OFFSET_X: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.note_field_offset_x = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_notefield_offset_x_for_side,
};
const NOTEFIELD_OFFSET_Y: NumericBinding = NumericBinding {
    parse: parse_i32,
    apply: |p, v| {
        p.note_field_offset_y = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_notefield_offset_y_for_side,
};
const VISUAL_DELAY: NumericBinding = NumericBinding {
    parse: parse_i32_ms,
    apply: |p, v| {
        p.visual_delay_ms = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_visual_delay_ms_for_side,
};
const GLOBAL_OFFSET_SHIFT: NumericBinding = NumericBinding {
    parse: parse_i32_ms,
    apply: |p, v| {
        p.global_offset_shift_ms = v;
        Outcome::persisted()
    },
    persist_for_side: gp::update_global_offset_shift_ms_for_side,
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
    apply: fn(&mut State, usize, &str, bool, gp::PlayerSide),
) -> Outcome {
    let Some(new_index) =
        super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
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
    let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
    apply(state, player_idx, &choice, should_persist, side);
    Outcome::persisted()
}

const NOTE_SKIN: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
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
    apply: |state, player_idx, row_id, delta| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
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
    apply: |state, player_idx, row_id, delta| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
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
    apply: |state, player_idx, row_id, delta| {
        apply_noteskin_delta(
            state,
            player_idx,
            row_id,
            delta,
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
    apply: |state, _player_idx, row_id, delta| {
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
    apply: |state, player_idx, _row_id, delta| {
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
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
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
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
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
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
        if should_persist {
            gp::update_mini_percent_for_side(side, val);
        }
        Outcome::persisted()
    },
};

const JUDGMENT_FONT: CustomBinding = CustomBinding {
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
        else {
            return Outcome::NONE;
        };
        let setting = assets::judgment_texture_choices()
            .get(new_index)
            .map(|choice| gp::JudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].judgment_graphic = setting;
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
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
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
        else {
            return Outcome::NONE;
        };
        let setting = assets::hold_judgment_texture_choices()
            .get(new_index)
            .map(|choice| gp::HoldJudgmentGraphic::new(&choice.key))
            .unwrap_or_default();
        state.player_profiles[player_idx].hold_judgment_graphic = setting;
        let (should_persist, side) = super::super::choice::persist_ctx(player_idx);
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
    apply: |state, player_idx, row_id, delta| {
        let Some(new_index) =
            super::super::choice::cycle_choice_index(state, player_idx, row_id, delta)
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
    b.push(Row {
        id: RowId::TypeOfSpeedMod,
        behavior: RowBehavior::Custom(TYPE_OF_SPEED_MOD),
        name: lookup_key("PlayerOptions", "TypeOfSpeedMod"),
        choices: vec![
            tr("PlayerOptions", "SpeedModTypeX").to_string(),
            tr("PlayerOptions", "SpeedModTypeC").to_string(),
            tr("PlayerOptions", "SpeedModTypeM").to_string(),
        ],
        selected_choice_index: [speed_mod.mod_type.choice_index(); PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "TypeOfSpeedModHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::SpeedMod,
        behavior: RowBehavior::Custom(SPEED_MOD),
        name: lookup_key("PlayerOptions", "SpeedMod"),
        choices: vec![speed_mod_value_str], // Display only the current value
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "SpeedModHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::Mini,
        behavior: RowBehavior::Custom(MINI),
        name: lookup_key("PlayerOptions", "Mini"),
        choices: (-100..=150).map(|v| format!("{v}%")).collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "MiniHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::Perspective,
        behavior: RowBehavior::Cycle(CycleBinding::Index(PERSPECTIVE)),
        name: lookup_key("PlayerOptions", "Perspective"),
        choices: vec![
            tr("PlayerOptions", "PerspectiveOverhead").to_string(),
            tr("PlayerOptions", "PerspectiveHallway").to_string(),
            tr("PlayerOptions", "PerspectiveDistant").to_string(),
            tr("PlayerOptions", "PerspectiveIncoming").to_string(),
            tr("PlayerOptions", "PerspectiveSpace").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "PerspectiveHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::NoteSkin,
        behavior: RowBehavior::Custom(NOTE_SKIN),
        name: lookup_key("PlayerOptions", "NoteSkin"),
        choices: if noteskin_names.is_empty() {
            vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()]
        } else {
            noteskin_names.to_vec()
        },
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "NoteSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MineSkin,
        behavior: RowBehavior::Custom(MINE_SKIN),
        name: lookup_key("PlayerOptions", "MineSkin"),
        choices: build_noteskin_override_choices(noteskin_names),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "MineSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ReceptorSkin,
        behavior: RowBehavior::Custom(RECEPTOR_SKIN),
        name: lookup_key("PlayerOptions", "ReceptorSkin"),
        choices: build_noteskin_override_choices(noteskin_names),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ReceptorSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::TapExplosionSkin,
        behavior: RowBehavior::Custom(TAP_EXPLOSION_SKIN),
        name: lookup_key("PlayerOptions", "TapExplosionSkin"),
        choices: build_tap_explosion_noteskin_choices(noteskin_names),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "TapExplosionSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::JudgmentFont,
        behavior: RowBehavior::Custom(JUDGMENT_FONT),
        name: lookup_key("PlayerOptions", "JudgmentFont"),
        choices: assets::judgment_texture_choices()
            .iter()
            .map(|choice| choice.label.clone())
            .collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "JudgmentFontHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::JudgmentOffsetX,
        behavior: RowBehavior::Numeric(JUDGMENT_OFFSET_X),
        name: lookup_key("PlayerOptions", "JudgmentOffsetX"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "JudgmentOffsetXHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::JudgmentOffsetY,
        behavior: RowBehavior::Numeric(JUDGMENT_OFFSET_Y),
        name: lookup_key("PlayerOptions", "JudgmentOffsetY"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "JudgmentOffsetYHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ComboFont,
        behavior: RowBehavior::Cycle(CycleBinding::Index(COMBO_FONT)),
        name: lookup_key("PlayerOptions", "ComboFont"),
        choices: vec![
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
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ComboFontHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ComboOffsetX,
        behavior: RowBehavior::Numeric(COMBO_OFFSET_X),
        name: lookup_key("PlayerOptions", "ComboOffsetX"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ComboOffsetXHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::ComboOffsetY,
        behavior: RowBehavior::Numeric(COMBO_OFFSET_Y),
        name: lookup_key("PlayerOptions", "ComboOffsetY"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ComboOffsetYHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::HoldJudgment,
        behavior: RowBehavior::Custom(HOLD_JUDGMENT),
        name: lookup_key("PlayerOptions", "HoldJudgment"),
        choices: assets::hold_judgment_texture_choices()
            .iter()
            .map(|choice| choice.label.clone())
            .collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "HoldJudgmentHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::BackgroundFilter,
        behavior: RowBehavior::Numeric(BACKGROUND_FILTER),
        name: lookup_key("PlayerOptions", "BackgroundFilter"),
        choices: (0..=gp::BackgroundFilter::MAX_PERCENT)
            .map(|v| format!("{v}%"))
            .collect(),
        selected_choice_index: [gp::BackgroundFilter::DEFAULT.percent() as usize; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "BackgroundFilterHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::NoteFieldOffsetX,
        behavior: RowBehavior::Numeric(NOTEFIELD_OFFSET_X),
        name: lookup_key("PlayerOptions", "NoteFieldOffsetX"),
        choices: (0..=50).map(|v| v.to_string()).collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "NoteFieldOffsetXHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::NoteFieldOffsetY,
        behavior: RowBehavior::Numeric(NOTEFIELD_OFFSET_Y),
        name: lookup_key("PlayerOptions", "NoteFieldOffsetY"),
        choices: (-50..=50).map(|v| v.to_string()).collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "NoteFieldOffsetYHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::VisualDelay,
        behavior: RowBehavior::Numeric(VISUAL_DELAY),
        name: lookup_key("PlayerOptions", "VisualDelay"),
        choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
        selected_choice_index: [100; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "VisualDelayHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::GlobalOffsetShift,
        behavior: RowBehavior::Numeric(GLOBAL_OFFSET_SHIFT),
        name: lookup_key("PlayerOptions", "GlobalOffsetShift"),
        choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
        selected_choice_index: [100; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "GlobalOffsetShiftHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::MusicRate,
        behavior: RowBehavior::Custom(MUSIC_RATE),
        name: lookup_key("PlayerOptions", "MusicRate"),
        choices: vec![fmt_music_rate(session_music_rate.clamp(0.5, 3.0))],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "MusicRateHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::Stepchart,
        behavior: RowBehavior::Custom(STEPCHART),
        name: lookup_key("PlayerOptions", "Stepchart"),
        choices: stepchart_choices,
        selected_choice_index: initial_stepchart_choice_index,
        help: tr("PlayerOptionsHelp", "StepchartHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: Some(stepchart_choice_indices),
        mirror_across_players: false,
    });
    b.push(Row {
        id: RowId::WhatComesNext,
        behavior: RowBehavior::Custom(super::WHAT_COMES_NEXT),
        name: lookup_key("PlayerOptions", "WhatComesNext"),
        choices: what_comes_next_choices(OptionsPane::Main, return_screen),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "WhatComesNextHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
        mirror_across_players: true,
    });
    b.push(Row {
        id: RowId::Exit,
        behavior: RowBehavior::Exit,
        name: lookup_key("Common", "Exit"),
        choices: vec![tr("Common", "Exit").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![String::new()],
        choice_difficulty_indices: None,
        mirror_across_players: false,
    });
    b.finish()
}
