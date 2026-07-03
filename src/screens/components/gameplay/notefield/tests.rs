use super::{
    TornadoBounds, Z_ERROR_BAR_AVERAGE, Z_HOLD_BODY, Z_HOLD_GLOW, Z_RECEPTOR, Z_RECEPTOR_GLOW,
    Z_TAP_NOTE, confusion_rotation_deg, error_bar_boundaries_s, error_bar_text_scalable_zoom,
    error_bar_trim_max_window_ix, hold_explosion_active, hold_explosion_enabled,
    hold_head_render_flags, hold_indicator_column_x, hud_y, itg_actor_glow_alpha,
    judgment_frame_size, let_go_head_beat, move_col_extra, note_slot_base_size,
    note_world_z_for_bumpy, note_x_offset, offset_center, receptor_row_center, scroll_receptor_y,
    tipsy_y_extra,
};
use crate::assets;
use crate::game::parsing::noteskin::load_itg_skin;
use crate::test_support::notefield_bench;
use deadlib_present::actors::Actor;
use deadsync_core::note::NoteType;
use deadsync_core::timing::beat_to_note_row;
use deadsync_gameplay::{AccelEffects, ActiveHold, VisualEffects};
use deadsync_noteskin::{NUM_QUANTIZATIONS, Quantization, Style};
use deadsync_profile as profile_data;
use deadsync_rules::note::Note;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing;

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

#[test]
fn visible_beat_probe_keeps_brake_warped_notes() {
    let accel = AccelEffects {
        brake: 1.0,
        ..Default::default()
    };
    let last = super::find_last_displayed_beat(0.0, 120.0, 1.0, false, |beat| {
        let y = super::apply_accel_y(
            beat * 64.0,
            0.0,
            0.0,
            super::field_effect_height(0.0),
            accel,
        );
        (y, true)
    })
    .expect("finite beat range");
    assert!(
        last > 3.5,
        "last beat {last} should include the warped note"
    );

    let notes = vec![test_note_at_beat(0.5), test_note_at_beat(3.5)];
    let note_indices = vec![0usize, 1usize];
    let mut visited = Vec::new();
    super::for_each_visible_note_index(
        &note_indices,
        &notes,
        Some((beat_to_note_row(0.0), beat_to_note_row(last))),
        |note_index| visited.push(note_index),
    );
    assert_eq!(visited, vec![0, 1]);
}

#[test]
fn zero_scroll_start_stagnates_negative_lead_in_window() {
    let timing = timing::TimingData::from_segments(
        0.0,
        0.0,
        &timing::TimingSegments {
            bpms: vec![(0.0, 120.0)],
            speeds: vec![timing::SpeedSegment {
                beat: 0.0,
                ratio: 0.1,
                delay: 0.0,
                unit: timing::SpeedUnit::Beats,
            }],
            scrolls: vec![
                timing::ScrollSegment {
                    beat: 0.0,
                    ratio: 0.0,
                },
                timing::ScrollSegment {
                    beat: 4.0,
                    ratio: 1.0,
                },
            ],
            ..timing::TimingSegments::default()
        },
        &[],
    );
    let current_beat = -12.0;
    let current_time_ns = timing.get_time_for_beat_ns(current_beat);
    let speed = timing.get_speed_multiplier_ns(current_beat, current_time_ns);
    let curr_disp_beat = timing.get_displayed_beat(current_beat);
    let later_disp_beat = timing.get_displayed_beat(-6.0);

    let last = super::find_last_displayed_beat(current_beat, 120.0, speed, false, |beat| {
        let note_disp_beat = timing.get_displayed_beat(beat);
        (
            super::beat_scroll_travel(note_disp_beat, curr_disp_beat, speed),
            true,
        )
    })
    .expect("finite lead-in visible range");
    let beat_zero_travel =
        super::beat_scroll_travel(timing.get_displayed_beat(0.0), curr_disp_beat, speed);

    assert!((speed - 0.1).abs() <= 0.0001);
    assert!((curr_disp_beat - 0.0).abs() <= 0.0001);
    assert!((later_disp_beat - 0.0).abs() <= 0.0001);
    assert!(
        beat_zero_travel.abs() <= 0.001,
        "beat zero should be stagnant at the receptor during lead-in, travel={beat_zero_travel}"
    );
    assert!(
        last >= 3.99,
        "slow lead-in should include the first four beats, last={last}"
    );
}

#[test]
fn brake_applies_before_scroll_multiplier_like_itg() {
    let accel = AccelEffects {
        brake: 1.0,
        ..Default::default()
    };
    let effect_height = super::field_effect_height(0.0);
    let raw_y = ScrollSpeedSetting::ARROW_SPACING;
    let scroll_speed = 2.0;
    let itg_order = super::apply_accel_y(raw_y, 0.0, 0.0, effect_height, accel) * scroll_speed;
    let pre_scaled_order =
        super::apply_accel_y(raw_y * scroll_speed, 0.0, 0.0, effect_height, accel);
    let expected_itg_order = raw_y * (raw_y / effect_height) * scroll_speed;

    assert!(itg_order < pre_scaled_order);
    assert!((itg_order - expected_itg_order).abs() <= 0.001);
}

#[test]
fn hold_head_render_flags_keep_early_hit_inactive_before_receptor() {
    let active = ActiveHold {
        note_index: 42,
        start_time_ns: 100_000_000_000,
        end_time_ns: 12_000_000_000,
        note_type: NoteType::Hold,
        let_go: false,
        is_pressed: true,
        life: 1.0,
        last_update_time_ns: 100_000_000_000,
    };
    let (engaged, use_active) = hold_head_render_flags(Some(&active), 99.99, 100.0);
    assert!(!engaged);
    assert!(!use_active);
}

#[test]
fn hold_explosion_waits_for_receptor_on_early_hit() {
    let active = ActiveHold {
        note_index: 42,
        start_time_ns: 100_000_000_000,
        end_time_ns: 12_000_000_000,
        note_type: NoteType::Hold,
        let_go: false,
        is_pressed: true,
        life: 1.0,
        last_update_time_ns: 100_000_000_000,
    };

    assert!(!hold_explosion_active(Some(&active), 99.99, 100.0));
    assert!(hold_explosion_active(Some(&active), 100.0, 100.0));
}

#[test]
fn hold_explosion_requires_live_hold_state() {
    let exhausted = ActiveHold {
        note_index: 7,
        start_time_ns: 100_000_000_000,
        end_time_ns: 8_000_000_000,
        note_type: NoteType::Hold,
        let_go: false,
        is_pressed: true,
        life: 0.0,
        last_update_time_ns: 100_000_000_000,
    };
    let let_go = ActiveHold {
        note_index: 7,
        start_time_ns: 100_000_000_000,
        end_time_ns: 8_000_000_000,
        note_type: NoteType::Hold,
        let_go: true,
        is_pressed: true,
        life: 1.0,
        last_update_time_ns: 100_000_000_000,
    };

    assert!(!hold_explosion_active(Some(&exhausted), 100.0, 100.0));
    assert!(!hold_explosion_active(Some(&let_go), 100.0, 100.0));
    assert!(!hold_explosion_active(None, 100.0, 100.0));
}

#[test]
fn hold_explosion_option_uses_holding_mask() {
    let enabled = profile_data::Profile::default();
    assert!(hold_explosion_enabled(&enabled));

    let mut disabled = profile_data::Profile::default();
    disabled
        .tap_explosion_active_mask
        .remove(profile_data::TapExplosionMask::HOLDING);

    assert!(!hold_explosion_enabled(&disabled));

    disabled
        .tap_explosion_active_mask
        .insert(profile_data::TapExplosionMask::HOLDING);
    disabled
        .tap_explosion_active_mask
        .remove(profile_data::TapExplosionMask::HELD);

    assert!(hold_explosion_enabled(&disabled));
}

#[test]
fn hold_head_render_flags_switch_to_active_at_receptor() {
    let mut active = ActiveHold {
        note_index: 42,
        start_time_ns: 100_000_000_000,
        end_time_ns: 12_000_000_000,
        note_type: NoteType::Hold,
        let_go: false,
        is_pressed: true,
        life: 1.0,
        last_update_time_ns: 100_000_000_000,
    };
    let (engaged, use_active) = hold_head_render_flags(Some(&active), 100.0, 100.0);
    assert!(engaged);
    assert!(use_active);

    active.is_pressed = false;
    let (engaged_released, use_active_released) =
        hold_head_render_flags(Some(&active), 100.0, 100.0);
    assert!(engaged_released);
    assert!(!use_active_released);
}

#[test]
fn roll_head_render_flags_stay_active_between_taps() {
    let active = ActiveHold {
        note_index: 42,
        start_time_ns: 100_000_000_000,
        end_time_ns: 12_000_000_000,
        note_type: NoteType::Roll,
        let_go: false,
        is_pressed: false,
        life: 1.0,
        last_update_time_ns: 100_000_000_000,
    };
    let (engaged, use_active) = hold_head_render_flags(Some(&active), 100.0, 100.0);
    assert!(engaged);
    assert!(use_active);
}

#[test]
fn hold_head_render_flags_require_engaged_life_state() {
    let exhausted = ActiveHold {
        note_index: 7,
        start_time_ns: 100_000_000_000,
        end_time_ns: 8_000_000_000,
        note_type: NoteType::Roll,
        let_go: false,
        is_pressed: true,
        life: 0.0,
        last_update_time_ns: 100_000_000_000,
    };
    let let_go = ActiveHold {
        note_index: 7,
        start_time_ns: 100_000_000_000,
        end_time_ns: 8_000_000_000,
        note_type: NoteType::Roll,
        let_go: true,
        is_pressed: true,
        life: 1.0,
        last_update_time_ns: 100_000_000_000,
    };
    assert_eq!(
        hold_head_render_flags(Some(&exhausted), 200.0, 100.0),
        (false, false)
    );
    assert_eq!(
        hold_head_render_flags(Some(&let_go), 200.0, 100.0),
        (false, false)
    );
}

#[test]
fn let_go_head_beat_stays_at_receptor_until_visible_clock_catches_up() {
    let beat = let_go_head_beat(100.0, 108.0, 102.0, 101.25);
    assert!((beat - 101.25).abs() <= 1e-6);
}

#[test]
fn let_go_head_beat_uses_last_held_once_visible_clock_has_caught_up() {
    let beat = let_go_head_beat(100.0, 108.0, 102.0, 103.0);
    assert!((beat - 102.0).abs() <= 1e-6);
}

#[test]
fn receptor_glow_draws_under_hold_body() {
    assert!(Z_RECEPTOR < Z_HOLD_BODY);
    assert!(Z_RECEPTOR_GLOW < Z_HOLD_BODY);
}

#[test]
fn hold_glow_draws_over_hold_body_like_itg_second_pass() {
    assert!(Z_HOLD_BODY < Z_HOLD_GLOW);
    assert!(Z_HOLD_GLOW < Z_TAP_NOTE);
}

#[test]
fn average_error_bar_draws_under_receptors() {
    assert!(i32::from(Z_ERROR_BAR_AVERAGE) < Z_RECEPTOR);
    assert!(i32::from(Z_ERROR_BAR_AVERAGE) < Z_TAP_NOTE);
}

#[test]
fn default_gameplay_field_emits_receptors_and_notes() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = notefield_bench::fixture();
            let actors = fixture.build(false);

            assert!(
                count_visible_z(&actors, Z_RECEPTOR as i16) >= 4,
                "default gameplay field should emit receptor actors"
            );
            assert!(
                count_visible_z(&actors, Z_TAP_NOTE as i16) > 0,
                "default gameplay field should emit tap note actors"
            );
        })
        .expect("spawn diagnostic notefield test")
        .join()
        .expect("diagnostic notefield test should not panic");
}

fn count_visible_z(actors: &[Actor], wanted_z: i16) -> usize {
    let mut count = 0;
    let mut stack: Vec<&Actor> = actors.iter().collect();
    while let Some(actor) = stack.pop() {
        count += match actor {
            Actor::Sprite { visible, z, .. }
            | Actor::Mesh { visible, z, .. }
            | Actor::TexturedMesh { visible, z, .. } => usize::from(*visible && *z == wanted_z),
            Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
                stack.extend(children.iter());
                0
            }
            Actor::SharedFrame { children, .. } => {
                stack.extend(children.iter());
                0
            }
            Actor::Shadow { child, .. } => {
                stack.push(child);
                0
            }
            Actor::Text { .. } | Actor::CameraPush { .. } | Actor::CameraPop => 0,
        };
    }
    count
}

#[test]
fn text_error_bar_scalable_zoom_matches_sl_fork_curve_at_default_threshold() {
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
fn note_actor_glow_clamps_like_itg_vertex_color() {
    assert_eq!(itg_actor_glow_alpha(1.3), 1.0);
    assert_eq!(itg_actor_glow_alpha(0.65), 0.65);
    assert_eq!(itg_actor_glow_alpha(f32::NAN), 0.0);
}

#[test]
fn bumpy_world_z_matches_itg_default_wave() {
    let z = note_world_z_for_bumpy(8.0 * std::f32::consts::PI, 1.0, 0.0, 0.0);
    assert!((z - 40.0).abs() <= 1e-4);
}

#[test]
fn bumpy_period_changes_wave_length_like_itg() {
    let z = note_world_z_for_bumpy(-2.0 * std::f32::consts::PI, 1.0, 0.0, -1.25);
    assert!((z - 40.0).abs() <= 1e-4);
}

#[test]
fn move_and_confusion_column_mods_match_itg_scaling() {
    let mut visual = VisualEffects::default();
    visual.move_x_cols[1] = 0.5;
    visual.move_y_cols[1] = -0.25;
    visual.confusion_offset_cols[1] = std::f32::consts::FRAC_PI_2;

    assert_eq!(move_col_extra(&visual.move_x_cols, 1), 32.0);
    assert_eq!(move_col_extra(&visual.move_y_cols, 1), -16.0);
    assert!((confusion_rotation_deg(0.0, visual, 1) + 90.0).abs() <= 1e-6);
}

#[test]
fn receptor_center_uses_zero_travel_x_effects() {
    let col_offsets = [-96.0, -32.0, 32.0, 96.0];
    let invert = [0.0; 4];
    let tornado = [TornadoBounds::default(); 4];
    let center = receptor_row_center(
        320.0,
        1,
        240.0,
        1.0,
        0.0,
        VisualEffects {
            drunk: 1.0,
            ..VisualEffects::default()
        },
        &col_offsets,
        &invert,
        &tornado,
    );
    let expected_x = 320.0
        + note_x_offset(
            1,
            0.0,
            1.0,
            0.0,
            VisualEffects {
                drunk: 1.0,
                ..VisualEffects::default()
            },
            &col_offsets,
            &invert,
            &tornado,
        );
    assert!((center[0] - expected_x).abs() <= 1e-6);
}

#[test]
fn receptor_center_uses_tipsy_y_offset() {
    let col_offsets = [-96.0, -32.0, 32.0, 96.0];
    let invert = [0.0; 4];
    let tornado = [TornadoBounds::default(); 4];
    let visual = VisualEffects {
        tipsy: 1.0,
        ..VisualEffects::default()
    };
    let center = receptor_row_center(
        320.0,
        2,
        240.0,
        1.25,
        0.0,
        visual,
        &col_offsets,
        &invert,
        &tornado,
    );
    assert!((center[1] - (240.0 + tipsy_y_extra(2, 1.25, visual.tipsy))).abs() <= 1e-6);
}

#[test]
fn centered_scroll_overshoots_like_itg() {
    assert!((hud_y(100.0, 500.0, 300.0, false, 2.0) - 500.0).abs() <= 1e-6);
    assert!((scroll_receptor_y(0.0, 2.0, 100.0, 500.0, 300.0) - 500.0).abs() <= 1e-6);
}

#[test]
fn hold_indicator_columns_use_receptor_lane_x() {
    let playfield_center_x = 123.0;
    let columns = [-96.0, -32.0, 32.0, 96.0];
    let field_zoom_80_mini = 1.0 - 0.8 * 0.5;
    let col_offsets = columns.map(|x| x * field_zoom_80_mini);
    let invert_distances = [0.0; 4];
    let tornado_bounds = [TornadoBounds::default(); 4];
    const EPS: f32 = 1e-5;

    assert!(
        (hold_indicator_column_x(
            playfield_center_x,
            0,
            0.0,
            0.0,
            VisualEffects::default(),
            &col_offsets,
            &invert_distances,
            &tornado_bounds,
        ) - (playfield_center_x - 57.6))
            .abs()
            <= EPS
    );
    assert!(
        (hold_indicator_column_x(
            playfield_center_x,
            3,
            0.0,
            0.0,
            VisualEffects::default(),
            &col_offsets,
            &invert_distances,
            &tornado_bounds,
        ) - (playfield_center_x + 57.6))
            .abs()
            <= EPS
    );

    let flipped = VisualEffects {
        flip: 1.0,
        ..VisualEffects::default()
    };
    assert!(
        (hold_indicator_column_x(
            playfield_center_x,
            0,
            0.0,
            0.0,
            flipped,
            &col_offsets,
            &invert_distances,
            &tornado_bounds,
        ) - (playfield_center_x + 57.6))
            .abs()
            <= EPS
    );
}

#[test]
fn judgment_frame_size_uses_logical_atlas_frame_dims() {
    let censored = "judgements/Test Censored 1x7 (doubleres).png";
    let tight_censored = "judgements/Test Censored Tight 1x7 (doubleres).png";
    let love = "judgements/Test Love 2x7 (doubleres).png";
    assets::register_texture_dims(censored, 600, 1400);
    assets::register_texture_dims(tight_censored, 600, 1050);
    assets::register_texture_dims(love, 880, 1036);

    assert_eq!(judgment_frame_size(censored), [300.0, 100.0]);
    assert_eq!(judgment_frame_size(tight_censored), [300.0, 75.0]);
    assert_eq!(judgment_frame_size(love), [220.0, 74.0]);

    let visible_art_h = 68.0 / 2.0;
    let original_drawn_art_h = visible_art_h * judgment_frame_size(censored)[1] / 100.0;
    let tight_drawn_art_h = visible_art_h * judgment_frame_size(tight_censored)[1] / 75.0;
    assert_eq!(original_drawn_art_h, tight_drawn_art_h);
}

#[test]
fn error_bar_boundaries_use_10ms_blue_fantastic_window() {
    let windows = timing::TimingProfile::default_itg_with_fa_plus().windows_s;
    let (bounds, len) = error_bar_boundaries_s(
        windows,
        Some(timing::FA_PLUS_W010_MS / 1000.0),
        true,
        error_bar_trim_max_window_ix(profile_data::ErrorBarTrim::Fantastic),
    );

    assert_eq!(len, 2);
    assert!((bounds[0] * 1000.0 - timing::FA_PLUS_W010_MS).abs() <= 0.001);
    assert!((bounds[1] - windows[0]).abs() <= 0.000001);
}

#[test]
fn cyber_model_tap_scale_uses_model_height_not_logical_height() {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let ns = load_itg_skin(&style, "cyber").expect("dance/cyber should load from assets/noteskins");
    let slot = ns
        .note_layers
        .first()
        .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
        .expect("cyber should expose model-backed tap-note layer for 4th notes");

    let logical_h = slot.logical_size()[1].max(1.0);
    let model_h = slot
        .model
        .as_ref()
        .map(|model| model.size()[1])
        .expect("cyber tap slot should be model-backed");
    assert!(
        model_h > f32::EPSILON,
        "cyber model-backed tap slot should have positive model height"
    );
    assert!(
        logical_h / model_h > 1.5,
        "regression guard: cyber logical height must stay larger than model height so this test catches logical-height scaling; logical={logical_h}, model={model_h}"
    );
    let scale_h = note_slot_base_size(slot, 1.0)[1];
    assert!(
        (scale_h - model_h).abs() <= 1e-4,
        "model-backed tap notes must scale by model height; got scale_h={scale_h}, model_h={model_h}"
    );
}

#[test]
fn hold_explosion_slot_respects_explosion_noteskin_choice() {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let cel_ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
    let default_ns =
        load_itg_skin(&style, "default").expect("dance/default should load from assets/noteskins");
    let ddr_vivid_ns = load_itg_skin(&style, "ddr-vivid")
        .expect("dance/ddr-vivid should load from assets/noteskins");

    let base_slot = cel_ns
        .hold_explosion_for_col(0, false)
        .expect("cel should define a hold explosion");
    let selected_slot = ddr_vivid_ns
        .hold_explosion_for_col(0, false)
        .expect("ddr-vivid should define a hold explosion");

    assert_ne!(
        selected_slot.texture_key(),
        base_slot.texture_key(),
        "hold explosions should come from the selected explosion noteskin"
    );
    assert!(
        selected_slot.texture_key().contains("ddr-vivid"),
        "selected hold explosion should resolve from ddr-vivid, got '{}'",
        selected_slot.texture_key()
    );
    assert!(
        default_ns.hold_explosion_for_col(0, false).is_none(),
        "a selected noteskin with blank hold explosions must not fall back to the base noteskin"
    );
}

#[test]
fn default_tap_circles_stay_inside_arrow_in_gameplay_layout() {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let ns =
        load_itg_skin(&style, "default").expect("dance/default should load from assets/noteskins");
    const EPSILON: f32 = 1e-3;

    for col in 0..style.num_cols {
        let note_idx = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
        let layers = ns
            .note_layers
            .get(note_idx)
            .expect("default should expose Q4th tap layers for each column");

        let mut arrow_bounds: Option<(f32, f32, f32, f32)> = None;
        let mut circle_bounds = Vec::new();

        for slot in layers.iter() {
            let draw = slot.model_draw_at(0.0, 0.0);
            if !draw.visible {
                continue;
            }
            let base_size = note_slot_base_size(slot, 1.0);
            let size = [
                base_size[0] * draw.zoom[0].max(0.0),
                base_size[1] * draw.zoom[1].max(0.0),
            ];
            if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                continue;
            }
            let local_offset = [draw.pos[0], draw.pos[1]];
            let center = offset_center([0.0, 0.0], local_offset, slot.base_rot_sin_cos());
            let half_w = size[0] * 0.5;
            let half_h = size[1] * 0.5;
            let bounds = (
                center[0] - half_w,
                center[0] + half_w,
                center[1] - half_h,
                center[1] + half_h,
            );
            let key = slot.texture_key().to_ascii_lowercase();
            if key.contains("_arrow") {
                arrow_bounds = Some(bounds);
            } else if key.contains("_circle") {
                circle_bounds.push(bounds);
            }
        }

        let (ax0, ax1, ay0, ay1) =
            arrow_bounds.expect("default tap layers should include arrow layer");
        assert_eq!(
            circle_bounds.len(),
            4,
            "default tap layers should include four circle layers"
        );
        for (idx, (cx0, cx1, cy0, cy1)) in circle_bounds.into_iter().enumerate() {
            assert!(
                cx0 >= ax0 - EPSILON
                    && cx1 <= ax1 + EPSILON
                    && cy0 >= ay0 - EPSILON
                    && cy1 <= ay1 + EPSILON,
                "column {col} circle {idx} escaped arrow bounds: circle=({cx0},{cx1},{cy0},{cy1}), arrow=({ax0},{ax1},{ay0},{ay1})"
            );
        }
    }
}
