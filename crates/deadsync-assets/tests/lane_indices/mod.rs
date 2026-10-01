use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline;

fn fixture(cols: usize, rows: usize, src: [i32; 2]) -> SpriteSlot {
    let key = format!("lane-indices {cols}x{rows}.png");
    let mut slot = slot_from_plan(deadsync_noteskin::atlas_sprite_slot_plan(
        key,
        (cols as u32 * 7, rows as u32 * 11),
        (7, 11),
        true,
    ));
    slot.def.src = src;
    slot.def.mirror_h = true;
    slot.def.mirror_v = true;
    slot
}

fn translate(spacing: [f32; 2]) -> NotePartTextureTranslate {
    NotePartTextureTranslate {
        note_color_spacing: spacing,
        ..Default::default()
    }
}

fn compare(a: &Option<Arc<SpriteSource>>, b: &Option<Arc<SpriteSource>>, slot: &SpriteSlot) {
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    if let (Some(a), Some(b)) = (a, b) {
        let mut old = slot.clone();
        old.source = Arc::clone(a);
        let mut new = slot.clone();
        new.source = Arc::clone(b);
        for time in [-1.0, 0.0, 0.125, 0.3, 1.0, 17.0, f32::NAN, f32::INFINITY] {
            assert_eq!(old.frame_index(time, time), new.frame_index(time, time));
            assert_eq!(
                old.frame_index_from_phase(time),
                new.frame_index_from_phase(time)
            );
        }
        for frame in 0..a.frame_count() + 3 {
            assert_eq!(
                old.uv_for_frame_at(frame, 0.0).map(f32::to_bits),
                new.uv_for_frame_at(frame, 0.0).map(f32::to_bits)
            );
        }
    }
}

#[test]
fn lane_indices_preserve_origins_grid_rates_uvs_and_rejections() {
    crate::noteskin::tests::init_asset_paths();
    for (cols, rows) in [(1, 1), (2, 3), (8, 4), (64, 2), (65, 2), (2, 129), (129, 3)] {
        for src in [[0, 0], [7, 11], [-3, -5], [999, 999]] {
            let slot = fixture(cols, rows, src);
            for spacing in [
                [0.0, 0.0],
                [1.0, 0.0],
                [0.0, -1.0],
                [1.0, 1.0],
                [f32::EPSILON, 0.0],
                [f32::NAN, 0.0],
            ] {
                for length in [-1.0, 0.0, 0.75, f32::NAN, f32::INFINITY] {
                    for beat in [false, true] {
                        let animation = NotePartAnimation {
                            length,
                            vivid: true,
                        };
                        compare(
                            &baseline::itg_note_animation_source(
                                &slot,
                                animation,
                                translate(spacing),
                                beat,
                            ),
                            &itg_note_animation_source(&slot, animation, translate(spacing), beat),
                            &slot,
                        );
                    }
                }
            }
        }
    }
    for animated in [false, true] {
        let mut slot = fixture(8, 4, [0, 0]);
        if animated {
            slot.source = source_from_plan(
                generated_animation_sprite_slot_plan(
                    "animated".into(),
                    (56, 44),
                    [7, 11],
                    4,
                    AnimationRate::FramesPerSecond(1.0),
                    true,
                )
                .source,
                &slot.def,
            );
        } else {
            slot.model = test_model_slot().model;
        }
        perf::assert_no_churn(|| {
            assert!(
                itg_note_animation_source(&slot, Default::default(), translate([1.0, 0.0]), false)
                    .is_none()
            )
        });
    }
}

#[test]
fn lane_indices_use_only_final_arrays_across_the_old_stack_boundary() {
    for count in [2, 8, 64, 65, 129, 1024] {
        let slot = fixture(count, 2, [7, 11]);
        let work = |old: bool| {
            black_box(if old {
                baseline::itg_note_animation_source(
                    &slot,
                    Default::default(),
                    translate([0.0, 1.0]),
                    false,
                )
            } else {
                itg_note_animation_source(&slot, Default::default(), translate([0.0, 1.0]), false)
            });
        };
        work(false);
        perf::assert_churn_budget(
            2,
            count * std::mem::size_of::<usize>() + 32 + std::mem::size_of::<SpriteSource>(),
            || work(false),
        );
        if count > 64 {
            perf::assert_reduced_churn(|| work(true), || work(false));
        }
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
fn benchmark_lane_indices() {
    crate::noteskin::tests::init_asset_paths();
    for (name, cols, rows, spacing) in [
        ("row8", 8, 4, [0.0, 1.0]),
        ("column4", 8, 4, [1.0, 0.0]),
        ("row64", 64, 2, [0.0, 1.0]),
        ("row65", 65, 2, [0.0, 1.0]),
        ("row1024", 1024, 2, [0.0, 1.0]),
        ("column129", 2, 129, [1.0, 0.0]),
        ("sequential", 8, 4, [0.0, 0.0]),
    ] {
        let slot = fixture(cols, rows, [7, 11]);
        compare(
            &baseline::itg_note_animation_source(
                &slot,
                Default::default(),
                translate(spacing),
                false,
            ),
            &itg_note_animation_source(&slot, Default::default(), translate(spacing), false),
            &slot,
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("shared_lane_{name}_{label}"), 16384, 1, || {
                black_box(if new {
                    itg_note_animation_source(
                        black_box(&slot),
                        black_box(Default::default()),
                        black_box(translate(spacing)),
                        black_box(false),
                    )
                } else {
                    baseline::itg_note_animation_source(
                        black_box(&slot),
                        black_box(Default::default()),
                        black_box(translate(spacing)),
                        black_box(false),
                    )
                });
            })
        });
    }
}
