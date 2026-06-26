mod actor_builder;
mod display_mods;
mod error_bar;
mod feedback;
mod holds;
mod measure_actors;
mod measure_lines;
mod mini_indicator;
mod notes;
mod placement;
mod receptors;
mod style;
mod transforms;

pub use actor_builder::*;
pub use display_mods::*;
pub use error_bar::*;
pub use feedback::*;
pub use holds::*;
pub use measure_actors::*;
pub use measure_lines::*;
pub use mini_indicator::*;
pub use notes::*;
pub use placement::*;
pub use receptors::*;
pub use style::COLUMN_CUE_Y_OFFSET;
pub use transforms::*;

#[cfg(test)]
use display_mods::{
    append_average_error_bar_part, append_mini_part, append_perspective_parts, append_turn_parts,
    disabled_timing_windows_name, join_display_mod_parts, push_transform_parts,
};
#[cfg(test)]
use holds::hold_head_part_for_roll;
#[cfg(test)]
use mini_indicator::rgba8;
#[cfg(test)]
mod tests {
    use deadlib_present::actors::{Actor, SizeSpec, SpriteSource, TextAlign};
    use deadlib_render::BlendMode;
    use deadsync_noteskin::NoteAnimPart;
    use std::sync::Arc;

    use super::{
        AccelYParams, BuiltNotefield, COLUMN_CUE_Y_OFFSET, DISPLAY_TURN_MIRROR,
        DISPLAY_TURN_RANDOM, DISPLAY_TURN_UD_MIRROR, GameplayModsAttackMode,
        GameplayModsTextParams, HudLayoutOffsets, HudLayoutParams, JudgmentTiltParams,
        LayoutMiniIndicatorPosition, MiniIndicatorColorStyle, MiniIndicatorMode,
        MiniIndicatorProgress, MiniIndicatorScoreType, MiniIndicatorSize,
        MiniIndicatorSubtractiveDisplay, NoteAlphaParams, NoteXParams, TapJudgmentRowsParams,
        TapReplacementHead, TornadoBounds, VisualEffectParams, ZmodComboColorParams,
        ZmodComboColorStyle, ZmodLayoutParams, ZmodMeasureCounterText, ZmodMiniIndicatorOutput,
        ZmodMiniIndicatorParams, ZmodMiniIndicatorText, actor_with_world_z, appearance_needs_rows,
        appearance_note_actor_alpha, appearance_note_alpha, appearance_note_glow,
        append_average_error_bar_part, append_beat_bar, append_cue_bar, append_edit_measure_number,
        append_mini_part, append_perspective_parts, append_turn_parts, apply_accel_y,
        apply_accel_y_with_peak, average_error_bar_mini_scale, beat_factor, beat_scroll_travel,
        beat_x_extra, bottom_cap_uv_window, bumpy_angle, clamp_rounded_i16,
        clipped_hold_body_bounds, column_cue_alpha, column_cue_height, column_cue_reverse_top_y,
        column_flash_alpha, column_flash_alpha_at, column_flash_color, column_flash_height,
        column_flash_layout, column_flash_reverse_top_y, combo_actor_zoom,
        compute_invert_distances, compute_tornado_bounds, crossover_cue_height, default_column_x,
        disabled_timing_windows_name, drunk_x_extra, edit_bar_candidate_step_rows,
        edit_bar_scroll_speed, edit_beat_bar_info_for_row, edit_beat_scroll_travel,
        effective_mini_value, error_bar_boundaries_s, error_bar_color_for_window,
        error_bar_flash_alpha, error_bar_text_scalable_zoom, error_bar_tick_alpha,
        field_effect_height, fill_lane_col_offsets, find_first_displayed_beat,
        find_last_displayed_beat, for_each_visible_hold_index, for_each_visible_note_index,
        gameplay_mods_text, held_miss_zoom, hold_body_bottom_for_tail_cap,
        hold_body_segment_budget, hold_draw_span, hold_glow_color, hold_head_part_for_roll,
        hold_indicator_column_x, hold_overlaps_visible_window, hold_parts_for_note_type,
        hold_segment_pose, hold_strip_actor, hold_strip_glow_actor, hold_strip_quad,
        hold_strip_row_3d, hold_tail_cap_bounds, hud_layout_ys, hud_y, itg_actor_glow_alpha,
        itg_actor_rotation_z, join_display_mod_parts, judgment_actor_zoom,
        judgment_tilt_rotation_deg, maybe_mirror_uv_horiz_for_reverse_flipped,
        mine_hides_after_resolution, mine_part, mod_divisor, mod_percent_key, move_col_extra,
        note_itg_row, note_world_z_for_bumpy, note_x_extra, note_x_offset, notefield_view_proj,
        offset_center, player_metric_y, push_transform_parts, quantize_centi_i32,
        quantize_centi_u32, quantize_step, receptor_row_center, rgba8, scale_cap_to_arrow,
        scale_effect_size, scale_sprite_to_arrow, share_actor_range, signed_effect_active,
        sm_scale, smoothstep01, song_time_ns_delta_seconds, song_time_ns_to_seconds,
        stream_segment_index_exclusive_end, stream_segment_index_inclusive_end, tap_judgment_rows,
        tap_part_for_note_type, tap_replacement_head, timing_window_from_num, tiny_spacing_scale,
        tipsy_y_extra, top_cap_rotation_deg, tornado_x_extra, translated_uv_rect,
        visual_arrow_effect_zoom, visual_confusion_rotation_deg, visual_effect_params_for_col,
        visual_hold_body_needs_z_buffer, visual_note_rotation_z, visual_pulse_inner_zoom,
        visual_pulse_zoom_for_y, visual_tiny_zoom, visual_use_legacy_hold_sprites,
        zmod_broken_run_counter_text, zmod_broken_run_end, zmod_broken_run_segment,
        zmod_combo_glow_color, zmod_combo_glow_pair, zmod_combo_quint_active,
        zmod_combo_rainbow_color, zmod_combo_solid_color, zmod_indicator_default_color,
        zmod_indicator_detailed_color, zmod_layout_ys, zmod_measure_counter_text,
        zmod_mini_indicator_output, zmod_mini_indicator_zoom, zmod_pacemaker_color,
        zmod_percent_from_points, zmod_resolved_combo_color, zmod_resolved_mini_indicator_mode,
        zmod_rival_color, zmod_run_timer_index, zmod_static_combo_color, zmod_stream_prog_color,
        zmod_stream_prog_completion_for_beat, zmod_subtractive_counter_state,
        zmod_subtractive_points,
    };
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_rules::judgment::{JudgeGrade, TimingWindow};
    use deadsync_rules::note::{HoldData, MineResult, Note, NoteCountStat};
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::stream::StreamSegment;
    use deadsync_rules::timing::{self, TimeSignatureSegment};

    fn test_note_at_beat(beat: f32) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Tap,
            row_index: beat_to_note_row(beat).max(0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn test_note_at_dense_row(beat: f32, row_index: usize) -> Note {
        let mut note = test_note_at_beat(beat);
        note.row_index = row_index;
        note
    }

    fn test_hold_at_beat(beat: f32, end_beat: f32) -> Note {
        let mut note = test_note_at_beat(beat);
        note.note_type = NoteType::Hold;
        note.hold = Some(HoldData {
            end_row_index: beat_to_note_row(end_beat).max(0) as usize,
            end_beat,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: note.row_index,
            last_held_beat: beat,
        });
        note
    }

    #[test]
    fn edit_beat_bar_labels_default_measure_indices() {
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(0.0), &[])
                .and_then(|info| info.measure_index),
            Some(0)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(1.0), &[])
                .and_then(|info| info.measure_index),
            None
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(4.0), &[])
                .and_then(|info| info.measure_index),
            Some(1)
        );
    }

    #[test]
    fn edit_beat_bar_frames_use_sixteenth_spacing() {
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(0.25), &[]).map(|info| info.frame),
            Some(3)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(0.5), &[]).map(|info| info.frame),
            Some(2)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(1.0), &[]).map(|info| info.frame),
            Some(1)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(4.0), &[]).map(|info| info.frame),
            Some(0)
        );
    }

    #[test]
    fn edit_beat_bar_labels_follow_time_signature_segments() {
        let segments = [
            TimeSignatureSegment {
                beat: 0.0,
                numerator: 3,
                denominator: 4,
            },
            TimeSignatureSegment {
                beat: 6.0,
                numerator: 4,
                denominator: 4,
            },
        ];

        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(0.0), &segments)
                .and_then(|info| info.measure_index),
            Some(0)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(3.0), &segments)
                .and_then(|info| info.measure_index),
            Some(1)
        );
        assert_eq!(
            edit_beat_bar_info_for_row(beat_to_note_row(6.0), &segments)
                .and_then(|info| info.measure_index),
            Some(2)
        );
    }

    #[test]
    fn edit_bar_candidate_step_rows_match_segment_grid() {
        assert_eq!(edit_bar_candidate_step_rows(&[]), beat_to_note_row(0.25));

        let segments = [
            TimeSignatureSegment {
                beat: 0.0,
                numerator: 3,
                denominator: 4,
            },
            TimeSignatureSegment {
                beat: 6.25,
                numerator: 4,
                denominator: 4,
            },
        ];
        assert_eq!(
            edit_bar_candidate_step_rows(&segments),
            beat_to_note_row(0.25)
        );
    }

    #[test]
    fn edit_bar_scroll_speed_matches_legacy_modes() {
        assert_eq!(
            edit_bar_scroll_speed(ScrollSpeedSetting::XMod(2.5), 200.0, 1.0),
            2.5
        );
        assert_eq!(
            edit_bar_scroll_speed(ScrollSpeedSetting::CMod(600.0), 200.0, 1.0),
            4.0
        );
        assert!(
            (edit_bar_scroll_speed(ScrollSpeedSetting::MMod(400.0), 200.0, 1.0) - 2.0).abs()
                <= 1e-6
        );
    }

    #[test]
    fn beat_bar_actor_builds_solid_measure_quad() {
        let mut actors = Vec::new();
        append_beat_bar(&mut actors, false, 0, 120.0, 80.0, 256.0, 1.0, 2.0, 0.4, 80);

        assert_eq!(actors.len(), 1);
        match &actors[0] {
            Actor::Sprite {
                align,
                offset,
                size,
                scale,
                source,
                tint,
                z,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, [120.0, 80.0]);
                assert!(matches!(
                    size,
                    [SizeSpec::Px(w), SizeSpec::Px(h)]
                        if (*w - 256.0).abs() <= 1e-6 && (*h - 2.0).abs() <= 1e-6
                ));
                assert_eq!(*scale, [1.0, 1.0]);
                assert!(matches!(source, SpriteSource::Solid));
                assert_eq!(*tint, [1.0, 1.0, 1.0, 0.4]);
                assert_eq!(*z, 80);
            }
            actor => panic!("expected measure quad, got {actor:?}"),
        }
    }

    #[test]
    fn edit_beat_bar_actor_splits_dashed_frames() {
        let mut actors = Vec::new();
        append_beat_bar(&mut actors, true, 2, 50.0, 20.0, 50.0, 1.0, 2.0, 0.75, 80);

        assert_eq!(actors.len(), 3);
        let expected_x = [25.0, 45.0, 65.0];
        for (actor, x) in actors.iter().zip(expected_x) {
            match actor {
                Actor::Sprite {
                    align,
                    offset,
                    size,
                    scale,
                    tint,
                    z,
                    ..
                } => {
                    assert_eq!(*align, [0.0, 0.5]);
                    assert_eq!(*offset, [x, 20.0]);
                    assert!(matches!(
                        size,
                        [SizeSpec::Px(w), SizeSpec::Px(h)]
                            if *w > 0.0 && (*h - 2.0).abs() <= 1e-6
                    ));
                    assert_eq!(*scale, [1.0, 1.0]);
                    assert_eq!(*tint, [1.0, 1.0, 1.0, 0.75]);
                    assert_eq!(*z, 80);
                }
                actor => panic!("expected dashed segment, got {actor:?}"),
            }
        }
    }

    #[test]
    fn edit_measure_number_actor_respects_edit_mode_and_measure_index() {
        let mut actors = Vec::new();
        append_edit_measure_number(&mut actors, false, Some(4), 12.0, 34.0, 1.0, 80);
        append_edit_measure_number(&mut actors, true, Some(-1), 12.0, 34.0, 1.0, 80);
        append_edit_measure_number(&mut actors, true, None, 12.0, 34.0, 1.0, 80);
        append_edit_measure_number(&mut actors, true, Some(4), 12.0, 34.0, 0.5, 80);

        assert_eq!(actors.len(), 1);
        match &actors[0] {
            Actor::Text {
                align,
                offset,
                font,
                content,
                align_text,
                z,
                scale,
                shadow_len,
                ..
            } => {
                assert_eq!(*align, [1.0, 0.5]);
                assert_eq!(*offset, [12.0, 34.0]);
                assert_eq!(*font, "miso");
                assert_eq!(content.as_str(), "4");
                assert_eq!(*align_text, TextAlign::Right);
                assert_eq!(*z, 81);
                assert_eq!(*scale, [0.45, 0.45]);
                assert_eq!(*shadow_len, [2.0, -2.0]);
            }
            actor => panic!("expected measure number text, got {actor:?}"),
        }
    }

    #[test]
    fn cue_bar_actor_keeps_color_and_measure_layer() {
        let mut actors = Vec::new();
        append_cue_bar(&mut actors, 10.0, 20.0, 30.0, 4.0, (0.2, 0.4, 0.6), 0.8, 80);

        assert_eq!(actors.len(), 1);
        match &actors[0] {
            Actor::Sprite {
                align,
                offset,
                size,
                scale,
                tint,
                z,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, [10.0, 20.0]);
                assert!(matches!(
                    size,
                    [SizeSpec::Px(w), SizeSpec::Px(h)]
                        if (*w - 30.0).abs() <= 1e-6 && (*h - 4.0).abs() <= 1e-6
                ));
                assert_eq!(*scale, [1.0, 1.0]);
                assert_eq!(*tint, [0.2, 0.4, 0.6, 0.8]);
                assert_eq!(*z, 80);
            }
            actor => panic!("expected cue bar quad, got {actor:?}"),
        }
    }

    #[test]
    fn beat_measure_travel_applies_mini_once_like_notes() {
        let raw = beat_scroll_travel(12.0, 8.0, 1.25);
        let field_zoom = 0.75;
        let player_speed = 5.0;
        let expected = (12.0 - 8.0) * ScrollSpeedSetting::ARROW_SPACING * 1.25;

        assert!((raw - expected).abs() <= 0.001);

        let note_y = raw * field_zoom * player_speed;
        let old_measure_y = raw * field_zoom * field_zoom * player_speed;

        assert!((note_y - expected * field_zoom * player_speed).abs() <= 0.001);
        assert!(
            (note_y - old_measure_y).abs() > 100.0,
            "double-applying field zoom would drift measure lines away from notes"
        );
    }

    #[test]
    fn edit_beat_travel_uses_step_editor_spacing() {
        let edit_raw = edit_beat_scroll_travel(44.0, 40.0);
        let displayed_raw = beat_scroll_travel(42.0, 40.0, 0.5);

        assert!((edit_raw - 4.0 * ScrollSpeedSetting::ARROW_SPACING).abs() <= 0.001);
        assert!((displayed_raw - ScrollSpeedSetting::ARROW_SPACING).abs() <= 0.001);
        assert!(
            (edit_raw - displayed_raw).abs() > 100.0,
            "ITG's step editor ignores displayed beat and speed segments"
        );
    }

    #[test]
    fn translated_uv_rect_offsets_all_edges() {
        assert_eq!(
            translated_uv_rect([0.1, 0.2, 0.3, 0.4], [0.5, -0.1]),
            [0.6, 0.1, 0.8, 0.3]
        );
    }

    #[test]
    fn reverse_flipped_cap_uv_only_mirrors_when_both_flags_are_enabled() {
        let uv = [0.125, 0.25, 0.75, 0.875];
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, true, true),
            [0.75, 0.25, 0.125, 0.875]
        );
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, true, false),
            uv
        );
        assert_eq!(
            maybe_mirror_uv_horiz_for_reverse_flipped(uv, false, true),
            uv
        );
    }

    #[test]
    fn reverse_flipped_top_cap_rotation_matches_itg_parity_path() {
        assert!((top_cap_rotation_deg(true, true) - 180.0).abs() <= f32::EPSILON);
        assert!((top_cap_rotation_deg(true, false) - 0.0).abs() <= f32::EPSILON);
        assert!((top_cap_rotation_deg(false, true) - 0.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn sprite_scale_uses_height_as_arrow_target() {
        assert_eq!(scale_sprite_to_arrow([32, 64], 128.0), [64.0, 128.0]);
        assert_eq!(scale_sprite_to_arrow([32, 0], 128.0), [32.0, 0.0]);
        assert_eq!(scale_sprite_to_arrow([-32, 64], 128.0), [0.0, 128.0]);
        assert_eq!(scale_sprite_to_arrow([32, 64], 0.0), [32.0, 64.0]);
    }

    #[test]
    fn cap_scale_uses_width_as_arrow_target() {
        assert_eq!(scale_cap_to_arrow([32, 16], 64.0), [64.0, 32.0]);
        assert_eq!(scale_cap_to_arrow([0, 16], 64.0), [0.0, 16.0]);
        assert_eq!(scale_cap_to_arrow([32, -16], 64.0), [64.0, 0.0]);
        assert_eq!(scale_cap_to_arrow([32, 16], 0.0), [32.0, 16.0]);
    }

    #[test]
    fn effect_size_applies_field_and_effect_zoom() {
        assert_eq!(scale_effect_size([64.0, 32.0], 1.25, 2.0), [160.0, 80.0]);
    }

    #[test]
    fn offset_center_applies_rotated_local_offset() {
        let center = offset_center(
            [10.0, 20.0],
            [3.0, 4.0],
            [
                std::f32::consts::FRAC_PI_2.sin(),
                std::f32::consts::FRAC_PI_2.cos(),
            ],
        );
        assert!((center[0] - 6.0).abs() <= 1e-6);
        assert!((center[1] - 23.0).abs() <= 1e-6);
    }

    #[test]
    fn default_column_x_centers_lanes() {
        assert_eq!(default_column_x(0, 4), -96.0);
        assert_eq!(default_column_x(3, 4), 96.0);
        assert_eq!(default_column_x(0, 1), 0.0);
    }

    #[test]
    fn fill_lane_col_offsets_uses_noteskin_columns_when_present() {
        let mut out = [0.0; 4];
        fill_lane_col_offsets(&mut out, Some(&[-100, -20, 20, 100]), 4, 1.5, 0.5);
        assert_eq!(out, [-75.0, -15.0, 15.0, 75.0]);
    }

    #[test]
    fn fill_lane_col_offsets_falls_back_for_missing_noteskin_columns() {
        let mut out = [999.0; 4];
        fill_lane_col_offsets(&mut out, Some(&[-100, -20]), 4, 1.0, 1.0);
        assert_eq!(out, [-100.0, -20.0, 32.0, 96.0]);
    }

    #[test]
    fn fill_lane_col_offsets_only_updates_active_columns() {
        let mut out = [999.0; 5];
        fill_lane_col_offsets::<i32>(&mut out, None, 4, 1.0, 1.0);
        assert_eq!(out, [-96.0, -32.0, 32.0, 96.0, 999.0]);
    }

    #[test]
    fn compute_invert_distances_mirrors_sides() {
        let cols = [-96.0, -32.0, 32.0, 96.0];
        let mut out = [0.0; 4];
        compute_invert_distances(&cols, &mut out);
        assert_eq!(out, [64.0, -64.0, 64.0, -64.0]);
    }

    #[test]
    fn compute_tornado_bounds_uses_neighbor_window() {
        let cols = [-160.0, -96.0, -32.0, 32.0, 96.0, 160.0];
        let mut out = [TornadoBounds::default(); 6];
        compute_tornado_bounds(&cols, &mut out);
        assert_eq!(
            out[0],
            TornadoBounds {
                min_x: -160.0,
                max_x: -32.0
            }
        );
        assert_eq!(
            out[3],
            TornadoBounds {
                min_x: -96.0,
                max_x: 160.0
            }
        );

        let cols = [-96.0, -32.0, 32.0, 96.0];
        let mut out = [TornadoBounds::default(); 4];
        compute_tornado_bounds(&cols, &mut out);
        assert_eq!(
            out[0],
            TornadoBounds {
                min_x: -96.0,
                max_x: 96.0
            }
        );
    }

    #[test]
    fn sm_scale_interpolates_and_handles_degenerate_inputs() {
        assert!((sm_scale(0.25, 0.0, 1.0, 100.0, 200.0) - 125.0).abs() <= 1e-6);
        assert!((sm_scale(2.0, 0.0, 1.0, 0.0, 10.0) - 20.0).abs() <= 1e-6);
        assert_eq!(sm_scale(0.25, 1.0, 1.0, 100.0, 200.0), 200.0);
    }

    #[test]
    fn quantize_step_rounds_to_nearest_step() {
        assert!((quantize_step(0.24, 0.5) - 0.0).abs() <= 1e-6);
        assert!((quantize_step(0.26, 0.5) - 0.5).abs() <= 1e-6);
        assert!((quantize_step(-0.50, 0.3333) + 0.3333).abs() <= 1e-4);
    }

    #[test]
    fn quantize_centi_keys_round_and_sanitize_inputs() {
        assert_eq!(quantize_centi_i32(1.234), 123);
        assert_eq!(quantize_centi_i32(-1.235), -124);
        assert_eq!(quantize_centi_i32(f64::NAN), 0);
        assert_eq!(quantize_centi_u32(1.235), 124);
        assert_eq!(quantize_centi_u32(-1.0), 0);
        assert_eq!(quantize_centi_u32(f64::INFINITY), 0);
    }

    #[test]
    fn mod_and_i16_keys_round_sanitize_and_clamp() {
        assert_eq!(mod_percent_key(1.234), 123);
        assert_eq!(mod_percent_key(-1.235), -124);
        assert_eq!(mod_percent_key(f32::NAN), 0);
        assert_eq!(mod_percent_key(1000.0), i16::MAX);
        assert_eq!(clamp_rounded_i16(12.5), 13);
        assert_eq!(clamp_rounded_i16(f32::NAN), 0);
        assert_eq!(clamp_rounded_i16(f32::NEG_INFINITY), 0);
        assert_eq!(clamp_rounded_i16(40_000.0), i16::MAX);
    }

    fn empty_mods_params() -> GameplayModsTextParams<'static> {
        GameplayModsTextParams {
            speed: ScrollSpeedSetting::XMod(1.0),
            noteskin: "devcel-2024",
            insert_mask: 0,
            remove_mask: 0,
            holds_mask: 0,
            turn_bits: 0,
            attack_mode: GameplayModsAttackMode::On,
            mini_percent: 0,
            spacing_percent: 0,
            visual_delay_ms: 0,
            average_error_bar_active: false,
            avg_error_bar_intensity_centi: 100,
            avg_error_bar_interval_ms: 100,
            accel: [0; 5],
            visual: [0; 9],
            appearance: [0; 5],
            scroll: [0; 5],
            perspective_tilt: 0,
            perspective_skew: 0,
            dark: 0,
            blind: 0,
            cover: 0,
            disabled_timing_windows: 0,
        }
    }

    #[test]
    fn display_mods_mini_keeps_full_percent() {
        let mut parts = Vec::new();
        append_mini_part(&mut parts, 100);
        assert_eq!(parts, vec!["100% Mini".to_string()]);
    }

    #[test]
    fn display_mods_keep_spaces_inside_one_option_atomic() {
        let text =
            join_display_mod_parts(&["devcel-2024".to_string(), "-4ms VisualDelay".to_string()]);

        assert_eq!(text, "devcel-2024, -4ms\u{00A0}VisualDelay");
    }

    #[test]
    fn display_mods_append_average_error_bar_config() {
        let mut params = empty_mods_params();
        params.average_error_bar_active = true;
        params.avg_error_bar_intensity_centi = 175;
        params.avg_error_bar_interval_ms = 300;

        let mut parts = Vec::new();
        append_average_error_bar_part(&mut parts, params);

        assert_eq!(parts, vec!["ErrorBar1.75x(Avg:300ms)".to_string()]);
    }

    #[test]
    fn display_mods_skip_average_error_bar_config_when_inactive() {
        let mut parts = Vec::new();
        append_average_error_bar_part(&mut parts, empty_mods_params());

        assert!(parts.is_empty());
    }

    #[test]
    fn display_mods_append_all_active_turns_in_itg_order() {
        let mut parts = Vec::new();
        append_turn_parts(&mut parts, DISPLAY_TURN_MIRROR | DISPLAY_TURN_RANDOM);
        assert_eq!(parts, vec!["Mirror".to_string(), "Random".to_string()]);
    }

    #[test]
    fn display_mods_use_simply_love_turn_names() {
        let mut parts = Vec::new();
        append_turn_parts(&mut parts, DISPLAY_TURN_UD_MIRROR);
        assert_eq!(parts, vec!["UD-Mirror".to_string()]);
    }

    #[test]
    fn display_mods_transform_order_matches_itg() {
        let mut parts = Vec::new();
        push_transform_parts(
            &mut parts,
            (1 << 0) | (1 << 1) | (1 << 7),
            (1 << 0) | (1 << 1),
            1 << 3,
        );
        assert_eq!(
            parts,
            vec![
                "NoRolls".to_string(),
                "NoMines".to_string(),
                "Little".to_string(),
                "Wide".to_string(),
                "Big".to_string(),
                "Mines".to_string(),
            ]
        );
    }

    #[test]
    fn display_mods_transform_masks_use_legacy_bit_assignments() {
        let mut parts = Vec::new();
        push_transform_parts(&mut parts, 0xFF, 0xFF, 0x1F);
        assert_eq!(
            parts,
            vec![
                "NoHolds".to_string(),
                "NoRolls".to_string(),
                "NoMines".to_string(),
                "Little".to_string(),
                "Wide".to_string(),
                "Big".to_string(),
                "Quick".to_string(),
                "BMRize".to_string(),
                "Skippy".to_string(),
                "Mines".to_string(),
                "Echo".to_string(),
                "Stomp".to_string(),
                "Planted".to_string(),
                "Floored".to_string(),
                "Twister".to_string(),
                "HoldsToRolls".to_string(),
                "NoJumps".to_string(),
                "NoHands".to_string(),
                "NoLifts".to_string(),
                "NoFakes".to_string(),
                "NoQuads".to_string(),
            ]
        );
    }

    #[test]
    fn display_mods_use_itg_disabled_timing_window_names() {
        assert_eq!(
            disabled_timing_windows_name((1 << 3) | (1 << 4)),
            Some("No W4/W5".to_string())
        );
        assert_eq!(
            disabled_timing_windows_name((1 << 0) | (1 << 1)),
            Some("No W1/W2".to_string())
        );
    }

    #[test]
    fn display_mods_perspective_names_match_itg_rules() {
        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, 0, 0);
        assert_eq!(parts, vec!["Overhead".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, -100, 100);
        assert_eq!(parts, vec!["100% Incoming".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, 100, 0);
        assert_eq!(parts, vec!["100% Distant".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, -100, 0);
        assert_eq!(parts, vec!["100% Hallway".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, 75, 75);
        assert_eq!(parts, vec!["75% Space".to_string()]);

        let mut parts = Vec::new();
        append_perspective_parts(&mut parts, 50, 25);
        assert_eq!(parts, vec!["25% Skew".to_string(), "50% Tilt".to_string()]);
    }

    #[test]
    fn gameplay_mods_text_formats_full_option_list() {
        let mut params = empty_mods_params();
        params.speed = ScrollSpeedSetting::CMod(725.0);
        params.attack_mode = GameplayModsAttackMode::Off;
        params.turn_bits = DISPLAY_TURN_MIRROR;
        params.remove_mask = (1 << 0) | (1 << 1);
        params.mini_percent = 100;
        params.visual_delay_ms = -4;
        params.average_error_bar_active = true;
        params.avg_error_bar_intensity_centi = 175;
        params.avg_error_bar_interval_ms = 300;
        params.disabled_timing_windows = (1 << 3) | (1 << 4);

        assert_eq!(
            gameplay_mods_text(params),
            "C725, 100%\u{00A0}Mini, NoAttacks, Mirror, NoMines, Little, Overhead, devcel-2024, -4ms\u{00A0}VisualDelay, ErrorBar1.75x(Avg:300ms), No\u{00A0}W4/W5"
        );
    }

    #[test]
    fn gameplay_mods_text_formats_mmod_like_legacy_display() {
        let mut params = empty_mods_params();
        params.speed = ScrollSpeedSetting::MMod(600.0);

        assert_eq!(gameplay_mods_text(params), "m600, Overhead, devcel-2024");
    }

    #[test]
    fn gameplay_mods_text_includes_effect_sections_in_legacy_order() {
        let mut params = empty_mods_params();
        params.noteskin = "";
        params.accel = [1, 2, 3, 4, 5];
        params.visual = [6, 7, 8, 9, 10, 11, 12, 13, 14];
        params.mini_percent = 15;
        params.spacing_percent = 16;
        params.appearance = [17, 18, 19, 20, 21];
        params.scroll = [22, 23, 24, 25, 26];
        params.dark = 27;
        params.blind = 28;
        params.cover = 29;

        assert_eq!(
            gameplay_mods_text(params),
            concat!(
                "1x, 1%\u{00A0}Boost, 2%\u{00A0}Brake, 3%\u{00A0}Wave, ",
                "4%\u{00A0}Expand, 5%\u{00A0}Boomerang, 6%\u{00A0}Drunk, ",
                "7%\u{00A0}Dizzy, 8%\u{00A0}Confusion, 9%\u{00A0}Flip, ",
                "10%\u{00A0}Invert, 11%\u{00A0}Tornado, 12%\u{00A0}Tipsy, ",
                "13%\u{00A0}Bumpy, 14%\u{00A0}Beat, 15%\u{00A0}Mini, ",
                "16%\u{00A0}Spacing, 17%\u{00A0}Hidden, 18%\u{00A0}Sudden, ",
                "19%\u{00A0}Stealth, 20%\u{00A0}Blink, 21%\u{00A0}RandomVanish, ",
                "22%\u{00A0}Reverse, 23%\u{00A0}Split, 24%\u{00A0}Alternate, ",
                "25%\u{00A0}Cross, 26%\u{00A0}Centered, 27%\u{00A0}Dark, ",
                "28%\u{00A0}Blind, 29%\u{00A0}Hide\u{00A0}BG, Overhead"
            )
        );
    }

    #[test]
    fn beat_factor_pulses_early_in_each_beat() {
        assert_eq!(beat_factor(-0.25), 0.0);
        assert_eq!(beat_factor(0.3), 0.0);
        assert!((beat_factor(0.0) - 20.0).abs() <= 1e-6);
        assert!((beat_factor(1.0) + 20.0).abs() <= 1e-6);
    }

    #[test]
    fn mod_divisor_preserves_sign_near_zero() {
        assert_eq!(mod_divisor(2.0), 2.0);
        assert_eq!(mod_divisor(0.0005), 0.001);
        assert_eq!(mod_divisor(-0.0005), -0.001);
        assert_eq!(mod_divisor(0.0), 0.001);
        assert_eq!(mod_divisor(-0.0), -0.001);
    }

    #[test]
    fn bumpy_angle_sanitizes_non_finite_options() {
        assert!((bumpy_angle(16.0, f32::NAN, f32::NAN) - 1.0).abs() <= 1e-6);
        assert!((bumpy_angle(16.0, 1.0, 1.0) - 3.625).abs() <= 1e-6);
    }

    #[test]
    fn accel_y_boost_matches_itg_formula() {
        let accel = AccelYParams {
            boost: 1.0,
            ..AccelYParams::default()
        };
        let y = apply_accel_y(120.0, 0.0, 480.0, 480.0, accel);
        assert!((y - 166.15385).abs() <= 0.001);
    }

    #[test]
    fn accel_y_brake_matches_itg_pre_scroll_order() {
        let raw_y = ScrollSpeedSetting::ARROW_SPACING;
        let effect_height = 480.0;
        let scroll_speed = 2.0;
        let accel = AccelYParams {
            brake: 1.0,
            ..AccelYParams::default()
        };
        let itg_order = apply_accel_y(raw_y, 0.0, effect_height, 480.0, accel) * scroll_speed;
        let pre_scaled_order =
            apply_accel_y(raw_y * scroll_speed, 0.0, effect_height, 480.0, accel);
        let expected_itg_order = raw_y * (raw_y / effect_height) * scroll_speed;

        assert!(itg_order < pre_scaled_order);
        assert!((itg_order - expected_itg_order).abs() <= 0.001);
    }

    #[test]
    fn accel_y_reports_boomerang_peak_side() {
        let accel = AccelYParams {
            boomerang: 1.0,
            ..AccelYParams::default()
        };
        assert!(apply_accel_y_with_peak(100.0, 0.0, 480.0, 480.0, accel).1);
        assert!(apply_accel_y_with_peak(300.0, 0.0, 480.0, 480.0, accel).1);
        assert!(!apply_accel_y_with_peak(400.0, 0.0, 480.0, 480.0, accel).1);
    }

    #[test]
    fn note_world_z_for_bumpy_uses_itg_sine_formula() {
        let z = note_world_z_for_bumpy(8.0 * std::f32::consts::PI, 1.0, 0.0, 0.0);
        assert!((z - 40.0).abs() <= 0.0001);

        let z = note_world_z_for_bumpy(-2.0 * std::f32::consts::PI, 1.0, 0.0, -1.25);
        assert!((z - 40.0).abs() <= 0.0001);

        let z = note_world_z_for_bumpy(-8.0 * std::f32::consts::PI, 1.0, 0.0, 0.0);
        assert!((z + 40.0).abs() <= 0.0001);

        assert_eq!(note_world_z_for_bumpy(8.0, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(note_world_z_for_bumpy(8.0, f32::NAN, 0.0, 0.0), 0.0);
    }

    #[test]
    fn itg_actor_rotation_z_converts_to_world_space() {
        assert_eq!(itg_actor_rotation_z(90.0), -90.0);
    }

    #[test]
    fn visual_hold_z_buffer_ignores_column_bumpy_like_itg() {
        assert!(!visual_hold_body_needs_z_buffer(VisualEffectParams {
            bumpy: 0.0,
            ..VisualEffectParams::default()
        }));
        assert!(visual_hold_body_needs_z_buffer(VisualEffectParams {
            bumpy: 1.0,
            ..VisualEffectParams::default()
        }));
    }

    #[test]
    fn visual_legacy_hold_sprites_disable_for_dynamic_effects() {
        assert!(visual_use_legacy_hold_sprites(0.0, 0.0, 0.0, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.1, 0.0, 0.0, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.1, 0.0, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.0, -0.1, 0.0, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.0, 0.0, 0.1, 0.0));
        assert!(!visual_use_legacy_hold_sprites(0.0, 0.0, 0.0, 0.0, 0.1));
        assert!(!visual_use_legacy_hold_sprites(
            0.0,
            0.0,
            0.0,
            0.0,
            f32::NAN
        ));
    }

    #[test]
    fn visual_effect_params_for_col_applies_column_mods() {
        let params = visual_effect_params_for_col(
            VisualEffectParams {
                tiny: 0.25,
                confusion_offset: 0.5,
                bumpy: 0.75,
                ..VisualEffectParams::default()
            },
            1,
            &[9.0, -0.5],
            &[9.0, f32::NAN],
            &[9.0, 0.25],
        );
        assert!((params.tiny + 0.25).abs() <= 1e-6);
        assert!((params.confusion_offset - 0.5).abs() <= 1e-6);
        assert!((params.bumpy - 1.0).abs() <= 1e-6);

        let params = visual_effect_params_for_col(params, 2, &[0.0], &[0.0, 0.0, 0.25], &[0.0]);
        assert!((params.tiny + 0.25).abs() <= 1e-6);
        assert!((params.confusion_offset - 0.75).abs() <= 1e-6);
        assert!((params.bumpy - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn visual_pulse_outer_zoom_matches_itg_formula() {
        let params = VisualEffectParams {
            pulse_outer: 1.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_pulse_zoom_for_y(0.0, params) - 1.0).abs() <= 1e-6);
        assert!(
            (visual_pulse_zoom_for_y(0.4 * 64.0 * std::f32::consts::FRAC_PI_2, params) - 1.5).abs()
                <= 1e-6
        );
        assert!(
            (visual_pulse_zoom_for_y(-0.4 * 64.0 * std::f32::consts::FRAC_PI_2, params) - 0.5)
                .abs()
                <= 1e-6
        );
    }

    #[test]
    fn visual_pulse_inner_zero_clamps_like_itg() {
        let params = VisualEffectParams {
            pulse_inner: -2.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_pulse_inner_zoom(params) - 0.01).abs() <= 1e-6);

        let params = VisualEffectParams {
            pulse_inner: 1.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_pulse_inner_zoom(params) - 1.5).abs() <= 1e-6);
    }

    #[test]
    fn visual_tiny_zoom_matches_itg_power_formula() {
        assert!(
            (visual_tiny_zoom(VisualEffectParams {
                tiny: 2.0,
                ..VisualEffectParams::default()
            }) - 0.5_f32.powf(2.0))
            .abs()
                <= 1e-6
        );
        assert!(
            (visual_tiny_zoom(VisualEffectParams {
                tiny: -0.5,
                ..VisualEffectParams::default()
            }) - 0.5_f32.powf(-0.5))
            .abs()
                <= 1e-6
        );
        assert_eq!(
            visual_tiny_zoom(VisualEffectParams {
                tiny: f32::NAN,
                ..VisualEffectParams::default()
            }),
            1.0
        );
    }

    #[test]
    fn visual_arrow_effect_zoom_combines_tiny_and_pulse() {
        let params = VisualEffectParams {
            tiny: 1.0,
            pulse_outer: 1.0,
            ..VisualEffectParams::default()
        };
        assert!((visual_arrow_effect_zoom(0.0, params) - 0.5).abs() <= 1e-6);

        let doubled = VisualEffectParams {
            tiny: -1.0,
            ..VisualEffectParams::default()
        };
        let base = scale_effect_size([64.0, 64.0], 1.25, 1.0);
        let scaled = scale_effect_size([64.0, 64.0], 1.25, visual_arrow_effect_zoom(0.0, doubled));
        assert!((scaled[0] - base[0] * 2.0).abs() <= 1e-6);
        assert!((scaled[1] - base[1] * 2.0).abs() <= 1e-6);
    }

    #[test]
    fn note_x_offset_applies_tiny_and_column_move_after_effects() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let move_x = [0.0, 0.5, 0.0, 0.0];
        let params = NoteXParams {
            screen_height: 480.0,
            drunk: 1.0,
            ..NoteXParams::default()
        };
        let offset = note_x_offset(
            1,
            0.0,
            1.0,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            &move_x,
            params,
            1.0,
        );
        let base = col_offsets[1]
            + note_x_extra(1, 0.0, 1.0, 0.0, &col_offsets, &invert, &tornado, params);
        assert!((offset - (base * 0.5 + 32.0)).abs() <= 1e-6);
    }

    #[test]
    fn receptor_row_center_uses_zero_travel_x_and_tipsy_y() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let params = NoteXParams {
            screen_height: 480.0,
            drunk: 1.0,
            ..NoteXParams::default()
        };
        let center = receptor_row_center(
            320.0,
            2,
            240.0,
            1.25,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            &[0.0; 4],
            &[0.0, 0.0, -0.25, 0.0],
            params,
            0.0,
            1.0,
        );
        let expected_x = 320.0
            + note_x_offset(
                2,
                0.0,
                1.25,
                0.0,
                &col_offsets,
                &invert,
                &tornado,
                &[0.0; 4],
                params,
                0.0,
            );
        let expected_y = 240.0 + tipsy_y_extra(2, 1.25, 1.0) - 16.0;
        assert!((center[0] - expected_x).abs() <= 1e-6);
        assert!((center[1] - expected_y).abs() <= 1e-6);
    }

    #[test]
    fn hold_indicator_column_x_uses_zero_travel_note_offset() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let move_x = [0.0, -0.5, 0.0, 0.0];
        let params = NoteXParams {
            screen_height: 480.0,
            invert: 1.0,
            ..NoteXParams::default()
        };
        let x = hold_indicator_column_x(
            320.0,
            1,
            0.75,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            &move_x,
            params,
            0.5,
        );
        let expected = 320.0
            + note_x_offset(
                1,
                0.0,
                0.75,
                0.0,
                &col_offsets,
                &invert,
                &tornado,
                &move_x,
                params,
                0.5,
            );
        assert!((x - expected).abs() <= 1e-6);
    }

    #[test]
    fn visual_confusion_offset_converts_static_rotation_to_actor_space() {
        let params = VisualEffectParams {
            confusion_offset: std::f32::consts::FRAC_PI_2,
            ..VisualEffectParams::default()
        };
        assert!((visual_confusion_rotation_deg(0.0, params) + 90.0).abs() <= 1e-6);
    }

    #[test]
    fn visual_note_rotation_converts_confusion_and_dizzy_to_actor_space() {
        let params = VisualEffectParams {
            confusion: 1.5,
            ..VisualEffectParams::default()
        };
        let rotation = visual_note_rotation_z(12.0, 3.5, true, params);
        let itg_expected = (3.5_f32 * params.confusion).rem_euclid(std::f32::consts::TAU)
            * (-180.0 / std::f32::consts::PI);
        assert!((rotation + itg_expected).abs() <= 1e-6);

        let params = VisualEffectParams {
            dizzy: 2.0,
            ..VisualEffectParams::default()
        };
        let rotation = visual_note_rotation_z(6.75, 3.5, false, params);
        let itg_expected =
            ((6.75 - 3.5) * params.dizzy) % std::f32::consts::TAU * (180.0 / std::f32::consts::PI);
        assert!((rotation + itg_expected).abs() <= 1e-6);
    }

    #[test]
    fn visual_negative_dizzy_rotates_notes_like_itgmania() {
        let params = VisualEffectParams {
            dizzy: -0.5,
            ..VisualEffectParams::default()
        };
        let rotation = visual_note_rotation_z(70.0, 68.0, false, params);
        let itg_expected =
            ((70.0 - 68.0) * params.dizzy) % std::f32::consts::TAU * (180.0 / std::f32::consts::PI);

        assert!(rotation.abs() > 1.0);
        assert!((rotation + itg_expected).abs() <= 1e-6);
    }

    #[test]
    fn smoothstep01_clamps_and_eases() {
        assert_eq!(smoothstep01(-1.0), 0.0);
        assert_eq!(smoothstep01(0.0), 0.0);
        assert_eq!(smoothstep01(1.0), 1.0);
        assert_eq!(smoothstep01(2.0), 1.0);
        assert!((smoothstep01(0.5) - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn reverse_column_cue_bounds_match_simply_love() {
        let lane_width = 64.0;
        let screen_height = 480.0;
        let receptor_reverse_y = 145.0;
        let cue_height = column_cue_height(screen_height);
        let top = column_cue_reverse_top_y(lane_width, cue_height, 0.0, receptor_reverse_y);
        let bottom = top + cue_height;

        assert!((cue_height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
        assert!((crossover_cue_height(screen_height) - 130.0).abs() <= 1e-6);
        assert!((COLUMN_CUE_Y_OFFSET - 80.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_default_layout_matches_original_simply_love() {
        let lane_width = 64.0;
        let screen_height = 480.0;
        let receptor_reverse_y = 145.0;
        let layout = column_flash_layout(false);
        let height = column_flash_height(screen_height, layout);
        let top = column_flash_reverse_top_y(layout, lane_width, height, 0.0, receptor_reverse_y);
        let bottom = top + height;

        assert!((layout.y_offset - 80.0).abs() <= 1e-6);
        assert!((layout.fade - 0.333).abs() <= 1e-6);
        assert!((height - 400.0).abs() <= 1e-6);
        assert!((top - 17.0).abs() <= 1e-6);
        assert!((bottom - 417.0).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_compact_layout_matches_chris_reference() {
        let lane_width = 64.0;
        let screen_height = 480.0;
        let receptor_reverse_y = 145.0;
        let layout = column_flash_layout(true);
        let height = column_flash_height(screen_height, layout);
        let top = column_flash_reverse_top_y(layout, lane_width, height, 0.0, receptor_reverse_y);
        let bottom = top + height;

        assert!((layout.y_offset - 70.0).abs() <= 1e-6);
        assert!((layout.height_trim - 270.0).abs() <= 1e-6);
        assert!((layout.fade - 0.2).abs() <= 1e-6);
        assert!((height - 140.0).abs() <= 1e-6);
        assert!((top - 247.0).abs() <= 1e-6);
        assert!((bottom - 387.0).abs() <= 1e-6);
        assert_eq!(column_flash_height(100.0, layout), 0.0);
    }

    #[test]
    fn column_flash_alpha_at_decays_quadratically() {
        assert!((column_flash_alpha_at(1.0, 1.0, 0.5, 0.66) - 0.66).abs() <= 1e-6);
        assert!((column_flash_alpha_at(1.0, 1.25, 0.5, 0.66) - 0.495).abs() <= 1e-6);
        assert_eq!(column_flash_alpha_at(1.0, 1.5, 0.5, 0.66), 0.0);
    }

    #[test]
    fn column_flash_alpha_at_rejects_invalid_inputs() {
        assert_eq!(column_flash_alpha_at(1.0, 0.9, 0.5, 0.66), 0.0);
        assert_eq!(column_flash_alpha_at(1.0, 1.1, 0.0, 0.66), 0.0);
        assert_eq!(column_flash_alpha_at(1.0, f32::NAN, 0.5, 0.66), 0.0);
    }

    #[test]
    fn column_flash_alpha_matches_brightness_options() {
        let normal = column_flash_alpha(0.0, 0.0, 0.5, false);
        let dimmed = column_flash_alpha(0.0, 0.0, 0.5, true);

        assert!((normal - 0.66).abs() <= 1e-6);
        assert!((dimmed - 0.3).abs() <= 1e-6);
    }

    #[test]
    fn column_flash_colors_match_reference_palette() {
        assert_eq!(
            column_flash_color(JudgeGrade::Miss, false, 0.3),
            [1.0, 0.0, 0.0, 0.3]
        );
        assert_eq!(
            column_flash_color(JudgeGrade::Decent, false, 0.3),
            [0.70, 0.36, 1.00, 0.3]
        );
        assert_eq!(
            column_flash_color(JudgeGrade::Fantastic, false, 0.3),
            [1.0, 1.0, 1.0, 0.3]
        );
    }

    #[test]
    fn field_effect_height_adds_tilt_margin() {
        assert_eq!(field_effect_height(480.0, 0.0), 480.0);
        assert_eq!(field_effect_height(480.0, -0.5), 580.0);
    }

    #[test]
    fn signed_effect_active_rejects_zero_epsilon_and_nan() {
        assert!(!signed_effect_active(0.0));
        assert!(!signed_effect_active(f32::EPSILON));
        assert!(!signed_effect_active(f32::NAN));
        assert!(signed_effect_active(-0.01));
    }

    #[test]
    fn tipsy_y_extra_matches_itg_column_wave() {
        assert_eq!(tipsy_y_extra(0, 0.0, 0.0), 0.0);
        assert_eq!(tipsy_y_extra(0, 0.0, f32::NAN), 0.0);
        assert!((tipsy_y_extra(0, 0.0, -1.0) + 25.6).abs() <= 1e-6);
    }

    #[test]
    fn beat_x_extra_uses_beat_factor_wave() {
        assert_eq!(beat_x_extra(0.0, 20.0, 0.0), 0.0);
        assert!((beat_x_extra(0.0, 20.0, 1.0) - 20.0).abs() <= 1e-6);
        let expected = 20.0 * (1.0_f32 + std::f32::consts::FRAC_PI_2).sin();
        assert!((beat_x_extra(15.0, 20.0, 1.0) - expected).abs() <= 1e-6);
    }

    #[test]
    fn drunk_x_extra_uses_column_and_y_phase() {
        assert_eq!(drunk_x_extra(0, 0.0, 0.0, 480.0, 0.0), 0.0);
        assert_eq!(drunk_x_extra(0, 0.0, 0.0, 480.0, f32::NAN), 0.0);
        assert!((drunk_x_extra(0, 0.0, 0.0, 480.0, -1.0) + 32.0).abs() <= 1e-6);
    }

    #[test]
    fn tornado_x_extra_scales_toward_bound_arc() {
        let bounds = TornadoBounds {
            min_x: -96.0,
            max_x: 96.0,
        };
        assert_eq!(tornado_x_extra(0.0, 0.0, bounds, 480.0, 0.0), 0.0);
        assert!((tornado_x_extra(0.0, 0.0, bounds, 480.0, 1.0) - 0.0).abs() <= 1e-4);
        let expected = {
            let radians = std::f32::consts::PI + 80.0 * 6.0 / 480.0;
            sm_scale(radians.cos(), -1.0, 1.0, -96.0, 96.0) + 96.0
        };
        assert!((tornado_x_extra(80.0, -96.0, bounds, 480.0, 1.0) - expected).abs() <= 1e-4);
    }

    #[test]
    fn note_x_extra_flip_moves_to_mirrored_column() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let delta = note_x_extra(
            0,
            64.0,
            0.0,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            NoteXParams {
                screen_height: 480.0,
                flip: 1.0,
                tornado: 0.0,
                drunk: 0.0,
                invert: 0.0,
                beat: 0.0,
            },
        );
        assert!((delta - 192.0).abs() <= 1e-6);
    }

    #[test]
    fn note_x_extra_keeps_negative_position_mods_active_like_itg() {
        let col_offsets = [-96.0, -32.0, 32.0, 96.0];
        let invert = [0.0; 4];
        let tornado = [TornadoBounds::default(); 4];
        let delta = note_x_extra(
            0,
            0.0,
            0.0,
            0.0,
            &col_offsets,
            &invert,
            &tornado,
            NoteXParams {
                screen_height: 480.0,
                tornado: 0.0,
                drunk: -1.0,
                flip: -0.5,
                invert: 0.0,
                beat: 0.0,
            },
        );

        assert!((delta + 128.0).abs() <= 1e-6);
        assert!((tipsy_y_extra(0, 0.0, -1.0) + 25.6).abs() <= 1e-6);
    }

    #[test]
    fn appearance_blink_alpha_matches_itg_boolean_behavior() {
        let partial = appearance_note_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                blink: 0.3,
                ..NoteAlphaParams::default()
            },
        );
        let full = appearance_note_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                blink: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        assert!((partial - full).abs() <= 1e-6);
        assert_eq!(full, 0.0);

        let visible_phase = appearance_note_alpha(
            100.0,
            0.1,
            0.0,
            NoteAlphaParams {
                blink: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        assert!((visible_phase - 0.9999).abs() <= 1e-6);
    }

    #[test]
    fn appearance_hidden_and_sudden_alpha_match_itg_fade_bands() {
        let hidden = NoteAlphaParams {
            hidden: 1.0,
            ..NoteAlphaParams::default()
        };
        assert_eq!(appearance_note_alpha(-1.0, 0.0, 0.0, hidden), 1.0);
        assert_eq!(appearance_note_alpha(120.0, 0.0, 0.0, hidden), 0.0);
        assert!((appearance_note_alpha(140.0, 0.0, 0.0, hidden) - 0.5).abs() <= 1e-6);
        assert_eq!(appearance_note_alpha(160.0, 0.0, 0.0, hidden), 1.0);

        let sudden = NoteAlphaParams {
            sudden: 1.0,
            ..NoteAlphaParams::default()
        };
        assert_eq!(appearance_note_alpha(160.0, 0.0, 0.0, sudden), 1.0);
        assert!((appearance_note_alpha(180.0, 0.0, 0.0, sudden) - 0.5).abs() <= 1e-6);
        assert_eq!(appearance_note_alpha(200.0, 0.0, 0.0, sudden), 0.0);
    }

    #[test]
    fn appearance_hidden_sudden_combo_widens_fade_bands() {
        let combo = NoteAlphaParams {
            hidden: 1.0,
            sudden: 1.0,
            ..NoteAlphaParams::default()
        };
        assert_eq!(appearance_note_alpha(110.0, 0.0, 0.0, combo), 0.0);
        assert_eq!(appearance_note_alpha(160.0, 0.0, 0.0, combo), 1.0);
        assert_eq!(appearance_note_alpha(210.0, 0.0, 0.0, combo), 0.0);
    }

    #[test]
    fn appearance_random_vanish_fades_near_center_line() {
        let random_vanish = NoteAlphaParams {
            random_vanish: 1.0,
            ..NoteAlphaParams::default()
        };
        assert_eq!(appearance_note_alpha(160.0, 0.0, 0.0, random_vanish), 0.0);
        assert_eq!(appearance_note_alpha(320.0, 0.0, 0.0, random_vanish), 1.0);
    }

    #[test]
    fn appearance_stealth_glow_matches_itg_visibility_curve() {
        let glow = appearance_note_glow(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                stealth: 0.25,
                ..NoteAlphaParams::default()
            },
        );
        assert!((glow - 0.65).abs() <= 1e-6);
    }

    #[test]
    fn appearance_note_actor_alpha_matches_itg_visibility_gate() {
        let half_visible = appearance_note_actor_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                stealth: 0.5,
                ..NoteAlphaParams::default()
            },
        );
        let mostly_visible = appearance_note_actor_alpha(
            100.0,
            0.0,
            0.0,
            NoteAlphaParams {
                stealth: 0.25,
                ..NoteAlphaParams::default()
            },
        );
        assert_eq!(half_visible, 0.0);
        assert_eq!(mostly_visible, 1.0);
    }

    #[test]
    fn appearance_needs_rows_only_for_y_varying_effects() {
        assert!(!appearance_needs_rows(NoteAlphaParams::default()));
        assert!(appearance_needs_rows(NoteAlphaParams {
            hidden: 1.0,
            ..NoteAlphaParams::default()
        }));
        assert!(appearance_needs_rows(NoteAlphaParams {
            sudden: 1.0,
            ..NoteAlphaParams::default()
        }));
        assert!(appearance_needs_rows(NoteAlphaParams {
            random_vanish: 1.0,
            ..NoteAlphaParams::default()
        }));
        assert!(!appearance_needs_rows(NoteAlphaParams {
            blink: 1.0,
            stealth: 1.0,
            ..NoteAlphaParams::default()
        }));
    }

    #[test]
    fn appearance_sudden_offset_shifts_fade_band_like_itg() {
        let base = appearance_note_alpha(
            180.0,
            0.0,
            0.0,
            NoteAlphaParams {
                sudden: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        let shifted = appearance_note_alpha(
            180.0,
            0.0,
            0.0,
            NoteAlphaParams {
                sudden: 1.0,
                sudden_offset: 1.0,
                ..NoteAlphaParams::default()
            },
        );
        assert!(shifted > base);
    }

    #[test]
    fn tiny_spacing_scale_sanitizes_and_shrinks() {
        assert_eq!(tiny_spacing_scale(0.0), 1.0);
        assert_eq!(tiny_spacing_scale(f32::NAN), 1.0);
        assert!((tiny_spacing_scale(1.0) - 0.5).abs() <= 1e-6);
        assert_eq!(tiny_spacing_scale(-1.0), 1.0);
    }

    #[test]
    fn move_col_extra_scales_finite_columns() {
        assert_eq!(move_col_extra(&[0.0, 0.5], 1), 32.0);
        assert_eq!(move_col_extra(&[f32::NAN], 0), 0.0);
        assert_eq!(move_col_extra(&[], 4), 0.0);
    }

    #[test]
    fn itg_actor_glow_alpha_clamps_like_itg_vertex_color() {
        assert_eq!(itg_actor_glow_alpha(1.3), 1.0);
        assert_eq!(itg_actor_glow_alpha(0.65), 0.65);
        assert_eq!(itg_actor_glow_alpha(f32::NAN), 0.0);
    }

    #[test]
    fn hold_glow_color_uses_white_with_alpha() {
        assert_eq!(hold_glow_color(0.25), [1.0, 1.0, 1.0, 0.25]);
    }

    #[test]
    fn column_cue_alpha_fades_in_and_out() {
        assert!((column_cue_alpha(0.0, 1.0) - 0.0).abs() <= 1e-6);
        assert!((column_cue_alpha(0.0375, 1.0) - 0.4375).abs() <= 1e-6);
        assert!((column_cue_alpha(0.075, 1.0) - 0.75).abs() <= 1e-6);
        assert!((column_cue_alpha(0.15, 1.0) - 1.0).abs() <= 1e-6);
        assert!((column_cue_alpha(0.5, 1.0) - 1.0).abs() <= 1e-6);
        assert!((column_cue_alpha(0.925, 1.0) - 0.75).abs() <= 1e-6);
        assert!((column_cue_alpha(0.95, 1.0) - 0.5555556).abs() <= 1e-6);
        assert!((column_cue_alpha(1.0, 1.0) - 0.0).abs() <= 1e-6);
    }

    #[test]
    fn column_cue_alpha_rejects_invalid_ranges() {
        assert_eq!(column_cue_alpha(-0.1, 1.0), 0.0);
        assert_eq!(column_cue_alpha(1.1, 1.0), 0.0);
        assert_eq!(column_cue_alpha(0.1, 0.3), 0.0);
        assert_eq!(column_cue_alpha(f32::NAN, 1.0), 0.0);
        assert_eq!(column_cue_alpha(0.1, f32::INFINITY), 0.0);
    }

    #[test]
    fn error_bar_tick_alpha_matches_tick_modes() {
        assert_eq!(error_bar_tick_alpha(-0.1, 0.5, false), 0.0);
        assert_eq!(error_bar_tick_alpha(0.2, 0.5, false), 1.0);
        assert_eq!(error_bar_tick_alpha(0.5, 0.5, false), 0.0);

        assert_eq!(error_bar_tick_alpha(0.02, 0.5, true), 1.0);
        assert!((error_bar_tick_alpha(0.265, 0.5, true) - 0.5).abs() <= 1e-6);
        assert_eq!(error_bar_tick_alpha(0.5, 0.5, true), 0.0);
        assert_eq!(error_bar_tick_alpha(f32::NAN, 0.5, false), 0.0);
        assert_eq!(error_bar_tick_alpha(0.02, 0.0, true), 1.0);
    }

    #[test]
    fn error_bar_flash_alpha_falls_back_to_base_alpha() {
        assert!((error_bar_flash_alpha(1.0, None, 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.0, Some(1.2), 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.6, Some(1.0), 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(f32::NAN, Some(1.0), 0.5) - 0.3).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.0, Some(f32::NAN), 0.5) - 0.3).abs() <= 1e-6);
    }

    #[test]
    fn error_bar_flash_alpha_fades_from_full_to_base() {
        assert!((error_bar_flash_alpha(1.0, Some(1.0), 0.5) - 1.0).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.25, Some(1.0), 0.5) - 0.65).abs() <= 1e-6);
        assert!((error_bar_flash_alpha(1.49, Some(1.0), 0.5) - 0.314).abs() <= 1e-6);
    }

    #[test]
    fn error_bar_boundaries_insert_fa_plus_split() {
        let windows = [0.015, 0.0225, 0.045, 0.09, 0.135];
        let (bounds, len) = error_bar_boundaries_s(windows, Some(0.010), true, 0);

        assert_eq!(len, 2);
        assert!((bounds[0] - 0.010).abs() <= 1e-6);
        assert!((bounds[1] - windows[0]).abs() <= 1e-6);
    }

    #[test]
    fn error_bar_boundaries_clamp_to_max_window() {
        let windows = [0.015, 0.0225, 0.045, 0.09, 0.135];
        let (bounds, len) = error_bar_boundaries_s(windows, None, false, 99);

        assert_eq!(len, 5);
        assert_eq!(&bounds[..len], &windows);
    }

    #[test]
    fn stream_segment_indices_handle_boundaries_and_nan() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 4,
                is_break: false,
            },
            StreamSegment {
                start: 4,
                end: 8,
                is_break: true,
            },
        ];

        assert_eq!(stream_segment_index_exclusive_end(&segs, 4.0), 1);
        assert_eq!(stream_segment_index_inclusive_end(&segs, 4.0), 0);
        assert_eq!(
            stream_segment_index_exclusive_end(&segs, f32::NEG_INFINITY),
            0
        );
        assert_eq!(
            stream_segment_index_inclusive_end(&segs, f32::NEG_INFINITY),
            0
        );
        assert_eq!(
            stream_segment_index_exclusive_end(&segs, f32::INFINITY),
            segs.len()
        );
        assert_eq!(
            stream_segment_index_inclusive_end(&segs, f32::INFINITY),
            segs.len()
        );
        assert_eq!(
            stream_segment_index_exclusive_end(&segs, f32::NAN),
            segs.len()
        );
        assert_eq!(
            stream_segment_index_inclusive_end(&segs, f32::NAN),
            segs.len()
        );
        assert_eq!(zmod_run_timer_index(&segs, 3.0), Some(0));
        assert_eq!(zmod_run_timer_index(&segs, 4.0), Some(0));
        assert_eq!(zmod_run_timer_index(&segs, 4.5), Some(1));
        assert_eq!(zmod_run_timer_index(&segs, 8.0), Some(1));
        assert_eq!(zmod_run_timer_index(&segs, 9.0), None);
    }

    #[test]
    fn zmod_broken_run_merges_short_breaks_and_adjacent_streams() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 10,
                is_break: true,
            },
            StreamSegment {
                start: 10,
                end: 14,
                is_break: false,
            },
            StreamSegment {
                start: 14,
                end: 20,
                is_break: true,
            },
        ];

        assert_eq!(zmod_broken_run_end(&segs, 0), (14, true));
        assert_eq!(zmod_broken_run_segment(&segs, 9.0), Some((0, 14, true)));
        assert_eq!(zmod_broken_run_segment(&segs, 15.0), Some((3, 20, false)));
        assert_eq!(zmod_broken_run_segment(&segs, 21.0), None);

        let three_measure_break = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 11,
                is_break: true,
            },
            StreamSegment {
                start: 11,
                end: 15,
                is_break: false,
            },
        ];

        assert_eq!(zmod_broken_run_end(&three_measure_break, 0), (15, true));
        assert_eq!(
            zmod_broken_run_segment(&three_measure_break, 9.0),
            Some((0, 15, true))
        );
        assert_eq!(
            zmod_broken_run_segment(&three_measure_break, 12.0),
            Some((0, 15, true))
        );
    }

    #[test]
    fn zmod_measure_counter_text_describes_current_and_lookahead_segments() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 12,
                is_break: true,
            },
            StreamSegment {
                start: 12,
                end: 20,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 0, false, 2, 1.0),
            Some(ZmodMeasureCounterText::Ratio {
                current: 4,
                total: 8
            })
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 0, false, 0, 1.0),
            Some(ZmodMeasureCounterText::Ratio {
                current: 4,
                total: 8
            })
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 1, true, 2, 1.0),
            Some(ZmodMeasureCounterText::Break(4))
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 1, true, 2, 1.5),
            Some(ZmodMeasureCounterText::Break(6))
        );
        assert_eq!(
            zmod_measure_counter_text(36.0, 9.0, &segs, 1, false, 2, 1.0),
            Some(ZmodMeasureCounterText::Break(3))
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 2, true, 2, 1.0),
            Some(ZmodMeasureCounterText::Total(8))
        );
        assert_eq!(
            zmod_measure_counter_text(52.0, 13.0, &segs, 2, false, 2, 1.0),
            Some(ZmodMeasureCounterText::Ratio {
                current: 2,
                total: 8
            })
        );
        assert_eq!(
            zmod_measure_counter_text(12.0, 3.0, &segs, 0, false, 2, 2.0),
            Some(ZmodMeasureCounterText::Ratio {
                current: 7,
                total: 16
            })
        );
        assert_eq!(
            zmod_measure_counter_text(36.0, 9.0, &segs, 1, false, 0, 1.0),
            None
        );
    }

    #[test]
    fn zmod_measure_counter_text_handles_negative_song_time() {
        let stream_first = [StreamSegment {
            start: 0,
            end: 8,
            is_break: false,
        }];
        let break_first = [
            StreamSegment {
                start: 0,
                end: 2,
                is_break: true,
            },
            StreamSegment {
                start: 2,
                end: 8,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_measure_counter_text(-4.0, -1.0, &stream_first, 0, false, 1, 1.0),
            Some(ZmodMeasureCounterText::Break(2))
        );
        assert_eq!(
            zmod_measure_counter_text(-4.0, -1.0, &break_first, 0, false, 1, 1.0),
            Some(ZmodMeasureCounterText::Break(4))
        );
    }

    #[test]
    fn zmod_broken_run_counter_text_uses_merged_stream_length() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 10,
                is_break: true,
            },
            StreamSegment {
                start: 10,
                end: 14,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_broken_run_counter_text(3.0, &segs, 0, 14),
            Some(ZmodMeasureCounterText::Ratio {
                current: 4,
                total: 14
            })
        );
        assert_eq!(
            zmod_broken_run_counter_text(-1.0, &segs, 0, 14),
            Some(ZmodMeasureCounterText::Break(2))
        );
        assert_eq!(
            zmod_broken_run_counter_text(-1.2, &segs, 0, 14),
            Some(ZmodMeasureCounterText::Break(2))
        );
        assert_eq!(zmod_broken_run_counter_text(9.0, &segs, 1, 10), None);

        let future_stream = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: true,
            },
            StreamSegment {
                start: 8,
                end: 12,
                is_break: false,
            },
        ];
        assert_eq!(
            zmod_broken_run_counter_text(7.5, &future_stream, 1, 12),
            Some(ZmodMeasureCounterText::Total(4))
        );
    }

    #[test]
    fn timing_window_from_num_saturates_to_w5() {
        assert_eq!(timing_window_from_num(0), TimingWindow::W0);
        assert_eq!(timing_window_from_num(4), TimingWindow::W4);
        assert_eq!(timing_window_from_num(5), TimingWindow::W5);
        assert_eq!(timing_window_from_num(99), TimingWindow::W5);
    }

    #[test]
    fn zmod_percent_from_points_matches_two_decimal_floor() {
        assert_eq!(zmod_percent_from_points(-5, 100), 0.0);
        assert_eq!(zmod_percent_from_points(1, 3), 33.33);
        assert_eq!(zmod_percent_from_points(125, 100), 125.0);
        assert_eq!(zmod_percent_from_points(50, 0), 0.0);
    }

    #[test]
    fn error_bar_colors_follow_judgment_palette() {
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W0, true),
            [33.0 / 255.0, 204.0 / 255.0, 232.0 / 255.0, 1.0]
        );
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W1, true),
            [1.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W1, false),
            [33.0 / 255.0, 204.0 / 255.0, 232.0 / 255.0, 1.0]
        );
        assert_eq!(
            error_bar_color_for_window(TimingWindow::W5, true),
            [201.0 / 255.0, 133.0 / 255.0, 94.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn zmod_subtractive_counter_uses_whites_for_ex_paths() {
        let itg = MiniIndicatorProgress {
            w2: 4,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&itg, MiniIndicatorScoreType::Itg),
            (4, false)
        );
        let itg_many = MiniIndicatorProgress { w2: 11, ..itg };
        assert_eq!(
            zmod_subtractive_counter_state(&itg_many, MiniIndicatorScoreType::Itg),
            (11, true)
        );

        let ex = MiniIndicatorProgress {
            w2: 0,
            white_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&ex, MiniIndicatorScoreType::Ex),
            (7, false)
        );
        let ex_w2 = MiniIndicatorProgress { w2: 1, ..ex };
        assert_eq!(
            zmod_subtractive_counter_state(&ex_w2, MiniIndicatorScoreType::Ex),
            (7, true)
        );

        let hard_ex = MiniIndicatorProgress {
            w2: 1,
            white_10ms_count: 7,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_counter_state(&hard_ex, MiniIndicatorScoreType::HardEx),
            (7, true)
        );
        let let_go = MiniIndicatorProgress { let_go: 1, ..itg };
        assert_eq!(
            zmod_subtractive_counter_state(&let_go, MiniIndicatorScoreType::Itg),
            (4, true)
        );
    }

    #[test]
    fn zmod_subtractive_points_supports_all_score_types() {
        let itg = MiniIndicatorProgress {
            current_possible_dp: 20,
            actual_dp: 16,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&itg, MiniIndicatorScoreType::Itg),
            4
        );

        let itg_mine = MiniIndicatorProgress {
            current_possible_dp: 0,
            actual_dp: -6,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&itg_mine, MiniIndicatorScoreType::Itg),
            6
        );
        let itg_over_scored = MiniIndicatorProgress {
            current_possible_dp: 16,
            actual_dp: 20,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&itg_over_scored, MiniIndicatorScoreType::Itg),
            0
        );

        let ex = MiniIndicatorProgress {
            white_count: 3,
            w2: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(zmod_subtractive_points(&ex, MiniIndicatorScoreType::Ex), 6);

        let ex_with_great = MiniIndicatorProgress {
            white_count: 3,
            w2: 1,
            w3: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&ex_with_great, MiniIndicatorScoreType::Ex),
            11
        );
        let ex_penalties = MiniIndicatorProgress {
            white_count: 3,
            w2: 1,
            w3: 1,
            w4: 1,
            w5: 1,
            miss: 1,
            let_go: 1,
            mines_hit: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&ex_penalties, MiniIndicatorScoreType::Ex),
            36
        );

        let hard_ex = MiniIndicatorProgress {
            white_10ms_count: 3,
            w2: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&hard_ex, MiniIndicatorScoreType::HardEx),
            8
        );
        let hard_ex_penalties = MiniIndicatorProgress {
            white_10ms_count: 3,
            w2: 1,
            w3: 1,
            w4: 1,
            w5: 1,
            miss: 1,
            let_go: 1,
            mines_hit: 1,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_subtractive_points(&hard_ex_penalties, MiniIndicatorScoreType::HardEx),
            40
        );
    }

    #[test]
    fn zmod_mini_indicator_zoom_matches_size_setting() {
        assert!(
            (zmod_mini_indicator_zoom(MiniIndicatorSize::Default) - 0.35).abs() <= f32::EPSILON
        );
        assert!((zmod_mini_indicator_zoom(MiniIndicatorSize::Large) - 0.5).abs() <= f32::EPSILON);
    }

    #[test]
    fn effective_mini_value_applies_fallback_big_and_clamp() {
        assert_eq!(effective_mini_value(80.0, 25.0, 0.0), 0.8);
        assert_eq!(effective_mini_value(f32::NAN, 25.0, 0.0), 0.25);
        assert_eq!(effective_mini_value(80.0, 25.0, 1.0), -0.2);
        assert_eq!(effective_mini_value(80.0, 25.0, 0.25), -0.2);
        assert_eq!(effective_mini_value(-200.0, 25.0, 0.0), -1.0);
        assert_eq!(effective_mini_value(200.0, 25.0, 0.0), 1.5);
    }

    #[test]
    fn zmod_pace_colors_match_expected_channels() {
        assert_eq!(zmod_rival_color(99.0, 98.0), [0.0, 1.0, 1.0, 1.0]);
        assert_eq!(zmod_rival_color(98.0, 99.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(zmod_rival_color(98.25, 98.0), [0.75, 0.75, 1.0, 1.0]);
        assert_eq!(zmod_rival_color(98.0, 98.25), [1.0, 0.25, 0.75, 1.0]);

        let ahead = zmod_pacemaker_color(101.0, 100.0);
        assert!((ahead[0] - 0.99).abs() <= 1e-6);
        assert!((ahead[1] - 0.51).abs() <= 1e-6);
        assert_eq!(ahead[2], 1.0);
        assert_eq!(ahead[3], 1.0);
        assert_eq!(
            zmod_pacemaker_color(10025.0, 10000.0),
            [0.75, 0.75, 1.0, 1.0]
        );
        assert_eq!(
            zmod_pacemaker_color(10000.0, 10025.0),
            [1.0, 0.25, 0.75, 1.0]
        );
    }

    #[test]
    fn zmod_combo_glow_color_interpolates_sine_phase() {
        fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
            for i in 0..4 {
                assert!(
                    (actual[i] - expected[i]).abs() <= 1e-6,
                    "channel {i}: {} != {}",
                    actual[i],
                    expected[i]
                );
            }
        }

        let color1 = [0.0, 0.2, 0.4, 1.0];
        let color2 = [1.0, 0.6, 0.0, 1.0];

        assert_rgba_close(
            zmod_combo_glow_color(color1, color2, 0.0),
            [0.5, 0.4, 0.2, 1.0],
        );
        assert_rgba_close(
            zmod_combo_glow_color(color1, color2, 0.2),
            [1.0, 0.6, 0.0, 1.0],
        );
        assert_rgba_close(
            zmod_combo_glow_color(color1, color2, 0.6),
            [0.0, 0.2, 0.4, 1.0],
        );
    }

    #[test]
    fn zmod_combo_grade_colors_match_palettes() {
        fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
            for i in 0..4 {
                assert!(
                    (actual[i] - expected[i]).abs() <= 1e-6,
                    "channel {i}: {} != {}",
                    actual[i],
                    expected[i]
                );
            }
        }

        let (fa1, fa2) = zmod_combo_glow_pair(JudgeGrade::Fantastic, false);
        assert_rgba_close(fa1, [200.0 / 255.0, 1.0, 1.0, 1.0]);
        assert_rgba_close(fa2, [107.0 / 255.0, 240.0 / 255.0, 1.0, 1.0]);
        let (excellent1, excellent2) = zmod_combo_glow_pair(JudgeGrade::Excellent, false);
        assert_rgba_close(excellent1, [253.0 / 255.0, 1.0, 201.0 / 255.0, 1.0]);
        assert_rgba_close(
            excellent2,
            [253.0 / 255.0, 219.0 / 255.0, 133.0 / 255.0, 1.0],
        );
        let (decent1, decent2) = zmod_combo_glow_pair(JudgeGrade::Decent, false);
        assert_eq!(decent1, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(decent2, [1.0, 1.0, 1.0, 1.0]);
        assert_rgba_close(
            zmod_combo_solid_color(JudgeGrade::Excellent, false),
            [226.0 / 255.0, 156.0 / 255.0, 24.0 / 255.0, 1.0],
        );
        assert_eq!(
            zmod_combo_solid_color(JudgeGrade::Miss, false),
            [1.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(
            zmod_combo_solid_color(JudgeGrade::Decent, false),
            [1.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn zmod_combo_quint_uses_fa_plus_palette() {
        fn assert_rgba_close(actual: [f32; 4], expected: [f32; 4]) {
            for i in 0..4 {
                assert!(
                    (actual[i] - expected[i]).abs() <= 1e-6,
                    "channel {i}: {} != {}",
                    actual[i],
                    expected[i]
                );
            }
        }

        let (quint1, quint2) = zmod_combo_glow_pair(JudgeGrade::Fantastic, true);
        assert_rgba_close(quint1, [247.0 / 255.0, 192.0 / 255.0, 254.0 / 255.0, 1.0]);
        assert_rgba_close(quint2, [233.0 / 255.0, 40.0 / 255.0, 1.0, 1.0]);
        assert_rgba_close(
            zmod_combo_solid_color(JudgeGrade::Fantastic, true),
            [233.0 / 255.0, 40.0 / 255.0, 1.0, 1.0],
        );
    }

    #[test]
    fn zmod_combo_quint_active_requires_fa_plus_and_only_w0_hits() {
        let quint = timing::WindowCounts {
            w0: 3,
            ..timing::WindowCounts::default()
        };
        assert!(zmod_combo_quint_active(true, quint));
        assert!(!zmod_combo_quint_active(false, quint));

        let with_w1 = timing::WindowCounts { w1: 1, ..quint };
        assert!(!zmod_combo_quint_active(true, with_w1));

        let with_miss = timing::WindowCounts { miss: 1, ..quint };
        assert!(!zmod_combo_quint_active(true, with_miss));
    }

    #[test]
    fn zmod_resolved_mini_indicator_mode_uses_legacy_fallbacks() {
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::RivalScoring, true, true),
            MiniIndicatorMode::RivalScoring
        );
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::None, true, true),
            MiniIndicatorMode::SubtractiveScoring
        );
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::None, false, true),
            MiniIndicatorMode::Pacemaker
        );
        assert_eq!(
            zmod_resolved_mini_indicator_mode(MiniIndicatorMode::None, false, false),
            MiniIndicatorMode::None
        );
    }

    #[test]
    fn zmod_resolved_combo_color_gates_full_combo_rainbow() {
        let params = ZmodComboColorParams {
            style: ZmodComboColorStyle::Rainbow,
            full_combo_mode: true,
            combo: 10,
            full_combo_grade: Some(JudgeGrade::Decent),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            quint_active: false,
            elapsed_s: 0.0,
        };
        assert_eq!(zmod_resolved_combo_color(params), [1.0, 1.0, 1.0, 1.0]);

        let active = ZmodComboColorParams {
            full_combo_grade: Some(JudgeGrade::Great),
            ..params
        };
        assert_eq!(
            zmod_resolved_combo_color(active),
            zmod_combo_rainbow_color(0.0, false, 10)
        );
    }

    #[test]
    fn zmod_resolved_combo_color_uses_current_or_full_grade() {
        let current = ZmodComboColorParams {
            style: ZmodComboColorStyle::Solid,
            full_combo_mode: false,
            combo: 0,
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Great),
            quint_active: false,
            elapsed_s: 0.0,
        };
        assert_eq!(
            zmod_static_combo_color(current),
            zmod_combo_solid_color(JudgeGrade::Great, false)
        );

        let full = ZmodComboColorParams {
            full_combo_mode: true,
            quint_active: true,
            ..current
        };
        assert_eq!(
            zmod_resolved_combo_color(full),
            zmod_combo_solid_color(JudgeGrade::Fantastic, true)
        );
    }

    #[test]
    fn zmod_indicator_default_color_uses_judgment_thresholds() {
        assert_eq!(
            zmod_indicator_default_color(96.0),
            [33.0 / 255.0, 204.0 / 255.0, 232.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(89.0),
            [226.0 / 255.0, 156.0 / 255.0, 24.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(80.0),
            [102.0 / 255.0, 201.0 / 255.0, 85.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(68.0),
            [180.0 / 255.0, 92.0 / 255.0, 1.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_default_color(67.99),
            [1.0, 48.0 / 255.0, 48.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn zmod_indicator_detailed_color_uses_expanded_thresholds() {
        assert_eq!(zmod_indicator_detailed_color(99.0), [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(
            zmod_indicator_detailed_color(98.0),
            [37.0 / 255.0, 110.0 / 255.0, 206.0 / 255.0, 1.0]
        );
        assert_eq!(zmod_indicator_detailed_color(96.0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(
            zmod_indicator_detailed_color(94.0),
            [253.0 / 255.0, 163.0 / 255.0, 7.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_detailed_color(90.0),
            [121.0 / 255.0, 169.0 / 255.0, 1.0 / 255.0, 1.0]
        );
        assert_eq!(
            zmod_indicator_detailed_color(85.0),
            [185.0 / 255.0, 50.0 / 255.0, 226.0 / 255.0, 1.0]
        );
        assert_eq!(zmod_indicator_detailed_color(84.99), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn zmod_combo_rainbow_color_applies_scroll_combo_offset() {
        assert_eq!(
            zmod_combo_rainbow_color(0.0, false, 0),
            [1.0, 0.0, 0.0, 1.0]
        );

        let scrolled = zmod_combo_rainbow_color(0.0, true, 10);
        assert!((scrolled[0] - 1.0).abs() <= 1e-6);
        assert!((scrolled[1] - 0.78).abs() <= 1e-6);
        assert_eq!(scrolled[2], 0.0);
        assert_eq!(scrolled[3], 1.0);

        let cyanish = zmod_combo_rainbow_color(1.0, false, 0);
        assert_eq!(cyanish[0], 0.0);
        assert_eq!(cyanish[1], 1.0);
        assert!((cyanish[2] - 0.1).abs() <= 1e-6);
        assert_eq!(cyanish[3], 1.0);
    }

    #[test]
    fn zmod_mini_indicator_output_handles_subtractive_count_and_percent() {
        let params = ZmodMiniIndicatorParams {
            mode: MiniIndicatorMode::SubtractiveScoring,
            color_style: MiniIndicatorColorStyle::Default,
            subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
            score_type: MiniIndicatorScoreType::Itg,
            combo_color: [0.2, 0.3, 0.4, 1.0],
            is_failing: false,
            life: 1.0,
            rival_score_percent: 0.0,
            target_score_percent: 0.0,
            stream_completion: None,
        };
        let count = MiniIndicatorProgress {
            judged_any: true,
            kept_percent: 99.0,
            lost_percent: 1.0,
            w2: 4,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_mini_indicator_output(&count, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::NegativeInt(4),
                color: rgba8(0xff, 0x55, 0xcc),
            })
        );

        let combo_params = ZmodMiniIndicatorParams {
            color_style: MiniIndicatorColorStyle::Combo,
            ..params
        };
        assert_eq!(
            zmod_mini_indicator_output(&count, combo_params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::NegativeInt(4),
                color: [0.2, 0.3, 0.4, 1.0],
            })
        );

        let forced_percent = MiniIndicatorProgress { w3: 1, ..count };
        assert_eq!(
            zmod_mini_indicator_output(&forced_percent, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 1.0,
                    negative: true,
                },
                color: zmod_indicator_default_color(99.0),
            })
        );

        let failing_params = ZmodMiniIndicatorParams {
            is_failing: true,
            ..params
        };
        assert_eq!(
            zmod_mini_indicator_output(&count, failing_params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 1.0,
                    negative: true,
                },
                color: zmod_indicator_default_color(99.0),
            })
        );

        let points_params = ZmodMiniIndicatorParams {
            subtractive_display: MiniIndicatorSubtractiveDisplay::Points,
            color_style: MiniIndicatorColorStyle::Detailed,
            ..params
        };
        let points = MiniIndicatorProgress {
            judged_any: true,
            kept_percent: 94.0,
            current_possible_dp: 20,
            actual_dp: 16,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_mini_indicator_output(&points, points_params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::NegativeInt(4),
                color: zmod_indicator_detailed_color(94.0),
            })
        );
    }

    #[test]
    fn zmod_mini_indicator_output_handles_rival_pacemaker_and_stream() {
        let mut params = ZmodMiniIndicatorParams {
            mode: MiniIndicatorMode::RivalScoring,
            color_style: MiniIndicatorColorStyle::Default,
            subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
            score_type: MiniIndicatorScoreType::Itg,
            combo_color: [0.2, 0.3, 0.4, 1.0],
            is_failing: false,
            life: 1.0,
            rival_score_percent: 99.0,
            target_score_percent: 98.0,
            stream_completion: Some(0.95),
        };
        let progress = MiniIndicatorProgress {
            judged_any: true,
            current_score_percent: 98.0,
            current_possible_ratio: 0.5,
            ..MiniIndicatorProgress::default()
        };
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 48.5,
                    negative: false,
                },
                color: zmod_rival_color(98.0, 49.5),
            })
        );

        params.mode = MiniIndicatorMode::PaceScoring;
        assert_eq!(
            zmod_mini_indicator_output(
                &MiniIndicatorProgress {
                    judged_any: true,
                    pace_percent: 97.25,
                    ..MiniIndicatorProgress::default()
                },
                params,
            ),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(97.25),
                color: zmod_indicator_default_color(97.25),
            })
        );

        params.mode = MiniIndicatorMode::Pacemaker;
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::SignedPercent {
                    value: 49.0,
                    negative: false,
                },
                color: zmod_pacemaker_color(9800.0, 4900.0),
            })
        );

        params.mode = MiniIndicatorMode::StreamProg;
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(95.0),
                color: [0.0, 1.0, 0.5, 1.0],
            })
        );
        params.stream_completion = Some(0.3);
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(30.0),
                color: zmod_stream_prog_color(0.3),
            })
        );
        params.stream_completion = Some(1.2);
        assert_eq!(
            zmod_mini_indicator_output(&progress, params),
            Some(ZmodMiniIndicatorOutput {
                text: ZmodMiniIndicatorText::Percent(100.0),
                color: zmod_stream_prog_color(1.2),
            })
        );
    }

    #[test]
    fn zmod_stream_prog_completion_counts_stream_beats_only() {
        let segs = [
            StreamSegment {
                start: 0,
                end: 2,
                is_break: false,
            },
            StreamSegment {
                start: 2,
                end: 4,
                is_break: true,
            },
            StreamSegment {
                start: 4,
                end: 6,
                is_break: false,
            },
        ];

        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, -1.0),
            Some(0.0)
        );
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, f32::NAN),
            Some(0.0)
        );
        assert_eq!(zmod_stream_prog_completion_for_beat(0.0, &segs, 1.0), None);
        assert_eq!(zmod_stream_prog_completion_for_beat(4.0, &[], 1.0), None);
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, 3.0),
            Some(0.25)
        );
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, 19.0),
            Some(0.75)
        );
        assert_eq!(
            zmod_stream_prog_completion_for_beat(4.0, &segs, 23.0),
            Some(1.0)
        );
    }

    #[test]
    fn error_bar_text_scalable_zoom_matches_sl_fork_curve_at_default_threshold() {
        fn assert_close(actual: f32, expected: f32) {
            assert!(
                (actual - expected).abs() <= 0.0001,
                "{actual} != {expected}"
            );
        }

        let w2_ms = timing::TimingProfile::default_itg_with_fa_plus().windows_s[0] * 1000.0;
        let smaller_white_ms = timing::FA_PLUS_W010_MS;
        let w1_ms = timing::FA_PLUS_W0_MS;
        let inner_mid_ms = (smaller_white_ms + w1_ms) * 0.5;
        let fantastic_mid_ms = (w1_ms + w2_ms) * 0.5;

        assert_close(
            error_bar_text_scalable_zoom(inner_mid_ms, 10.0, w2_ms),
            0.35,
        );
        assert_close(
            error_bar_text_scalable_zoom(fantastic_mid_ms, 10.0, w2_ms),
            0.4,
        );
        assert_close(error_bar_text_scalable_zoom(w2_ms + 1.0, 10.0, w2_ms), 0.45);
    }

    #[test]
    fn player_metric_y_applies_notefield_offset_after_reverse_mix() {
        const EPS: f32 = 1e-5;
        let center_y = 240.0;
        let offset_y = -22.0;

        let standard = player_metric_y(center_y, offset_y, 0.0, -90.0, 90.0);
        let reverse = player_metric_y(center_y, offset_y, 1.0, -90.0, 90.0);
        let split = player_metric_y(center_y, offset_y, 0.5, -90.0, 90.0);

        assert!((standard - 128.0).abs() <= EPS);
        assert!((reverse - 308.0).abs() <= EPS);
        assert!((split - 218.0).abs() <= EPS);
    }

    #[test]
    fn notefield_view_proj_rejects_invalid_screen_sizes() {
        assert!(notefield_view_proj(0.0, 480.0, 320.0, 240.0, 0.0, 0.0, false).is_none());
        assert!(notefield_view_proj(640.0, f32::NAN, 320.0, 240.0, 0.0, 0.0, false).is_none());
    }

    #[test]
    fn notefield_view_proj_returns_finite_matrix_for_flat_field() {
        let matrix = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.0, 0.0, false)
            .expect("valid notefield projection");

        assert!(matrix.to_cols_array().into_iter().all(f32::is_finite));
    }

    #[test]
    fn notefield_view_proj_maps_centered_world_coords_to_clip_space() {
        let matrix = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.0, 0.0, false)
            .expect("valid notefield projection");
        let center = matrix.project_point3(glam::Vec3::ZERO);
        let top_right = matrix.project_point3(glam::Vec3::new(320.0, 240.0, 0.0));

        assert!(center.x.abs() <= 1e-5);
        assert!(center.y.abs() <= 1e-5);
        assert!((top_right.x - 1.0).abs() <= 1e-5);
        assert!((top_right.y - 1.0).abs() <= 1e-5);
    }

    #[test]
    fn notefield_view_proj_changes_with_tilt_skew_and_reverse() {
        let flat = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.0, 0.0, false)
            .expect("flat projection");
        let tilted = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.5, 0.3, false)
            .expect("tilted projection");
        let reverse = notefield_view_proj(640.0, 480.0, 320.0, 240.0, 0.5, 0.3, true)
            .expect("reverse projection");

        assert_ne!(flat.to_cols_array(), tilted.to_cols_array());
        assert_ne!(tilted.to_cols_array(), reverse.to_cols_array());
    }

    #[test]
    fn hud_y_only_uses_reverse_branch_for_full_reverse() {
        let normal_y = 100.0;
        let reverse_y = 200.0;
        let centered_y = 300.0;
        assert!((hud_y(normal_y, reverse_y, centered_y, false, 0.3) - 160.0).abs() <= 1e-6);
        assert!((hud_y(normal_y, reverse_y, centered_y, true, 0.3) - 230.0).abs() <= 1e-6);
    }

    fn default_zmod_layout_params() -> ZmodLayoutParams {
        ZmodLayoutParams {
            judgment_height: 40.0,
            has_error_bar: true,
            has_judgment_texture: true,
            error_bar_up: false,
            has_measure_counter: false,
            measure_counter_up: false,
            broken_run: false,
            mini_indicator_position: LayoutMiniIndicatorPosition::Default,
        }
    }

    #[test]
    fn hud_layout_offsets_apply_independently() {
        let params = HudLayoutParams {
            zmod: default_zmod_layout_params(),
            has_judgment_texture: true,
            error_bar_up: false,
            error_bar_offset: 25.0,
        };
        let base = hud_layout_ys(100.0, 160.0, false, HudLayoutOffsets::default(), params);
        let moved_judgment = hud_layout_ys(
            100.0,
            160.0,
            false,
            HudLayoutOffsets {
                judgment_extra_y: 25.0,
                ..HudLayoutOffsets::default()
            },
            params,
        );
        assert_eq!(moved_judgment.judgment_y, 125.0);
        assert_eq!(moved_judgment.zmod_layout.combo_y, base.zmod_layout.combo_y);
        assert_eq!(
            moved_judgment.zmod_layout.subtractive_scoring_y,
            base.zmod_layout.subtractive_scoring_y
        );
        assert_eq!(moved_judgment.error_bar_y, base.error_bar_y);

        let moved_combo = hud_layout_ys(
            100.0,
            160.0,
            false,
            HudLayoutOffsets {
                combo_extra_y: -30.0,
                ..HudLayoutOffsets::default()
            },
            params,
        );
        assert_eq!(moved_combo.judgment_y, base.judgment_y);
        assert_eq!(
            moved_combo.zmod_layout.combo_y,
            base.zmod_layout.combo_y - 30.0
        );
        assert_eq!(moved_combo.error_bar_y, base.error_bar_y);

        let moved_error_bar = hud_layout_ys(
            100.0,
            160.0,
            false,
            HudLayoutOffsets {
                error_bar_extra_y: 18.0,
                ..HudLayoutOffsets::default()
            },
            params,
        );
        assert_eq!(moved_error_bar.judgment_y, base.judgment_y);
        assert_eq!(
            moved_error_bar.zmod_layout.combo_y,
            base.zmod_layout.combo_y
        );
        assert_eq!(moved_error_bar.error_bar_y, base.error_bar_y + 18.0);
    }

    #[test]
    fn zmod_layout_places_measure_and_subtractive_rows() {
        let mut params = default_zmod_layout_params();
        params.has_measure_counter = true;
        params.measure_counter_up = true;
        params.broken_run = true;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);

        assert_eq!(layout.measure_counter_y, Some(56.0));
        assert_eq!(layout.subtractive_scoring_y, 143.0);
        assert_eq!(layout.subtractive_scoring_addx, 0.0);
        assert_eq!(layout.combo_y, 171.0);

        params.mini_indicator_position = LayoutMiniIndicatorPosition::UnderUpArrow;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);
        assert_eq!(layout.subtractive_scoring_y, 76.0);
        assert_eq!(layout.subtractive_scoring_addx, -60.0);
    }

    #[test]
    fn zmod_layout_preserves_legacy_row_reservation_branches() {
        let mut params = default_zmod_layout_params();
        params.mini_indicator_position = LayoutMiniIndicatorPosition::UnderUpArrow;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);
        assert_eq!(layout.measure_counter_y, None);
        assert_eq!(layout.subtractive_scoring_y, 72.0);
        assert_eq!(layout.subtractive_scoring_addx, 0.0);
        assert_eq!(layout.combo_y, 160.0);

        params = default_zmod_layout_params();
        params.has_measure_counter = true;
        params.measure_counter_up = false;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);
        assert_eq!(layout.measure_counter_y, Some(143.0));
        assert_eq!(layout.subtractive_scoring_y, 72.0);
        assert_eq!(layout.combo_y, 176.0);

        params = default_zmod_layout_params();
        params.has_measure_counter = true;
        params.measure_counter_up = true;
        params.has_judgment_texture = false;
        params.error_bar_up = true;
        let layout = zmod_layout_ys(100.0, 160.0, false, params);
        assert_eq!(layout.measure_counter_y, Some(72.0));
        assert_eq!(layout.subtractive_scoring_y, 128.0);
    }

    #[test]
    fn hud_layout_error_bar_matches_legacy_judgment_branches() {
        let mut params = HudLayoutParams {
            zmod: default_zmod_layout_params(),
            has_judgment_texture: true,
            error_bar_up: false,
            error_bar_offset: 25.0,
        };

        let reverse = hud_layout_ys(100.0, 160.0, true, HudLayoutOffsets::default(), params);
        assert_eq!(reverse.error_bar_y, 125.0);
        assert_eq!(reverse.error_bar_max_h, 10.0);

        params.error_bar_up = true;
        let up = hud_layout_ys(100.0, 160.0, false, HudLayoutOffsets::default(), params);
        assert_eq!(up.error_bar_y, 75.0);
        assert_eq!(up.error_bar_max_h, 10.0);

        params.has_judgment_texture = false;
        params.zmod.has_judgment_texture = false;
        let no_judgment = hud_layout_ys(100.0, 160.0, false, HudLayoutOffsets::default(), params);
        assert_eq!(no_judgment.error_bar_y, 100.0);
        assert_eq!(no_judgment.error_bar_max_h, 30.0);
    }

    #[test]
    fn combo_actor_zoom_matches_itgmania_player_mini_formula() {
        assert!((combo_actor_zoom(0.0) - 1.0).abs() <= 1e-6);
        assert!((combo_actor_zoom(1.0) - 0.5).abs() <= 1e-6);
        assert!((combo_actor_zoom(0.5) - 0.5_f32.sqrt()).abs() <= 1e-6);
        assert!((combo_actor_zoom(-1.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_actor_zoom_matches_itgmania_player_mini_formula_without_judgment_back() {
        assert!((judgment_actor_zoom(0.0, false, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.0, false, 0.0, 0.0) - 0.5).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.5, false, 0.0, 0.0) - 0.5_f32.sqrt()).abs() <= 1e-6);
        assert!((judgment_actor_zoom(-1.0, false, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, -1.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, 1.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, false, -1.0, 1.0) - 1.0).abs() <= 1e-6);
        for &mini in &[-1.0_f32, 0.0, 0.25, 0.5, 1.0, 1.5] {
            assert!(
                (judgment_actor_zoom(mini, false, -1.0, 0.0) - combo_actor_zoom(mini)).abs()
                    <= 1e-6
            );
        }
    }

    #[test]
    fn judgment_actor_zoom_matches_arrow_cloud_judgment_back_formula() {
        assert!((judgment_actor_zoom(0.35, true, 0.0, 0.0) - 0.825).abs() <= 1e-6);
        assert!((judgment_actor_zoom(1.5, true, 0.0, 0.0) - 0.35).abs() <= 1e-6);
        assert!((judgment_actor_zoom(-1.0, true, 0.0, 0.0) - 1.0).abs() <= 1e-6);
        assert!((judgment_actor_zoom(0.0, true, -1.0, 0.0) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn judgment_tilt_thresholds_deadzone_and_cap() {
        let params = JudgmentTiltParams {
            enabled: true,
            grade: JudgeGrade::Fantastic,
            time_error_ms: 5.0,
            min_threshold_ms: 5.0,
            max_threshold_ms: 20.0,
            multiplier: 1.0,
        };
        assert_eq!(judgment_tilt_rotation_deg(params), 0.0);
        assert!(
            (judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 10.0,
                ..params
            }) + 1.5)
                .abs()
                <= 1e-6
        );
        assert!(
            (judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 40.0,
                ..params
            }) + 4.5)
                .abs()
                <= 1e-6
        );
        assert!(
            (judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 40.0,
                max_threshold_ms: 30.0,
                ..params
            }) + 7.5)
                .abs()
                <= 1e-6
        );
        assert_eq!(
            judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 40.0,
                min_threshold_ms: 30.0,
                max_threshold_ms: 5.0,
                ..params
            }),
            0.0
        );
    }

    #[test]
    fn judgment_tilt_keeps_early_late_direction() {
        let params = JudgmentTiltParams {
            enabled: true,
            grade: JudgeGrade::Fantastic,
            time_error_ms: -10.0,
            min_threshold_ms: 0.0,
            max_threshold_ms: 50.0,
            multiplier: 1.0,
        };
        assert!(judgment_tilt_rotation_deg(params) > 0.0);
        assert!(
            judgment_tilt_rotation_deg(JudgmentTiltParams {
                time_error_ms: 10.0,
                ..params
            }) < 0.0
        );
    }

    fn tap_rows_params(time_error_ms: f32) -> TapJudgmentRowsParams {
        TapJudgmentRowsParams {
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W0),
            time_error_ms,
            frame_rows: 7,
            show_fa_plus_window: false,
            fa_plus_10ms_blue_window: false,
            split_15_10ms: false,
            custom_fantastic_window: false,
        }
    }

    #[test]
    fn tap_judgment_rows_overlay_white_for_split_15_10_hits() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                split_15_10ms: true,
                ..tap_rows_params(12.0)
            }),
            (0, Some(1))
        );
    }

    #[test]
    fn tap_judgment_rows_keep_plain_blue_when_split_is_off() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
    }

    #[test]
    fn tap_judgment_rows_show_fa_plus_uses_white_w1_without_10ms_split() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                window: Some(TimingWindow::W1),
                time_error_ms: 16.0,
                show_fa_plus_window: true,
                ..tap_rows_params(0.0)
            }),
            (1, None)
        );
    }

    #[test]
    fn tap_judgment_rows_use_10ms_blue_window() {
        let blue = TapJudgmentRowsParams {
            show_fa_plus_window: true,
            fa_plus_10ms_blue_window: true,
            time_error_ms: timing::FA_PLUS_W010_MS,
            ..tap_rows_params(0.0)
        };
        let white = TapJudgmentRowsParams {
            time_error_ms: 12.0,
            ..blue
        };

        assert_eq!(tap_judgment_rows(blue), (0, None));
        assert_eq!(tap_judgment_rows(white), (1, None));
    }

    #[test]
    fn tap_judgment_rows_split_keeps_blue_base_above_10ms() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: true,
                split_15_10ms: true,
                ..tap_rows_params(12.0)
            }),
            (0, Some(1))
        );
    }

    #[test]
    fn tap_judgment_rows_ignore_split_without_fa_plus_window() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                split_15_10ms: true,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
    }

    #[test]
    fn tap_judgment_rows_defer_to_custom_window_over_fixed_split() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                window: Some(TimingWindow::W1),
                time_error_ms: 14.0,
                show_fa_plus_window: true,
                split_15_10ms: true,
                custom_fantastic_window: true,
                ..tap_rows_params(0.0)
            }),
            (1, None)
        );
    }

    #[test]
    fn tap_judgment_rows_shift_non_fantastic_rows_for_seven_row_assets() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                grade: JudgeGrade::Excellent,
                window: Some(TimingWindow::W2),
                time_error_ms: 18.0,
                ..tap_rows_params(0.0)
            }),
            (2, None)
        );
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                grade: JudgeGrade::Great,
                window: Some(TimingWindow::W3),
                time_error_ms: 60.0,
                ..tap_rows_params(0.0)
            }),
            (3, None)
        );
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                grade: JudgeGrade::Miss,
                window: None,
                time_error_ms: 180.0,
                ..tap_rows_params(0.0)
            }),
            (6, None)
        );
    }

    #[test]
    fn tap_judgment_rows_keep_six_row_assets_unsplit() {
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                split_15_10ms: true,
                frame_rows: 6,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: true,
                frame_rows: 6,
                ..tap_rows_params(12.0)
            }),
            (0, None)
        );
        assert_eq!(
            tap_judgment_rows(TapJudgmentRowsParams {
                grade: JudgeGrade::Excellent,
                window: Some(TimingWindow::W1),
                time_error_ms: 18.0,
                show_fa_plus_window: true,
                split_15_10ms: true,
                frame_rows: 6,
                ..tap_rows_params(0.0)
            }),
            (1, None)
        );
    }

    #[test]
    fn average_error_bar_mini_scale_shrinks_with_mini() {
        assert!((average_error_bar_mini_scale(0.0) - 1.1).abs() <= 1e-6);
        assert!((average_error_bar_mini_scale(1.0) - 0.555).abs() <= 1e-6);
        assert!((average_error_bar_mini_scale(-0.5) - 1.3725).abs() <= 1e-6);
        assert_eq!(average_error_bar_mini_scale(4.0), 0.0);
    }

    #[test]
    fn held_miss_zoom_pops_then_fades() {
        assert_eq!(held_miss_zoom(0.0, 0.0), (0.8, 0.75));
        assert!((held_miss_zoom(0.05, 0.0).0 - 0.7625).abs() <= 1e-6);
        assert_eq!(held_miss_zoom(0.2, 0.0), (0.75, 0.75));
        assert_eq!(held_miss_zoom(0.4, 0.0), (0.5625, 0.5625));
        let faded = held_miss_zoom(0.5, 0.0);
        assert!(faded.0.abs() <= 1e-6);
        assert!(faded.1.abs() <= 1e-6);
        assert_eq!(held_miss_zoom(0.2, 1.0), (0.375, 0.375));
        assert_eq!(held_miss_zoom(0.2, -1.0), (1.125, 1.125));
    }

    #[test]
    fn hold_tail_cap_bounds_join_at_body_bottom_for_normal_scroll() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        let (top, bottom) = hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(96.0))
            .expect("cap should draw");
        assert_eq!((top, bottom), (96.0, 120.0));
    }

    #[test]
    fn hold_tail_cap_bounds_tracks_visible_body_inside_cap_range() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        let (top, bottom) = hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(90.0))
            .expect("cap should stay attached to the visible body edge");
        assert_eq!((top, bottom), (90.0, 114.0));
    }

    #[test]
    fn hold_tail_cap_bounds_falls_back_when_body_is_below_tail_anchor() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(104.0), Some(160.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn hold_tail_cap_bounds_skip_when_body_does_not_reach_tail() {
        let body_tail_y = 100.0;
        let cap_height = 24.0;
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(20.0), Some(70.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, Some(140.0), Some(200.0)),
            Some((100.0, 124.0))
        );
        assert_eq!(
            hold_tail_cap_bounds(body_tail_y, cap_height, None, Some(95.0)),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn collapsed_hold_body_uses_tail_cap_fallback_bounds() {
        let body_top = 120.0;
        let body_bottom = 120.0;
        let natural_top = 100.0;
        let natural_bottom = 100.0;
        assert_eq!(
            clipped_hold_body_bounds(body_top, body_bottom, natural_top, natural_bottom),
            None
        );
        assert_eq!(
            hold_tail_cap_bounds(natural_bottom, 24.0, None, None),
            Some((100.0, 124.0))
        );
    }

    #[test]
    fn clipped_hold_body_bounds_rejects_zero_height_span() {
        assert_eq!(clipped_hold_body_bounds(100.0, 100.0, 100.0, 100.0), None);
    }

    #[test]
    fn hold_body_bottom_for_tail_cap_joins_tail_edge_with_overlap() {
        assert_eq!(hold_body_bottom_for_tail_cap(140.0, 100.0, 0.0), 140.0);
        assert_eq!(hold_body_bottom_for_tail_cap(140.0, 100.0, 24.0), 101.0);
        assert_eq!(hold_body_bottom_for_tail_cap(99.5, 100.0, 24.0), 101.0);
        assert_eq!(hold_body_bottom_for_tail_cap(80.0, 100.0, 24.0), 80.0);
    }

    #[test]
    fn collapsed_hold_draw_span_still_draws_caps() {
        assert_eq!(hold_draw_span(120.0, 120.0, 480.0), Some((120.0, 120.0)));
    }

    #[test]
    fn hold_draw_span_uses_legacy_overscan_window() {
        assert_eq!(hold_draw_span(-300.0, -250.0, 480.0), None);
        assert_eq!(hold_draw_span(700.0, 720.0, 480.0), None);
        assert_eq!(hold_draw_span(-450.0, 100.0, 480.0), Some((-400.0, 100.0)));
        assert_eq!(hold_draw_span(100.0, 920.0, 480.0), Some((100.0, 880.0)));
        assert_eq!(hold_draw_span(f32::NAN, 120.0, 480.0), None);
    }

    #[test]
    fn tiny_hold_body_repeat_uses_mesh_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 0.25);
        assert!(budget >= 3602);
        assert!(!allow_legacy);
    }

    #[test]
    fn normal_hold_body_repeat_keeps_legacy_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(900.0, 64.0);
        assert_eq!(budget, 2048);
        assert!(allow_legacy);
    }

    #[test]
    fn long_small_hold_body_repeat_uses_mesh_budget() {
        let (budget, allow_legacy) = hold_body_segment_budget(2000.0, 2.0);
        assert_eq!(budget, 2048);
        assert!(!allow_legacy);
    }

    #[test]
    fn hold_strip_row_3d_preserves_row_z() {
        let row = hold_strip_row_3d(
            [64.0, 128.0, 12.5],
            [0.0, 16.0],
            8.0,
            0.0,
            1.0,
            0.5,
            [1.0; 4],
        );
        assert!((row[0].pos[2] - 12.5).abs() <= 1e-6);
        assert!((row[1].pos[2] - 12.5).abs() <= 1e-6);
    }

    #[test]
    fn hold_strip_quad_matches_legacy_triangle_order() {
        let top = hold_strip_row_3d([0.0, 0.0, 0.0], [0.0, 16.0], 1.0, 0.0, 1.0, 0.0, [1.0; 4]);
        let bottom = hold_strip_row_3d([0.0, 10.0, 0.0], [0.0, 16.0], 1.0, 0.0, 1.0, 1.0, [1.0; 4]);
        let quad = hold_strip_quad(top, bottom);
        assert_eq!(quad[0].pos, top[0].pos);
        assert_eq!(quad[1].pos, top[1].pos);
        assert_eq!(quad[2].pos, bottom[1].pos);
        assert_eq!(quad[3].pos, top[0].pos);
        assert_eq!(quad[4].pos, bottom[1].pos);
        assert_eq!(quad[5].pos, bottom[0].pos);
    }

    #[test]
    fn hold_strip_actor_carries_depth_test_flag() {
        let actor = hold_strip_actor(
            Arc::from("hold.png"),
            Arc::from([]),
            BlendMode::Alpha,
            true,
            42,
        );
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                align: [0.0, 0.0],
                size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                glow: [1.0, 1.0, 1.0, 0.0],
                geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
                depth_test: true,
                ..
            }
        ));
    }

    #[test]
    fn hold_strip_glow_actor_uses_texture_mask_pass() {
        let actor = hold_strip_glow_actor(Arc::from("hold.png"), Arc::from([]), true, 43);
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                align: [0.0, 0.0],
                size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                tint: [1.0, 1.0, 1.0, 0.0],
                glow: [1.0, 1.0, 1.0, 1.0],
                geom_cache_key: deadlib_render::INVALID_TMESH_CACHE_KEY,
                depth_test: true,
                ..
            }
        ));
    }

    #[test]
    fn actor_with_world_z_updates_textured_mesh_depth() {
        let actor = hold_strip_actor(
            Arc::from("hold.png"),
            Arc::from([]),
            BlendMode::Alpha,
            false,
            42,
        );
        let actor = actor_with_world_z(actor, 12.5);
        assert!(matches!(
            actor,
            Actor::TexturedMesh {
                world_z,
                ..
            } if (world_z - 12.5).abs() <= 1e-6
        ));
    }

    #[test]
    fn share_actor_range_drains_into_shared_frame() {
        let mut actors = vec![
            hold_strip_actor(
                Arc::from("a.png"),
                Arc::from([]),
                BlendMode::Alpha,
                false,
                1,
            ),
            hold_strip_actor(
                Arc::from("b.png"),
                Arc::from([]),
                BlendMode::Alpha,
                false,
                2,
            ),
        ];
        let shared = share_actor_range(&mut actors, 1).expect("range should be shared");
        assert_eq!(actors.len(), 2);
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].len(), 1);
        match &actors[1] {
            Actor::SharedFrame {
                size,
                children,
                blend,
                ..
            } => {
                assert!(matches!(size, [SizeSpec::Fill, SizeSpec::Fill]));
                assert!(Arc::ptr_eq(children, &shared[0]));
                assert_eq!(*blend, None);
            }
            _ => panic!("expected shared frame"),
        }
    }

    #[test]
    fn built_notefield_empty_has_no_actor_outputs() {
        let built = BuiltNotefield::empty(320.0);
        assert_eq!(built.layout_center_x, 320.0);
        assert!(built.field_actors.is_empty());
        assert!(built.judgment_actors.is_none());
        assert!(built.combo_actors.is_none());
    }

    #[test]
    fn tap_note_types_choose_noteskin_animation_parts() {
        assert!(matches!(
            tap_part_for_note_type(NoteType::Tap),
            NoteAnimPart::Tap
        ));
        assert!(matches!(
            tap_part_for_note_type(NoteType::Fake),
            NoteAnimPart::Fake
        ));
        assert!(matches!(
            tap_part_for_note_type(NoteType::Lift),
            NoteAnimPart::Lift
        ));
        assert_eq!(mine_part(), NoteAnimPart::Mine);
    }

    #[test]
    fn hold_note_types_choose_noteskin_animation_parts() {
        let hold = hold_parts_for_note_type(NoteType::Hold);
        assert_eq!(hold.head, NoteAnimPart::HoldHead);
        assert_eq!(hold.body, NoteAnimPart::HoldBody);
        assert_eq!(hold.topcap, NoteAnimPart::HoldTopCap);
        assert_eq!(hold.bottomcap, NoteAnimPart::HoldBottomCap);

        let roll = hold_parts_for_note_type(NoteType::Roll);
        assert_eq!(roll.head, NoteAnimPart::RollHead);
        assert_eq!(roll.body, NoteAnimPart::RollBody);
        assert_eq!(roll.topcap, NoteAnimPart::RollTopCap);
        assert_eq!(roll.bottomcap, NoteAnimPart::RollBottomCap);

        assert_eq!(hold_head_part_for_roll(false), NoteAnimPart::HoldHead);
        assert_eq!(hold_head_part_for_roll(true), NoteAnimPart::RollHead);
    }

    #[test]
    fn same_row_tap_replacement_selects_enabled_head() {
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, false, true, false, true),
            Some(TapReplacementHead {
                is_roll: false,
                part: NoteAnimPart::HoldHead
            })
        );
        assert_eq!(
            tap_replacement_head(NoteType::Lift, false, true, false, true, true),
            Some(TapReplacementHead {
                is_roll: true,
                part: NoteAnimPart::RollHead
            })
        );
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, true, true, true, true),
            Some(TapReplacementHead {
                is_roll: false,
                part: NoteAnimPart::HoldHead
            })
        );
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, true, true, true, false),
            Some(TapReplacementHead {
                is_roll: true,
                part: NoteAnimPart::RollHead
            })
        );
    }

    #[test]
    fn same_row_tap_replacement_ignores_disabled_or_nontap_notes() {
        assert_eq!(
            tap_replacement_head(NoteType::Tap, true, false, false, true, true),
            None
        );
        assert_eq!(
            tap_replacement_head(NoteType::Hold, true, true, true, true, true),
            None
        );
        assert_eq!(
            tap_replacement_head(NoteType::Fake, true, true, true, true, true),
            None
        );
    }

    #[test]
    fn bottom_cap_uv_window_matches_itg_add_to_tex_coord_progression() {
        let (v0, v1) = bottom_cap_uv_window(0.0, 1.0, 12.0, 24.0, false)
            .expect("partial cap should produce UVs");
        assert!((v0 - 0.5).abs() <= 1e-6);
        assert!((v1 - 1.0).abs() <= 1e-6);

        let (full_v0, full_v1) =
            bottom_cap_uv_window(0.0, 1.0, 24.0, 24.0, false).expect("full cap should produce UVs");
        assert!((full_v0 - 0.0).abs() <= 1e-6);
        assert!((full_v1 - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_honors_top_anchor_when_reverse() {
        let (v0, v1) = bottom_cap_uv_window(0.2, 0.8, 12.0, 24.0, true)
            .expect("top-anchored reverse path should produce UVs");
        assert!((v0 - 0.2).abs() <= 1e-6);
        assert!((v1 - 0.5).abs() <= 1e-6);
    }

    #[test]
    fn bottom_cap_uv_window_rejects_degenerate_inputs() {
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 0.0, 24.0, false), None);
        assert_eq!(bottom_cap_uv_window(0.0, 1.0, 24.0, 0.0, false), None);
    }

    #[test]
    fn hold_segment_pose_keeps_vertical_segments_unrotated() {
        let (center, length, rotation) = hold_segment_pose([32.0, 100.0], [32.0, 180.0]);
        assert_eq!(center, [32.0, 140.0]);
        assert!((length - 80.0).abs() <= 1e-6);
        assert!(rotation.abs() <= 1e-6);
    }

    #[test]
    fn hold_segment_pose_uses_diagonal_length_and_rotation() {
        let (center, length, rotation) = hold_segment_pose([0.0, 0.0], [30.0, 40.0]);
        assert_eq!(center, [15.0, 20.0]);
        assert!((length - 50.0).abs() <= 1e-6);
        assert!((rotation - 36.869_896).abs() <= 1e-5);
    }

    #[test]
    fn song_time_ns_helpers_convert_signed_deltas() {
        assert_eq!(song_time_ns_to_seconds(1_500_000_000), 1.5);
        assert_eq!(
            song_time_ns_delta_seconds(1_250_000_000, 2_000_000_000),
            -0.75
        );

        let wide_delta = song_time_ns_delta_seconds(i64::MAX, i64::MIN);
        assert!(wide_delta.is_finite());
        assert!(wide_delta > 1.0e10);
    }

    #[test]
    fn mine_hides_after_any_final_resolution() {
        assert!(!mine_hides_after_resolution(None));
        assert!(mine_hides_after_resolution(Some(MineResult::Hit)));
        assert!(mine_hides_after_resolution(Some(MineResult::Avoided)));
    }

    #[test]
    fn visible_note_window_uses_itg_rows_not_dense_rows() {
        let notes = vec![
            test_note_at_dense_row(0.0, 0),
            test_note_at_dense_row(4.0, 1),
        ];
        let note_indices = vec![0usize, 1usize];
        let mut visited = Vec::new();

        for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(3.5), beat_to_note_row(4.5))),
            |note_index| visited.push(note_index),
        );

        assert_eq!(note_itg_row(&notes[1]), beat_to_note_row(4.0));
        assert_eq!(visited, vec![1]);
    }

    #[test]
    fn visible_note_window_clamps_negative_track_rows() {
        let notes = vec![
            test_note_at_dense_row(-1.0, 0),
            test_note_at_dense_row(0.0, 1),
        ];
        let note_indices = vec![0usize, 1usize];
        let mut visited = Vec::new();

        for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(-2.0), beat_to_note_row(0.0))),
            |note_index| visited.push(note_index),
        );

        assert_eq!(visited, vec![1]);
    }

    #[test]
    fn visible_note_window_rejects_fully_negative_ranges() {
        let notes = vec![test_note_at_dense_row(-1.0, 0)];
        let note_indices = vec![0usize];
        let mut visited = Vec::new();

        for_each_visible_note_index(
            &note_indices,
            &notes,
            Some((beat_to_note_row(-4.0), beat_to_note_row(-2.0))),
            |note_index| visited.push(note_index),
        );

        assert!(visited.is_empty());
    }

    #[test]
    fn visible_hold_window_includes_holds_started_before_range() {
        let notes = vec![test_hold_at_beat(0.0, 8.0), test_hold_at_beat(12.0, 16.0)];
        let hold_indices = vec![0usize, 1usize];
        let visible_range = Some((beat_to_note_row(4.0), beat_to_note_row(5.0)));
        let mut visited = Vec::new();

        for_each_visible_hold_index(&hold_indices, &notes, visible_range, |note_index| {
            visited.push(note_index);
        });

        assert!(hold_overlaps_visible_window(0, &notes, visible_range));
        assert!(!hold_overlaps_visible_window(1, &notes, visible_range));
        assert_eq!(visited, vec![0]);
    }

    #[test]
    fn visible_hold_window_rejects_fully_negative_ranges() {
        let notes = vec![test_hold_at_beat(-4.0, -1.0)];
        let hold_indices = vec![0usize];
        let visible_range = Some((beat_to_note_row(-4.0), beat_to_note_row(-1.0)));
        let mut visited = Vec::new();

        for_each_visible_hold_index(&hold_indices, &notes, visible_range, |note_index| {
            visited.push(note_index);
        });

        assert!(!hold_overlaps_visible_window(0, &notes, visible_range));
        assert!(visited.is_empty());
    }

    #[test]
    fn find_first_displayed_beat_uses_note_count_cutoff() {
        let stats = (0..80)
            .map(|i| NoteCountStat {
                beat: i as f32 * 0.25,
                notes_lower: i,
                notes_upper: i + 1,
            })
            .collect::<Vec<_>>();

        let first =
            find_first_displayed_beat(20.0, 120.0, &stats, |_| 0.0).expect("finite beat range");

        assert!((3.9..=4.1).contains(&first), "first beat was {first}");
    }

    #[test]
    fn find_first_displayed_beat_falls_back_without_count_cache() {
        let first = find_first_displayed_beat(8.0, 120.0, &[], |beat| (beat - 4.0) * 64.0)
            .expect("finite beat range");

        assert!((4.0..=4.001).contains(&first), "first beat was {first}");
    }

    #[test]
    fn find_first_displayed_beat_rejects_invalid_inputs() {
        assert_eq!(
            find_first_displayed_beat(f32::NAN, 120.0, &[], |_| 0.0),
            None
        );
        assert_eq!(
            find_first_displayed_beat(0.0, f32::INFINITY, &[], |_| 0.0),
            None
        );
    }

    #[test]
    fn find_last_displayed_beat_searches_until_draw_distance() {
        let last = find_last_displayed_beat(0.0, 120.0, 1.0, false, |beat| (beat * 64.0, true))
            .expect("finite beat range");

        assert!((last - 1.875).abs() <= 0.001, "last beat was {last}");
    }

    #[test]
    fn find_last_displayed_beat_caps_slow_scroll_lookahead() {
        let last = find_last_displayed_beat(4.0, 120.0, 0.5, false, |_| (0.0, true))
            .expect("finite beat range");

        assert_eq!(last, 20.0);
    }

    #[test]
    fn find_last_displayed_beat_handles_invalid_and_boomerang_inputs() {
        assert_eq!(
            find_last_displayed_beat(f32::NAN, 120.0, 1.0, false, |_| (0.0, true)),
            None
        );
        assert_eq!(
            find_last_displayed_beat(0.0, f32::INFINITY, 1.0, false, |_| (0.0, true)),
            None
        );

        let normal = find_last_displayed_beat(0.0, 120.0, 1.0, false, |_| (200.0, false)).unwrap();
        let boomerang =
            find_last_displayed_beat(0.0, 120.0, 1.0, true, |_| (200.0, false)).unwrap();

        assert!(boomerang > normal);
    }
}
