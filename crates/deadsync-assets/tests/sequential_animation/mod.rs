use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline;

fn fixture(grid: [usize; 2], src: [i32; 2], size: [i32; 2]) -> SpriteSlot {
    let mut slot = test_model_slot();
    slot.model = None;
    slot.def.src = src;
    slot.def.size = size;
    slot.source_size = size;
    slot.source = source_from_plan(
        SpriteSourcePlan::Atlas {
            texture_key: format!("sequential-perf {}x{}.png", grid[0], grid[1]),
            tex_dims: (512, 256),
        },
        &slot.def,
    );
    // The shared sheet-metadata cache is outside the source allocation under
    // test. Warm it so a first key registration cannot enter scoped counts.
    let _ = assets::sprite_sheet_dims(slot.texture_key());
    slot
}

fn effective_indices(source: &SpriteSource) -> Vec<usize> {
    match source {
        SpriteSource::Atlas { .. } => Vec::new(),
        SpriteSource::Animated {
            frame_count,
            frame_indices,
            ..
        } => (0..*frame_count)
            .map(|frame| {
                frame_indices
                    .as_ref()
                    .and_then(|indices| indices.get(frame))
                    .copied()
                    .unwrap_or(frame)
            })
            .collect(),
    }
}

fn compare(a: &Option<Arc<SpriteSource>>, b: &Option<Arc<SpriteSource>>, def: &SpriteDefinition) {
    assert_eq!(a.is_some(), b.is_some());
    let (Some(a), Some(b)) = (a, b) else {
        return;
    };
    assert_eq!(a.texture_key(), b.texture_key());
    assert_eq!(a.frame_count(), b.frame_count());
    assert_eq!(a.frame_size(), b.frame_size());
    assert_eq!(a.is_beat_based(), b.is_beat_based());
    assert_eq!(a.texel_scale(), b.texel_scale());
    assert_eq!(effective_indices(a), effective_indices(b));
    match (a.as_ref(), b.as_ref()) {
        (
            SpriteSource::Animated {
                rate: ar,
                grid: ag,
                frame_durations: ad,
                ..
            },
            SpriteSource::Animated {
                rate: br,
                grid: bg,
                frame_durations: bd,
                ..
            },
        ) => {
            assert_eq!(ar, br);
            assert_eq!(ag, bg);
            assert_eq!(ad, bd);
        }
        _ => panic!("animation source expected"),
    }
    for frame in (0..a.frame_count() * 2 + 1).chain([usize::MAX]) {
        match (a.as_ref(), b.as_ref()) {
            (
                SpriteSource::Animated {
                    uv_cache: ac,
                    frame_indices: ai,
                    ..
                },
                SpriteSource::Animated {
                    uv_cache: bc,
                    frame_indices: bi,
                    ..
                },
            ) => {
                for inset in [false, true] {
                    assert_eq!(
                        ac.get(ai.as_deref(), frame, inset).map(f32::to_bits),
                        bc.get(bi.as_deref(), frame, inset).map(f32::to_bits),
                        "{def:?}, frame={frame}"
                    );
                }
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn sequential_animation_preserves_offsets_color_axes_uvs_and_rate() {
    for grid in [[1, 1], [1, 8], [8, 1], [8, 4], [3, 5]] {
        for src in [[0, 0], [64, 128], [-32, -64], [999, 999]] {
            for size in [[64, 64], [-64, 32], [0, 0]] {
                let slot = fixture(grid, src, size);
                let before = format!("{slot:?}");
                for spacing in [
                    [0.0, 0.0],
                    [0.125, 0.0],
                    [0.0, -0.25],
                    [0.1, 0.2],
                    [f32::EPSILON, 0.0],
                    [f32::NAN, 0.0],
                ] {
                    let translate = NotePartTextureTranslate {
                        note_color_spacing: spacing,
                        ..Default::default()
                    };
                    for beat in [false, true] {
                        for length in [0.0, -1.0, 1.0, 2.5, f32::NAN, f32::INFINITY] {
                            let animation = NotePartAnimation {
                                length,
                                vivid: true,
                            };
                            let old = baseline::itg_note_animation_source(
                                &slot, animation, translate, beat,
                            );
                            let new = itg_note_animation_source(&slot, animation, translate, beat);
                            compare(&old, &new, &slot.def);
                        }
                    }
                }
                assert_eq!(before, format!("{slot:?}"));
            }
        }
    }
}

#[test]
fn sequential_animation_preserves_delay_rebuilds_frame_selection_and_pause() {
    for grid in [[8, 4], [3, 5]] {
        for src in [[0, 0], [64, 128], [-32, -64], [999, 999]] {
            for spacing in [[0.0, 0.0], [0.125, 0.0], [0.0, 0.125]] {
                for script in [
                    "",
                    "setallstatedelays,0.1",
                    "setstateproperties,Sprite.LinearFrames(4,0.5)",
                ] {
                    for frame in [0, 3, usize::MAX] {
                        let original = fixture(grid, src, [64; 2]);
                        let translate = NotePartTextureTranslate {
                            note_color_spacing: spacing,
                            ..Default::default()
                        };
                        let mut old = original.clone();
                        let mut new = original.clone();
                        old.source = baseline::itg_note_animation_source(
                            &original,
                            NotePartAnimation::default(),
                            translate,
                            true,
                        )
                        .unwrap();
                        new.source = itg_note_animation_source(
                            &original,
                            NotePartAnimation::default(),
                            translate,
                            true,
                        )
                        .unwrap();
                        itg_apply_state_properties_from_script(&mut old, script, false);
                        itg_apply_state_properties_from_script(&mut new, script, false);
                        assert_eq!(old.def, new.def);
                        compare(
                            &Some(Arc::clone(&old.source)),
                            &Some(Arc::clone(&new.source)),
                            &old.def,
                        );
                        itg_apply_frame_override(&mut old, frame);
                        itg_apply_frame_override(&mut new, frame);
                        assert_eq!(old.animation_start_frame, new.animation_start_frame);
                        assert_eq!(
                            old.animation_start_time.to_bits(),
                            new.animation_start_time.to_bits()
                        );
                        for time in [0.0, 0.1, 0.7, f32::INFINITY, f32::NAN] {
                            let frame = old.frame_index(time, time);
                            assert_eq!(frame, new.frame_index(time, time));
                            assert_eq!(
                                old.uv_for_frame_at(frame, time).map(f32::to_bits),
                                new.uv_for_frame_at(frame, time).map(f32::to_bits)
                            );
                        }
                        freeze_sprite_animation(&mut old);
                        freeze_sprite_animation(&mut new);
                        assert_eq!(old.def, new.def);
                        assert_eq!(old.animation_start_frame, new.animation_start_frame);
                        assert_eq!(format!("{:?}", old.source), format!("{:?}", new.source));
                        assert_eq!(old.uv_for_frame_at(0, 0.0), new.uv_for_frame_at(0, 0.0));
                    }
                }
            }
        }
    }
}

#[test]
fn animation_keeps_model_and_already_animated_fallbacks_unchanged() {
    let mut slot = fixture([8, 4], [64, 128], [64; 2]);
    let animation = NotePartAnimation::default();
    let translate = NotePartTextureTranslate::default();
    slot.model = test_model_slot().model;
    perf::assert_no_churn(|| {
        assert!(itg_note_animation_source(&slot, animation, translate, true).is_none())
    });
    slot.model = None;
    slot.source = itg_note_animation_source(&slot, animation, translate, true).unwrap();
    perf::assert_no_churn(|| {
        assert!(itg_note_animation_source(&slot, animation, translate, true).is_none())
    });
    assert!(baseline::itg_note_animation_source(&slot, animation, translate, true).is_none());
}

#[test]
fn sequential_frames_remove_index_storage_and_rejected_single_axes_do_not_allocate() {
    for grid in [[2, 1], [8, 4], [16, 16], [3, 5]] {
        let slot = fixture(grid, [64, 128], [64; 2]);
        perf::assert_reduced_churn(
            || {
                black_box(baseline::itg_note_animation_source(
                    &slot,
                    NotePartAnimation::default(),
                    NotePartTextureTranslate::default(),
                    true,
                ));
            },
            || {
                black_box(itg_note_animation_source(
                    &slot,
                    NotePartAnimation::default(),
                    NotePartTextureTranslate::default(),
                    true,
                ));
            },
        );
        let source = itg_note_animation_source(
            &slot,
            NotePartAnimation::default(),
            NotePartTextureTranslate::default(),
            true,
        )
        .unwrap();
        assert!(matches!(source.as_ref(), SpriteSource::Animated {
            frame_indices: Some(indices), ..
        } if indices.is_empty()));
    }
    for (grid, spacing) in [([8, 1], [0.125, 0.0]), ([1, 8], [0.0, 0.125])] {
        let slot = fixture(grid, [0, 0], [64; 2]);
        perf::assert_no_churn(|| {
            assert!(
                itg_note_animation_source(
                    &slot,
                    NotePartAnimation::default(),
                    NotePartTextureTranslate {
                        note_color_spacing: spacing,
                        ..Default::default()
                    },
                    true
                )
                .is_none()
            );
        });
    }
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_sequential_animation() {
    for (name, grid, spacing) in [
        ("two", [2, 1], [0.0, 0.0]),
        ("sheet", [8, 4], [0.0, 0.0]),
        ("large", [16, 16], [0.0, 0.0]),
        ("odd", [3, 5], [0.0, 0.0]),
        ("column", [8, 4], [0.125, 0.0]),
        ("row", [8, 4], [0.0, 0.125]),
        ("reject", [8, 1], [0.125, 0.0]),
    ] {
        let slot = fixture(grid, [64, 128], [64; 2]);
        let translate = NotePartTextureTranslate {
            note_color_spacing: spacing,
            ..Default::default()
        };
        pairs(|label, new| {
            perf::measure_sampled(&format!("load_sheet_{name}_{label}"), 32768, 1, || {
                if new {
                    black_box(itg_note_animation_source(
                        black_box(&slot),
                        NotePartAnimation::default(),
                        translate,
                        true,
                    ));
                } else {
                    black_box(baseline::itg_note_animation_source(
                        black_box(&slot),
                        NotePartAnimation::default(),
                        translate,
                        true,
                    ));
                }
            })
        });
    }
    let slot = fixture([8, 4], [64, 128], [64; 2]);
    let old = baseline::itg_note_animation_source(
        &slot,
        NotePartAnimation::default(),
        NotePartTextureTranslate::default(),
        true,
    )
    .unwrap();
    let new = itg_note_animation_source(
        &slot,
        NotePartAnimation::default(),
        NotePartTextureTranslate::default(),
        true,
    )
    .unwrap();
    pairs(|label, is_new| {
        let source = if is_new { &new } else { &old };
        let SpriteSource::Animated {
            uv_cache,
            frame_indices,
            ..
        } = source.as_ref()
        else {
            unreachable!()
        };
        perf::measure_sampled(&format!("load_sheet_sampling_{label}"), 1024, 4096, || {
            for i in 0..4096 {
                black_box(uv_cache.get(black_box(frame_indices.as_deref()), black_box(i), true));
            }
        });
    });
}
