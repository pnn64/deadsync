use super::{
    JudgmentSpriteMetadata, ResolvedJudgmentAssets, error_bar_trim_max_window_ix,
    hold_explosion_enabled, judgment_frame_size, prewarm_actor_resources,
    resolved_held_miss_texture, resolved_hold_judgment_texture, resolved_judgment_texture,
};
use crate::assets;
use crate::notefield_style::notefield_style;
use crate::screens::gameplay::GameplayNoteskinAssets;
use deadlib_present::actors::ActorResourceArena;
use deadsync_assets::noteskin::load_itg_skin;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{ActiveHold, hold_explosion_active, hold_head_render_flags};
use deadsync_notefield::{error_bar_boundaries_s, offset_center};
use deadsync_noteskin::{NUM_QUANTIZATIONS, NoteskinSlot, Quantization, Style};
use deadsync_profile as profile_data;
use deadsync_rules::timing;
use std::sync::Arc;

#[test]
fn cached_judgment_assets_match_legacy_resolution() {
    let mut none = profile_data::Profile::default();
    none.judgment_graphic = profile_data::JudgmentGraphic::new("None");
    none.hold_judgment_graphic = profile_data::HoldJudgmentGraphic::new("None");
    none.held_miss_graphic = profile_data::HeldMissGraphic::new("None");

    let mut held_miss = profile_data::Profile::default();
    held_miss.held_miss_graphic = profile_data::HeldMissGraphic::new("Love");

    for profile in [profile_data::Profile::default(), none, held_miss] {
        let cached = ResolvedJudgmentAssets::from_profile(&profile);
        assert_eq!(
            cached.judgment().map(|texture| texture.key.as_ref()),
            resolved_judgment_texture(&profile).map(|texture| texture.key.as_ref())
        );
        assert_eq!(
            cached.hold_judgment().map(|texture| texture.key.as_ref()),
            resolved_hold_judgment_texture(&profile).map(|texture| texture.key.as_ref())
        );
        assert_eq!(
            cached.held_miss().map(|(texture, _)| texture.key.as_ref()),
            resolved_held_miss_texture(&profile).map(|texture| texture.key.as_ref())
        );

        let legacy_metadata = resolved_judgment_texture(&profile).map(|texture| {
            let (frame_cols, frame_rows) = assets::parse_sprite_sheet_dims(texture.key.as_ref());
            JudgmentSpriteMetadata {
                frame_size: judgment_frame_size(texture.key.as_ref()),
                frame_cols: frame_cols as usize,
                frame_rows: frame_rows as usize,
            }
        });
        assert_eq!(cached.judgment_sprite_metadata(), legacy_metadata);

        if let Some((texture, scale)) = cached.held_miss() {
            let expected = if assets::parse_texture_hints(texture.key.as_ref()).doubleres {
                0.5
            } else {
                1.0
            };
            assert_eq!(scale, expected);
        }
    }
}

#[test]
fn cached_judgment_metadata_refreshes_only_when_registry_generation_changes() {
    let cached = ResolvedJudgmentAssets::from_profile(&profile_data::Profile::default());
    let first = JudgmentSpriteMetadata {
        frame_size: [128.0, 64.0],
        frame_cols: 2,
        frame_rows: 7,
    };
    let refreshed = JudgmentSpriteMetadata {
        frame_size: [256.0, 128.0],
        frame_cols: 4,
        frame_rows: 8,
    };

    assert_eq!(
        cached.judgment_sprite_metadata_for_generation(10, |_| first),
        Some(first)
    );
    assert_eq!(
        cached.judgment_sprite_metadata_for_generation(10, |_| {
            panic!("same registry generation must reuse cached metadata")
        }),
        Some(first)
    );
    assert_eq!(
        cached.judgment_sprite_metadata_for_generation(11, |_| refreshed),
        Some(refreshed)
    );
}

fn note_slot_base_size<S: NoteskinSlot>(slot: &S, scale: f32) -> [f32; 2] {
    if let Some(model) = slot.model() {
        let size = model.size();
        if size[0] > f32::EPSILON && size[1] > f32::EPSILON {
            return [size[0] * scale, size[1] * scale];
        }
    }
    let logical = slot.logical_size();
    [logical[0] * scale, logical[1] * scale]
}

#[test]
fn actor_resource_prewarm_covers_every_noteskin_slot() {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let noteskin = Arc::new(
        load_itg_skin(&style, "default").expect("dance/default should load from assets/noteskins"),
    );
    let assets = GameplayNoteskinAssets {
        noteskin: [Some(Arc::clone(&noteskin)), None],
        mine_noteskin: [Some(Arc::clone(&noteskin)), None],
        receptor_noteskin: [Some(Arc::clone(&noteskin)), None],
        tap_explosion_noteskin: [Some(Arc::clone(&noteskin)), None],
    };
    let profiles = std::array::from_fn(|_| profile_data::Profile::default());
    let arena = ActorResourceArena::default();

    prewarm_actor_resources(&arena, &assets, &profiles, 1);
    let warmed = arena.stats();
    noteskin.for_each_slot(|slot| {
        let _ = slot.actor_texture_source(&arena);
    });

    assert!(warmed.textures > 0);
    assert_eq!(arena.stats().texture_misses, warmed.texture_misses);
    assert_eq!(arena.stats().texture_saturated, 0);
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
fn receptor_glow_draws_under_hold_body() {
    let style = notefield_style();
    assert!(style.receptor.target_z < style.actors.hold_body_z);
    assert!(style.receptor.press_glow_z < style.actors.hold_body_z);
}

#[test]
fn hold_glow_draws_over_hold_body_like_itg_second_pass() {
    let actors = notefield_style().actors;
    assert!(actors.hold_body_z < actors.hold_glow_z);
    assert!(actors.hold_glow_z < actors.note_z);
}

#[test]
fn average_error_bar_draws_under_receptors() {
    let z = notefield_style().error_bar.average_z;
    assert!(z < notefield_style().receptor.target_z);
    assert!(z < notefield_style().actors.note_z);
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
