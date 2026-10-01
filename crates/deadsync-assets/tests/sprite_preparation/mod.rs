use super::*;
use crate::perf;
use deadsync_noteskin::script::{SpriteAnimationCommandPlan as Command, SpriteStatePropertiesPlan};
use std::hint::black_box;

mod baseline;

fn plan(key: &str, count: Option<usize>, indexed: bool) -> SpriteSlotPlan {
    let mut plan = if let Some(count) = count {
        generated_animation_sprite_slot_plan(
            key.to_owned(),
            (512, 256),
            [64, 64],
            count,
            AnimationRate::FramesPerSecond(30.0),
            false,
        )
    } else {
        deadsync_noteskin::atlas_sprite_slot_plan(key.to_owned(), (512, 256), (64, 64), true)
    };
    plan.def.src = [37, 79];
    plan.def.mirror_h = true;
    if let SpriteSourcePlan::Animated {
        grid,
        frame_indices,
        frame_durations,
        ..
    } = &mut plan.source
    {
        *grid = (8, 4);
        *frame_indices = indexed.then(|| vec![7, 2, 11, 0]);
        *frame_durations = Some(vec![0.125, 0.25, -0.0, f32::NAN]);
    }
    plan
}

fn fixture(key: &str, count: Option<usize>, indexed: bool) -> SpriteSlot {
    let mut slot = slot_from_plan(plan(key, count, indexed));
    slot.animation_start_frame = 2;
    slot.animation_start_time = 0.375;
    slot.uv_offset = [0.125, -0.25];
    slot.uv_velocity = [0.03, -0.04];
    let _ = assets::sprite_sheet_dims(slot.texture_key());
    let _ = assets::texture_source_frame_dims_from_real(slot.texture_key(), 512, 256);
    slot
}

fn snapshot(slot: &SpriteSlot) -> String {
    format!("{slot:?}").replacen(
        &format!("stable_id: {}", slot.stable_id()),
        "stable_id: <identity>",
        1,
    )
}

fn compare(old: &SpriteSlot, new: &SpriteSlot) {
    assert_eq!(snapshot(old), snapshot(new));
    match (old.source.as_ref(), new.source.as_ref()) {
        (
            SpriteSource::Animated {
                rate: a,
                frame_durations: ad,
                frame_indices: ai,
                ..
            },
            SpriteSource::Animated {
                rate: b,
                frame_durations: bd,
                frame_indices: bi,
                ..
            },
        ) => {
            let bits = |rate: AnimationRate| match rate {
                AnimationRate::FramesPerSecond(v) => (false, v.to_bits()),
                AnimationRate::FramesPerBeat(v) => (true, v.to_bits()),
            };
            assert_eq!(bits(*a), bits(*b));
            assert_eq!(ai, bi);
            assert_eq!(
                ad.as_deref()
                    .map(|d| d.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
                bd.as_deref()
                    .map(|d| d.iter().map(|v| v.to_bits()).collect::<Vec<_>>())
            );
        }
        (SpriteSource::Atlas { .. }, SpriteSource::Atlas { .. }) => {}
        _ => panic!("source variants differ"),
    }
    for time in [
        -10.0,
        -0.0,
        0.0,
        0.125,
        0.5,
        1.0,
        10.0,
        f32::NAN,
        f32::INFINITY,
    ] {
        assert_eq!(
            old.frame_index(time, time * 3.0),
            new.frame_index(time, time * 3.0)
        );
        assert_eq!(
            old.frame_index_from_phase(time),
            new.frame_index_from_phase(time)
        );
        for frame in [0, 1, 7, new.source.frame_count().saturating_sub(1)] {
            assert_eq!(
                old.uv_for_frame_at(frame, time).map(f32::to_bits),
                new.uv_for_frame_at(frame, time).map(f32::to_bits)
            );
        }
    }
}

fn state(count: usize, delays: &[f32]) -> Command {
    Command::StateProperties(SpriteStatePropertiesPlan {
        frame_count: count,
        frame_delays: delays.to_vec(),
    })
}

fn translation(spacing: [f32; 2]) -> NotePartTextureTranslate {
    NotePartTextureTranslate {
        note_color_spacing: spacing,
        ..Default::default()
    }
}

fn compare_animation(initial: &SpriteSlot, length: f32, spacing: [f32; 2], beat: bool) {
    let animation = NotePartAnimation {
        length,
        vivid: true,
    };
    let old = baseline::itg_note_animation_source(initial, animation, translation(spacing), beat);
    let new = itg_note_animation_source(initial, animation, translation(spacing), beat);
    assert_eq!(old.is_some(), new.is_some());
    if let (Some(old), Some(new)) = (old, new) {
        assert!(Arc::ptr_eq(
            &initial.texture_key_shared(),
            &new.texture_key_shared()
        ));
        let mut a = initial.clone();
        let mut b = initial.clone();
        a.source = old;
        b.source = new;
        compare(&a, &b);
    }
}

#[test]
fn frame_overrides_preserve_atlas_geometry_and_animated_seek_behavior() {
    for key in [
        "sprite-prep.png",
        "sprite-prep 1x1.png",
        "sprite-prep 4x2.png",
        "sprite-prep 8x8 (doubleres).png",
    ] {
        for count in [None, Some(0), Some(1), Some(7), Some(65)] {
            for indexed in [false, true] {
                let mut initial = fixture(key, count, indexed);
                initial.def.mirror_v = true;
                initial.set_rotation_deg(13);
                for frame in [0, 1, 3, 7, 63, 128, usize::MAX] {
                    let mut old = initial.clone();
                    let mut new = initial.clone();
                    baseline::itg_apply_frame_override(&mut old, frame);
                    itg_apply_frame_override(&mut new, frame);
                    compare(&old, &new);
                    assert!(Arc::ptr_eq(
                        &initial.texture_key_shared(),
                        &new.texture_key_shared()
                    ));
                    if count.is_some() {
                        assert!(Arc::ptr_eq(&initial.source, &new.source));
                    }
                }
            }
        }
    }
}

#[test]
fn frame_overrides_reset_caches_and_preserve_shared_or_weak_owners() {
    for mode in 0..3 {
        let mut new = fixture("sprite-prep 4x2.png", None, false);
        if let SpriteSource::Atlas {
            cached_handle,
            cached_generation,
            cached_actor_texture,
            ..
        } = new.source.as_ref()
        {
            cached_handle.store(17, Ordering::Relaxed);
            cached_generation.store(29, Ordering::Relaxed);
            cached_actor_texture.store(83, Ordering::Relaxed);
        }
        let pointer = Arc::as_ptr(&new.source);
        let mut old = new.clone();
        baseline::itg_apply_frame_override(&mut old, 6);
        let retained = (mode == 1).then(|| Arc::clone(&new.source));
        let weak = (mode == 2).then(|| Arc::downgrade(&new.source));
        let before = format!("{:?}", new.source);
        itg_apply_frame_override(&mut new, 6);
        compare(&old, &new);
        if mode == 0 {
            assert_eq!(pointer, Arc::as_ptr(&new.source));
        } else {
            assert_ne!(pointer, Arc::as_ptr(&new.source));
        }
        if let Some(retained) = retained {
            assert_eq!(before, format!("{retained:?}"));
        }
        if let Some(weak) = weak {
            assert!(weak.upgrade().is_none());
        }
    }
    let mut unique = fixture("sprite-prep 4x2.png", None, false);
    perf::assert_no_churn(|| itg_apply_frame_override(&mut unique, 3));
    let mut animated = fixture("sprite-prep 4x2.png", Some(7), true);
    perf::assert_no_churn(|| itg_apply_frame_override(&mut animated, usize::MAX));
}

#[test]
fn note_animation_lanes_preserve_indices_origins_rates_and_uvs() {
    for key in [
        "sprite-prep.png",
        "sprite-prep 1x8.png",
        "sprite-prep 8x1.png",
        "sprite-prep 8x4.png",
        "sprite-prep 64x2.png",
        "sprite-prep 65x2.png",
        "sprite-prep 2x129.png",
    ] {
        for src in [[0, 0], [37, 79], [-4, -8], [999, 999]] {
            for size in [[64, 64], [-64, -32], [0, 0]] {
                let mut initial = fixture(key, None, false);
                initial.def.src = src;
                initial.def.size = size;
                for spacing in [
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [0.0, -1.0],
                    [1.0, 1.0],
                    [f32::EPSILON, 0.0],
                    [f32::NAN, 0.0],
                    [0.0, f32::INFINITY],
                ] {
                    for length in [
                        -1.0,
                        -0.0,
                        0.0,
                        f32::from_bits(1),
                        1e-6,
                        0.75,
                        f32::NAN,
                        f32::INFINITY,
                    ] {
                        for beat in [false, true] {
                            compare_animation(&initial, length, spacing, beat);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn note_animation_rejections_keep_sources_and_do_not_allocate() {
    for (key, count, model, spacing) in [
        ("sprite-prep.png", None, false, [0.0, 0.0]),
        ("sprite-prep 8x4.png", Some(7), false, [0.0, 0.0]),
        ("sprite-prep 8x4.png", None, true, [0.0, 0.0]),
        ("sprite-prep 8x4.png", None, false, [1.0, 1.0]),
        ("sprite-prep 8x1.png", None, false, [1.0, 0.0]),
    ] {
        let mut initial = fixture(key, count, false);
        if model {
            initial.model = test_model_slot().model;
        }
        let before = snapshot(&initial);
        compare_animation(&initial, 1.0, spacing, false);
        perf::assert_no_churn(|| {
            assert!(
                itg_note_animation_source(
                    &initial,
                    Default::default(),
                    translation(spacing),
                    false
                )
                .is_none()
            );
        });
        assert_eq!(before, snapshot(&initial));
    }
}

fn animation_snapshot(
    animation: &Option<deadsync_noteskin::SpriteStatePropertiesAnimation>,
) -> String {
    format!("{animation:?}")
}

#[test]
fn owned_and_borrowed_state_planners_match_parent_delay_bits_and_geometry() {
    for grid in [[0, 0], [1, 1], [4, 2], [8, 8], [65, 2]] {
        for count in [0, 1, 2, 7, 32, 64, 65, 129, usize::MAX] {
            for delays in [
                vec![],
                vec![0.125],
                vec![0.25, 0.5],
                vec![
                    -0.0,
                    f32::NAN,
                    -1.0,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::from_bits(1),
                ],
                vec![0.1; 160],
            ] {
                for src in [[0, 0], [37, 79], [-3, -7], [999, 999]] {
                    for beat in [false, true] {
                        let old = baseline::sprite_state_properties_animation(
                            [512, 256],
                            grid,
                            src,
                            count,
                            &delays,
                            beat,
                        );
                        let borrowed = deadsync_noteskin::sprite_state_properties_animation(
                            [512, 256],
                            grid,
                            src,
                            count,
                            &delays,
                            beat,
                        );
                        let owned = sprite_state_properties_animation_owned(
                            [512, 256],
                            grid,
                            src,
                            count,
                            delays.clone(),
                            beat,
                        );
                        for new in [&borrowed, &owned] {
                            assert_eq!(animation_snapshot(&old), animation_snapshot(new));
                            if let (Some(a), Some(b)) = (&old, new) {
                                assert_eq!(
                                    a.frame_durations
                                        .iter()
                                        .map(|v| v.to_bits())
                                        .collect::<Vec<_>>(),
                                    b.frame_durations
                                        .iter()
                                        .map(|v| v.to_bits())
                                        .collect::<Vec<_>>()
                                );
                                let rate = |r| match r {
                                    AnimationRate::FramesPerBeat(v) => (true, v.to_bits()),
                                    AnimationRate::FramesPerSecond(v) => (false, v.to_bits()),
                                };
                                assert_eq!(rate(a.rate), rate(b.rate));
                            }
                        }
                    }
                }
            }
        }
    }
    for len in [0, 2, 7, 16] {
        let mut delays = Vec::with_capacity(32);
        delays.resize(len, -0.25);
        let pointer = delays.as_ptr();
        let capacity = delays.capacity();
        let mut result = None;
        perf::assert_no_churn(|| {
            result = sprite_state_properties_animation_owned(
                [512, 256],
                [4, 2],
                [0, 0],
                7,
                delays,
                true,
            );
        });
        let result = result.unwrap();
        assert_eq!(pointer, result.frame_durations.as_ptr());
        assert_eq!(capacity, result.frame_durations.capacity());
    }
}

#[test]
fn owned_state_commands_preserve_complete_slots_owners_and_command_sequences() {
    for mode in 0..3 {
        for count in [0, 1, 2, 7, 32, 65, 129] {
            for delays in [
                vec![],
                vec![0.125, 0.25],
                vec![f32::NAN, -0.0, -1.0, f32::INFINITY],
                vec![0.1; 160],
            ] {
                let mut new = fixture("sprite-prep 8x8.png", Some(7), true);
                let mut old = new.clone();
                baseline::itg_apply_sprite_animation_plan(&mut old, state(count, &delays), true);
                let retained = (mode == 1).then(|| Arc::clone(&new.source));
                let weak = (mode == 2).then(|| Arc::downgrade(&new.source));
                let before = format!("{:?}", new.source);
                itg_apply_sprite_animation_plan(&mut new, state(count, &delays), true);
                compare(&old, &new);
                if let Some(retained) = retained {
                    assert_eq!(before, format!("{retained:?}"));
                }
                if let Some(weak) = weak {
                    assert_eq!(weak.upgrade().is_some(), count <= 1);
                }
            }
        }
    }
    for script in [
        "SetStateProperties,Sprite.LinearFrames(7,0.875);SetAllStateDelays,0.25;setstate,3;pause",
        "SetAllStateDelays,0.1;SetStateProperties,Sprite.LinearFrames(3,0.75);play",
    ] {
        let mut old = fixture("sprite-prep 8x4.png", None, false);
        let mut new = old.clone();
        apply_sprite_animation_script_plans(&mut old, script, true, |slot, command, beat| {
            baseline::itg_apply_sprite_animation_plan(slot, command, beat)
        });
        apply_sprite_animation_script_plans(
            &mut new,
            script,
            true,
            itg_apply_sprite_animation_plan,
        );
        let commands = HashMap::from([("initcommand".to_owned(), script.to_owned())]);
        itg_apply_initial_sprite_state(&mut old, &commands);
        itg_apply_initial_sprite_state(&mut new, &commands);
        compare(&old, &new);
    }
}

#[test]
fn preparation_reduces_complete_owning_churn() {
    let initial = fixture("sprite-prep 8x4.png", None, false);
    perf::assert_reduced_churn(
        || {
            let mut slot = initial.clone();
            baseline::itg_apply_frame_override(&mut slot, 3);
            black_box(slot);
        },
        || {
            let mut slot = initial.clone();
            itg_apply_frame_override(&mut slot, 3);
            black_box(slot);
        },
    );
    for spacing in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        let _ = itg_note_animation_source(&initial, Default::default(), translation(spacing), true);
        perf::assert_reduced_churn(
            || {
                black_box(baseline::itg_note_animation_source(
                    &initial,
                    Default::default(),
                    translation(spacing),
                    true,
                ));
            },
            || {
                black_box(itg_note_animation_source(
                    &initial,
                    Default::default(),
                    translation(spacing),
                    true,
                ));
            },
        );
    }
    for delays in [vec![0.125; 32], vec![0.125; 64]] {
        let command = state(32, &delays);
        perf::assert_reduced_churn(
            || {
                let mut slot = initial.clone();
                baseline::itg_apply_sprite_animation_plan(&mut slot, command.clone(), true);
                black_box(slot);
            },
            || {
                let mut slot = initial.clone();
                itg_apply_sprite_animation_plan(&mut slot, command.clone(), true);
                black_box(slot);
            },
        );
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
fn benchmark_sprite_preparation() {
    for (name, key, count) in [
        ("static", "sprite-prep 8x4.png".to_owned(), None),
        ("long_key", format!("{} 8x4.png", "k".repeat(1024)), None),
        ("animated_seek", "sprite-prep 8x4.png".to_owned(), Some(7)),
    ] {
        let initial = fixture(&key, count, true);
        let mut old = initial.clone();
        let mut new = initial.clone();
        baseline::itg_apply_frame_override(&mut old, 3);
        itg_apply_frame_override(&mut new, 3);
        compare(&old, &new);
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_frame_{name}_{label}"), 256, 64, || {
                for _ in 0..64 {
                    let mut slot = black_box(&initial).clone();
                    if new {
                        itg_apply_frame_override(&mut slot, black_box(3));
                    } else {
                        baseline::itg_apply_frame_override(&mut slot, black_box(3));
                    }
                    black_box(slot);
                }
            })
        });
    }
    pairs(|label, new| {
        perf::measure_sampled_with_setup(
            &format!("prep_frame_unique_{label}"),
            128,
            64,
            || {
                (0..64)
                    .map(|_| fixture("sprite-prep 8x4.png", None, false))
                    .collect::<Vec<_>>()
            },
            |slots| {
                for slot in slots {
                    if new {
                        itg_apply_frame_override(black_box(slot), black_box(3));
                    } else {
                        baseline::itg_apply_frame_override(black_box(slot), black_box(3));
                    }
                }
            },
        )
    });
    for (name, key, spacing) in [
        ("sequential", "sprite-prep 8x4.png".to_owned(), [0.0, 0.0]),
        ("color_x", "sprite-prep 8x4.png".to_owned(), [1.0, 0.0]),
        ("color_y", "sprite-prep 8x4.png".to_owned(), [0.0, 1.0]),
        ("boundary64", "sprite-prep 64x2.png".to_owned(), [0.0, 1.0]),
        ("large65", "sprite-prep 65x2.png".to_owned(), [0.0, 1.0]),
        (
            "long_key",
            format!("{} 8x4.png", "k".repeat(1024)),
            [1.0, 0.0],
        ),
        ("rejected", "sprite-prep 8x4.png".to_owned(), [1.0, 1.0]),
    ] {
        let initial = fixture(&key, None, false);
        compare_animation(&initial, 0.75, spacing, true);
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_note_{name}_{label}"), 256, 64, || {
                for _ in 0..64 {
                    let animation = black_box(NotePartAnimation {
                        length: 0.75,
                        vivid: true,
                    });
                    let translate = black_box(translation(spacing));
                    black_box(if new {
                        itg_note_animation_source(black_box(&initial), animation, translate, true)
                    } else {
                        baseline::itg_note_animation_source(
                            black_box(&initial),
                            animation,
                            translate,
                            true,
                        )
                    });
                }
            })
        });
    }
    for (name, key, count, delays) in [
        ("exact", "sprite-prep 8x4.png", 32, vec![0.125; 32]),
        ("padded", "sprite-prep 8x4.png", 32, vec![0.125, 0.25]),
        ("truncated", "sprite-prep 8x4.png", 32, vec![0.125; 64]),
        ("large", "sprite-prep 2x129.png", 129, vec![0.125; 129]),
    ] {
        let initial = fixture(key, Some(64), true);
        let command = state(count, &delays);
        let mut old = initial.clone();
        let mut new = initial.clone();
        baseline::itg_apply_sprite_animation_plan(&mut old, command.clone(), true);
        itg_apply_sprite_animation_plan(&mut new, command.clone(), true);
        compare(&old, &new);
        pairs(|label, new| {
            perf::measure_sampled(&format!("prep_state_{name}_{label}"), 256, 64, || {
                for _ in 0..64 {
                    let mut slot = black_box(&initial).clone();
                    let command = black_box(&command).clone();
                    if new {
                        itg_apply_sprite_animation_plan(&mut slot, command, true);
                    } else {
                        baseline::itg_apply_sprite_animation_plan(&mut slot, command, true);
                    }
                    black_box(slot);
                }
            })
        });
    }
    let command = state(32, &[0.125; 32]);
    pairs(|label, new| {
        perf::measure_sampled_with_setup(
            &format!("prep_state_unique_{label}"),
            128,
            64,
            || {
                (0..64)
                    .map(|_| fixture("sprite-prep 8x4.png", Some(64), true))
                    .collect::<Vec<_>>()
            },
            |slots| {
                for slot in slots {
                    if new {
                        itg_apply_sprite_animation_plan(
                            black_box(slot),
                            black_box(&command).clone(),
                            true,
                        );
                    } else {
                        baseline::itg_apply_sprite_animation_plan(
                            black_box(slot),
                            black_box(&command).clone(),
                            true,
                        );
                    }
                }
            },
        )
    });
}
