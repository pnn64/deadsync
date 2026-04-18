use super::*;

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
    });
    b.push(Row {
        id: RowId::SpeedMod,
        name: lookup_key("PlayerOptions", "SpeedMod"),
        choices: vec![speed_mod_value_str], // Display only the current value
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "SpeedModHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Mini,
        name: lookup_key("PlayerOptions", "Mini"),
        choices: (-100..=150).map(|v| format!("{v}%")).collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "MiniHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Perspective,
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
    });
    b.push(Row {
        id: RowId::NoteSkin,
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
    });
    b.push(Row {
        id: RowId::MineSkin,
        name: lookup_key("PlayerOptions", "MineSkin"),
        choices: build_noteskin_override_choices(noteskin_names),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "MineSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ReceptorSkin,
        name: lookup_key("PlayerOptions", "ReceptorSkin"),
        choices: build_noteskin_override_choices(noteskin_names),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ReceptorSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::TapExplosionSkin,
        name: lookup_key("PlayerOptions", "TapExplosionSkin"),
        choices: build_tap_explosion_noteskin_choices(noteskin_names),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "TapExplosionSkinHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::JudgmentFont,
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
    });
    b.push(Row {
        id: RowId::JudgmentOffsetX,
        name: lookup_key("PlayerOptions", "JudgmentOffsetX"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "JudgmentOffsetXHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::JudgmentOffsetY,
        name: lookup_key("PlayerOptions", "JudgmentOffsetY"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "JudgmentOffsetYHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ComboFont,
        name: lookup_key("PlayerOptions", "ComboFont"),
        choices: vec![
            tr("PlayerOptions", "ComboFontWendy").to_string(),
            tr("PlayerOptions", "ComboFontArialRounded").to_string(),
            tr("PlayerOptions", "ComboFontAsap").to_string(),
            tr("PlayerOptions", "ComboFontBebasNeue").to_string(),
            tr("PlayerOptions", "ComboFontSourceCode").to_string(),
            tr("PlayerOptions", "ComboFontWork").to_string(),
            tr("PlayerOptions", "ComboFontWendyCursed").to_string(),
            tr("PlayerOptions", "ComboFontNone").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ComboFontHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ComboOffsetX,
        name: lookup_key("PlayerOptions", "ComboOffsetX"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ComboOffsetXHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ComboOffsetY,
        name: lookup_key("PlayerOptions", "ComboOffsetY"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "ComboOffsetYHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::HoldJudgment,
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
    });
    b.push(Row {
        id: RowId::BackgroundFilter,
        name: lookup_key("PlayerOptions", "BackgroundFilter"),
        choices: vec![
            tr("PlayerOptions", "BackgroundFilterOff").to_string(),
            tr("PlayerOptions", "BackgroundFilterDark").to_string(),
            tr("PlayerOptions", "BackgroundFilterDarker").to_string(),
            tr("PlayerOptions", "BackgroundFilterDarkest").to_string(),
        ],
        selected_choice_index: [3; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "BackgroundFilterHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::NoteFieldOffsetX,
        name: lookup_key("PlayerOptions", "NoteFieldOffsetX"),
        choices: (0..=50).map(|v| v.to_string()).collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "NoteFieldOffsetXHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::NoteFieldOffsetY,
        name: lookup_key("PlayerOptions", "NoteFieldOffsetY"),
        choices: (-50..=50).map(|v| v.to_string()).collect(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "NoteFieldOffsetYHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::VisualDelay,
        name: lookup_key("PlayerOptions", "VisualDelay"),
        choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
        selected_choice_index: [100; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "VisualDelayHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::GlobalOffsetShift,
        name: lookup_key("PlayerOptions", "GlobalOffsetShift"),
        choices: (-100..=100).map(|v| format!("{v}ms")).collect(),
        selected_choice_index: [100; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "GlobalOffsetShiftHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::MusicRate,
        name: lookup_key("PlayerOptions", "MusicRate"),
        choices: vec![fmt_music_rate(session_music_rate.clamp(0.5, 3.0))],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "MusicRateHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Stepchart,
        name: lookup_key("PlayerOptions", "Stepchart"),
        choices: stepchart_choices,
        selected_choice_index: initial_stepchart_choice_index,
        help: tr("PlayerOptionsHelp", "StepchartHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
        choice_difficulty_indices: Some(stepchart_choice_indices),
    });
    b.push(Row {
        id: RowId::WhatComesNext,
        name: lookup_key("PlayerOptions", "WhatComesNext"),
        choices: what_comes_next_choices(OptionsPane::Main, return_screen),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: tr("PlayerOptionsHelp", "WhatComesNextHelp")
            .split("\\n")
            .map(|s| s.to_string())
            .collect(),
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
